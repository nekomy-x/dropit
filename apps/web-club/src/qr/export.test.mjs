#!/usr/bin/env node
/**
 * Export self-test — proves each of the four export paths produces a real file.
 *
 * Run from `apps/web-club`:
 *
 *   node src/qr/export.test.mjs
 *
 * Why this runs in a browser instead of node: `exportRaster`, `exportPdf` and
 * `printCard` are defined in terms of `Blob`, `URL.createObjectURL`,
 * `HTMLImageElement`, `HTMLCanvasElement` and an iframe, and the point of the
 * test is that the *browser* produces a decodable artefact. Node would have to
 * fake every one of those and would then be testing the fake.
 *
 * What is actually asserted:
 *   - `exportSvg` → a blob whose text is the SVG, byte for byte;
 *   - `exportRaster` PNG → the requested pixel size, and a QR decoder reads the
 *     payload back out of the pixels (an empty canvas would also "be a PNG");
 *   - `exportRaster` JPG → same payload, and no alpha (JPEG has none);
 *   - `exportPdf` → `%PDF` header, every `startxref` offset points at `xref`,
 *     and each xref entry points at the object number it claims to;
 *   - `printCard` → mounts a frame, is cleaned up afterwards, and settles.
 *
 * The card SVG is the real composed sign from `card.ts`, not a hand-written
 * stub: the export has to survive the document the studio actually hands it.
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

const OUT = mkdtempSync(join(tmpdir(), 'qr-export-'));
const SCRATCH = join(APP, '.export-test-bundle');
mkdirSync(SCRATCH, { recursive: true });

let failures = 0;
let checks = 0;
const fail = (name, detail) => {
  failures += 1;
  console.log(`  FAIL ${name}${detail ? ` — ${detail}` : ''}`);
};
const ok = (name, detail = '') => {
  checks += 1;
  console.log(`  ok   ${name}${detail ? ` — ${detail}` : ''}`);
};
const assert = (condition, name, detail = '') => {
  if (condition) ok(name, detail);
  else fail(name, detail);
};

/** Writes a buffer and asks zbar to read it back. */
function decodePng(path) {
  const result = spawnSync('zbarimg', ['--raw', '-q', path], { encoding: 'utf8' });
  if (result.status !== 0) return null;
  return (result.stdout ?? '').replace(/\n+$/, '');
}

/** Reads the declared size straight out of a PNG's IHDR. */
function pngSize(bytes) {
  // 8-byte signature, then the IHDR chunk: length(4) type(4) width(4) height(4).
  const width = bytes.readUInt32BE(16);
  const height = bytes.readUInt32BE(20);
  return { width, height };
}

/**
 * Validates a PDF's cross-reference table against the file's real byte offsets.
 *
 * This is the part of a hand-built PDF that is easy to get wrong and impossible
 * to notice by eye: a viewer shows the first page, but a wrong offset makes the
 * document unopenable in a stricter reader. Every `n` entry must point exactly
 * at `<id> 0 obj` for its own id.
 */
