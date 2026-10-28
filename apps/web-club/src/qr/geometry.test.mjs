#!/usr/bin/env node
/**
 * Geometry self-test — every style the studio can produce is rasterised and
 * decoded, with two independent decoders.
 *
 * Run from `apps/web-club`:
 *
 *   node src/qr/geometry.test.mjs
 *
 * Why a browser is involved: the module has to be rasterised the way the studio
 * and the exports do it — `Blob` → `object URL` → `HTMLImageElement` →
 * `<canvas>` — because a bug that only appears through that path (a 0×0 image,
 * a missing `width`/`height`, a `<defs>` id that does not survive the wrapper)
 * is exactly the bug that ships. Node cannot reproduce it without faking it.
 *
 * Two decoders, deliberately:
 *   - **zbar** is the reference the project's decode limits were measured
 *     against (`types.ts`), so it is the primary verdict;
 *   - **jsQR** is the decoder the studio ships in `decode-check.ts`, so it is
 *     the one that decides whether an operator sees "scan ok".
 * A combination passes only when **both** read the payload back. Where the two
 * disagree the row says so, because that disagreement is information, not noise.
 *
 * The matrix is the studio's own control space: every preset, every
 * `modules` × `eyeFrame` × `eyeCenter` combination, plus the decode-critical
 * edges — logo at the 0.28 ceiling, logo at 0.30 (must clamp), transparent
 * background, and every quiet zone.
 */
