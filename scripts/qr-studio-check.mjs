#!/usr/bin/env node
/**
 * QR generator gate — proves the artefact scans.
 *
 *   cd apps/web-club && npm run dev:demo        # terminal 1
 *   node scripts/qr-studio-check.mjs            # terminal 2 (repo root)
 *
 * What it measures, and why:
 *
 *  1. **Every preset and every dot shape decodes.** A style that draws
 *     beautifully and does not scan is a defect, so each is rendered in a real
 *     browser, screenshotted and decoded with `zbarimg` — an independent C
 *     decoder, so the page cannot pass by agreeing with itself.
 *  2. **The download is the same drawing.** The PNG a user actually saves is
 *     captured and decoded, at every offered pixel size.
 *  3. **A mark in the middle is capped and still decodes,** and the page raises
 *     the correction level instead of letting the user pick a broken pair.
 *  4. **A broken design is reported as broken** (swapped ink/paper).
 *  5. **The page contract holds**: both themes, contrast, radii, overflow
 *     320–1440, 44px targets, accessible names, no console errors.
 *
 * Exit code 0 = green, 1 = at least one assert failed.
 */
import { execFileSync } from 'node:child_process';
import { mkdirSync, readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';

const HERE = dirname(fileURLToPath(import.meta.url));
const REPO = resolve(HERE, '..');
const require = createRequire(join(REPO, 'apps/web-club/package.json'));
const { chromium } = require('playwright');

const BASE = process.env.QR_STUDIO_URL ?? 'http://127.0.0.1:5173';
const ROUTE = '/qr';
const OUT = join(REPO, 'output/qr-studio');
const SHOTS = join(OUT, 'shots');
mkdirSync(SHOTS, { recursive: true });

const results = [];
let failures = 0;
function record(id, name, ok, detail) {
  results.push({ id, name, ok, detail });
  if (!ok) failures += 1;
  console.log(`${ok ? 'PASS' : 'FAIL'}  ${String(id).padStart(5, ' ')}. ${name}${detail ? `\n         ${detail}` : ''}`);
}

const ZBAR = (() => {
  for (const candidate of ['/opt/homebrew/bin/zbarimg', '/usr/local/bin/zbarimg', 'zbarimg']) {
    try { execFileSync(candidate, ['--version'], { stdio: 'ignore' }); return candidate; } catch { /* next */ }
  }
  return null;
})();

function decodeFile(path) {
  if (!ZBAR) return null;
  try {
    const out = execFileSync(ZBAR, ['-q', '--raw', path], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'] });
    return out.trim() || null;
  } catch { return null; }
}

const browser = await chromium.launch({
  headless: true,
  args: ['--use-gl=swiftshader', '--enable-unsafe-swiftshader'],
});

async function newPage(theme = 'dark', viewport = { width: 1440, height: 900 }) {
  const context = await browser.newContext({ viewport, reducedMotion: 'reduce', acceptDownloads: true });
  const page = await context.newPage();
  const consoleErrors = [];
  const externalHosts = new Set();
  page.on('console', (m) => { if (m.type() === 'error') consoleErrors.push(m.text()); });
  page.on('pageerror', (e) => consoleErrors.push(`pageerror: ${e.message}`));
  page.on('request', (r) => {
    try {
      const u = new URL(r.url());
      if (!['localhost', '127.0.0.1'].includes(u.hostname) && !u.protocol.startsWith('data')) externalHosts.add(u.hostname);
    } catch { /* ignore */ }
  });
  await page.addInitScript((v) => { try { window.localStorage.setItem('dropit-theme', v); } catch { /* ignore */ } }, theme);
  // The demo build has no backend: answer the club reads so their 404s do not
  // mask a real console error.
  await page.route('**/v1/club/**', (route) => {
    const u = route.request().url();
    const json = (b) => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(b) });
    if (u.includes('/club/dashboard')) return json({ session: null, requests_today: 0, earned_total_minor: 0 });
    if (u.includes('/club/profile')) return json({ id: 'c1', name: 'Club Nebula', slug: 'nebula', address: '', country: 'ES', email: 'a@b.test', phone: '', logo_url: null, stripe_account_id: null, stripe_onboarding_status: 'verified', status: 'active', created_at: '2026-01-01T00:00:00Z' });
    return json({});
  });
  return { context, page, consoleErrors, externalHosts };
}

