#!/usr/bin/env node
/** Generate the public association for the APK that will actually be shipped.
 * DROPIT_GUEST_ANDROID_SHA256=AA:BB:... node scripts/guest-app-links.mjs
 * The fingerprint is public. Never use the keystore password here. */
import { mkdirSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const fingerprint = process.env.DROPIT_GUEST_ANDROID_SHA256?.trim().toUpperCase();
const iosAppId = process.env.DROPIT_GUEST_IOS_APP_ID?.trim();
const packageName = process.env.DROPIT_GUEST_ANDROID_PACKAGE ?? 'com.example.dropit_guest';
if (!fingerprint && !iosAppId) throw new Error('Set DROPIT_GUEST_ANDROID_SHA256 and/or DROPIT_GUEST_IOS_APP_ID (TeamID.BundleID).');
if (fingerprint && !/^(?:[0-9A-F]{2}:){31}[0-9A-F]{2}$/.test(fingerprint)) {
  throw new Error('Set DROPIT_GUEST_ANDROID_SHA256 to the signing certificate SHA-256 fingerprint (32 colon-separated bytes).');
}
if (!/^[a-zA-Z]\w*(?:\.[a-zA-Z]\w*)+$/.test(packageName)) throw new Error('Invalid Android package name');
if (iosAppId && !/^[A-Z0-9]{10}\.[A-Za-z0-9.-]+$/.test(iosAppId)) throw new Error('Invalid iOS TeamID.BundleID');
const directory = fileURLToPath(new URL('../apps/web-club/public/.well-known/', import.meta.url));
mkdirSync(directory, { recursive: true });
if (fingerprint) writeFileSync(`${directory}/assetlinks.json`, `${JSON.stringify([{
  relation: ['delegate_permission/common.handle_all_urls'],
  target: { namespace: 'android_app', package_name: packageName, sha256_cert_fingerprints: [fingerprint] },
}], null, 2)}\n`);
if (iosAppId) writeFileSync(`${directory}/apple-app-site-association`, `${JSON.stringify({
  applinks: { apps: [], details: [{ appID: iosAppId, paths: ['/join'] }] },
}, null, 2)}\n`);
console.log('Wrote guest app associations in apps/web-club/public/.well-known. Build web-club and serve over HTTPS on the app host.');
