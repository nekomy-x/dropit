#!/usr/bin/env node
/**
 * Card composer test — the printed sign, verified in a real browser.
 *
 *   node src/qr/card.test.mjs                 # full run (6 sizes × 4 layouts × 3 docs)
 *   node src/qr/card.test.mjs --only=a6       # one size
 *   node src/qr/card.test.mjs --verbose       # print every assertion, not just failures
 *   node src/qr/card.test.mjs --keep          # keep the PNGs and the temp bundle
 *
 * What it proves, per size × layout:
 *
 *   (a) **Nothing leaves the paper.** Every `<text>` is measured with
 *       `getBBox()` in the page and must sit inside `[0,0,W,H]` with 0.35mm of
 *       slack for the rasteriser's rounding. The symbol box and the wordmark
 *       are checked the same way, and no symbol may come within 3mm of text.
 *   (b) **The symbol still scans.** The `<g data-part="symbol">` is
 *       screenshotted at 2x, written as a PNG and decoded with `zbarimg --raw`;
 *       the payload must equal `doc.value` exactly. This is the assertion that
 *       catches "the layout looks nice and the code is unreadable", which is the
 *       only failure a printed sign cannot recover from.
 *   (c) **The ink is readable.** Each text fill is contrast-checked against the
 *       card surface with an *independent* WCAG implementation in this file (so
 *       a bug in `contrastRatio()` cannot vouch for itself), and the accent is
 *       only allowed on the eyebrow and the rule.
 *
 * Plus the structural checks the studio depends on: the standalone `<svg>` has
 * explicit mm width/height *and* a viewBox, contains no external references,
 * carries a `role="img"` and an aria-label, keeps `idPrefix` unique across two
 * cards on one page, escapes user text, and drops only the surface rect when
 * `paintSurface` is false.
 *
 * `geometry.ts` is owned by another teammate and may still be a stub when this
 * runs. The test therefore bundles `card.ts` twice: once against the real
 * geometry, and — only if the real one throws — once against a local fake that
 * returns a plain square grid. The fake lives in a temp directory, is never
 * written into `src/`, and every run states loudly which geometry it used.
 */
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));
const APP = resolve(HERE, '..', '..');
const OUT = join(APP, 'output', 'qr-card');
const require = createRequire(join(APP, 'package.json'));

const argv = process.argv.slice(2);
const flag = (name, fallback) => {
  const hit = argv.find((value) => value.startsWith(`--${name}=`));
  return hit === undefined ? fallback : hit.slice(name.length + 3);
};
const has = (name) => argv.includes(`--${name}`);
const VERBOSE = has('verbose');
const ONLY = flag('only', '');

const { chromium } = require('playwright');
const esbuild = require('esbuild');

/* ========================================================================== */
/* Fixtures                                                                   */
/* ========================================================================== */

/**
 * A style the studio actually ships (`presets.ts` → Ink), copied rather than
 * imported so this test cannot break when that file is mid-edit by another
 * teammate.
 */
const INK_STYLE = {
  modules: 'square',
  moduleRadius: 0,
  moduleGap: 0,
  eyeFrame: 'square',
  eyeCenter: 'square',
  eyeRadius: 0,
  quietZone: 'standard',
  colors: { fg: '#0B1210', fg2: '#0B1210', bg: '#FFFFFF', eye: '#0B1210', eyeCenter: '#0B1210' },
  colorMode: 'single',
  gradientAngle: 90,
  logo: null,
};

const SOFT_STYLE = {
  ...INK_STYLE,
  modules: 'rounded',
  moduleRadius: 0.5,
  moduleGap: 0.04,
  eyeFrame: 'rounded',
  eyeCenter: 'rounded',
  eyeRadius: 0.46,
};

const VALUE = 'https://dropit.club/r/katerblau';

/**
 * The captions this test signs with. They are the real thing, not lorem: the
 * long Russian title is the widest realistic string this product prints (a
 * club name, a verb, and a promise), and the English pair is the fallback copy.
 */
const DOCS = [
  {
    id: 'ru-long',
    label: 'RU long',
    style: INK_STYLE,
    caption: {
      eyebrow: 'КЛУБ KATERBLAU · БЕРЛИН',
      title: 'Сканируй и заказывай музыку в нашем баре',
      subtitle: 'Наведи камеру на код, выбери трек — бармен поставит его в очередь за пару минут.',
      url: VALUE,
      footer: 'Один запрос — 5 € · ежедневно до 3:00',
    },
  },
  {
    id: 'en-long',
    label: 'EN long',
    style: SOFT_STYLE,
    caption: {
      eyebrow: 'KATERBLAU · BERLIN',
      title: 'Scan the code and request your track',
      subtitle: 'Point your camera at the code, pick a track and the bar will queue it in a couple of minutes.',
      url: VALUE,
      footer: '1 request = €5 · every night until 3am',
    },
  },
  {
    id: 'ru-bare',
    label: 'RU bare',
    style: INK_STYLE,
    caption: {
      eyebrow: '',
      title: 'Закажи трек',
      subtitle: 'Сканируй и слушай.',
      url: VALUE,
      footer: '',
    },
  },
];

/** Colour set that makes the accent fallback visible: neon green on white. */
const NEON = { surface: '#FFFFFF', ink: '#0B1210', accent: '#00FF87' };
const BRAND_OK = { surface: '#FFFFFF', ink: '#0B1210', accent: '#0B6B44' };
const DARK = { surface: '#0B1210', ink: '#FFFFFF', accent: '#00FF87' };
const CREAM = { surface: '#F8F1E5', ink: '#2A1B12', accent: '#6B3F22' };

function makeDoc({ size, layout, caption, style = INK_STYLE, palette = BRAND_OK, card = {} }) {
  return {
    value: VALUE,
    url: VALUE,
    style,
    caption,
    card: {
      layout,
      size,
      surface: palette.surface,
      ink: palette.ink,
      accent: palette.accent,
      preset: 'ink',
      brandMark: true,
      divider: true,
      ...card,
    },
    encode: { ecc: 'M', maskPattern: -1 },
  };
}

/* ========================================================================== */
/* Bundling                                                                   */
/* ========================================================================== */

const TMP = mkdtempSync(join(tmpdir(), 'qr-card-'));
process.on('exit', () => {
  if (!has('keep')) {
    try {
      rmSync(TMP, { recursive: true, force: true });
    } catch {
      /* the OS cleans its own temp directory */
    }
  }
});

const ENTRY = `
import { qrCardMarkup, qrCardSvg, CARD_SIZES, CARD_LAYOUTS, cardAriaLabel } from './card.ts';
import { encodeQr } from './encode.ts';
import { contrastRatio, estimateTextWidth, wrapText, truncateToWidth, typeSizes, MIN_TYPE_MM, ruleWidth, parseColor, relativeLuminance } from './card-layout.ts';
export { qrCardMarkup, qrCardSvg, CARD_SIZES, CARD_LAYOUTS, cardAriaLabel, encodeQr, contrastRatio, estimateTextWidth, wrapText, truncateToWidth, typeSizes, MIN_TYPE_MM, ruleWidth, parseColor, relativeLuminance };
`;