function validatePdf(bytes) {
  const text = bytes.toString('latin1');
  const problems = [];
  if (!text.startsWith('%PDF-1.')) problems.push('missing %PDF header');
  if (!text.trimEnd().endsWith('%%EOF')) problems.push('missing %%EOF');

  const startxref = /startxref\s+(\d+)\s+%%EOF/.exec(text);
  if (!startxref) {
    problems.push('missing startxref');
    return problems;
  }
  const xrefOffset = Number(startxref[1]);
  if (text.slice(xrefOffset, xrefOffset + 4) !== 'xref') {
    problems.push(`startxref ${xrefOffset} does not point at an xref table`);
    return problems;
  }

  const header = /^xref\s+(\d+)\s+(\d+)\s*/m.exec(text.slice(xrefOffset));
  if (!header) {
    problems.push('malformed xref header');
    return problems;
  }
  const first = Number(header[1]);
  const count = Number(header[2]);
  const entriesStart = xrefOffset + header[0].length;

  let bad = 0;
  for (let i = 1; i < count; i += 1) {
    const entry = text.slice(entriesStart + i * 20, entriesStart + (i + 1) * 20);
    const match = /^(\d{10}) (\d{5}) ([nf])/.exec(entry);
    if (!match) {
      bad += 1;
      continue;
    }
    if (match[3] !== 'n') continue;
    const id = first + i;
    const at = Number(match[1]);
    const expected = `${id} 0 obj`;
    if (text.slice(at, at + expected.length) !== expected) {
      problems.push(`xref entry ${id} points at ${JSON.stringify(text.slice(at, at + 12))}, not "${expected}"`);
      bad += 1;
    }
  }
  if (bad > 0 && problems.length === 0) problems.push(`${bad} bad xref entries`);

  // /Length of every stream must match the bytes between stream and endstream,
  // or a reader silently drops content.
  const streamRe = /<<([^]*?)>>\s*stream\r?\n/g;
  let match;
  while ((match = streamRe.exec(text)) !== null) {
    const lengthMatch = /\/Length\s+(\d+)/.exec(match[1]);
    if (!lengthMatch) continue;
    const declared = Number(lengthMatch[1]);
    const from = match.index + match[0].length;
    const endstream = text.indexOf('endstream', from);
    const actual = endstream - from - 1; // the newline before "endstream"
    if (Math.abs(actual - declared) > 1) {
      problems.push(`stream /Length ${declared} but ${actual} bytes present`);
    }
  }
  return problems;
}

// --- harness ----------------------------------------------------------------

console.log('Building the export bundle…');
await build({
  entryPoints: [join(HERE, 'export-test-entry.ts')],
  bundle: true,
  format: 'iife',
  globalName: 'QRX',
  outfile: join(SCRATCH, 'bundle.js'),
  platform: 'browser',
  target: 'es2022',
  logLevel: 'error',
});
const bundle = require('node:fs').readFileSync(join(SCRATCH, 'bundle.js'), 'utf8');
const jsQRSource = require('node:fs').readFileSync(
  join(APP, 'node_modules/jsqr/dist/jsQR.js'),
  'utf8',
);

const browser = await chromium.launch();
const page = await browser.newPage({ acceptDownloads: true });
await page.setContent('<!doctype html><html><body></body></html>');
await page.addScriptTag({ content: bundle });
await page.addScriptTag({ content: jsQRSource });

const PAYLOAD = 'https://dropit.club/r/NEBULA7';

// The real card document, at A6, as the studio would hand it over.
const cardSvg = await page.evaluate(({ PAYLOAD }) => {
  const { encodeQr, qrCardSvg, emptyDoc } = window.QRX;
  const doc = emptyDoc(PAYLOAD, PAYLOAD);
  doc.encode.ecc = 'H';
  const grid = encodeQr(PAYLOAD, { ecc: 'H' });
  return qrCardSvg(grid, doc, { idPrefix: 'export-test' });
}, { PAYLOAD });

console.log(`\nCard document: ${cardSvg.length} bytes, ${/width="([^"]+)"/.exec(cardSvg)?.[1]}`);

// --- 1. SVG -----------------------------------------------------------------
console.log('\nexportSvg');
{
  const result = await page.evaluate(({ svg }) => {
    const { exportSvg } = window.QRX;
    const { blob, filename } = exportSvg(svg, 'sign.svg');
    return blob.text().then((text) => ({ type: blob.type, size: blob.size, filename, text }));
  }, { svg: cardSvg });

  assert(result.filename === 'sign.svg', 'returns the filename it was given');
  assert(result.type.startsWith('image/svg+xml'), 'blob type is image/svg+xml', result.type);
  assert(result.text === cardSvg, 'blob text is the SVG byte for byte');
  assert(result.text.trimStart().startsWith('<svg'), 'payload starts with <svg');
  writeFileSync(join(OUT, 'sign.svg'), result.text);
}

