#!/usr/bin/env node
/**
 * Developer API documentation gate (ADR-010 §8).
 *
 *   node scripts/api-docs-check.mjs
 *
 * Enforces, with no dependencies:
 *
 *   1. every `docs/api/sections/*.md` has valid front matter, an id equal to
 *      its file name, a unique order, and only known scopes in `requires`;
 *   2. every `x-required-scope` value in `docs/api/openapi-developer.yaml` is a
 *      known scope, and every operation declares its permission — either a
 *      known scope or an explicit `x-no-scope: true` (an operation with neither
 *      is filtered out of the served `/openapi.json` for every key);
 *   3. every path in `docs/api/openapi-developer.yaml` is mentioned in at
 *      least one guide, and every `{template}` path a guide mentions exists in
 *      the contract;
 *   4. `crates/gateway-api/src/developer_docs_content.rs` matches the markdown
 *      (stale generated Rust fails the build);
 *   5. every `sections/<name>.md` link in `docs/api/README.md` resolves.
 *
 * Paths are compared after prefixing the `servers[0].url` when the contract
 * writes them server-relative (`/sessions` instead of `/v1/developer/sessions`).
 * Exit code 0 = clean, 1 = at least one problem.
 */
import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

import {
  loadSections,
  render,
  OUTPUT_FILE,
  SCOPES,
  SECTIONS_DIR,
} from './build-api-docs.mjs';

const CONTRACT = fileURLToPath(new URL('../docs/api/openapi-developer.yaml', import.meta.url));
const README = fileURLToPath(new URL('../docs/api/README.md', import.meta.url));
const DEFAULT_PREFIX = '/v1/developer';

const problems = [];

function problem(message) {
  problems.push(message);
}

// --------------------------------------------------------------------------
// 1. Guides: front matter, ids, order, scopes (loadSections validates all)
// --------------------------------------------------------------------------
let sections = [];
try {
  sections = loadSections();
} catch (error) {
  problem(error.message);
}

const corpus = sections.map((section) => section.body).join('\n\n');
const sectionFiles = new Set(
  existsSync(SECTIONS_DIR)
    ? readdirSync(SECTIONS_DIR).filter((name) => name.endsWith('.md'))
    : [],
);

// --------------------------------------------------------------------------
// 2 + 3. Contract: scopes and path coverage
// --------------------------------------------------------------------------
const contract = existsSync(CONTRACT) ? readFileSync(CONTRACT, 'utf8') : null;
if (contract === null) {
  problem('docs/api/openapi-developer.yaml is missing');
} else {
  const serverPrefix = readServerPrefix(contract);
  const paths = readPaths(contract, serverPrefix);
  if (paths.length === 0) {
    problem(
      'docs/api/openapi-developer.yaml declares no paths — the public contract is empty',
    );
  }
  // Coverage needs the guides: if a section failed to load, one front-matter
  // error would otherwise cascade into "every path is undocumented".
  if (sections.length > 0) {
    for (const path of paths) {
      if (!pathDocumented(path, corpus)) {
        problem(`path ${path} from openapi-developer.yaml is not mentioned in any guide`);
      }
    }
  }
  for (const scope of readRequiredScopes(contract)) {
    if (!SCOPES.includes(scope)) {
      problem(`x-required-scope "${scope}" in openapi-developer.yaml is not a known scope`);
    }
  }
  checkOperationScopes(contract, serverPrefix);
  // The reverse check only means something once the contract has paths; an
  // empty contract is already reported above and would flag every guide.
  if (paths.length > 0) checkDocumentedTemplates(contract, serverPrefix, corpus);
}

/** First `servers[].url`, used to expand server-relative path keys. */
function readServerPrefix(text) {
  const lines = text.split('\n');
  const start = lines.findIndex((line) => /^servers:\s*$/.test(line));
  if (start < 0) return DEFAULT_PREFIX;
  for (const line of lines.slice(start + 1)) {
    if (/^\S/.test(line) && line.trim() !== '') break; // left the servers block
    const match = /^\s*-\s*url:\s*["']?([^"'\s]+)["']?/.exec(line);
    if (match) return match[1];
  }
  return DEFAULT_PREFIX;
}

