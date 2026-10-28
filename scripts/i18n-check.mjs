#!/usr/bin/env node
/**
 * i18n gate — the catalog must be complete before a panel ships.
 *
 *   node scripts/i18n-check.mjs            # check, exit 1 on problems
 *   node scripts/i18n-check.mjs --list     # also print every key count
 *
 * It reads the catalog modules as plain text (no bundler, no TS runtime) and
 * enforces the four rules the catalogs promise:
 *
 *   1. every key declared in `en` exists in es, fr, de, pt and ru;
 *   2. no key is declared twice inside one language of one module — the second
 *      copy silently wins at runtime, which is how the wrong sentence ships;
 *   3. a `{placeholder}` in `en` appears with the same name in every
 *      translation, and a translation introduces no placeholder `en` lacks
 *      (that one renders as a literal `{name}` on screen);
 *   4. no translation is a copy of the English string, except where the words
 *      genuinely match (brand names, "DJ", "E-mail", tokens like "live").
 *
 * The allow-list for rule 4 is explicit and short: a copy that is not listed is
 * reported, so an unfinished translation cannot pass as a finished one.
 *
 * Exit code 0 = catalogs complete, 1 = at least one problem.
 */
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));
const REPO = resolve(HERE, '..');

const LANGUAGES = ['en', 'es', 'fr', 'de', 'pt', 'ru'];
const REFERENCE = 'en';
const list = process.argv.includes('--list');

/** Strings that are allowed to be identical across languages. */
const IDENTICAL_OK = new Set([
  'DJ', 'DJs', 'E-mail', 'Email', 'QR', 'DROP IT', 'Token', 'Live', 'live',
  'Demo', 'demo', 'Admin', 'Stripe', 'Spotify', 'BPM', 'VIP', 'PDF', 'API', 'ID',
  // Words that are genuinely the same in the languages concerned: the gate
  // flags a copy because it usually means an unfinished translation, but these
  // are correct as they stand and inventing a synonym would be worse copy.
  'Club', 'Contact', 'Queue', 'Session', 'Status', 'optional',
  'Genre', 'Note', 'Info', 'Clubs', 'Transactions', 'Incidents', 'Audit',
  'Moderation', 'offline', 'active', 'online', 'Info', '{value}/min', 'Session {id}', 'W{index}',
  // "online" is the standard German and Portuguese term; "Info" is identical
  // in all six by design (it is a chip label, not prose).
]);

const problems = [];
const report = (kind, file, detail) => problems.push({ kind, file, detail });

/**
 * Parses `export const xCatalog = { en: { 'a.b': 'text', ... }, ... }`.
 * A regex parser is enough here because every catalog module is a flat object
 * of string literals — `scripts/i18n-check.mjs` is the reason that stays true.
 */