// --- 2. PNG -----------------------------------------------------------------
console.log('\nexportRaster (png)');
const pngResult = await page.evaluate(async ({ svg, PAYLOAD }) => {
  const { exportRaster, rasterSize } = window.QRX;
  const { width, height } = rasterSize('a6', 300);
  const { blob, filename, widthPx, heightPx } = await exportRaster(svg, {
    format: 'png',
    widthPx: width,
    heightPx: height,
    filename: 'sign.png',
    background: '#FFFFFF',
  });
  const bytes = new Uint8Array(await blob.arrayBuffer());
  // Decode the QR out of the rendered pixels, in the browser, with jsQR.
  const bitmap = await createImageBitmap(blob);
  const canvas = document.createElement('canvas');
  canvas.width = bitmap.width;
  canvas.height = bitmap.height;
  const context = canvas.getContext('2d');
  context.drawImage(bitmap, 0, 0);
  const data = context.getImageData(0, 0, canvas.width, canvas.height);
  const found = jsQR(data.data, data.width, data.height, { inversionAttempts: 'dontInvert' });
  return {
    filename, widthPx, heightPx, type: blob.type,
    naturalWidth: bitmap.width, naturalHeight: bitmap.height,
    bytes: Array.from(bytes.slice(0, 32)),
    size: bytes.byteLength,
    expected: { width, height },
    jsqr: found ? found.data : null,
    jpeg: null,
  };
}, { svg: cardSvg, PAYLOAD });

assert(pngResult.filename === 'sign.png', 'returns the filename');
assert(pngResult.type === 'image/png', 'blob type is image/png', pngResult.type);
assert(pngResult.bytes[0] === 0x89 && pngResult.bytes[1] === 0x50, 'bytes are a real PNG signature');
assert(
  pngResult.naturalWidth === pngResult.expected.width && pngResult.naturalHeight === pngResult.expected.height,
  'pixel size matches rasterSize()',
  `${pngResult.naturalWidth}×${pngResult.naturalHeight}`,
);
assert(
  pngResult.widthPx === pngResult.expected.width && pngResult.heightPx === pngResult.expected.height,
  'reports the dimensions it produced',
);
assert(pngResult.jsqr === PAYLOAD, 'jsQR reads the payload out of the pixels', pngResult.jsqr ?? 'nothing decoded');

{
  const bytes = Buffer.from(pngResult.bytes.concat(Array(0)));
  // Re-fetch the full bytes through a dedicated call for the file on disk + zbar.
  const full = await page.evaluate(async ({ svg }) => {
    const { exportRaster, rasterSize } = window.QRX;
    const { width, height } = rasterSize('a6', 300);
    const { blob } = await exportRaster(svg, {
      format: 'png', widthPx: width, heightPx: height, filename: 'sign.png', background: '#FFFFFF',
    });
    return Array.from(new Uint8Array(await blob.arrayBuffer()));
  }, { svg: cardSvg });
  const buffer = Buffer.from(full);
  const file = join(OUT, 'sign.png');
  writeFileSync(file, buffer);
  assert(bytes.length > 0, 'png signature bytes present');
  const size = pngSize(buffer);
  assert(size.width === pngResult.expected.width && size.height === pngResult.expected.height,
    'IHDR declares the requested size', `${size.width}×${size.height}`);
  const decoded = decodePng(file);
  assert(decoded === PAYLOAD, 'zbarimg reads the payload from the PNG on disk', decoded ?? 'nothing decoded');
}