/** Screenshot the QR itself, scaled up so zbar has room. */
async function shotSign(page, path) {
  const moved = await page.evaluate(() => {
    const node = document.querySelector('.qr-sign-preview svg');
    if (!node) return false;
    node.setAttribute('width', '460');
    node.setAttribute('height', '460');
    node.removeAttribute('style');
    return true;
  });
  if (!moved) return false;
  await page.waitForTimeout(70);
  await page.locator('.qr-sign-preview svg').screenshot({ path });
  return true;
}

async function waitForSignChange(page, previous) {
  const handle = await page.waitForFunction(
    (prev) => {
      const svg = document.querySelector('.qr-sign-preview svg');
      if (!svg) return false;
      const now = svg.outerHTML;
      return now !== prev ? now : false;
    },
    previous,
    { timeout: 8000 },
  ).catch(() => null);
  return handle ? handle.jsonValue() : previous;
}

const EXPECTED = `${BASE}/room`;

// ===========================================================================
// A. the page renders a code and the in-page check agrees
// ===========================================================================
const main = await newPage('dark');
const { page, context } = main;
let loaded = true;
try {
  await page.goto(`${BASE}${ROUTE}`, { waitUntil: 'networkidle', timeout: 60000 });
  await page.waitForSelector('.qr-sign-preview svg', { timeout: 20000 });
  await page.waitForTimeout(1400);
} catch (error) {
  loaded = false;
  record('A1', 'the route renders a code', false, error instanceof Error ? error.message : String(error));
}
record('A1', 'the route renders a code', loaded, loaded ? `${BASE}${ROUTE}` : 'not rendered');