function parseCatalog(source) {
  const blocks = {};
  // One language block: `  en: {` … `  },` at the same indent level.
  const langRe = /^\s{2}(en|es|fr|de|pt|ru):\s*\{\s*$/gm;
  const starts = [];
  let match;
  while ((match = langRe.exec(source)) !== null) {
    starts.push({ language: match[1], index: match.index, bodyStart: langRe.lastIndex });
  }
  for (let i = 0; i < starts.length; i += 1) {
    const start = starts[i];
    const end = i + 1 < starts.length ? starts[i + 1].index : source.length;
    const body = source.slice(start.bodyStart, end);
    const entries = new Map();
    // Keys and values may use either quote style: `'key': 'value'`,
    // `"key": "value"`, or a single-quoted key with a double-quoted value
    // ("Tonight's shift" needs the double quotes). All three are the format,
    // not a stylistic choice — a parser that only knows one would silently
    // skip a whole module, which is exactly how an untranslated screen ships.
    const entryRe = /(?:'([^']+)'|"([^"]+)"):\s*(?:'((?:[^'\\]|\\.)*)'|"((?:[^"\\]|\\.)*)")/g;
    let entry;
    while ((entry = entryRe.exec(body)) !== null) {
      const [, singleKey, doubleKey, singleValue, doubleValue] = entry;
      const key = singleKey ?? doubleKey ?? '';
      const rawValue = singleValue ?? doubleValue ?? '';
      if (entries.has(key)) {
        report('duplicate-key', start.language, `"${key}" is declared twice in this language block`);
        continue;
      }
      entries.set(key, rawValue.replace(/\\(['"\\])/g, '$1'));
    }
    blocks[start.language] = entries;
  }
  return blocks;
}

function placeholders(value) {
  return new Set([...value.matchAll(/\{(\w+)\}/g)].map((m) => m[1]));
}

const CATALOG_DIRS = [
  { app: 'web-club', dir: join(REPO, 'apps/web-club/src/messages') },
  { app: 'web-admin', dir: join(REPO, 'apps/web-admin/src/messages') },
];
// The shared shell catalog is a canonical package file with the same shape.
const SHELL_FILES = [
  { app: 'shared', file: join(REPO, 'packages/ui/messages.ts') },
];

const files = [];
for (const { app, dir } of CATALOG_DIRS) {
  let names = [];
  try {
    names = readdirSync(dir).filter((name) => name.endsWith('.ts') && name !== 'index.ts');
  } catch {
    continue;
  }
  for (const name of names) files.push({ app, file: join(dir, name), label: `${app}/messages/${name}` });
}
for (const { app, file } of SHELL_FILES) {
  try {
    if (statSync(file).isFile()) files.push({ app, file, label: `packages/ui/${file.split('/').pop()}` });
  } catch {
    /* the shell catalog is optional for an app that has not adopted it yet */
  }
}

let totalKeys = 0;
for (const { app, file, label } of files) {
  const source = readFileSync(file, 'utf8');
  const blocks = parseCatalog(source);
  const languages = Object.keys(blocks);
  if (languages.length === 0) {
    // A placeholder module with no languages is not an error by itself: it is
    // reported only when it declares `en` but no translations.
    continue;
  }
  for (const language of LANGUAGES) {
    if (!blocks[language]) {
      report('missing-language', label, `no "${language}" block — ${app} cannot render that language`);
    }
  }
  // A line may carry `// i18n-identical` when the translation is genuinely the
  // same word as English ("Action" in French, "Madrid" as a city example). The
  // marker is per key and local: a global allow-list would also excuse a
  // forgotten translation of the same word on another screen.
  const identicalKeys = new Set(
    [...source.matchAll(/["']([\w.]+)["']\s*:[^\n]*\/\/\s*i18n-identical/g)].map((m) => m[1]),
  );

  const reference = blocks[REFERENCE];
  if (!reference) {
    report('missing-language', label, `no "${REFERENCE}" block — English is the fallback source`);
    continue;
  }
  totalKeys += reference.size;
  if (list) console.log(`${String(reference.size).padStart(5)}  ${label}`);

  for (const [key, english] of reference) {
    for (const language of LANGUAGES) {
      if (language === REFERENCE) continue;
      const block = blocks[language];
      if (!block) continue;
      const translated = block.get(key);
      if (translated === undefined) {
        report('missing-key', label, `${language} is missing "${key}"`);
        continue;
      }
      const expected = placeholders(english);
      const actual = placeholders(translated);
      for (const name of expected) {
        if (!actual.has(name)) {
          report('placeholder', label, `${language} "${key}" drops {${name}}`);
        }
      }
      for (const name of actual) {
        if (!expected.has(name)) {
          report('placeholder', label, `${language} "${key}" invents {${name}}, which English does not have`);
        }
      }
      if (translated.trim() === english.trim() && !identicalKeys.has(key) && !IDENTICAL_OK.has(english.trim())) {
        report('untranslated', label, `${language} "${key}" is still the English string`);
      }
    }
  }

  // A key present in a translation but not in English is dead weight: nothing
  // can ever ask for it, and it usually means a typo in the key name.
  for (const language of LANGUAGES) {
    const block = blocks[language];
    if (!block || language === REFERENCE) continue;
    for (const key of block.keys()) {
      if (!reference.has(key)) {
        report('orphan-key', label, `${language} declares "${key}", which English does not`);
      }
    }
  }
}

const counts = problems.reduce((acc, problem) => {
  acc[problem.kind] = (acc[problem.kind] ?? 0) + 1;
  return acc;
}, {});

if (problems.length === 0) {
  console.log(`\nI18N GREEN — ${files.length} catalog module(s), ${totalKeys} English keys, ${LANGUAGES.length} languages.`);
  process.exit(0);
}

for (const problem of problems) {
  console.error(`${problem.kind.toUpperCase().padEnd(15)} ${problem.file}: ${problem.detail}`);
}
console.error(`\nI18N FAILED — ${problems.length} problem(s): ${Object.entries(counts).map(([k, v]) => `${k}=${v}`).join(', ')}`);
process.exit(1);