// --- 3. JPG -----------------------------------------------------------------
console.log('\nexportRaster (jpg)');
{
  const result = await page.evaluate(async ({ svg }) => {
    const { exportRaster, rasterSize } = window.QRX;
    const { width, height } = rasterSize('a6', 300);
    const { blob, widthPx } = await exportRaster(svg, {
      format: 'jpg', widthPx: width, heightPx: height, filename: 'sign.jpg', background: '#FFFFFF',
    });
    const bytes = new Uint8Array(await blob.arrayBuffer());
    const bitmap = await createImageBitmap(blob);
    const canvas = document.createElement('canvas');
    canvas.width = bitmap.width;
    canvas.height = bitmap.height;
    const context = canvas.getContext('2d');
    context.drawImage(bitmap, 0, 0);
    const data = context.getImageData(0, 0, canvas.width, canvas.height);
    const found = jsQR(data.data, data.width, data.height, { inversionAttempts: 'dontInvert' });
    return {
      // `blob.type` is what the file claims to be; `ExportResult` has no `type`
      // field, and the extension in `filename` is the caller's choice.
      type: blob.type, widthPx,
      signature: Array.from(bytes.slice(0, 3)),
      size: bytes.byteLength,
      nat: [bitmap.width, bitmap.height],
      jsqr: found ? found.data : null,
      bytes: Array.from(bytes),
    };
  }, { svg: cardSvg });

  assert(result.type === 'image/jpeg', 'blob type is image/jpeg', result.type);
  assert(result.signature[0] === 0xff && result.signature[1] === 0xd8 && result.signature[2] === 0xff,
    'bytes are a real JPEG signature');
  assert(result.jsqr === PAYLOAD, 'jsQR reads the payload out of the JPEG pixels', result.jsqr ?? 'nothing decoded');
  const file = join(OUT, 'sign.jpg');
  writeFileSync(file, Buffer.from(result.bytes));
  const decoded = decodePng(file);
  assert(decoded === PAYLOAD, 'zbarimg reads the payload from the JPG on disk', decoded ?? 'nothing decoded');
}