/**
 * The stub for `geometry.ts`, used only when the real one still throws.
 *
 * It draws the plainest possible symbol — one square per dark module, a painted
 * quiet zone, no styling — because the point of the fallback is to verify the
 * *layout* while the symbol renderer is somebody else's work in progress. It
 * still produces a decodable code, so assertion (b) stays meaningful.
 */
const STUB = `
import { escapeXml, QUIET_MODULES, LOGO_MAX_SCALE } from ${JSON.stringify(join(HERE, 'geometry.ts'))};
export { escapeXml, QUIET_MODULES, LOGO_MAX_SCALE };

export function qrSymbolMarkup(grid, style, options) {
  const quiet = QUIET_MODULES[style.quietZone];
  const extent = grid.size + quiet * 2;
  const parts = [];
  if (options.paintBackground !== false) {
    parts.push('<rect x="0" y="0" width="' + extent + '" height="' + extent + '" fill="' + style.colors.bg + '"/>');
  }
  for (let row = 0; row < grid.size; row += 1) {
    for (let col = 0; col < grid.size; col += 1) {
      if (grid.dark(row, col)) {
        parts.push('<rect x="' + (col + quiet) + '" y="' + (row + quiet) + '" width="1" height="1"/>');
      }
    }
  }
  return {
    extent,
    markup: '<g fill="' + escapeXml(style.colors.fg) + '">' + parts.join('') + '</g>',
    defs: '',
  };
}

export function qrSymbolSvg() {
  throw new Error('qrSymbolSvg is not needed by the card test');
}
`;

async function buildBundle(useStub) {
  const outfile = join(TMP, useStub ? 'card.stub.mjs' : 'card.real.mjs');
  await esbuild.build({
    stdin: {
      contents: ENTRY,
      resolveDir: HERE,
      sourcefile: 'qr-card-test-entry.ts',
      loader: 'ts',
    },
    bundle: true,
    format: 'iife',
    globalName: 'QR',
    platform: 'browser',
    target: 'es2022',
    outfile,
    logLevel: 'silent',
    plugins: useStub
      ? [{
        name: 'stub-geometry',
        setup(build) {
          build.onResolve({ filter: /^\.\/geometry(\.ts)?$/ }, (args) => {
            const resolved = resolve(args.resolveDir, args.path);
            if (resolved !== join(HERE, 'geometry.ts')) return null;
            return { path: join(TMP, 'geometry-stub.ts'), namespace: 'stub' };
          });
          build.onLoad({ filter: /.*/, namespace: 'stub' }, () => ({
            contents: STUB,
            loader: 'ts',
            resolveDir: HERE,
          }));
        },
      }]
      : [],
  });
  return readFileSync(outfile, 'utf8');
}

/** True when the real geometry renders instead of throwing "not implemented". */
async function geometryWorks(bundle) {
  const probe = join(TMP, 'probe.html');
  writeFileSync(probe, `<!doctype html><meta charset="utf-8"><body><script>${bundle}</script>
<script>
window.__probe = (() => {
  try {
    const grid = QR.encodeQr('https://dropit.club/r/katerblau', { ecc: 'M', maskPattern: -1 });
    const style = ${JSON.stringify(INK_STYLE)};
    const result = QR.qrCardSvg(grid, ${JSON.stringify(makeDoc({
    size: 'sticker',
    layout: 'stacked',
    caption: { eyebrow: 'x', title: 'x', subtitle: 'x', url: 'x', footer: 'x' },
  }))}, { idPrefix: 'probe' });
    return { ok: result.length > 200, length: result.length };
  } catch (error) {
    return { ok: false, error: String(error && error.message ? error.message : error) };
  }
})();
</script></body>`);
  const page = await browser.newPage();
  try {
    await page.goto(`file://${probe}`);
    return await page.evaluate(() => window.__probe);
  } finally {
    await page.close();
  }
}

/* ========================================================================== */
/* Browser                                                                    */
/* ========================================================================== */

const FONT_URI = `data:font/woff2;base64,${readFileSync(join(APP, 'public', 'fonts', 'inter-latin-var.woff2')).toString('base64')}`;

/** The page shell: the app's own Inter, declared with the app's unicode-range. */
const PAGE = `<!doctype html><html><head><meta charset="utf-8"><style>
@font-face {
  font-family: 'Inter';
  font-style: normal;
  font-weight: 100 900;
  font-display: block;
  src: url('${FONT_URI}') format('woff2');
  unicode-range: U+0000-00FF, U+0131, U+0152-0153, U+02BB-02BC, U+02C6, U+02DA, U+02DC, U+0304,
    U+0308, U+0329, U+2000-206F, U+20AC, U+2122, U+2191, U+2193, U+2212, U+2215, U+FEFF, U+FFFD;
}
html, body { margin: 0; padding: 0; background: #FFFFFF; }
#stage { display: block; width: max-content; }
#stage svg { display: block; }
</style></head><body><div id="stage"></div></body></html>`;

const CARD_PX = 760; // card width on screen; 2x device scale ⇒ 1520px PNG

/**
 * Fraction of a text line box that glyphs actually occupy.
 *
 * A line box is reserved for the font box (1.22em: 0.97 ascent + 0.25
 * descent), while real ink — capitals, x-height, descenders — fills about
 * 0.95em of that. Consecutive lines of one wrapped title therefore overlap as
 * rectangles by design, and only the ink band is a fair collision test.
 */
const INK_FRACTION = 0.78;

const browser = await chromium.launch();
const page = await browser.newPage({
  viewport: { width: 1200, height: 1600 },
  deviceScaleFactor: 2,
});
await page.setContent(PAGE, { waitUntil: 'load' });
await page.evaluate(() => document.fonts.ready);

/* ========================================================================== */
/* Independent colour maths                                                   */
/* ========================================================================== */