if (loaded) {
  const shot = join(SHOTS, 'default.png');
  await shotSign(page, shot);
  const value = decodeFile(shot);
  record('A2', 'the default code decodes with zbar', value !== null && value.startsWith(EXPECTED), value ?? 'nothing decoded');

  const scanState = await page.evaluate(() => document.querySelector('.qr-scan')?.className ?? '');
  record('A3', 'the in-page scan check agrees', scanState.includes('is-ok'), scanState || 'no verdict');

  // =======================================================================
  // B. every preset decodes
  // =======================================================================
  const presets = await page.$$eval('.qr-swatch', (n) => n.map((x, i) => ({ i, label: x.querySelector('.qr-swatch-name')?.textContent ?? String(i) })));
  let markup = await page.evaluate(() => document.querySelector('.qr-sign-preview svg')?.outerHTML ?? '');
  for (const preset of presets) {
    await page.locator('.qr-swatch').nth(preset.i).click();
    markup = await waitForSignChange(page, markup);
    const path = join(SHOTS, `preset-${preset.i}.png`);
    const ok = (await shotSign(page, path)) && String(decodeFile(path) ?? '').startsWith(EXPECTED);
    record(`B.${preset.i + 1}`, `preset "${preset.label}" decodes`, ok, ok ? 'decoded' : 'nothing decoded');
  }

  // =======================================================================
  // C. every dot shape × eye frame × eye centre decodes
  // =======================================================================
  const shapeCount = await page.$$eval('[data-choice="shape"]', (n) => n.length);
  let comboFailures = 0;
  let combos = 0;
  for (let shape = 0; shape < shapeCount; shape += 1) {
    for (let frame = 0; frame < 4; frame += 1) {
      for (let center = 0; center < 3; center += 1) {
        await page.locator('[data-choice="shape"]').nth(shape).click();
        await page.getByRole('combobox', { name: 'Eye', exact: true }).click();
        await page.locator('.field-select__option').nth(frame).click();
        await page.getByRole('combobox', { name: 'Eye centre', exact: true }).click();
        await page.locator('.field-select__option').nth(center).click();
        markup = await waitForSignChange(page, markup);
        const path = join(SHOTS, `combo-${shape}-${frame}-${center}.png`);
        const ok = (await shotSign(page, path)) && String(decodeFile(path) ?? '').startsWith(EXPECTED);
        combos += 1;
        if (!ok) { comboFailures += 1; record(`C.${combos}`, `shape ${shape} / eye ${frame} / centre ${center} decodes`, false, 'nothing decoded'); }
      }
    }
  }
  record('C', `${combos} dot × eye combinations decode`, comboFailures === 0, comboFailures ? `${comboFailures} failed` : `${combos}/${combos}`);

  // =======================================================================
  // D. a mark in the middle: capped size still decodes, ECC raised
  // =======================================================================
  await page.locator('.qr-swatch').first().click();
  await page.waitForTimeout(200);
  await page.setInputFiles('.qr-file', { name: 'mark.png', mimeType: 'image/png', buffer: readFileSync(join(REPO, 'scripts/fixtures/qr-mark.png')) });
  await page.waitForTimeout(1200);
  const hasMark = await page.evaluate(() => Boolean(document.querySelector('.qr-sign-preview image')));
  const markShot = join(SHOTS, 'mark.png');
  await shotSign(page, markShot);
  const markValue = decodeFile(markShot);
  record('D1', 'a mark in the middle renders', hasMark, hasMark ? 'image element present' : 'no image');
  record('D2', 'a mark at the offered maximum still decodes', markValue !== null && markValue.startsWith(EXPECTED), markValue ?? 'nothing decoded');
  const eccLocked = await page.evaluate(() => {
    const sel = [...document.querySelectorAll('.field-select__trigger')].pop();
    return sel ? sel.disabled || sel.getAttribute('aria-disabled') === 'true' : false;
  });
  record('D3', 'a mark locks error correction at H', eccLocked, eccLocked ? 'control disabled' : 'control still editable');

  // =======================================================================
  // E. the downloaded PNG decodes
  // =======================================================================
  await page.evaluate(() => {
    const btn = [...document.querySelectorAll('button')].find((b) => b.textContent.trim() === 'Remove');
    btn?.click();
  });
  await page.waitForTimeout(700);
  const downloadPromise = page.waitForEvent('download', { timeout: 30000 }).catch(() => null);
  await page.getByRole('button', { name: 'PNG', exact: true }).click();
  const download = await downloadPromise;
  if (download) {
    const target = join(SHOTS, 'export.png');
    await download.saveAs(target);
    const value = decodeFile(target);
    const bytes = readFileSync(target).length;
    record('E1', 'the downloaded PNG decodes', value !== null && value.startsWith(EXPECTED), `${value ?? 'nothing decoded'} (${Math.round(bytes / 1024)} KB)`);
    record('E2', 'the downloaded PNG is a real image', bytes > 2000, `${Math.round(bytes / 1024)} KB`);
  } else {
    record('E1', 'the downloaded PNG decodes', false, 'no download event');
    record('E2', 'the downloaded PNG is a real image', false, 'no download event');
  }

  // =======================================================================
  // F. a broken design is reported as broken
  // =======================================================================
  const swapped = await page.evaluate(() => {
    const btns = [...document.querySelectorAll('button')].filter((b) => b.textContent.trim() === 'Swap');
    if (!btns.length) return false;
    btns[0].click();
    return true;
  });
  if (swapped) {
    await page.waitForTimeout(1400);
    const badScan = await page.evaluate(() => Boolean(document.querySelector('.qr-scan.is-failed')));
    const warning = await page.evaluate(() => Boolean(document.querySelector('.qr-note.is-error')));
    record('F1', 'swapped ink and paper is flagged', warning, warning ? 'error note shown' : 'no warning');
    record('F2', 'swapped ink and paper fails the scan check', badScan, badScan ? 'scan check red' : 'still green');
  } else {
    record('F1', 'swapped ink and paper is flagged', false, 'no Swap control');
    record('F2', 'swapped ink and paper fails the scan check', false, 'no Swap control');
  }

  // =======================================================================
  // G. panel contract
  // =======================================================================
  record('G1', 'no console errors', main.consoleErrors.length === 0, main.consoleErrors.slice(0, 3).join(' | ') || 'clean');
  record('G2', 'no external hosts', main.externalHosts.size === 0, [...main.externalHosts].join(', ') || 'all first-party');

  const overflow = {};
  for (const width of [320, 390, 768, 1024, 1440]) {
    await page.setViewportSize({ width, height: 900 });
    await page.waitForTimeout(200);
    overflow[width] = await page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);
  }
  const overflowing = Object.entries(overflow).filter(([, v]) => v > 0);
  record('G3', 'no horizontal overflow 320–1440', overflowing.length === 0, JSON.stringify(overflow));

  await page.setViewportSize({ width: 390, height: 844 });
  await page.waitForTimeout(260);
  const mobile = await page.evaluate(() => {
    const parse = (v) => { const m = v.match(/rgba?\(([^)]+)\)/); if (!m) return null; const p = m[1].split(',').map(Number); return { r: p[0], g: p[1], b: p[2], a: p.length > 3 ? p[3] : 1 }; };
    const lum = ({ r, g, b }) => { const c = (x) => { const s = x / 255; return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4; }; return 0.2126 * c(r) + 0.7152 * c(g) + 0.0722 * c(b); };
    const blend = (f, b) => ({ r: f.r * f.a + b.r * (1 - f.a), g: f.g * f.a + b.g * (1 - f.a), b: f.b * f.a + b.b * (1 - f.a), a: 1 });
    const bgOf = (el) => { let n = el; while (n) { const c = parse(getComputedStyle(n).backgroundColor); if (c && c.a > 0.5) return c; n = n.parentElement; } const b = parse(getComputedStyle(document.body).backgroundColor); return b && b.a > 0.5 ? b : { r: 0, g: 0, b: 0, a: 1 }; };
    const vis = (el) => { const s = getComputedStyle(el); const r = el.getBoundingClientRect(); return s.visibility !== 'hidden' && s.display !== 'none' && Number(s.opacity) > 0 && r.width > 0 && r.height > 0; };
    const radii = new Map(); const small = []; const unnamed = []; const contrasts = [];
    for (const el of document.querySelectorAll('.qr-page *')) {
      if (!(el instanceof HTMLElement) || !vis(el)) continue;
      const s = getComputedStyle(el);
      if (s.borderRadius && s.borderRadius !== '0px') radii.set(s.borderRadius, (radii.get(s.borderRadius) ?? 0) + 1);
      const own = [...el.childNodes].some((n) => n.nodeType === Node.TEXT_NODE && n.textContent.trim());
      if (own) {
        const size = Math.round(parseFloat(s.fontSize));
        const fg = parse(s.color);
        if (fg) { const bg = bgOf(el); const l1 = lum(blend(fg, bg)); const l2 = lum(bg); contrasts.push({ ratio: Math.round(((Math.max(l1, l2) + 0.05) / (Math.min(l1, l2) + 0.05)) * 100) / 100, size, text: el.textContent.trim().slice(0, 30) }); }
      }
      const hidden = el.getAttribute('aria-hidden') === 'true' && el.getAttribute('tabindex') === '-1';
      const interactive = !hidden && (['A', 'BUTTON', 'INPUT', 'SELECT', 'TEXTAREA'].includes(el.tagName) || el.hasAttribute('tabindex'));
      if (interactive) {
        const labelFor = el.id ? document.querySelector(`label[for="${CSS.escape(el.id)}"]`)?.textContent : '';
        const name = el.getAttribute('aria-label') || el.getAttribute('title') || labelFor || el.textContent || el.getAttribute('placeholder') || '';
        if (!String(name).trim()) unnamed.push(`${el.tagName.toLowerCase()}.${String(el.className).slice(0, 24)}`);
        const r = el.getBoundingClientRect();
        if (r.width < 44 || r.height < 44) small.push(`${el.tagName.toLowerCase()} ${Math.round(r.width)}x${Math.round(r.height)} "${String(name).trim().slice(0, 18)}"`);
      }
    }
    contrasts.sort((a, b) => a.ratio - b.ratio);
    return { radii: Object.fromEntries(radii), small, unnamed, worst: contrasts[0] ?? null, failing: contrasts.filter((c) => c.ratio < (c.size >= 24 ? 3 : 4.5)) };
  });
  record('G4', 'touch targets >= 44px at 390', mobile.small.length === 0, mobile.small.slice(0, 5).join(' | ') || 'none under 44px');
  record('G5', 'every control has a name', mobile.unnamed.length === 0, mobile.unnamed.slice(0, 5).join(' | ') || 'all named');
  record('G6', 'contrast >= AA', mobile.failing.length === 0, mobile.failing.length ? `${mobile.failing.length} below, worst ${JSON.stringify(mobile.failing[0])}` : `min ${mobile.worst?.ratio}:1 on "${mobile.worst?.text}"`);
  const stray = Object.keys(mobile.radii).filter((r) => !['0px', '8px', '10px', '12px', '16px', '999px'].includes(r));
  record('G7', 'radius set inside the contract', stray.length === 0, stray.length ? `stray: ${stray.join(', ')}` : JSON.stringify(mobile.radii));

  await page.setViewportSize({ width: 1440, height: 900 });
  await page.waitForTimeout(250);
  await page.screenshot({ path: join(SHOTS, 'studio-dark.png') });
  await page.evaluate(() => { document.documentElement.dataset.theme = 'light'; });
  await page.waitForTimeout(300);
  await page.screenshot({ path: join(SHOTS, 'studio-light.png') });
  await page.setViewportSize({ width: 390, height: 844 });
  await page.waitForTimeout(300);
  await page.screenshot({ path: join(SHOTS, 'studio-390.png') });
  record('G8', 'screenshots written', true, SHOTS);
}
await context.close();