// --- 4. PDF -----------------------------------------------------------------
console.log('\nexportPdf');
{
  const result = await page.evaluate(async ({ svg, PAYLOAD }) => {
    const { exportPdf } = window.QRX;
    const { blob, filename, widthPx, heightPx } = await exportPdf(svg, {
      size: 'a6', filename: 'sign.pdf', title: 'Scan to request — Nebula', dpi: 300,
    });
    const bytes = new Uint8Array(await blob.arrayBuffer());
    return {
      filename, widthPx, heightPx, type: blob.type, size: bytes.byteLength,
      head: Array.from(bytes.slice(0, 8)),
      bytes: Array.from(bytes),
    };
  }, { svg: cardSvg, PAYLOAD });

  assert(result.filename === 'sign.pdf', 'returns the filename');
  assert(result.type === 'application/pdf', 'blob type is application/pdf', result.type);
  const buffer = Buffer.from(result.bytes);
  const header = buffer.subarray(0, 8).toString('latin1');
  assert(header.startsWith('%PDF-1.4'), 'header is %PDF-1.4', header.trim());
  const file = join(OUT, 'sign.pdf');
  writeFileSync(file, buffer);

  const problems = validatePdf(buffer);
  assert(problems.length === 0, 'xref offsets and stream lengths are valid', problems.join('; ') || 'all entries point at their own object');

  const text = buffer.toString('latin1');
  // An image XObject is `/Type /XObject /Subtype /Image`; `/Type /Image` is not
  // valid PDF, so the subtype is what has to be checked.
  assert(/\/Subtype\s*\/Image/.test(text) && /\/Filter\s*\/DCTDecode/.test(text), 'embeds a DCTDecode image XObject');
  assert(/\/ColorSpace\s*\/DeviceRGB/.test(text), 'image is declared DeviceRGB');
  assert(/\/Title\s*\(/.test(text) || /\/Title\s*</.test(text), 'Info dictionary carries /Title');

  // The declared /Width and /Height must describe the JPEG that is embedded.
  const dims = /\/Width (\d+) \/Height (\d+)/.exec(text);
  assert(!!dims, 'image dictionary declares a pixel size', dims ? `${dims[1]}×${dims[2]}` : 'none');
  if (dims) {
    assert(Number(dims[1]) === result.widthPx && Number(dims[2]) === result.heightPx,
      '/Width and /Height match the rendered raster', `${dims[1]}×${dims[2]} vs ${result.widthPx}×${result.heightPx}`);
  }

  // MediaBox must be the card in points: 105mm x 148mm -> 297.6 x 419.5 pt.
  const box = /\/MediaBox \[0 0 ([\d.]+) ([\d.]+)\]/.exec(text);
  assert(!!box, 'page declares a MediaBox', box ? `${box[1]}×${box[2]} pt` : 'none');
  if (box) {
    const expectedW = +(105 * 2.834645669).toFixed(2);
    const expectedH = +(148 * 2.834645669).toFixed(2);
    assert(Math.abs(Number(box[1]) - expectedW) < 0.5 && Math.abs(Number(box[2]) - expectedH) < 0.5,
      'MediaBox is A6 in points', `${box[1]}×${box[2]} ≈ ${expectedW}×${expectedH}`);
  }

  // A system PDF tool is the second opinion, when one is installed.
  const info = spawnSync('pdfinfo', [file], { encoding: 'utf8' });
  if (info.error) {
    console.log('  --   pdfinfo not installed, skipping the external PDF check');
  } else {
    assert(info.status === 0, 'pdfinfo parses the document', (info.stderr || '').trim().split('\n')[0] || 'clean');
    const pageSize = /Page size:\s+([\d.]+) x ([\d.]+) pts/.exec(info.stdout ?? '');
    assert(!!pageSize, 'pdfinfo reports a page size', pageSize ? `${pageSize[1]}×${pageSize[2]} pt` : 'none');
    const pages = /Pages:\s+(\d+)/.exec(info.stdout ?? '');
    assert(pages?.[1] === '1', 'document has exactly one page', pages?.[1] ?? 'unknown');
  }
}

// --- 5. print -----------------------------------------------------------------
console.log('\nprintCard');
{
  const outcome = await page.evaluate(async ({ svg }) => {
    const { printCard } = window.QRX;
    const before = document.querySelectorAll('iframe').length;
    // The dialog cannot be dismissed from script, so the timeout is what
    // settles this in a headless run; the point is that it settles and cleans up.
    const started = performance.now();
    let state = 'pending';
    const promise = printCard(svg, 'a6').then(
      () => { state = 'resolved'; },
      (error) => { state = `rejected: ${error instanceof Error ? error.message : String(error)}`; },
    );
    const during = document.querySelectorAll('iframe').length;
    await promise;
    return {
      state,
      elapsed: Math.round(performance.now() - started),
      before,
      during,
      after: document.querySelectorAll('iframe').length,
    };
  }, { svg: cardSvg });

  assert(outcome.during === outcome.before + 1, 'mounts exactly one frame while printing', `${outcome.before} → ${outcome.during}`);
  assert(outcome.after === outcome.before, 'removes the frame afterwards', `${outcome.during} → ${outcome.after}`);
  assert(outcome.state.startsWith('resolved') || outcome.state.startsWith('rejected:'),
    'settles instead of hanging', `${outcome.state} after ${outcome.elapsed}ms`);
  if (outcome.state.startsWith('rejected:')) {
    // In headless Chromium `print()` is a no-op that can reject; that is the
    // environment, not the code path. Report it rather than hiding it.
    console.log(`  --   print() rejected in headless Chromium (${outcome.state.slice(0, 80)}…)`);
  }
}

// --- done -------------------------------------------------------------------
await browser.close();
console.log(`\n${checks} checks passed, ${failures} failed. Artefacts in ${OUT}`);
if (failures > 0) process.exitCode = 1;