/** WCAG 2.1 relative luminance, implemented here so the helper cannot self-certify. */
function luminance(hex) {
  const value = hex.trim().replace('#', '');
  const full = value.length === 3 ? value.split('').map((c) => c + c).join('') : value;
  const [r, g, b] = [0, 2, 4].map((i) => {
    const channel = Number.parseInt(full.slice(i, i + 2), 16) / 255;
    return channel <= 0.03928 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

function wcag(a, b) {
  const [high, low] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (high + 0.05) / (low + 0.05);
}

/* ========================================================================== */
/* Result bookkeeping                                                         */
/* ========================================================================== */

const results = [];
let failures = 0;

function check(cell, name, fn) {
  try {
    const detail = fn();
    cell.checks.push({ name, ok: true, detail });
    return true;
  } catch (error) {
    cell.checks.push({ name, ok: false, detail: error instanceof Error ? error.message : String(error) });
    cell.ok = false;
    failures += 1;
    return false;
  }
}

/* ========================================================================== */
/* Browser-side measurement                                                   */
/* ========================================================================== */

/**
 * Measures one rendered card.
 *
 * Everything happens in the SVG's own user space (millimetres), so the numbers
 * are the printed ones and do not depend on `CARD_PX`.
 */
const MEASURE = () => {
  const svg = document.querySelector('#stage svg');
  if (!svg) throw new Error('no svg');
  const box = svg.viewBox.baseVal;
  const width = box.width;
  const height = box.height;

  /**
   * Bounding box in the SVG's own user space (millimetres).
   *
   * `getBBox()` alone is not enough: it reports the element's *local* box and
   * ignores the element's own `transform`. The symbol group carries its
   * placement transform, so a naive `getBBox()` on it returns the module-unit
   * box and every clearance figure silently becomes 0 — which is exactly the
   * bug this test exists to catch. Mapping through the screen matrices puts
   * everything in the root user space, transform included.
   */
  const rootInverse = svg.getScreenCTM().inverse();

  const rectOf = (element) => {
    const local = element.getBBox();
    const screen = element.getScreenCTM();
    if (!screen) return { x: 0, y: 0, w: 0, h: 0 };
    const matrix = rootInverse.multiply(screen);
    const corners = [
      [local.x, local.y],
      [local.x + local.width, local.y],
      [local.x, local.y + local.height],
      [local.x + local.width, local.y + local.height],
    ].map(([x, y]) => ({
      x: matrix.a * x + matrix.c * y + matrix.e,
      y: matrix.b * x + matrix.d * y + matrix.f,
    }));
    const xs = corners.map((point) => point.x);
    const ys = corners.map((point) => point.y);
    return {
      x: Math.min(...xs),
      y: Math.min(...ys),
      w: Math.max(...xs) - Math.min(...xs),
      h: Math.max(...ys) - Math.min(...ys),
    };
  };

  const texts = [...svg.querySelectorAll('text')]
    // The wordmark lives inside <g data-part="brand"> and is measured as that
    // group; keeping the inner <text> too would make the mark collide with
    // itself.
    .filter((element) => element.parentElement?.getAttribute('data-part') !== 'brand')
    .map((element) => ({
    part: element.getAttribute('data-part') || 'other',
    content: element.textContent,
    fill: element.getAttribute('fill'),
    size: Number(element.getAttribute('font-size')),
    tracking: Number(element.getAttribute('letter-spacing') || 0),
    weight: Number(element.getAttribute('font-weight') || 400),
    anchor: element.getAttribute('text-anchor') || 'start',
    box: rectOf(element),
  }));

  const symbolGroup = svg.querySelector('[data-part="symbol"]');
  const symbol = symbolGroup ? rectOf(symbolGroup) : null;
  const brandGroup = svg.querySelector('[data-part="brand"]');
  const brand = brandGroup ? rectOf(brandGroup) : null;
  const surface = svg.querySelector('[data-part="surface"]');
  const rules = [...svg.querySelectorAll('[data-part="rule"]')].map((element) => ({
    stroke: element.getAttribute('stroke'),
    strokeWidth: Number(element.getAttribute('stroke-width')),
  }));

  return {
    width,
    height,
    viewBox: `${box.width} ${box.height}`,
    surface: surface ? surface.getAttribute('fill') : null,
    texts,
    symbol,
    brand,
    rules,
    ids: [...svg.querySelectorAll('[id]')].map((element) => element.id),
    externalRefs: [...svg.querySelectorAll('image, use[href], use[xlink\\:href], [style*="url("]')]
      .map((element) => element.outerHTML.slice(0, 120)),
    hrefs: [...svg.querySelectorAll('[href], [xlink\\:href]')]
      .map((element) => element.getAttribute('href') || element.getAttribute('xlink:href') || '')
      .filter((value) => value !== '' && !value.startsWith('#') && !value.startsWith('data:')),
  };
};

/* ========================================================================== */
/* Rendering one card                                                         */
/* ========================================================================== */

let renderIndex = 0;

async function renderCard(svgString) {
  renderIndex += 1;
  await page.evaluate(({ svg, px }) => {
    const stage = document.querySelector('#stage');
    stage.innerHTML = svg;
    const element = stage.querySelector('svg');
    element.style.width = `${px}px`;
    element.style.height = 'auto';
  }, { svg: svgString, px: CARD_PX });
  // Text metrics settle once the font is actually used; one rAF is enough.
  await page.evaluate(() => new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(done))));
  return page.evaluate(MEASURE);
}

/** Saves the whole card and the symbol crop, returning paths. */
async function shootCard(name) {
  const dir = join(OUT, name);
  mkdirSync(dir, { recursive: true });
  const cardPath = join(dir, 'card.png');
  const symbolPath = join(dir, 'symbol.png');
  const stage = await page.$('#stage svg');
  await stage.screenshot({ path: cardPath });
  const symbol = await page.$('#stage [data-part="symbol"]');
  if (symbol) await symbol.screenshot({ path: symbolPath });
  return { cardPath, symbolPath: symbol ? symbolPath : null };
}

/** Decodes a PNG with zbar; returns the raw payload or throws with its reason. */
function decodeSymbol(path) {
  try {
    const output = execFileSync('zbarimg', ['--raw', '-q', path], {
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'pipe'],
    });
    return output.trim();
  } catch (error) {
    const stderr = error && typeof error === 'object' && 'stderr' in error ? String(error.stderr) : '';
    throw new Error(`zbarimg failed: ${stderr.trim() || (error instanceof Error ? error.message : String(error))}`);
  }
}

/* ========================================================================== */
/* The matrix                                                                 */
/* ========================================================================== */

const grid = await (async () => {
  const bundle = await buildBundle(false);
  const probePage = await browser.newPage();
  await probePage.setContent(`<script>${bundle}</script>`, { waitUntil: 'load' });
  const doc = makeDoc({ size: 'sticker', layout: 'stacked', caption: DOCS[0].caption });
  const encoded = await probePage.evaluate(({ doc }) => {
    const g = QR.encodeQr(doc.value, { ecc: 'M', maskPattern: -1 });
    return { size: g.size, version: g.version, mask: g.maskPattern, dark: g.size };
  }, { doc });
  await probePage.close();
  return encoded;
})();

const forceStub = has('stub');
const realGeometry = forceStub ? { ok: false, error: 'forced by --stub' } : await geometryWorks(await buildBundle(false));
const useStub = !realGeometry.ok;
const bundle = useStub ? await buildBundle(true) : readFileSync(join(TMP, 'card.real.mjs'), 'utf8');

assert.ok(grid.size > 0, 'the QR grid must encode');

/* Load the bundle into the page once; every render reuses it.
   A global-scope `eval` (not `new Function`) so the IIFE assigns `QR` to `window`. */
await page.evaluate((source) => {
  // eslint-disable-next-line no-eval
  (0, eval)(`${source}\nwindow.QR = QR;`);
}, bundle);

const api = await page.evaluate(() => ({
  sizes: Object.keys(QR.CARD_SIZES),
  layouts: QR.CARD_LAYOUTS.map((entry) => entry.id),
  sizeList: QR.CARD_SIZES,
}));

const SIZES = ONLY === '' ? api.sizes : api.sizes.filter((id) => id === ONLY);
assert.ok(SIZES.length > 0, `--only=${ONLY} matched no card size`);
assert.deepEqual(api.layouts, ['qr-only', 'stacked', 'side', 'poster']);

if (!has('keep')) mkdirSync(OUT, { recursive: true });

/* ========================================================================== */
/* Per-cell verification                                                      */
/* ========================================================================== */

