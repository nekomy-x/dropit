#!/usr/bin/env node
/**
 * Build the Developer API guides into the binary.
 *
 *   node scripts/build-api-docs.mjs           # regenerate developer_docs_content.rs
 *   node scripts/build-api-docs.mjs --check   # verify the generated file is current
 *
 * Source of truth: `docs/api/sections/*.md`, each with YAML front matter
 *
 *   ---
 *   id: sessions
 *   title: Sessions
 *   summary: One line for the docs menu.
 *   requires: ["sessions:read"]
 *   order: 100
 *   ---
 *
 * The front matter becomes `DocSection` fields; the whole file (front matter
 * included) is embedded with `include_str!`, and the HTTP handler strips the
 * front matter at runtime. `--check` exits 1 when the generated Rust differs
 * from what the markdown would produce, so a stale binary cannot ship.
 *
 * No dependencies: plain Node, same as the other gates in `scripts/`.
 */
import { readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath, pathToFileURL } from 'node:url';

/** Canonical scope list; mirrors `crates/common/src/api_key.rs`. */
export const SCOPES = [
  'docs:read',
  'profile:read',
  'sessions:read',
  'sessions:write',
  'qr:write',
  'requests:read',
  'requests:write',
  'catalog:read',
  'stats:read',
];

export const SECTIONS_DIR = fileURLToPath(new URL('../docs/api/sections/', import.meta.url));
export const OUTPUT_FILE = fileURLToPath(
  new URL('../crates/gateway-api/src/developer_docs_content.rs', import.meta.url),
);

const HEADER = `//! GENERATED FILE — do not edit. Run \`node scripts/build-api-docs.mjs\`.
//!
//! Секции документации Developer API v1 (ADR-010). Источник правды —
//! \`docs/api/sections/*.md\`: front matter становится полями структуры,
//! тело встраивается в бинарник через \`include_str!\`. Ручные правки этого
//! файла затираются при следующей сборке, а \`--check\` в гейте падает.
`;

const STRUCT = `
/// Одна секция документации.
pub struct DocSection {
    pub id: &'static str,
    pub title: &'static str,
    pub summary: &'static str,
    /// Скоупы, без которых секция не видна ключу.
    pub requires: &'static [&'static str],
    /// Порядок в меню (меньше — выше).
    pub order: i32,
    /// Markdown вместе с front matter; хендлер отрезает front matter.
    pub markdown: &'static str,
}

/// Все секции в порядке меню (\`order\`).
pub const SECTIONS: &[DocSection] = &[
`;

/** Rust string literal. Inputs are validated to be single-line UTF-8 text. */
function rust(value) {
  return `"${value.replace(/\\/g, '\\\\').replace(/"/g, '\\"')}"`;
}

/** Split one section file into front matter and body. Throws on malformed input. */
export function parseSection(raw, fileName) {
  const text = raw.replace(/^\uFEFF/, '');
  if (!text.startsWith('---\n')) {
    throw new Error(`${fileName}: file must start with "---" followed by front matter`);
  }
  const lines = text.split('\n');
  const close = lines.indexOf('---', 1);
  if (close < 0) throw new Error(`${fileName}: front matter is not closed with "---"`);
  const front = lines.slice(1, close);
  const body = lines.slice(close + 1).join('\n').trim();

  const meta = {};
  for (const line of front) {
    if (line.trim() === '' || line.trim().startsWith('#')) continue;
    const match = /^([A-Za-z_]+):\s*(.*)$/.exec(line);
    if (!match) throw new Error(`${fileName}: cannot parse front matter line "${line}"`);
    const [, key, value] = match;
    if (Object.prototype.hasOwnProperty.call(meta, key)) {
      throw new Error(`${fileName}: duplicate front matter key "${key}"`);
    }
    meta[key] = value;
  }

  const allowed = ['id', 'title', 'summary', 'requires', 'order'];
  for (const key of Object.keys(meta)) {
    if (!allowed.includes(key)) throw new Error(`${fileName}: unknown front matter key "${key}"`);
  }
  for (const key of allowed) {
    if (meta[key] === undefined) throw new Error(`${fileName}: missing front matter key "${key}"`);
  }

  const id = unquote(meta.id);
  if (!/^[a-z][a-z0-9-]*$/.test(id)) {
    throw new Error(`${fileName}: id must be lower-kebab-case, got "${id}"`);
  }
  const expectedId = fileName.replace(/\.md$/, '');
  if (id !== expectedId) {
    throw new Error(`${fileName}: id "${id}" must equal the file name "${expectedId}"`);
  }

  const title = unquote(meta.title);
  const summary = unquote(meta.summary);
  if (title === '') throw new Error(`${fileName}: title is empty`);
  if (summary === '') throw new Error(`${fileName}: summary is empty`);
  if (/[\r\n]/.test(title) || /[\r\n]/.test(summary)) {
    throw new Error(`${fileName}: title and summary must be single-line`);
  }
  // eslint-disable-next-line no-control-regex
  if (/[\u0000-\u001f]/.test(title + summary)) {
    throw new Error(`${fileName}: title and summary must not contain control characters`);
  }

  let requires;
  try {
    requires = JSON.parse(unquote(meta.requires));
  } catch {
    throw new Error(
      `${fileName}: requires must be a JSON array, e.g. [] or ["sessions:read"] (got ${meta.requires})`,
    );
  }
  if (!Array.isArray(requires) || requires.some((scope) => typeof scope !== 'string')) {
    throw new Error(`${fileName}: requires must be an array of scope strings`);
  }
  if (new Set(requires).size !== requires.length) {
    throw new Error(`${fileName}: requires contains duplicate scopes`);
  }
  for (const scope of requires) {
    if (!SCOPES.includes(scope)) {
      throw new Error(`${fileName}: unknown scope "${scope}" (known: ${SCOPES.join(', ')})`);
    }
  }

  if (!/^\d+$/.test(meta.order)) {
    throw new Error(`${fileName}: order must be a positive integer, got "${meta.order}"`);
  }
  const order = Number(meta.order);

  if (body.length < 200) {
    throw new Error(`${fileName}: body looks empty (${body.length} chars) — sections must be real guides`);
  }
  if (!body.startsWith('# ')) {
    throw new Error(`${fileName}: body must start with a "# " heading`);
  }

  return { id, title, summary, requires, order, body };
}