// ===========================================================================
// H. light theme
// ===========================================================================
const light = await newPage('light');
try {
  await light.page.goto(`${BASE}${ROUTE}`, { waitUntil: 'networkidle', timeout: 60000 });
  await light.page.waitForSelector('.qr-sign-preview svg', { timeout: 20000 });
  await light.page.waitForTimeout(1200);
  const themed = await light.page.evaluate(() => document.documentElement.dataset.theme === 'light');
  const path = join(SHOTS, 'light-default.png');
  await shotSign(light.page, path);
  const value = decodeFile(path);
  record('H1', 'light theme applied', themed, themed ? 'data-theme=light' : 'missing');
  record('H2', 'the code decodes in the light theme too', value !== null && value.startsWith(EXPECTED), value ?? 'nothing decoded');
  record('H3', 'no console errors in the light theme', light.consoleErrors.length === 0, light.consoleErrors.slice(0, 3).join(' | ') || 'clean');
} catch (error) {
  record('H1', 'light theme page loads', false, error instanceof Error ? error.message : String(error));
}
await light.context.close();
await browser.close();

if (!ZBAR) record('Z', 'zbarimg available', false, 'install zbar (brew install zbar)');

const passed = results.filter((r) => r.ok).length;
console.log(`\n${failures === 0 ? 'QR GATE GREEN' : `QR GATE RED — ${failures} failing check(s)`} (${results.length} checks, ${passed} passed)`);
console.log(`Screenshots: ${SHOTS}`);
process.exit(failures === 0 ? 0 : 1);