/** Path keys under the top-level `paths:` block. */
function readPaths(text, serverPrefix) {
  const lines = text.split('\n');
  const paths = [];
  let inside = false;
  for (const line of lines) {
    if (/^paths:\s*(\{\s*\})?\s*$/.test(line)) {
      inside = true;
      continue;
    }
    if (!inside) continue;
    if (/^[A-Za-z_][\w-]*:/.test(line)) break; // next top-level key
    const match = /^\s{2}(["']?)(\/[^"'\s:]+)\1:\s*(#.*)?$/.exec(line);
    if (match) paths.push(expandPath(match[2], serverPrefix));
  }
  return [...new Set(paths)];
}

function expandPath(path, serverPrefix) {
  if (path.startsWith('/v1/developer')) return path;
  const serverPath = serverPrefix.replace(/^[a-z]+:\/\/[^/]+/i, '');
  const base = serverPath.endsWith('/') ? serverPath.slice(0, -1) : serverPath;
  return `${base}${path}`;
}

function escapeRegExp(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

/** A path is covered by its literal text or by a concrete example of it. */
function pathDocumented(path, text) {
  const boundary = '(?![A-Za-z0-9_/{}-])';
  if (new RegExp(escapeRegExp(path) + boundary).test(text)) return true;
  const templated = escapeRegExp(path)
    .replace(/\\\{[^}]+\\\}/g, '[A-Za-z0-9_.~-]+')
    .replace(/\\\{/g, '{');
  return new RegExp(templated + boundary).test(text);
}

/**
 * Every `x-required-scope:` value in the contract, deduplicated. `null` / `none`
 * / `~` are explicit "no scope" sentinels (paired with `x-no-scope: true`), not
 * scope names.
 */
function readRequiredScopes(text) {
  const values = new Set();
  const regex = /x-required-scope:\s*["']?([^"'\s#]+)["']?/g;
  let match;
  while ((match = regex.exec(text)) !== null) {
    const value = match[1];
    if (["null", "none", "~"].includes(value)) continue;
    values.add(value);
  }
  return [...values];
}

/** Every operation in the contract, with the lines that belong to it. */
function readOperations(text, serverPrefix) {
  const lines = text.split('\n');
  const operations = [];
  let inside = false;
  let currentPath = null;
  let current = null;

  const close = (end) => {
    if (!current) return;
    current.end = end;
    current.block = lines.slice(current.start, end);
    operations.push(current);
    current = null;
  };

  for (let index = 0; index < lines.length; index += 1) {
    const line = lines[index];
    if (!inside) {
      if (/^paths:\s*(\{\s*\})?\s*$/.test(line)) inside = true;
      continue;
    }
    if (/^[A-Za-z_][\w-]*:/.test(line)) {
      close(index); // next top-level key ends the paths block
      break;
    }
    const pathMatch = /^\s{2}(["']?)(\/[^"'\s:]+)\1:/.exec(line);
    if (pathMatch) {
      close(index);
      currentPath = expandPath(pathMatch[2], serverPrefix);
      continue;
    }
    const methodMatch = /^\s{4}(get|post|put|patch|delete|head|options):/.exec(line);
    if (methodMatch && currentPath) {
      close(index);
      current = { path: currentPath, method: methodMatch[1], start: index + 1, end: lines.length };
    }
  }
  close(lines.length);
  return operations;
}

/**
 * Every Developer API operation must declare its permission: either a known
 * scope in `x-required-scope`, or `x-no-scope: true` for an endpoint that is
 * authenticated but needs no data scope (`GET /me` today). An operation with
 * neither is invisible to the served `/openapi.json` filter — a contract gap,
 * not a documentation problem, and it fails the gate.
 *
 * The path is not hardcoded: any operation may opt out of scopes explicitly,
 * and anything that does not opt out must name a scope.
 */
function checkOperationScopes(text, serverPrefix) {
  const operations = readOperations(text, serverPrefix);
  const noScopeOps = [];
  for (const operation of operations) {
    const label = `${operation.method.toUpperCase()} ${operation.path}`;
    let declared = null;
    for (const line of operation.block) {
      const scope = /x-required-scope:\s*["']?([^"'\s#]+)["']?/.exec(line);
      if (scope) declared = scope[1];
    }
    const noScope = operation.block.some((line) => /x-no-scope:\s*true\b/.test(line));
    const badNoScope = operation.block.some(
      (line) => /x-no-scope:/.test(line) && !/x-no-scope:\s*true\b/.test(line),
    );
    if (badNoScope) {
      problem(`${label} declares x-no-scope with a value other than true`);
    }
    if (declared !== null && declared !== 'null' && declared !== 'none' && declared !== '~') {
      if (noScope) {
        problem(`${label} declares both a scope (${declared}) and x-no-scope: true`);
      }
      continue; // known-or-unknown is validated by the scope scan above
    }
    if (noScope) {
      noScopeOps.push(label);
      continue;
    }
    problem(
      `${label} has neither a known x-required-scope nor x-no-scope: true — ` +
        'the served /openapi.json would filter it out for every key',
    );
  }
  return { count: operations.length, noScopeOps };
}

/**
 * Reverse check: every `{template}` path in a guide must exist in the contract.
 * Concrete examples (UUIDs) are not checked — a literal id is an example, not a
 * route — but a templated path is a claim about the surface.
 */
function checkDocumentedTemplates(text, serverPrefix, corpus) {
  const contractPaths = new Set(readPaths(text, serverPrefix));
  const regex = /\/v1\/developer\/[A-Za-z0-9_/{}-]*\{[A-Za-z0-9_-]+\}[A-Za-z0-9_/{}-]*/g;
  const seen = new Set();
  let match;
  while ((match = regex.exec(corpus)) !== null) {
    const path = match[0].replace(/[.,;:)]+$/, '');
    if (seen.has(path)) continue;
    seen.add(path);
    if (!contractPaths.has(path)) {
      problem(`guide mentions ${path}, which is not a path in openapi-developer.yaml`);
    }
  }
}

// --------------------------------------------------------------------------
// 4. Generated Rust is current
// --------------------------------------------------------------------------
if (sections.length > 0) {
  const expected = render(sections);
  if (!existsSync(OUTPUT_FILE)) {
    problem('crates/gateway-api/src/developer_docs_content.rs is missing — run node scripts/build-api-docs.mjs');
  } else if (readFileSync(OUTPUT_FILE, 'utf8') !== expected) {
    problem(
      'developer_docs_content.rs is stale — run node scripts/build-api-docs.mjs (the binary embeds the old guides)',
    );
  }
}

// --------------------------------------------------------------------------
// 5. README links resolve
// --------------------------------------------------------------------------
if (existsSync(README)) {
  const readme = readFileSync(README, 'utf8');
  const regex = /\]\(([^)\s]*sections\/[^)\s]+\.md)\)/g;
  let match;
  while ((match = regex.exec(readme)) !== null) {
    const target = match[1].replace(/^\.\//, '');
    if (!sectionFiles.has(target.replace(/^sections\//, ''))) {
      problem(`docs/api/README.md links to missing guide ${match[1]}`);
    }
  }
}

// --------------------------------------------------------------------------
// Report
// --------------------------------------------------------------------------
if (problems.length === 0) {
  const scopeCount = readRequiredScopes(contract ?? '').filter((scope) =>
    SCOPES.includes(scope),
  ).length;
  const pathCount = readPaths(contract ?? '', readServerPrefix(contract ?? '')).length;
  console.log(
    `API DOCS GREEN — ${sections.length} guides, ${pathCount} contract paths, ${scopeCount} scopes referenced.`,
  );
  process.exit(0);
}

for (const message of problems) {
  console.error(`api-docs: ${message}`);
}
console.error(`\n${problems.length} problem(s).`);
process.exit(1);
