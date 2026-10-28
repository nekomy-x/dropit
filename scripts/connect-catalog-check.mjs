#!/usr/bin/env node
/**
 * Connect catalog parity — the gap `scripts/i18n-check.mjs` leaves open.
 *
 *   node scripts/connect-catalog-check.mjs          # check, exit 1 on problems
 *   node scripts/connect-catalog-check.mjs --list   # also print key counts
 *
 * `i18n-check.mjs` parses `'quoted': 'value'` catalogs only. The Connect
 * catalog (`apps/web-club/src/connect/messages.ts`) is written with bare
 * identifier keys, so the main gate never looked at it. This script reads the
 * same file with the syntax it actually uses and enforces the three rules that
 * matter for a public page:
 *
 *   1. every key in `en` exists in es/fr/de/pt/ru, and no block invents a key
 *      the others do not have;
 *   2. no key is declared twice inside one block — the second copy wins
 *      silently at runtime, which is how the wrong sentence ships;
 *   3. a `{placeholder}` in `en` appears with the same name in every
 *      translation, and no translation introduces one `en` lacks.
 *
 * The trailing `validationKeys` map (Russian sentence → key) is outside the
 * catalog object and is not parsed; it is a lookup table, not copy.
 *
 * Exit code 0 = clean, 1 = at least one problem.
 */
import { readFileSync } from 'node:fs';

const SOURCE = new URL('../apps/web-club/src/connect/messages.ts', import.meta.url);
const LANGUAGES = ['en', 'es', 'fr', 'de', 'pt', 'ru'];
const REFERENCE = 'en';
const list = process.argv.includes('--list');

const source = readFileSync(SOURCE, 'utf8');

/** Everything from `const messages = {` to `} satisfies` is the catalog. */
const start = source.indexOf('const messages = {');
const end = source.indexOf('} satisfies', start);
if (start < 0 || end < 0) {
  console.error('could not find the catalog object in connect/messages.ts');
  process.exit(1);
}
const catalog = source.slice(start, end);

const blocks = {};
const headerRe = new RegExp(`^  (${LANGUAGES.join('|')}):\\s*\\{$`, 'gm');
const headers = [];
let match;
while ((match = headerRe.exec(catalog)) !== null) {
  headers.push({ language: match[1], index: match.index, bodyStart: headerRe.lastIndex });
}
for (let i = 0; i < headers.length; i += 1) {
  const header = headers[i];
  const bodyEnd = i + 1 < headers.length ? headers[i + 1].index : catalog.length;
  const body = catalog.slice(header.bodyStart, bodyEnd);
  const entries = new Map();
  const duplicates = [];
  // `key: 'value',` or `'key': "value",` — both quote styles occur.
  const entryRe = /(?:'([^']+)'|([A-Za-z_][\w.]*)):\s*(?:'((?:[^'\\]|\\.)*)'|"((?:[^"\\]|\\.)*)")/g;
  let entry;
  while ((entry = entryRe.exec(body)) !== null) {
    const key = entry[1] ?? entry[2];
    if (entries.has(key)) duplicates.push(key);
    entries.set(key, entry[3] ?? entry[4] ?? '');
  }
  blocks[header.language] = { entries, duplicates };
}

const problems = [];
const report = (kind, language, detail) => problems.push({ kind, language, detail });

for (const language of LANGUAGES) {
  if (!blocks[language]) {
    report('missing-block', language, 'the language block was not found');
    continue;
  }
  for (const key of blocks[language].duplicates) report('duplicate', language, key);
}

const reference = blocks[REFERENCE]?.entries ?? new Map();
for (const language of LANGUAGES) {
  const block = blocks[language];
  if (!block || language === REFERENCE) continue;
  for (const key of reference.keys()) {
    if (!block.entries.has(key)) report('missing', language, key);
  }
  for (const key of block.entries.keys()) {
    if (!reference.has(key)) report('orphan', language, key);
  }
}

const placeholders = (value) =>
  [...value.matchAll(/\{(\w+)\}/g)]
    .map((hit) => hit[1])
    .sort()
    .join(',');

for (const language of LANGUAGES) {
  const block = blocks[language];
  if (!block) continue;
  for (const [key, value] of block.entries) {
    const expected = placeholders(reference.get(key) ?? '');
    const actual = placeholders(value);
    if (expected !== actual) report('placeholder', language, `${key}: [${actual}] vs en [${expected}]`);
  }
}

if (list) {
  for (const language of LANGUAGES) {
    console.log(`${language}: ${blocks[language]?.entries.size ?? 0} keys`);
  }
}

if (problems.length === 0) {
  console.log(`CONNECT CATALOG GREEN — ${LANGUAGES.length} languages, ${reference.size} keys each.`);
  process.exit(0);
}

for (const problem of problems) {
  console.error(`${problem.kind} ${problem.language}: ${problem.detail}`);
}
console.error(`\n${problems.length} problem(s) in connect/messages.ts`);
process.exit(1);