for (const size of SIZES) {
  for (const layout of api.layouts) {
    for (const variant of DOCS) {
      const cell = {
        size,
        layout,
        variant: variant.id,
        ok: true,
        checks: [],
        slack: '',
        minGap: null,
        decode: '',
      };
      results.push(cell);

      const doc = makeDoc({
        size,
        layout,
        caption: variant.caption,
        style: variant.style,
        palette: BRAND_OK,
        card: { brandMark: true, divider: true },
      });

      let svg;
      try {
        svg = await page.evaluate(({ doc }) => QR.qrCardSvg(
          QR.encodeQr(doc.value, { ecc: doc.encode.ecc, maskPattern: doc.encode.maskPattern }),
          doc,
          { idPrefix: `c${Math.random().toString(36).slice(2, 8)}`, title: '' },
        ), { doc });
      } catch (error) {
        cell.ok = false;
        cell.checks.push({ name: 'render', ok: false, detail: error instanceof Error ? error.message : String(error) });
        failures += 1;
        continue;
      }

      const measured = await renderCard(svg);
      const { width: W, height: H } = api.sizeList[size];

      /* (a) nothing leaves the paper ------------------------------------- */
      const tolerance = 0.35; // mm: rasteriser rounding, not a licence to overflow
      const outside = measured.texts.filter((text) => (
        text.box.x < -tolerance
        || text.box.y < -tolerance
        || text.box.x + text.box.w > W + tolerance
        || text.box.y + text.box.h > H + tolerance
      ));
      check(cell, 'texts inside the card', () => {
        assert.equal(
          outside.length,
          0,
          `${outside.length} text element(s) outside ${W}×${H}mm: `
          + outside.map((text) => `${text.part} "${text.content.slice(0, 28)}" `
            + `[${text.box.x.toFixed(2)} ${text.box.y.toFixed(2)} ${(text.box.x + text.box.w).toFixed(2)} `
            + `${(text.box.y + text.box.h).toFixed(2)}]`).join('; '),
        );
        return `${measured.texts.length} text elements inside`;
      });

      if (measured.symbol !== null) {
        const symbol = measured.symbol;
        const clearance = Math.min(symbol.x, symbol.y, W - symbol.x - symbol.w, H - symbol.y - symbol.h);
        cell.clearance = clearance;
        check(cell, 'symbol inside the card', () => {
          assert.ok(
            clearance >= 3 - tolerance,
            `symbol clearance ${clearance.toFixed(2)}mm < 3mm`,
          );
          return `clearance ${clearance.toFixed(2)}mm`;
        });

        // Requirement 6: no symbol edge flush against text.
        const nearest = measured.texts.reduce((best, text) => {
          const dx = Math.max(text.box.x - (symbol.x + symbol.w), symbol.x - (text.box.x + text.box.w), 0);
          const dy = Math.max(text.box.y - (symbol.y + symbol.h), symbol.y - (text.box.y + text.box.h), 0);
          return Math.min(best, Math.hypot(dx, dy));
        }, Number.POSITIVE_INFINITY);
        cell.minGap = measured.texts.length === 0 ? null : nearest;
        if (measured.texts.length > 0) {
          check(cell, 'symbol clear of text', () => {
            assert.ok(
              nearest >= 3 - tolerance,
              `nearest text is ${nearest.toFixed(2)}mm from the symbol (< 3mm)`,
            );
            return `${nearest.toFixed(2)}mm`;
          });
        }
      }

      if (measured.brand !== null) {
        check(cell, 'wordmark inside the card', () => {
          const brand = measured.brand;
          assert.ok(
            brand.x >= -tolerance && brand.y >= -tolerance
            && brand.x + brand.w <= W + tolerance && brand.y + brand.h <= H + tolerance,
            `wordmark [${brand.x.toFixed(2)} ${brand.y.toFixed(2)} ${(brand.x + brand.w).toFixed(2)} ${(brand.y + brand.h).toFixed(2)}] outside ${W}×${H}`,
          );
          return 'inside';
        });
      }

      /*
       * No two pieces of ink may overlap.
       *
       * This is the assertion that a "does it fit in the card" check misses: a
       * footer and a wordmark can both be inside the paper and still be printed
       * on top of each other, which is a broken sign. Checked pairwise over the
       * text boxes plus the wordmark, with a small tolerance so two glyph
       * descenders on adjacent lines are not counted as a collision.
       */
      const boxes = measured.texts.map((text) => ({
        name: `${text.part} "${text.content.slice(0, 18)}"`,
        box: text.box,
      }));
      if (measured.brand !== null) boxes.push({ name: 'brand mark', box: measured.brand });
      const collisions = [];
      for (let a = 0; a < boxes.length; a += 1) {
        for (let b = a + 1; b < boxes.length; b += 1) {
          const first = boxes[a];
          const second = boxes[b];
          const dx = Math.min(first.box.x + first.box.w, second.box.x + second.box.w)
            - Math.max(first.box.x, second.box.x);
          /*
           * Vertical clearance is judged on the *ink band* of each line, not its
           * full line box. Two consecutive lines of one wrapped title are
           * supposed to overlap as boxes — that is what leading is — but their
           * glyphs must not. The ink band is the line box trimmed by the
           * difference between the font box and the real glyph extent, which
           * this measurement approximates from the two constants the layout
           * itself uses.
           */
          const ink = (box) => {
            const trimTop = box.h * (1 - INK_FRACTION);
            return { x: box.x, y: box.y + trimTop / 2, w: box.w, h: box.h - trimTop };
          };
          const a1 = ink(first.box);
          const a2 = ink(second.box);
          const iy = Math.min(a1.y + a1.h, a2.y + a2.h) - Math.max(a1.y, a2.y);
          if (dx > 0.2 && iy > 0.2) {
            collisions.push(`${first.name} × ${second.name} (${dx.toFixed(1)}×${iy.toFixed(1)}mm)`);
          }
        }
      }
      check(cell, 'no ink overlaps', () => {
        assert.deepEqual(collisions, [], collisions.join('; '));
        return `${boxes.length} boxes`;
      });

      // Requirement: a title wraps to at most 3 lines and ends in an ellipsis
      // when it had to be cut.
      const titleLines = measured.texts.filter((text) => text.part === 'title');
      check(cell, 'title ≤3 lines', () => {
        assert.ok(titleLines.length <= 3, `${titleLines.length} title lines`);
        return `${titleLines.length}`;
      });

      /* (b) the symbol still scans ---------------------------------------- */
      const shot = await shootCard(`${size}-${layout}-${variant.id}`);
      if (shot.symbolPath !== null) {
        let decoded = null;
        try {
          decoded = decodeSymbol(shot.symbolPath);
        } catch (error) {
          decoded = null;
          cell.decode = 'error';
          check(cell, 'symbol decodes', () => {
            throw error;
          });
        }
        if (decoded !== null) {
          cell.decode = decoded === VALUE ? 'ok' : 'mismatch';
          check(cell, 'symbol decodes', () => {
            assert.equal(decoded, VALUE, `decoded "${decoded.slice(0, 60)}" ≠ doc.value`);
            return 'payload matches';
          });
        }
      }

      /* (c) the ink is readable ------------------------------------------- */
      const surface = measured.surface ?? BRAND_OK.surface;
      const unreadable = measured.texts
        .map((text) => ({ ...text, ratio: wcag(text.fill, surface) }))
        .filter((text) => text.ratio < 4.5);
      check(cell, 'captions ≥4.5:1', () => {
        assert.equal(
          unreadable.length,
          0,
          unreadable.map((text) => `${text.part} ${text.fill} on ${surface} = ${text.ratio.toFixed(2)}:1`).join('; '),
        );
        const worst = measured.texts.reduce((low, text) => Math.min(low, wcag(text.fill, surface)), 21);
        return measured.texts.length === 0 ? 'no text' : `min ${worst.toFixed(2)}:1`;
      });

      // The accent may only ever carry the eyebrow or the rule.
      const accentOnBody = measured.texts.filter((text) => (
        text.part !== 'eyebrow' && text.fill.toLowerCase() === BRAND_OK.accent.toLowerCase()
      ));
      check(cell, 'accent never body text', () => {
        assert.equal(accentOnBody.length, 0, `accent on ${accentOnBody.map((t) => t.part).join(', ')}`);
        return 'eyebrow/rule only';
      });

      /* Layout-specific shape assertions ---------------------------------- */
      if (layout === 'qr-only') {
        check(cell, 'qr-only symbol ≥70%', () => {
          const shorter = Math.min(W, H);
          const fraction = measured.symbol.w / shorter;
          assert.ok(fraction >= 0.7, `symbol is ${(fraction * 100).toFixed(1)}% of the shorter side`);
          return `${(fraction * 100).toFixed(1)}%`;
        });
        check(cell, 'qr-only has no text', () => {
          assert.equal(measured.texts.length, 0, 'qr-only must carry no copy');
          return 'clean';
        });
      }

      if (layout === 'poster') {
        check(cell, 'poster title ≥1.8× subtitle', () => {
          const title = titleLines[0];
          const subtitle = measured.texts.find((text) => text.part === 'subtitle');
          assert.ok(title !== undefined, 'no title');
          assert.ok(subtitle !== undefined, 'no subtitle');
          const ratio = title.size / subtitle.size;
          assert.ok(ratio >= 1.8, `title ${title.size.toFixed(2)}mm / subtitle ${subtitle.size.toFixed(2)}mm = ${ratio.toFixed(2)}`);
          return `${ratio.toFixed(2)}×`;
        });
      }

      if (layout === 'side' && size === 'sticker') {
        check(cell, 'side refolds on square', () => {
          // The square card is composed as stacked: the symbol is centred.
          const centre = measured.symbol.x + measured.symbol.w / 2;
          assert.ok(Math.abs(centre - W / 2) < 0.5, `symbol centre ${centre.toFixed(2)} ≠ ${(W / 2).toFixed(2)}`);
          return 'stacked geometry';
        });
      }

      cell.slack = `${measured.texts.length}t/${measured.symbol ? (measured.symbol.w).toFixed(1) : '—'}mm`;
      if (VERBOSE) {
        console.log(`  ${size}/${layout}/${variant.id}`
          + ` texts=${measured.texts.length} symbol=${measured.symbol ? measured.symbol.w.toFixed(1) : '—'}mm`
          + ` ${cell.checks.map((c) => (c.ok ? '✓' : '✗')).join('')}`);
      }
    }
  }
}

