#!/usr/bin/env node
/**
 * Publish the built guest APK for the test stand.
 *
 *   node scripts/publish-guest-apk.mjs                 # release build first
 *   node scripts/publish-guest-apk.mjs --debug         # debug build
 *   node scripts/publish-guest-apk.mjs --check         # only report what is there
 *
 * The public `/download` page probes `public/downloads/dropit-guest.apk` with a
 * HEAD request and shows the "Android APK" row only when the file exists. The
 * file is gitignored (`.gitignore` already ignores `*.apk`), so this script is
 * the only way it reaches a build — no binary is committed.
 *
 * It is a test-stand capability, not a store: the release build is signed for
 * the stand and the page labels it as a beta. Store listings stay env-driven
 * (`VITE_GUEST_IOS_URL` / `VITE_GUEST_ANDROID_URL`, see
 * `docs/ui/hub-download-contract.md`).
 */
import { copyFileSync, existsSync, mkdirSync, statSync, unlinkSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));
const REPO = resolve(HERE, '..');
const APK_DIR = join(REPO, 'apps/mobile-guest/build/app/outputs/flutter-apk');
const DEST_DIR = join(REPO, 'apps/web-club/public/downloads');
const DEST = join(DEST_DIR, 'dropit-guest.apk');
const SOURCES = [
  join(APK_DIR, 'app-release.apk'),
  join(APK_DIR, 'app-debug.apk'),
];

const checkOnly = process.argv.includes('--check');
const wantDebug = process.argv.includes('--debug');
const candidates = wantDebug ? [...SOURCES].reverse() : SOURCES;
const source = candidates.find((file) => existsSync(file) && statSync(file).size > 0);

const mb = (bytes) => `${(bytes / (1024 * 1024)).toFixed(1)} MB`;

if (!source) {
  console.error('No built guest APK found. Expected one of:');
  for (const file of SOURCES) console.error(`  ${file}`);
  console.error('Build it with: cd apps/mobile-guest && flutter build apk --release');
  process.exit(1);
}

const size = statSync(source).size;
console.log(`source: ${source} (${mb(size)})`);

if (checkOnly) {
  console.log(existsSync(DEST) ? `published: ${DEST} (${mb(statSync(DEST).size)})` : 'published: no');
  process.exit(0);
}

mkdirSync(DEST_DIR, { recursive: true });
if (existsSync(DEST)) unlinkSync(DEST);
copyFileSync(source, DEST);

console.log(`published: ${DEST} (${mb(statSync(DEST).size)})`);
console.log('The /download page shows the Android APK row on the next load (HEAD probe).');