function unquote(value) {
  const trimmed = value.trim();
  if (
    (trimmed.startsWith('"') && trimmed.endsWith('"')) ||
    (trimmed.startsWith("'") && trimmed.endsWith("'"))
  ) {
    return trimmed.slice(1, -1);
  }
  return trimmed;
}

/** Read and validate every section, sorted by `order`. Throws on any problem. */
export function loadSections() {
  const files = readdirSync(SECTIONS_DIR)
    .filter((name) => name.endsWith('.md'))
    .sort();
  if (files.length === 0) {
    throw new Error(`no sections in ${SECTIONS_DIR} — write docs/api/sections/*.md first`);
  }
  const sections = files.map((name) =>
    parseSection(readFileSync(`${SECTIONS_DIR}${name}`, 'utf8'), name),
  );
  const orders = new Map();
  for (const section of sections) {
    if (orders.has(section.order)) {
      throw new Error(
        `sections "${orders.get(section.order)}" and "${section.id}" share order ${section.order}`,
      );
    }
    orders.set(section.order, section.id);
  }
  sections.sort((a, b) => a.order - b.order || a.id.localeCompare(b.id));
  return sections;
}

/** Render the Rust file for the given sections. */
export function render(sections) {
  const entries = sections
    .map(
      (section) => `    DocSection {
        id: ${rust(section.id)},
        title: ${rust(section.title)},
        summary: ${rust(section.summary)},
        requires: &[${section.requires.map(rust).join(', ')}],
        order: ${section.order},
        markdown: include_str!("../../../docs/api/sections/${section.id}.md"),
    },`,
    )
    .join('\n');
  return `${HEADER}${STRUCT}${entries}\n];\n`;
}

function main() {
  const check = process.argv.includes('--check');
  let sections;
  try {
    sections = loadSections();
  } catch (error) {
    console.error(`build-api-docs: ${error.message}`);
    process.exit(1);
  }
  const expected = render(sections);

  if (check) {
    let actual;
    try {
      actual = readFileSync(OUTPUT_FILE, 'utf8');
    } catch {
      console.error(
        `build-api-docs: ${OUTPUT_FILE} is missing — run node scripts/build-api-docs.mjs`,
      );
      process.exit(1);
    }
    if (actual !== expected) {
      console.error('build-api-docs: developer_docs_content.rs is stale.');
      const a = actual.split('\n');
      const b = expected.split('\n');
      const at = a.findIndex((line, index) => line !== b[index]);
      const line = at < 0 ? Math.max(a.length, b.length) : at + 1;
      console.error(`  first difference at line ${line}`);
      console.error(`  run: node scripts/build-api-docs.mjs`);
      process.exit(1);
    }
    console.log(`API DOCS GENERATED FILE CURRENT — ${sections.length} sections.`);
    return;
  }

  writeFileSync(OUTPUT_FILE, expected);
  console.log(
    `API DOCS BUILT — ${sections.length} sections -> crates/gateway-api/src/developer_docs_content.rs`,
  );
  for (const section of sections) {
    const requires = section.requires.length === 0 ? 'no scope' : section.requires.join(', ');
    console.log(`  ${String(section.order).padStart(3)}  ${section.id.padEnd(16)} ${requires}`);
  }
}

const invokedDirectly =
  process.argv[1] !== undefined && import.meta.url === pathToFileURL(process.argv[1]).href;
if (invokedDirectly) main();