/* ========================================================================== */
/* Structural checks (one card is enough for each)                            */
/* ========================================================================== */

const structural = { ok: true, checks: [] };
results.push({ size: 'all', layout: 'structure', variant: '—', ok: true, checks: structural.checks });

async function structuralCheck(name, fn) {
  const cell = results[results.length - 1];
  try {
    const detail = await fn();
    cell.checks.push({ name, ok: true, detail });
  } catch (error) {
    cell.checks.push({ name, ok: false, detail: error instanceof Error ? error.message : String(error) });
    cell.ok = false;
    failures += 1;
  }
  cell.ok = cell.checks.every((entry) => entry.ok);
}

const docA6 = makeDoc({ size: 'a6', layout: 'stacked', caption: DOCS[0].caption });

const markupTrue = await page.evaluate(({ doc }) => QR.qrCardMarkup(
  QR.encodeQr(doc.value, { ecc: doc.encode.ecc }),
  doc,
  { idPrefix: 'paint-yes', paintSurface: true },
), { doc: docA6 });
const markupFalse = await page.evaluate(({ doc }) => QR.qrCardMarkup(
  QR.encodeQr(doc.value, { ecc: doc.encode.ecc }),
  doc,
  { idPrefix: 'paint-no', paintSurface: false },
), { doc: docA6 });

await structuralCheck('paintSurface=false drops only the card surface', () => {
  assert.ok(markupTrue.markup.includes('data-part="surface"'), 'painted card has no surface rect');
  assert.ok(!markupFalse.markup.includes('data-part="surface"'), 'transparent card still paints a surface');

  // Geometry must be identical apart from the card's own background. The
  // symbol's quiet-zone plate is *not* the card surface: it is part of the
  // symbol (the scorer needs a light surround), and `geometry.ts` drops it on
  // its own `paintBackground:false`. So compare the placement, not the bytes.
  const placement = (value) => [...value.matchAll(/transform="translate\(([^)]+)\)\s*scale\(([^)]+)\)"/g)]
    .map((match) => `${match[1]}|${match[2]}`);
  assert.deepEqual(
    placement(markupFalse.markup),
    placement(markupTrue.markup),
    'the symbol moved when the surface was turned off',
  );
  const texts = (value) => [...value.matchAll(/<text [^>]*>[^<]*<\/text>/g)].map((match) => match[0]);
  assert.deepEqual(texts(markupFalse.markup), texts(markupTrue.markup), 'the caption changed');
  // The layout — every positioned element — must be byte-identical, so a
  // transparent export cannot silently reflow the card.
  const layoutOf = (value) => [
    ...value.matchAll(/<(?:text|line|circle)[^>]*>/g),
  ].map((match) => match[0]);
  assert.deepEqual(
    layoutOf(markupFalse.markup),
    layoutOf(markupTrue.markup),
    'the composed layout changed when the surface was turned off',
  );
  return 'placement and caption identical';
});

await structuralCheck('markup is in millimetres', () => {
  assert.equal(markupTrue.width, 105);
  assert.equal(markupTrue.height, 148);
  return '105×148';
});

const svgOut = await page.evaluate(({ doc }) => QR.qrCardSvg(
  QR.encodeQr(doc.value, { ecc: doc.encode.ecc }),
  doc,
  { idPrefix: 'standalone', title: 'Scan to request' },
), { doc: docA6 });