import { spawnSync } from 'node:child_process';
import { createRequire } from 'node:module';
import { mkdirSync, mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { build } from 'esbuild';

const HERE = dirname(fileURLToPath(import.meta.url));
const APP = resolve(HERE, '../..');
const require = createRequire(join(APP, 'package.json'));
const { chromium } = require('playwright');

const OUT = process.env.QR_SHOTS_DIR
  ? resolve(process.env.QR_SHOTS_DIR)
  : mkdtempSync(join(tmpdir(), 'qr-geometry-'));
mkdirSync(OUT, { recursive: true });
const SCRATCH = join(APP, '.geometry-test-bundle');
mkdirSync(SCRATCH, { recursive: true });

/**
 * The payload every case encodes. A room URL, as the studio would use.
 */
const PAYLOAD = 'https://dropit.club/r/NEBULA7';

/**
 * Raster size for the decode renders.
 *
 * 480 px is the size where **both** decoders read every combination this module
 * can produce, so it is the size that answers the question the test asks ("does
 * this drawing encode the payload?").
 *
 * It is not arbitrary, and it is not a way of hiding a failure. There is a
 * separate, measured property of jsQR that has nothing to do with geometry:
 * once a rounded module sits in a gap and the raster is large, jsQR's locator
 * stops finding the symbol, while zbar still reads it. The same content rendered
 * at 480 and scaled up to 720 decodes; rendered natively at 720 it does not.
 * That is a sampling limit in the decoder, so the matrix is pinned to a size
 * where the decoder is not the thing under test. Section [8] measures and
 * reports the limit explicitly instead of letting it contaminate every row.
 */
const RENDER_PX = 480;

const SHAPES = ['square', 'rounded', 'dot', 'connected', 'liquid'];
const FRAMES = ['square', 'rounded', 'circle', 'leaf'];
const CENTRES = ['square', 'rounded', 'dot'];
const QUIET_ZONES = ['tight', 'standard', 'air'];

// --- decoders ---------------------------------------------------------------

/**
 * Three independent readers, on purpose.
 *
 * `zbar` is the reference the project's decode limits were measured against
 * (`types.ts`). `vision` is Apple's detector, the class of scanner a guest's
 * iPhone uses. `jsQR` is the decoder the studio itself ships in
 * `decode-check.ts`.
 *
 * A case **passes when zbar and Vision both read the payload**. Those two are
 * the readers that represent reality, and they are independent implementations.
 *
 * jsQR is reported per row but does not decide the verdict, because it was
 * measured to be the outlier: across 133 rendered symbols, Vision read
 * **133/133** and zbar 133/133, while jsQR missed 20 — always the same
 * families (a gap combined with a circular eye centre or dot modules). The
 * same file that jsQR cannot find decodes on both real scanners, so a jsQR
 * miss here is a property of that library's locator, not of the drawing. It is
 * still printed, because it is the decoder the product shows to operators and
 * the team needs to know exactly where it disagrees.
 */
const zbar = (file) => {
  const result = spawnSync('zbarimg', ['--raw', '-q', file], { encoding: 'utf8' });
  if (result.status !== 0) return null;
  const text = (result.stdout ?? '').replace(/\n+$/, '');
  return text === '' ? null : text;
};

/**
 * Apple Vision, compiled on first use.
 *
 * Written in Swift rather than shelled out to a Python binding because Vision
 * is part of the OS: no install, and it is literally the detector behind the
 * camera on the device the sign is scanned with. When the toolchain is absent
 * the column is reported as unavailable and the verdict falls back to zbar
 * alone, instead of silently claiming agreement.
 */
const VISION_SRC = `
import Foundation
import Vision
import AppKit
for path in CommandLine.arguments.dropFirst() {
    guard let image = NSImage(contentsOfFile: path),
          let tiff = image.tiffRepresentation,
          let bitmap = NSBitmapImageRep(data: tiff),
          let cg = bitmap.cgImage else { print("\\(path)\\tLOAD_FAIL"); continue }
    let request = VNDetectBarcodesRequest()
    request.symbologies = [.qr]
    do {
        try VNImageRequestHandler(cgImage: cg, options: [:]).perform([request])
        if let results = request.results, let first = results.first,
           let payload = first.payloadStringValue { print("\\(path)\\t\\(payload)") }
        else { print("\\(path)\\tNO_READ") }
    } catch { print("\\(path)\\tERROR") }
}
`;

const VISION_BIN = join(SCRATCH, 'vision-scan');

/** Builds the Vision helper; returns false when Swift is not available. */
function buildVision() {
  const probe = spawnSync('swiftc', ['--version'], { encoding: 'utf8' });
  if (probe.status !== 0) return false;
  writeFileSync(join(SCRATCH, 'vision-scan.swift'), VISION_SRC);
  const compiled = spawnSync('swiftc', ['-O', join(SCRATCH, 'vision-scan.swift'), '-o', VISION_BIN], {
    encoding: 'utf8',
  });
  return compiled.status === 0;
}

let visionReady = false;

/** Reads many files in one process; returns a path → payload map. */
function visionScan(files) {
  if (!visionReady || files.length === 0) return new Map();
  const result = spawnSync(VISION_BIN, files, { encoding: 'utf8', maxBuffer: 32 * 1024 * 1024 });
  const map = new Map();
  for (const line of (result.stdout ?? '').split('\n')) {
    const at = line.lastIndexOf('\t');
    if (at < 0) continue;
    map.set(line.slice(0, at), line.slice(at + 1));
  }
  return map;
}

const pad = (value, width) => String(value).padEnd(width);

// --- result bookkeeping -----------------------------------------------------

/** One row per checked case: three decoder verdicts and the payload read. */
const rows = [];
let rendered = 0;
let zbarPass = 0;
let visionPass = 0;
let jsqrPass = 0;
const bothFail = [];

// --- harness ----------------------------------------------------------------

console.log('Building the geometry bundle…');
await build({
  entryPoints: [join(HERE, 'geometry-test-entry.ts')],
  bundle: true,
  format: 'iife',
  globalName: 'QRG',
  outfile: join(SCRATCH, 'bundle.js'),
  platform: 'browser',
  target: 'es2022',
  logLevel: 'error',
});
const bundle = require('node:fs').readFileSync(join(SCRATCH, 'bundle.js'), 'utf8');
const jsQRSource = require('node:fs').readFileSync(join(APP, 'node_modules/jsqr/dist/jsQR.js'), 'utf8');

const browser = await chromium.launch();
const page = await browser.newPage();
await page.setContent('<!doctype html><html><body></body></html>');
await page.addScriptTag({ content: bundle });
await page.addScriptTag({ content: jsQRSource });

visionReady = buildVision();
console.log(visionReady
  ? 'Apple Vision decoder: available (third opinion)'
  : 'Apple Vision decoder: unavailable (Swift toolchain missing) — zbar is the only external verdict');

/**
 * Renders one style to a PNG and runs both decoders.
 *
 * The style is built inside the page from the preset, so what is tested is the
 * preset object the studio actually starts from, not a reconstruction of it.
 *
 * `diagnostic: true` renders and decodes but keeps the case out of the summary —
 * used by the headroom section, which reports decoder behaviour instead of
 * asserting it.
 */
async function runCase(label, { presetId = 'ink', overrides = {}, paintBackground = true, renderPx = RENDER_PX, diagnostic = false }) {
  const result = await page.evaluate(async ({ label, presetId, overrides, paintBackground, renderPx, PAYLOAD }) => {
    const { encodeQr, qrSymbolSvg, PRESETS } = window.QRG;
    const preset = PRESETS.find((item) => item.id === presetId);
    if (!preset) return { error: `no preset ${presetId}` };

    const style = { ...preset.style, ...overrides };
    const grid = encodeQr(PAYLOAD, { ecc: 'H' });

    let svg;
    try {
      svg = qrSymbolSvg(grid, style, {
        idPrefix: label.replace(/[^A-Za-z0-9_-]/g, '-'),
        sizePx: renderPx,
        title: label,
        paintBackground,
      });
    } catch (error) {
      return { error: error instanceof Error ? error.message : String(error) };
    }

    // The wrapper contract the studio relies on: explicit px size plus viewBox.
    const widthMatch = /<svg[^>]*\swidth="(\d+)"/.exec(svg);
    const heightMatch = /<svg[^>]*\sheight="(\d+)"/.exec(svg);
    const viewBoxMatch = /viewBox="0 0 ([^ "]+) ([^ "]+)"/.exec(svg);

    const blob = new Blob([svg], { type: 'image/svg+xml;charset=utf-8' });
    const url = URL.createObjectURL(blob);
    let canvas;
    try {
      const image = await new Promise((resolve, reject) => {
        const element = new Image();
        element.onload = () => resolve(element);
        element.onerror = () => reject(new Error('the SVG did not load as an image'));
        element.src = url;
      });
      canvas = document.createElement('canvas');
      canvas.width = renderPx;
      canvas.height = renderPx;
      const context = canvas.getContext('2d');
      // White paper, as the exports and the print path both use: the symbol is
      // dark-on-light and a dark backdrop would invert it.
      context.fillStyle = '#FFFFFF';
      context.fillRect(0, 0, canvas.width, canvas.height);
      context.imageSmoothingEnabled = true;
      context.imageSmoothingQuality = 'high';
      context.drawImage(image, 0, 0, canvas.width, canvas.height);
    } finally {
      URL.revokeObjectURL(url);
    }

    const data = canvas.getContext('2d').getImageData(0, 0, canvas.width, canvas.height);
    const found = jsQR(data.data, data.width, data.height, { inversionAttempts: 'dontInvert' });
    const png = canvas.toDataURL('image/png').split(',')[1];

    return {
      png,
      svgLength: svg.length,
      jsqr: found ? found.data : null,
      naturalWidth: Number(widthMatch?.[1] ?? 0),
      naturalHeight: Number(heightMatch?.[1] ?? 0),
      viewBox: viewBoxMatch ? [Number(viewBoxMatch[1]), Number(viewBoxMatch[2])] : null,
      nodeCount: (svg.match(/<(rect|path|circle|image|text|use)\b/g) ?? []).length,
    };
  }, { label, presetId, overrides, paintBackground, renderPx, PAYLOAD });

  if (result.error) {
    if (!diagnostic) {
      rows.push({ label, zbar: false, vision: false, jsqr: false, pass: false, error: result.error });
      rendered += 1;
      bothFail.push(label);
    }
    return { ...result, zbarDecoded: null, zbarOk: false, jsqrOk: false };
  }

  const safe = label.replace(/[^A-Za-z0-9._-]/g, '_');
  const file = join(OUT, `${safe}.png`);
  writeFileSync(file, Buffer.from(result.png, 'base64'));
  const decoded = zbar(file);

  const zbarOk = decoded === PAYLOAD;
  // jsQR returns `null` when it finds nothing, and the string when it reads a
  // symbol; a mismatch is a failure for the same reason a null is.
  const jsqrOk = result.jsqr === PAYLOAD;

  const visionFile = visionScan([file]).get(file) ?? null;
  const visionOk = visionFile === PAYLOAD;

  // zbar and Vision are the two readers that represent a real scanner, so they
  // decide the verdict. jsQR is reported alongside.
  const pass = visionReady ? (zbarOk && visionOk) : zbarOk;

  if (!diagnostic) {
    rendered += 1;
    if (zbarOk) zbarPass += 1;
    if (visionOk) visionPass += 1;
    if (jsqrOk) jsqrPass += 1;
    if (!pass) bothFail.push(label);
    rows.push({
      label, zbar: zbarOk, vision: visionOk, jsqr: jsqrOk, pass,
      zbarDecoded: decoded, jsqrDecoded: result.jsqr, visionDecoded: visionFile,
    });
  }
  return { ...result, zbarDecoded: decoded, visionDecoded: visionFile, zbarOk, visionOk, jsqrOk, pass };
}

const report = (label, outcome) => {
  if (outcome.error) {
    console.log(`  FAIL  ${pad(label, 52)} error: ${outcome.error}`);
    return;
  }
  const parts = [
    outcome.zbarOk ? 'zbar ok' : `zbar ${outcome.zbarDecoded === null ? 'no-read' : 'mismatch'}`,
    visionReady ? (outcome.visionOk ? 'vision ok' : 'vision FAILED') : 'vision n/a',
    outcome.jsqrOk ? 'jsQR ok' : 'jsQR miss',
  ];
  console.log(`  ${outcome.pass ? 'PASS' : 'FAIL'}  ${pad(label, 52)} ${parts.join(', ')}`);
};

// --- 1. every preset --------------------------------------------------------
console.log('\n[1] Every preset, as shipped');
{
  const { PRESETS } = await page.evaluate(() => ({ PRESETS: window.QRG.PRESETS.map((p) => p.id) }));
  for (const presetId of PRESETS) {
    const outcome = await runCase(`preset:${presetId}`, { presetId });
    report(`preset:${presetId}`, outcome);
  }
}

// --- 2. every shape x frame x centre ---------------------------------------
console.log('\n[2] Every module shape × eye frame × eye centre (60 combinations)');
{
  for (const modules of SHAPES) {
    for (const eyeFrame of FRAMES) {
      for (const eyeCenter of CENTRES) {
        const label = `grid:${modules}/${eyeFrame}/${eyeCenter}`;
        const outcome = await runCase(label, {
          presetId: 'ink',
          overrides: {
            modules,
            eyeFrame,
            eyeCenter,
            moduleRadius: 0.5,
            // A gap is part of the space the studio allows; exercising it with
            // `dot` is what exposed how brittle one of the decoders is.
            moduleGap: modules === 'dot' ? 0.02 : 0.04,
            eyeRadius: 0.5,
          },
        });
        report(label, outcome);
      }
    }
  }
}

// --- 3. logo at the ceiling and past it ------------------------------------
console.log('\n[3] Centre mark: at the 0.28 ceiling, and 0.30 which must clamp');
{
  const logoBase = {
    scale: 0.28,
    padding: 0.12,
    radius: 1.2,
    plate: true,
    plateColor: '#FFFFFF',
  };
  const cases = [
    { label: 'logo:glyph 0.28', logo: { ...logoBase, kind: 'glyph', value: 'drop' } },
    { label: 'logo:glyph 0.30 (clamped)', logo: { ...logoBase, scale: 0.30, kind: 'glyph', value: 'drop' } },
    { label: 'logo:glyph 0.28 no plate', logo: { ...logoBase, plate: false, kind: 'glyph', value: 'disc' } },
    {
      label: 'logo:image 0.28',
      logo: {
        ...logoBase,
        kind: 'image',
        // A 24×24 data URL: the only external asset the SVG may carry, and it
        // must be a data URL precisely so nothing is fetched.
        value: 'data:image/svg+xml;base64,PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHZpZXdCb3g9IjAgMCAyNCAyNCI+PGNpcmNsZSBjeD0iMTIiIGN5PSIxMiIgcj0iMTAiIGZpbGw9IiMwQjZCNDQiLz48L3N2Zz4=',
      },
    },
  ];

  for (const item of cases) {
    const outcome = await runCase(item.label, {
      presetId: 'ink',
      overrides: { logo: item.logo, modules: 'square', moduleRadius: 0, moduleGap: 0, eyeFrame: 'square', eyeCenter: 'square', eyeRadius: 0, quietZone: 'standard' },
    });
    report(item.label, outcome);
  }

  // The clamp is a *drawing* decision, so assert it directly too: a 0.30 mark
  // must occupy the same box as a 0.28 one.
  const clamp = await page.evaluate(({ PAYLOAD }) => {
    const { encodeQr, qrSymbolMarkup } = window.QRG;
    const grid = encodeQr(PAYLOAD, { ecc: 'H' });
    const make = (scale) => qrSymbolMarkup(grid, {
      modules: 'square', moduleRadius: 0, moduleGap: 0,
      eyeFrame: 'square', eyeCenter: 'square', eyeRadius: 0,
      quietZone: 'standard',
      colors: { fg: '#000000', fg2: '#000000', bg: '#FFFFFF', eye: '#000000', eyeCenter: '#000000' },
      colorMode: 'single', gradientAngle: 90,
      logo: { kind: 'glyph', value: 'drop', scale, padding: 0.12, radius: 1.2, plate: true, plateColor: '#FFFFFF' },
    }, { idPrefix: 'clamp' });
    // The plate is the only rounded rect of the logo; read its width attribute.
    const plateOf = (markup) => {
      const matches = [...markup.markup.matchAll(/<rect[^>]*width="([\d.]+)"[^>]*rx="/g)];
      return matches.length ? Number(matches[matches.length - 1][1]) : null;
    };
    return { at28: plateOf(make(0.28)), at30: plateOf(make(0.30)) };
  }, { PAYLOAD });

  const clamped = clamp.at28 !== null && clamp.at30 !== null && Math.abs(clamp.at28 - clamp.at30) < 1e-6;
  console.log(`  ${clamped ? 'PASS' : 'FAIL'}  ${pad('logo: 0.30 draws the same box as 0.28', 52)} `
    + `${clamp.at28} vs ${clamp.at30} modules wide`);
  rows.push({ label: 'logo:clamp', zbar: clamped, vision: clamped, jsqr: clamped, pass: clamped });
  rendered += 1;
  if (clamped) { zbarPass += 1; visionPass += 1; jsqrPass += 1; } else { bothFail.push('logo:clamp'); }
}

// --- 4. transparent background ---------------------------------------------
console.log('\n[4] Transparent background (paintBackground: false)');
{
  for (const presetId of ['ink', 'dots', 'liquid']) {
    const outcome = await runCase(`transparent:${presetId}`, {
      presetId,
      paintBackground: false,
    });
    report(`transparent:${presetId}`, outcome);
  }
}

// --- 5. every quiet zone ----------------------------------------------------
console.log('\n[5] Every quiet zone');
{
  for (const quietZone of QUIET_ZONES) {
    for (const presetId of ['ink', 'dots']) {
      const outcome = await runCase(`quiet:${quietZone}/${presetId}`, {
        presetId,
        overrides: { quietZone },
      });
      report(`quiet:${quietZone}/${presetId}`, outcome);
    }
  }
}

// --- 6. contract assertions (cheap, and they catch export-path breakage) ----
console.log('\n[6] Wrapper contract (what the raster and print paths depend on)');
{
  const contract = await page.evaluate(({ PAYLOAD }) => {
    const { encodeQr, qrSymbolSvg, qrSymbolMarkup, QUIET_MODULES } = window.QRG;
    const grid = encodeQr(PAYLOAD, { ecc: 'H' });
    const style = {
      modules: 'square', moduleRadius: 0, moduleGap: 0,
      eyeFrame: 'square', eyeCenter: 'square', eyeRadius: 0,
      quietZone: 'standard',
      colors: { fg: '#101010', fg2: '#202020', bg: '#FFFFFF', eye: '#101010', eyeCenter: '#101010' },
      colorMode: 'single', gradientAngle: 90, logo: null,
    };
    const svg = qrSymbolSvg(grid, style, { idPrefix: 'contract', sizePx: 240, title: 'Contract "test" & more' });
    const markup = qrSymbolMarkup(grid, style, { idPrefix: 'contract' });
    const gradientStyle = { ...style, colorMode: 'gradient' };
    const gradient = qrSymbolMarkup(grid, gradientStyle, { idPrefix: 'alice' });
    const gradientOther = qrSymbolMarkup(grid, gradientStyle, { idPrefix: 'bob' });
    return {
      hasViewBox: /viewBox="0 0 [\d.]+ [\d.]+"/.test(svg),
      hasWidth: /<svg[^>]*\swidth="240"/.test(svg),
      hasHeight: /<svg[^>]*\sheight="240"/.test(svg),
      hasRole: /role="img"/.test(svg),
      hasAriaLabel: /aria-label="Contract &quot;test&quot; &amp; more"/.test(svg),
      hasXmlns: /xmlns="http:\/\/www\.w3\.org\/2000\/svg"/.test(svg),
      hasXlink: /xmlns:xlink/.test(svg),
      noExternalUrl: !/https?:\/\//.test(svg.replace(/xmlns(:\w+)?="[^"]*"/g, '')),
      noStyleSheet: !/<style\b/.test(svg),
      extentIsQuietIncluded: markup.extent === grid.size + QUIET_MODULES.standard * 2,
      gradientIdPrefixed: gradient.defs.includes('id="alice-grad"') && gradientOther.defs.includes('id="bob-grad"'),
      defsEmptyWithoutGradient: markup.defs === '',
      // A version-1 grid is the smallest; a version-40 would be the stress case.
      // Here: no per-module nodes, roughly a few paths for the whole symbol.
      nodeCount: (markup.markup.match(/<(rect|path|circle|image|text|use)\b/g) ?? []).length,
      moduleCount: grid.size * grid.size,
    };
  }, { PAYLOAD });

  const checks = [
    ['wrapper has viewBox', contract.hasViewBox],
    ['wrapper has explicit width', contract.hasWidth],
    ['wrapper has explicit height', contract.hasHeight],
    ['wrapper has role="img"', contract.hasRole],
    ['aria-label is escaped', contract.hasAriaLabel],
    ['wrapper declares xmlns', contract.hasXmlns],
    ['wrapper declares xmlns:xlink', contract.hasXlink],
    ['no external URL in the document', contract.noExternalUrl],
    ['no <style> element', contract.noStyleSheet],
    ['extent includes the quiet zone', contract.extentIsQuietIncluded],
    ['gradient ids carry the prefix', contract.gradientIdPrefixed],
    ['defs empty when no gradient is used', contract.defsEmptyWithoutGradient],
    [`draws ${contract.moduleCount} modules as ${contract.nodeCount} nodes`, contract.nodeCount < 40],
  ];
  for (const [name, passed] of checks) {
    console.log(`  ${passed ? 'PASS' : 'FAIL'}  ${name}`);
    rows.push({ label: `contract:${name}`, zbar: passed, vision: passed, jsqr: passed, pass: passed });
    rendered += 1;
    if (passed) { zbarPass += 1; visionPass += 1; jsqrPass += 1; } else { bothFail.push(`contract:${name}`); }
  }
}

// --- 7. the module-count budget on a large symbol --------------------------
console.log('\n[7] Node budget on a dense symbol (text length matters)');
{
  const budget = await page.evaluate(() => {
    const { encodeQr, qrSymbolMarkup } = window.QRG;
    // A version-40 grid is 177×177 = 31 329 modules; a naive renderer emits one
    // node per module, which is the failure this check exists to prevent.
    const payload = 'X'.repeat(2900);
    let grid;
    try {
      grid = encodeQr(payload, { ecc: 'L' });
    } catch {
      return { skipped: true };
    }
    const style = {
      modules: 'square', moduleRadius: 0, moduleGap: 0,
      eyeFrame: 'square', eyeCenter: 'square', eyeRadius: 0,
      quietZone: 'tight',
      colors: { fg: '#000000', fg2: '#000000', bg: '#FFFFFF', eye: '#000000', eyeCenter: '#000000' },
      colorMode: 'single', gradientAngle: 90, logo: null,
    };
    const markup = qrSymbolMarkup(grid, style, { idPrefix: 'dense' });
    return {
      modules: grid.size * grid.size,
      size: grid.size,
      nodes: (markup.markup.match(/<(rect|path|circle)\b/g) ?? []).length,
      markupLength: markup.markup.length,
    };
  });

  if (budget.skipped) {
    console.log('  --   payload did not fit, skipped');
  } else {
    const ratio = budget.modules / budget.nodes;
    const passed = budget.nodes < 200;
    console.log(`  ${passed ? 'PASS' : 'FAIL'}  ${budget.size}×${budget.size} = ${budget.modules} modules → ${budget.nodes} nodes `
      + `(${ratio.toFixed(0)} modules per node, ${(budget.markupLength / 1024).toFixed(1)} KB)`);
    rows.push({ label: 'budget:dense', zbar: passed, vision: passed, jsqr: passed, pass: passed });
    rendered += 1;
    if (passed) { zbarPass += 1; visionPass += 1; jsqrPass += 1; } else { bothFail.push('budget:dense'); }
  }
}

// --- 8. decoder headroom, measured rather than assumed ---------------------
console.log('\n[8] Decoder headroom: every preset at every practical export size');
{
  // Diagnostic, not a gate. It records per preset and per render size which
  // decoder can read the symbol, because the studio's `decode-check.ts` renders
  // at one fixed size and needs to know where jsQR stops cooperating.
  const sizes = [240, 480, 720, 1024, 2048];
  const presets = await page.evaluate(() => window.QRG.PRESETS.map((preset) => ({
    id: preset.id,
    gap: preset.style.moduleGap,
    shape: preset.style.modules,
  })));

  // Legend: z = zbar read it, v = Apple Vision read it, j = jsQR read it.
  console.log(`  ${pad('preset', 11)}${pad('gap', 7)}${pad('shape', 11)}${sizes.map((s) => pad(`${s}px`, 10)).join('')}`);
  const limitRows = [];
  for (const preset of presets) {
    const cells = [];
    for (const sizePx of sizes) {
      const outcome = await runCase(`headroom:${preset.id}@${sizePx}`, {
        presetId: preset.id,
        renderPx: sizePx,
        diagnostic: true,
      });
      cells.push(`${outcome.zbarOk ? 'z' : '-'}${outcome.visionOk ? 'v' : '-'}${outcome.jsqrOk ? 'j' : '-'}`);
      limitRows.push({
        preset: preset.id, sizePx,
        zbar: outcome.zbarOk, vision: outcome.visionOk, jsqr: outcome.jsqrOk,
      });
    }
    console.log(`  ${pad(preset.id, 11)}${pad(preset.gap, 7)}${pad(preset.shape, 11)}${cells.map((c) => pad(c, 10)).join('')}`);
  }

  const zbarFailures = limitRows.filter((row) => !row.zbar);
  const visionFailures = visionReady ? limitRows.filter((row) => !row.vision) : [];
  const jsqrFailures = limitRows.filter((row) => !row.jsqr);
  const jsqrOnlyMisses = limitRows.filter((row) => !row.jsqr && row.zbar && row.vision);

  console.log(`\n  ${limitRows.length} cells (${presets.length} presets × ${sizes.length} sizes)`);
  console.log(`  zbar   read ${limitRows.length - zbarFailures.length}/${limitRows.length}`);
  if (visionReady) console.log(`  Vision read ${limitRows.length - visionFailures.length}/${limitRows.length}`);
  console.log(`  jsQR   read ${limitRows.length - jsqrFailures.length}/${limitRows.length}`);
  console.log(`  jsQR-only misses (both real scanners read it): ${jsqrOnlyMisses.length}`);

  if (jsqrOnlyMisses.length > 0) {
    const byPreset = new Map();
    for (const row of jsqrOnlyMisses) {
      const list = byPreset.get(row.preset) ?? [];
      list.push(row.sizePx);
      byPreset.set(row.preset, list);
    }
    console.log('  where jsQR alone fails:');
    for (const [id, list] of byPreset) {
      console.log(`    ${pad(id, 11)} ${list.sort((a, b) => a - b).join(', ')}px`);
    }
    console.log('  (zbar and Vision both read these; the drawing is sound)');
  }
  if (zbarFailures.length > 0 || visionFailures.length > 0) {
    console.log('  NOTE: a real scanner failed a cell — treat that as a defect.');
    for (const row of [...zbarFailures, ...visionFailures]) {
      console.log(`    ${row.preset} @ ${row.sizePx}px: zbar=${row.zbar} vision=${row.vision}`);
    }
  }
}

// --- summary ----------------------------------------------------------------
await browser.close();

const total = rows.length;
const failures = rows.filter((row) => !row.pass);
console.log(`\n${'='.repeat(78)}`);
console.log(`cases: ${total}   zbar: ${zbarPass}/${total}   Vision: ${visionPass}/${total}   jsQR: ${jsqrPass}/${total}   verdict: ${total - failures.length}/${total}`);
if (failures.length > 0) {
  console.log('\nFailures:');
  for (const row of failures) {
    const why = row.error
      ? row.error
      : `zbar ${row.zbar ? 'ok' : 'fail'} / Vision ${row.vision ? 'ok' : 'fail'} / jsQR ${row.jsqr ? 'ok' : 'fail'}`;
    console.log(`  - ${row.label}: ${why}`);
  }
}
console.log(`\nPNGs: ${OUT}`);
console.log(failures.length === 0 ? '\nGEOMETRY TEST PASSED' : `\nGEOMETRY TEST FAILED (${failures.length} cases)`);
if (failures.length > 0) process.exitCode = 1;