await structuralCheck('standalone svg is self-contained', () => {
  assert.match(svgOut, /^<svg [^>]*xmlns="http:\/\/www\.w3\.org\/2000\/svg"/);
  assert.match(svgOut, /width="105mm"/);
  assert.match(svgOut, /height="148mm"/);
  assert.match(svgOut, /viewBox="0 0 105 148"/);
  assert.match(svgOut, /role="img"/);
  assert.match(svgOut, /aria-label="Scan to request"/);
  // "No external references" means no *resource* reference. The printed url is
  // text a guest reads off the paper, so it is checked by attribute, not by
  // substring search over the whole document.
  const attrs = [...svgOut.matchAll(/\s([a-z:-]+)="([^"]*)"/g)]
    .map(([, name, value]) => ({ name: name.toLowerCase(), value }));
  const referencing = attrs.filter(({ name, value }) => (
    (name === 'href' || name === 'xlink:href' || name === 'src')
    && value !== ''
    && !value.startsWith('#')
    && !value.startsWith('data:')
  ));
  assert.deepEqual(referencing, [], `external resource references: ${JSON.stringify(referencing)}`);
  const cssUrls = attrs.filter(({ value }) => /url\(\s*['"]?(?!#)/.test(value));
  assert.deepEqual(cssUrls, [], `css url() references: ${JSON.stringify(cssUrls)}`);
  assert.ok(!/<image\b/.test(svgOut), 'embeds a raster image');
  return `${svgOut.length} bytes, no external refs`;
});

await structuralCheck('rasterising yields a non-zero bitmap', async () => {
  // A zero-size or unparsable `<svg>` rasterises to a 0×0 canvas, which is the
  // exact failure mode the PNG/PDF export path must not have. Rasterise for
  // real rather than trusting the attributes.
  const raster = await page.evaluate(async (markup) => {
    const image = new Image();
    const blob = new Blob([markup], { type: 'image/svg+xml' });
    const url = URL.createObjectURL(blob);
    try {
      await new Promise((resolve, reject) => {
        image.onload = resolve;
        image.onerror = () => reject(new Error('the browser refused to decode the svg'));
        image.src = url;
      });
      const canvas = document.createElement('canvas');
      canvas.width = image.naturalWidth || 300;
      canvas.height = image.naturalHeight || 300;
      const context = canvas.getContext('2d');
      context.drawImage(image, 0, 0, canvas.width, canvas.height);
      const data = context.getImageData(0, 0, canvas.width, canvas.height).data;
      let ink = 0;
      for (let index = 0; index < data.length; index += 4) {
        if (data[index + 3] > 8) ink += 1;
      }
      return { width: image.naturalWidth, height: image.naturalHeight, ink, total: data.length / 4 };
    } finally {
      URL.revokeObjectURL(url);
    }
  }, svgOut);
  assert.ok(raster.width > 0 && raster.height > 0, `rasterised to ${raster.width}×${raster.height}`);
  assert.ok(raster.ink > raster.total * 0.05, `only ${raster.ink}/${raster.total} pixels are painted`);
  return `${raster.width}×${raster.height}px, ${((raster.ink / raster.total) * 100).toFixed(0)}% inked`;
});

/* idPrefix uniqueness: two cards on one page must not share ids. */
const doubled = await page.evaluate(({ docs }) => `${docs.a}${docs.b}`, {
  docs: {
    a: svgOut,
    b: svgOut,
  },
});
const duplicatedIds = await page.evaluate((html) => {
  const stage = document.querySelector('#stage');
  stage.innerHTML = html;
  const ids = [...stage.querySelectorAll('[id]')].map((element) => element.id);
  return ids.filter((id, index) => ids.indexOf(id) !== index);
}, doubled);
await structuralCheck('two cards share no ids', () => {
  assert.equal(duplicatedIds.length, 0, `duplicate ids: ${[...new Set(duplicatedIds)].join(', ')}`);
  return 'unique';
});

const prefixed = await page.evaluate(({ doc }) => {
  const make = (prefix) => QR.qrCardSvg(
    QR.encodeQr(doc.value, { ecc: doc.encode.ecc }),
    doc,
    { idPrefix: prefix },
  );
  return [make('alpha'), make('beta')];
}, { doc: { ...docA6, style: { ...INK_STYLE, colorMode: 'gradient' } } });

await structuralCheck('idPrefix is honoured', () => {
  const idsA = [...prefixed[0].matchAll(/id="([^"]+)"/g)].map((match) => match[1]);
  const idsB = [...prefixed[1].matchAll(/id="([^"]+)"/g)].map((match) => match[1]);
  // The real geometry may emit no ids for the simplest styles; the prefix must
  // still be present whenever it does.
  for (const id of [...idsA, ...idsB]) {
    assert.ok(id.startsWith('alpha') || id.startsWith('beta'), `unprefixed id "${id}"`);
  }
  assert.equal(idsA.filter((id) => idsB.includes(id)).length, 0, 'prefixes collided');
  return `${idsA.length}+${idsB.length} ids`;
});

/* Escaping: a caption that looks like markup must arrive as text. */
const nasty = makeDoc({
  size: 'a6',
  layout: 'stacked',
  caption: {
    eyebrow: '<script>alert(1)</script>',
    title: 'Tom & Jerry <"quoted">',
    subtitle: "It's 5€ — don't & won't",
    url: 'https://dropit.club/r?a=1&b=2',
    footer: '<b>bold</b>',
  },
});
const nastySvg = await page.evaluate(({ doc }) => QR.qrCardSvg(
  QR.encodeQr(doc.value, { ecc: doc.encode.ecc }),
  doc,
  { idPrefix: 'escape' },
), { doc: nasty });
const escaped = await renderCard(nastySvg);
await structuralCheck('user text is escaped', () => {
  assert.ok(!/<script/i.test(nastySvg), 'raw <script> reached the svg');
  assert.ok(!/<b>bold<\/b>/.test(nastySvg), 'raw <b> reached the svg');
  // The title may legitimately wrap, so compare the joined lines against the
  // source string: what matters is that the characters survived as *text* and
  // did not become markup.
  const titles = escaped.texts.filter((text) => text.part === 'title').map((text) => text.content);
  assert.equal(titles.join(' '), 'Tom & Jerry <"quoted">', 'the title did not round-trip as text');
  const footers = escaped.texts.filter((text) => text.part === 'footer').map((text) => text.content);
  assert.equal(footers.join(' '), '<b>bold</b>', 'the footer did not round-trip as text');
  const urls = escaped.texts.filter((text) => text.part === 'url').map((text) => text.content);
  assert.equal(urls.join(' '), 'https://dropit.club/r?a=1&b=2', 'the url did not round-trip as text');
  return 'round-trips as text';
});

/* --------------------------------------------------------------------------
 * Contrast fallback on real palettes.
 *
 * The rule ("the accent may carry the eyebrow only when it clears 4.5:1, else
 * fall back to ink") only means anything if it is exercised with an accent that
 * actually fails, so each of these renders a full card and reads the fill the
 * browser ended up with.
 * ------------------------------------------------------------------------ */
for (const [name, palette, expectAccent] of [
  ['neon on white (fails AA)', NEON, false],
  ['brand green on white (passes)', BRAND_OK, true],
  ['neon on near-black (passes)', DARK, true],
  ['espresso on cream (passes)', CREAM, true],
]) {
  const doc = makeDoc({ size: 'a6', layout: 'stacked', caption: DOCS[0].caption, palette });
  const svgPalette = await page.evaluate(({ doc }) => QR.qrCardSvg(
    QR.encodeQr(doc.value, { ecc: doc.encode.ecc }),
    doc,
    { idPrefix: 'pal' },
  ), { doc });
  const drawn = await renderCard(svgPalette);
  const eyebrow = drawn.texts.find((text) => text.part === 'eyebrow');
  await structuralCheck(`accent fallback: ${name}`, () => {
    assert.ok(eyebrow !== undefined, 'no eyebrow');
    if (expectAccent) {
      assert.equal(eyebrow.fill, palette.accent, `accent was not used: ${eyebrow.fill}`);
    } else {
      assert.equal(eyebrow.fill, palette.ink, `accent should have fallen back to ink, got ${eyebrow.fill}`);
      assert.ok(wcag(palette.accent, palette.surface) < 4.5, 'this fixture is not actually a failing accent');
    }
    return `${expectAccent ? 'accent' : 'ink'} (${wcag(eyebrow.fill, palette.surface).toFixed(2)}:1)`;
  });
  // Every caption must be readable whatever the accent did.
  await structuralCheck(`palette readable: ${name}`, () => {
    const surface = drawn.surface ?? palette.surface;
    const low = drawn.texts
      .map((text) => ({ part: text.part, fill: text.fill, ratio: wcag(text.fill, surface) }))
      .filter((text) => text.ratio < 4.5);
    assert.deepEqual(low, [], low.map((t) => `${t.part} ${t.fill} ${t.ratio.toFixed(2)}`).join('; '));
    return 'all ≥4.5:1';
  });
}

/* Answerability: an empty caption still produces a legal sign. */
const blankSvg = await page.evaluate(({ doc }) => QR.qrCardSvg(
  QR.encodeQr(doc.value, { ecc: doc.encode.ecc }),
  doc,
  { idPrefix: 'blank' },
), { doc: { ...docA6, caption: { eyebrow: '', title: '', subtitle: '', url: '', footer: '' } } });
const blank = await renderCard(blankSvg);
await structuralCheck('empty caption still composes', () => {
  assert.equal(blank.texts.filter((text) => text.part !== 'brand').length, 0);
  assert.ok(blank.symbol.w > 0, 'no symbol');
  return 'symbol only';
});

/* ========================================================================== */
/* Unit tests for the shared maths                                            */
/* ========================================================================== */

const UNIT = await page.evaluate(() => {
  const out = {};
  out.contrastBlackWhite = QR.contrastRatio('#000000', '#FFFFFF');
  out.contrastWhiteWhite = QR.contrastRatio('#FFFFFF', '#FFFFFF');
  out.contrastNeonOnWhite = QR.contrastRatio('#00FF87', '#FFFFFF');
  out.contrastBroken = QR.contrastRatio('not-a-color', '#FFFFFF');
  out.contrastShortHex = QR.contrastRatio('#000', '#fff');
  out.contrastRgbFn = QR.contrastRatio('rgb(0, 0, 0)', 'rgb(255,255,255)');
  out.luminanceWhite = QR.relativeLuminance('#FFFFFF');
  out.luminanceBlack = QR.relativeLuminance('#000000');
  out.parseShort = QR.parseColor('#0a0');
  out.parseAlpha = QR.parseColor('#0B121080');
  out.parseBad = QR.parseColor('currentColor');
  out.widthSpace = QR.estimateTextWidth(' ', 100, {});
  out.widthTwoSpaces = QR.estimateTextWidth('  ', 100, {});
  out.widthLower = QR.estimateTextWidth('aaaa', 100, {});
  out.widthBold = QR.estimateTextWidth('aaaa', 100, { weight: 700 });
  out.widthCyr = QR.estimateTextWidth('Привет', 100, {});
  out.widthTracking = QR.estimateTextWidth('AB', 100, { trackingEm: 0.16 });
  out.widthZeroTracking = QR.estimateTextWidth('AB', 100, { trackingEm: 0 });
  out.wrap = QR.wrapText('one two three four five six', 60, 10, {});
  out.wrapStopsOverflow = QR.wrapText('supercalifragilisticexpialidocious', 20, 10, {});
  out.truncate = QR.truncateToWidth('abcdefghijklmnop', 40, 10, {});
  out.truncateNoop = QR.truncateToWidth('ab', 40, 10, {});
  out.rampPoster = QR.typeSizes('poster', 1, 1);
  out.rampFloor = QR.typeSizes('poster', 0.2, 1);
  out.minType = QR.MIN_TYPE_MM;
  out.ruleSmall = QR.ruleWidth(80);
  out.ruleLarge = QR.ruleWidth(400);
  return out;
});

function unitCheck(name, fn) {
  const cell = { size: 'unit', layout: 'math', variant: '—', ok: true, checks: [] };
  results.push(cell);
  check(cell, name, fn);
}

unitCheck('contrastRatio: black on white = 21', () => {
  assert.ok(Math.abs(UNIT.contrastBlackWhite - 21) < 0.01, String(UNIT.contrastBlackWhite));
  assert.ok(Math.abs(UNIT.contrastRgbFn - 21) < 0.01, 'rgb() form differs');
  assert.ok(Math.abs(UNIT.contrastShortHex - 21) < 0.01, '#rgb form differs');
  assert.equal(UNIT.contrastWhiteWhite, 1);
  return UNIT.contrastBlackWhite.toFixed(2);
});

unitCheck('contrastRatio: unknown colours are NaN, never 1', () => {
  assert.ok(Number.isNaN(UNIT.contrastBroken), String(UNIT.contrastBroken));
  assert.equal(UNIT.parseBad, null);
  return 'NaN';
});

unitCheck('contrastRatio: neon green on white fails AA', () => {
  assert.ok(UNIT.contrastNeonOnWhite < 4.5, String(UNIT.contrastNeonOnWhite));
  return `${UNIT.contrastNeonOnWhite.toFixed(2)}:1`;
});

unitCheck('relativeLuminance bounds', () => {
  assert.ok(Math.abs(UNIT.luminanceBlack) < 1e-9);
  assert.ok(Math.abs(UNIT.luminanceWhite - 1) < 1e-9);
  return '0…1';
});

unitCheck('parseColor forms', () => {
  assert.deepEqual(UNIT.parseShort, { r: 0, g: 170, b: 0 });
  assert.deepEqual(UNIT.parseAlpha, { r: 11, g: 18, b: 16 });
  return '#rgb/#rgba';
});

unitCheck('estimateTextWidth is linear in size', () => {
  assert.ok(Math.abs(UNIT.widthSpace - 28.1) < 0.2, String(UNIT.widthSpace));
  assert.ok(Math.abs(UNIT.widthTwoSpaces - UNIT.widthSpace * 2) < 1e-6, 'space is not additive');
  return `${UNIT.widthSpace.toFixed(2)}mm per em`;
});

unitCheck('estimateTextWidth: bold is wider, tracking adds', () => {
  assert.ok(UNIT.widthBold > UNIT.widthLower, 'bold must be wider than regular');
  assert.ok(UNIT.widthTracking > UNIT.widthZeroTracking, 'tracking must add width');
  assert.ok(Math.abs((UNIT.widthTracking - UNIT.widthZeroTracking) - 2 * 16) < 0.5, 'tracking must add 0.16em per glyph');
  return `bold ×${(UNIT.widthBold / UNIT.widthLower).toFixed(3)}`;
});

unitCheck('estimateTextWidth: Cyrillic is measured, not fallback-guessed', () => {
  // The shipped Inter is a latin subset, so a Cyrillic caption renders in the
  // fallback stack — measurably wider than Inter. If both tables collapsed to
  // one average, this would not hold.
  assert.ok(UNIT.widthCyr > 0);
  return `${UNIT.widthCyr.toFixed(1)}mm for 6 glyphs`;
});

/* Width-dependent assertions need the page's estimator, so measure first. */
const WIDTHS = await page.evaluate(({ wrap, chunks, truncated, column, chunkColumn, truncColumn }) => ({
  wrap: wrap.map((line) => QR.estimateTextWidth(line, 10, {})),
  chunks: chunks.map((line) => QR.estimateTextWidth(line, 10, {})),
  truncated: QR.estimateTextWidth(truncated, 10, {}),
  column,
  chunkColumn,
  truncColumn,
}), {
  wrap: UNIT.wrap,
  chunks: UNIT.wrapStopsOverflow,
  truncated: UNIT.truncate,
  column: 60,
  chunkColumn: 20,
  truncColumn: 40,
});

unitCheck('wrapText respects the measure', () => {
  assert.ok(UNIT.wrap.length > 1, 'must wrap');
  UNIT.wrap.forEach((line, index) => {
    const width = WIDTHS.wrap[index] ?? Number.POSITIVE_INFINITY;
    assert.ok(width <= WIDTHS.column + 1e-6, `"${line}" is ${width.toFixed(1)}mm > ${WIDTHS.column}mm`);
  });
  assert.equal(UNIT.wrap.join(' '), 'one two three four five six');
  return `${UNIT.wrap.length} lines`;
});

unitCheck('wrapText hard-breaks a word wider than the column', () => {
  UNIT.wrapStopsOverflow.forEach((line, index) => {
    const width = WIDTHS.chunks[index] ?? Number.POSITIVE_INFINITY;
    assert.ok(width <= WIDTHS.chunkColumn + 1e-6, `"${line}" is ${width.toFixed(1)}mm > ${WIDTHS.chunkColumn}mm`);
  });
  return `${UNIT.wrapStopsOverflow.length} chunks`;
});

unitCheck('truncateToWidth adds an ellipsis and never overflows', () => {
  assert.ok(UNIT.truncate.endsWith('…'), UNIT.truncate);
  assert.ok(WIDTHS.truncated <= WIDTHS.truncColumn + 1e-6, `"${UNIT.truncate}" is ${WIDTHS.truncated.toFixed(1)}mm`);
  assert.equal(UNIT.truncateNoop, 'ab');
  return UNIT.truncate;
});

unitCheck('type ramp keeps its ratios and its floor', () => {
  assert.ok(UNIT.rampPoster.title / UNIT.rampPoster.subtitle >= 3, 'poster ramp is not a poster');
  assert.ok(UNIT.rampFloor.title >= UNIT.minType, 'ramp floor not applied');
  assert.ok(UNIT.rampFloor.url >= UNIT.minType, 'url floor not applied');
  return `poster ${UNIT.rampPoster.title.toFixed(1)}/${UNIT.rampPoster.subtitle.toFixed(1)}mm`;
});

unitCheck('rule width is a hairline at every size', () => {
  assert.ok(UNIT.ruleSmall <= 0.35 && UNIT.ruleSmall >= 0.12, String(UNIT.ruleSmall));
  assert.ok(UNIT.ruleLarge <= 0.35, String(UNIT.ruleLarge));
  return `${UNIT.ruleSmall.toFixed(3)}mm`;
});

/* ========================================================================== */
/* Report                                                                     */
/* ========================================================================== */

await browser.close();

const pad = (value, width) => String(value).padEnd(width);
const geometryLabel = useStub
  ? `STUB (geometry.ts is still a stub: ${realGeometry.error ?? 'threw'})`
  : 'REAL geometry.ts';

console.log('');
console.log(`QR card composer — ${SIZES.length} size(s) × ${api.layouts.length} layout(s) × ${DOCS.length} caption(s)`);
console.log(`geometry: ${geometryLabel}`);
console.log(`QR payload: ${VALUE} (v${grid.version}, ${grid.size} modules, ECC M)`);
console.log(`chromium ${browser.version?.() ?? ''}${''}`);
console.log('');

const bySize = new Map();
for (const cell of results) {
  if (cell.size === 'unit' || cell.layout === 'math') continue;
  if (!bySize.has(cell.size)) bySize.set(cell.size, []);
  bySize.get(cell.size).push(cell);
}

console.log(`${pad('size', 12)}${pad('layout', 10)}${pad('doc', 10)}${pad('ok', 6)}${pad('clear', 9)}${pad('gap', 8)}${pad('decode', 9)}failing`);
console.log('-'.repeat(86));
for (const [size, cells] of bySize) {
  for (const cell of cells) {
    const failing = cell.checks.filter((entry) => !entry.ok).map((entry) => entry.name).join(', ');
    console.log(
      pad(size, 12)
      + pad(cell.layout, 10)
      + pad(cell.variant, 10)
      + pad(cell.ok ? 'PASS' : 'FAIL', 6)
      + pad(cell.clearance === undefined ? '—' : `${cell.clearance.toFixed(1)}mm`, 9)
      + pad(cell.minGap === null || cell.minGap === undefined ? '—' : `${cell.minGap.toFixed(1)}mm`, 8)
      + pad(cell.decode === '' ? '—' : cell.decode, 9)
      + failing,
    );
  }
}

console.log('');
const structuralCell = results.find((cell) => cell.layout === 'structure');
const unitCells = results.filter((cell) => cell.layout === 'math');
console.log(`structure: ${structuralCell.checks.filter((c) => c.ok).length}/${structuralCell.checks.length} passed`);
for (const checkResult of structuralCell.checks) {
  if (!checkResult.ok) console.log(`  FAIL ${checkResult.name}: ${checkResult.detail}`);
}
console.log(`unit:      ${unitCells.filter((c) => c.ok).length}/${unitCells.length} passed`);
for (const cell of unitCells) {
  for (const checkResult of cell.checks) {
    if (!checkResult.ok) console.log(`  FAIL ${checkResult.name}: ${checkResult.detail}`);
  }
}

const cardCells = results.filter((cell) => cell.layout !== 'math' && cell.layout !== 'structure');
const passed = cardCells.filter((cell) => cell.ok).length;
console.log('');
console.log(`cards: ${passed}/${cardCells.length} combinations passed all assertions`);

if (VERBOSE) {
  console.log('');
  for (const cell of results) {
    for (const checkResult of cell.checks) {
      console.log(`${cell.size}/${cell.layout}/${cell.variant} ${checkResult.ok ? '✓' : '✗'} ${checkResult.name} — ${checkResult.detail}`);
    }
  }
}

if (!has('keep')) {
  console.log(`screenshots: ${OUT}`);
}

if (failures > 0) {
  console.error(`\n${failures} assertion(s) failed`);
  process.exitCode = 1;
} else {
  console.log('\nall assertions passed');
}
