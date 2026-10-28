/**
 * Symbol geometry — the contract the studio draws against.
 *
 * This module turns a module grid plus a `QrStyle` into **markup in module
 * units**, so it can be composed into a printed card, previewed at any size, and
 * rasterised for export without three different drawing routines drifting apart.
 *
 * Coexistence contract (do not change without updating the studio and exports):
 *
 *   `qrSymbolMarkup()` returns markup in a coordinate system where **one module
 *   is one unit**, including the quiet zone. `extent` is the size of that square
 *   in units. The caller places it with `<g transform="translate(x y) scale(k)">`
 *   and never needs to know the QR version.
 *
 *   `qrSymbolSvg()` is the standalone wrapper (a complete `<svg>` element) used
 *   for thumbnails and for rasterising to PNG/JPEG/PDF.
 *
 * What must stay true for the symbol to scan (measured, see `types.ts`):
 *   - the quiet zone stays light and is at least 2 modules wide;
 *   - a logo is capped at `LOGO_MAX_SCALE` and needs a light plate;
 *   - the finder patterns keep their 7×7 ring and 3×3 centre with ≥1 module of
 *     separation, whatever frame/centre shape is chosen;
 *   - the format-information strip around each eye is drawn with the module
 *     shape, never merged into the eye.
 *
 * Implementation notes that are decisions rather than accidents:
 *
 * 1. **Runs, not modules.** A row's dark modules that share a fill are emitted
 *    as one subpath of one `<path>` per fill, so a 177×177 version-40 grid costs
 *    a handful of nodes instead of ~15 000. Runs are only merged horizontally
 *    when the shape allows it (a gap between modules ends the merge).
 *
 * 2. **Two inks alternate per 2×2 block.** `dual` is a checkerboard of 2×2
 *    module blocks rather than column bands: it survives every module shape and
 *    does not depend on the run-merging granularity above.
 *
 * 3. **Eyes are offsets, not strokes.** Each finder is a filled 7×7 frame, then
 *    the 5×5 hole inset by exactly one module, then the 3×3 centre. Insetting a
 *    rounded rect by one module means reducing every corner radius by one, so
 *    the ring stays exactly one module thick all the way round — which is what
 *    `leaf` needs to keep decoding.
 */
import type { QrGrid } from './encode';
import { finderOrigins, inFinder } from './encode';
import type { EyeCenter, EyeFrame, ModuleShape, QrColors, QrLogo, QrStyle } from './types';
import { glyphById } from './presets';

export interface SymbolOptions {
  /** Prepended to every generated id (gradients, clip paths) to keep them unique. */
  idPrefix: string;
  /** When false the quiet zone is left transparent instead of painted. */
  paintBackground?: boolean;
}

export interface SymbolMarkup {
  /** Square size of the drawing area in module units, quiet zone included. */
  extent: number;
  /** `<g>` content, in module units, already coloured. */
  markup: string;
  /** `<defs>` content the markup references (gradients, clips). Empty when unused. */
  defs: string;
}

export interface SymbolSvgOptions extends SymbolOptions {
  /** Width and height attributes in px; the viewBox keeps it vector. */
  sizePx: number;
  /** Accessible name; the element is `role="img"`. */
  title?: string;
  /** Extra class on the `<svg>` element. */
  className?: string;
}

/** Modules of quiet zone per `QuietZone` choice. Never below 2. */
export const QUIET_MODULES: Record<QrStyle['quietZone'], number> = {
  tight: 2,
  standard: 4,
  air: 6,
};

/** Ceiling for the centre mark, in fraction of the symbol's width. */
export const LOGO_MAX_SCALE = 0.28;

/** The three finder patterns are 7 modules across; everything below derives from it. */
const EYE_SPAN = 7;

/** Largest corner radius a module may have, as a fraction of a module. */
const MAX_MODULE_RADIUS = 0.5;

/** `moduleGap` is a fraction of a module and is clamped to this ceiling. */
const MAX_MODULE_GAP = 0.15;

/** Smallest `logo.scale` honoured, so a bad value cannot collapse to nothing. */
const MIN_LOGO_SCALE = 0.05;

/** Radius of a `rounded` eye centre, in modules (a 3-module box). */
const EYE_CENTRE_RADIUS = 0.75;

/**
 * Ceiling for the **effective** eye radius, as a fraction of the 7-module span.
 *
 * This is a decode limit, not a taste limit, and it was measured: a finder
 * pattern is located from the 1:1:3:1:1 dark/light run a scan line sees, and the
 * detector expects that run on lines that cross the eye near its corners as well
 * as through its centre. Past ~0.45 of the span the corners are rounded away,
 * the eye is only correct through its middle, and both zbar and jsQR lose it.
 *
 * So `0.5` from the studio (`eyeRadius` is documented as 0–0.5) is accepted and
 * drawn as `0.45`. The control keeps its range; the drawing keeps the corners.
 */
const MAX_EYE_RADIUS = 0.45 * EYE_SPAN;

/**
 * Radius of the two small corners on a `leaf` frame, in modules.
 *
 * The hole is the frame offset inward by one module, and a rounded rectangle
 * offsets by shrinking every radius by the inset. A corner radius below one
 * module therefore offset to a sharp corner that sits closer than one module to
 * the frame's arc, thinning the ring; at one module the offset corner coincides
 * with the arc's centre and the ring stays a full module thick.
 */
const LEAF_SMALL_RADIUS = 1;

// --- small numeric helpers --------------------------------------------------

/** Clamps a possibly-garbage number into `[lo, hi]`, falling back to `lo`. */
function clamp(value: number, lo: number, hi: number): number {
  if (!Number.isFinite(value)) return lo;
  return Math.min(hi, Math.max(lo, value));
}

/**
 * Formats a coordinate for path data: three decimals is far below a printer's
 * resolution and keeps a version-40 symbol from doubling its markup size.
 */
function num(value: number): string {
  if (!Number.isFinite(value)) return '0';
  const rounded = Math.round(value * 1000) / 1000;
  return rounded === 0 ? '0' : String(rounded);
}

/** An id fragment that is legal in `id`/`url(#…)`, prefixed when it must be. */
function safeId(value: string): string {
  const cleaned = value.replace(/[^A-Za-z0-9_.-]/g, '-');
  if (cleaned === '') return 'qr';
  return /^[A-Za-z_]/.test(cleaned) ? cleaned : `qr-${cleaned}`;
}

type Radii = readonly [number, number, number, number];

const SQUARE_RADII: Radii = [0, 0, 0, 0];

// --- path builders ----------------------------------------------------------

/** A run of modules: an axis-aligned rectangle, `width` modules wide and 1 tall. */
function rectPath(x: number, y: number, width: number, height: number): string {
  return `M${num(x)} ${num(y)}h${num(width)}v${num(height)}h${num(-width)}z`;
}

/** A circle as two half arcs; cheaper than a `<circle>` per module. */
function circlePath(cx: number, cy: number, radius: number): string {
  return `M${num(cx - radius)} ${num(cy)}a${num(radius)} ${num(radius)} 0 1 0 ${num(radius * 2)} 0a${num(radius)} ${num(radius)} 0 1 0 ${num(-radius * 2)} 0z`;
}

/**
 * A rectangle with an independent radius per corner (clockwise from top-left).
 * A zero radius is emitted as a plain line, so a square costs no arc segments.
 */
function roundedRectPath(
  x: number,
  y: number,
  width: number,
  height: number,
  radii: Radii,
): string {
  const limit = Math.min(width / 2, height / 2);
  const [tl, tr, br, bl] = radii as [number, number, number, number];
  const rtl = Math.min(clamp(tl, 0, limit), limit);
  const rtr = Math.min(clamp(tr, 0, limit), limit);
  const rbr = Math.min(clamp(br, 0, limit), limit);
  const rbl = Math.min(clamp(bl, 0, limit), limit);

  const parts: string[] = [`M${num(x + rtl)} ${num(y)}`];
  if (rtr > 0) {
    parts.push(`H${num(x + width - rtr)}`, `A${num(rtr)} ${num(rtr)} 0 0 1 ${num(x + width)} ${num(y + rtr)}`);
  } else {
    parts.push(`H${num(x + width)}`);
  }
  if (rbr > 0) {
    parts.push(`V${num(y + height - rbr)}`, `A${num(rbr)} ${num(rbr)} 0 0 1 ${num(x + width - rbr)} ${num(y + height)}`);
  } else {
    parts.push(`V${num(y + height)}`);
  }
  if (rbl > 0) {
    parts.push(`H${num(x + rbl)}`, `A${num(rbl)} ${num(rbl)} 0 0 1 ${num(x)} ${num(y + height - rbl)}`);
  } else {
    parts.push(`H${num(x)}`);
  }
  if (rtl > 0) {
    parts.push(`V${num(y + rtl)}`, `A${num(rtl)} ${num(rtl)} 0 0 1 ${num(x + rtl)} ${num(y)}`);
  } else {
    parts.push(`V${num(y)}`);
  }
  parts.push('z');
  return parts.join('');
}

// --- module geometry --------------------------------------------------------

/** A horizontal stretch of dark, same-fill modules in one row; `end` inclusive. */
interface ModuleRun {
  row: number;
  start: number;
  end: number;
  fill: string;
}

/**
 * The fill a data module is drawn with.
 *
 * `single` is one ink; `dual` alternates on a 2×2 block grid; `gradient`
 * references the `<linearGradient>` in `defs`. Both secondary inks in the
 * presets are still darker than the paper, so the symbol stays dark-on-light —
 * which is the only orientation the measured scanners read.
 */
function moduleFill(style: QrStyle, gradientId: string, row: number, col: number): string {
  if (style.colorMode === 'dual') {
    const block = ((row >> 1) + (col >> 1)) & 1;
    return block === 1 ? style.colors.fg2 : style.colors.fg;
  }
  if (style.colorMode === 'gradient') return `url(#${gradientId})`;
  return style.colors.fg;
}

/** True when the module would be drawn by the module renderer (not by the eyes). */
function isDrawn(grid: QrGrid, row: number, col: number): boolean {
  if (row < 0 || col < 0 || row >= grid.size || col >= grid.size) return false;
  if (inFinder(grid.size, row, col)) return false;
  return grid.dark(row, col);
}

/**
 * Splits the data area into horizontal runs.
 *
 * A run stops at the finder blocks (drawn by the eye renderer) and — in `dual`
 * mode — where the fill changes, so a merged subpath never carries two colours.
 */
function collectRuns(grid: QrGrid, fillOf: (row: number, col: number) => string): ModuleRun[] {
  const runs: ModuleRun[] = [];
  const { size } = grid;
  for (let row = 0; row < size; row += 1) {
    let col = 0;
    while (col < size) {
      if (inFinder(size, row, col) || !grid.dark(row, col)) {
        col += 1;
        continue;
      }
      const fill = fillOf(row, col);
      let end = col;
      while (
        end + 1 < size
        && !inFinder(size, row, end + 1)
        && grid.dark(row, end + 1)
        && fillOf(row, end + 1) === fill
      ) {
        end += 1;
      }
      runs.push({ row, start: col, end, fill });
      col = end + 1;
    }
  }
  return runs;
}

/**
 * Corner radii for a `connected` run.
 *
 * A corner is rounded only when **both** edges that meet there are exposed, so
 * a module inside a run keeps square junctions and the run reads as a stroke.
 * Neighbours inside a finder count as empty: the separator around an eye is
 * light anyway, so an edge facing an eye really is an outer edge.
 */
function connectedRadii(
  grid: QrGrid,
  run: ModuleRun,
  radius: number,
): Radii {
  const { row, start, end } = run;
  const west = !isDrawn(grid, row, start - 1);
  const east = !isDrawn(grid, row, end + 1);
  const northStart = !isDrawn(grid, row - 1, start);
  const northEnd = !isDrawn(grid, row - 1, end);
  const southStart = !isDrawn(grid, row + 1, start);
  const southEnd = !isDrawn(grid, row + 1, end);
  return [
    west && northStart ? radius : 0,
    east && northEnd ? radius : 0,
    east && southEnd ? radius : 0,
    west && southStart ? radius : 0,
  ];
}

/** Renders the data modules as one `<path>` per fill. */
function renderModules(grid: QrGrid, style: QrStyle, quiet: number, gradientId: string): string {
  const shape: ModuleShape = style.modules;
  const radius = clamp(style.moduleRadius, 0, MAX_MODULE_RADIUS);
  const gap = clamp(style.moduleGap, 0, MAX_MODULE_GAP);
  const inset = gap / 2;
  const side = 1 - gap;
  // `liquid` keeps the connection rule and rounds the exposed corners harder.
  const outerRadius = shape === 'liquid'
    ? Math.min(MAX_MODULE_RADIUS, radius + (MAX_MODULE_RADIUS - radius) * 0.6)
    : radius;

  const fillOf = (row: number, col: number) => moduleFill(style, gradientId, row, col);
  const runs = collectRuns(grid, fillOf);
  const buckets = new Map<string, string[]>();
  const bucket = (fill: string): string[] => {
    let paths = buckets.get(fill);
    if (!paths) {
      paths = [];
      buckets.set(fill, paths);
    }
    return paths;
  };

  for (const run of runs) {
    const paths = bucket(run.fill);
    const width = run.end - run.start + 1;
    const x = quiet + run.start;
    const y = quiet + run.row;

    if (shape === 'dot') {
      const dotRadius = side / 2;
      const cy = y + 0.5;
      for (let i = 0; i < width; i += 1) {
        paths.push(circlePath(x + i + 0.5, cy, dotRadius));
      }
      continue;
    }

    if (gap === 0) {
      // One subpath for the whole run: this is what keeps the markup small.
      if (shape === 'square') {
        paths.push(rectPath(x, y, width, 1));
      } else if (shape === 'rounded') {
        paths.push(roundedRectPath(x, y, width, 1, [radius, radius, radius, radius]));
      } else {
        paths.push(roundedRectPath(x, y, width, 1, connectedRadii(grid, run, outerRadius)));
      }
      continue;
    }

    // A gap separates the modules, so each one is its own shape.
    const moduleRadius = Math.min(outerRadius * side, side / 2);
    for (let i = 0; i < width; i += 1) {
      const cellX = x + i + inset;
      const cellY = y + inset;
      if (shape === 'square') {
        paths.push(rectPath(cellX, cellY, side, side));
      } else if (shape === 'rounded') {
        const r = Math.min(radius * side, side / 2);
        paths.push(roundedRectPath(cellX, cellY, side, side, [r, r, r, r]));
      } else {
        const cellRun: ModuleRun = { row: run.row, start: run.start + i, end: run.start + i, fill: run.fill };
        paths.push(roundedRectPath(cellX, cellY, side, side, connectedRadii(grid, cellRun, moduleRadius)));
      }
    }
  }

  const markup: string[] = [];
  for (const [fill, paths] of buckets) {
    markup.push(`<path fill="${escapeXml(fill)}" d="${paths.join('')}"/>`);
  }
  return markup.join('');
}

// --- eyes -------------------------------------------------------------------

/**
 * Corner radii of an eye frame, or `null` when the frame would be a circle.
 *
 * There is no `null` any more: a true circular frame was measured to fail on
 * zbar **and** jsQR at every radius. A circle keeps the 1:1:3:1:1 run only along
 * the two lines through its centre; every other line crossing the eye sees a
 * thinner ring and empty corners, so the detector cannot place the finder.
 *
 * `circle` therefore renders as the furthest-rounded square the finder tolerates
 * — a squircle. It still reads as a circle at sign sizes, and it keeps the
 * corners that carry the run.
 */
function frameRadii(frame: EyeFrame, radius: number): Radii {
  switch (frame) {
    case 'square':
      return SQUARE_RADII;
    case 'leaf':
      // Two opposite corners round hard, the other two stay square-ish: the leaf.
      return [radius, Math.min(radius, LEAF_SMALL_RADIUS), radius, Math.min(radius, LEAF_SMALL_RADIUS)];
    case 'circle':
    case 'rounded':
    default:
      return [radius, radius, radius, radius];
  }
}

/**
 * The exact inward offset of a rounded rectangle: every radius shrinks by the
 * inset. Offsetting the radii (rather than recomputing the frame at the inner
 * size) is what keeps the ring exactly one module thick on every frame.
 */
function insetRadii(radii: Radii, inset: number): Radii {
  return [
    Math.max(0, (radii[0] ?? 0) - inset),
    Math.max(0, (radii[1] ?? 0) - inset),
    Math.max(0, (radii[2] ?? 0) - inset),
    Math.max(0, (radii[3] ?? 0) - inset),
  ];
}

/** One shape of an eye (frame, hole or centre), filled. */
function eyeShape(x: number, y: number, span: number, radii: Radii, fill: string): string {
  return `<path fill="${escapeXml(fill)}" d="${roundedRectPath(x, y, span, span, radii)}"/>`;
}

/** The 3×3 centre of an eye, per `eyeCenter`. */
function centreShape(centre: EyeCenter, x: number, y: number, fill: string): string {
  const span = EYE_SPAN - 4;
  let d: string;
  if (centre === 'dot') {
    d = circlePath(x + span / 2, y + span / 2, span / 2);
  } else if (centre === 'rounded') {
    d = roundedRectPath(x, y, span, span, [EYE_CENTRE_RADIUS, EYE_CENTRE_RADIUS, EYE_CENTRE_RADIUS, EYE_CENTRE_RADIUS]);
  } else {
    d = rectPath(x, y, span, span);
  }
  return `<path fill="${escapeXml(fill)}" d="${d}"/>`;
}

/**
 * The three finder patterns.
 *
 * The hole is the frame inset by exactly one module with every radius reduced
 * by one: that is the exact inward offset of a rounded rectangle, so the ring
 * is one module thick everywhere — including on `leaf`, where the two big
 * corners would otherwise pinch the ring shut.
 *
 * The hole is painted with `colors.bg` even when the caller asked for a
 * transparent background. It is not decoration and not the quiet zone: it is
 * the light ring that tells a scanner where the eye ends. Leaving it unpainted
 * would leave a solid 7×7 ink block where a finder pattern must be.
 */
function renderEyes(grid: QrGrid, style: QrStyle, quiet: number): string {
  const { eyeFrame: frame, eyeCenter: centre, eyeRadius } = style;
  // `eyeRadius` is 0–0.5 of the span, but 0.5 is past the measured decode limit:
  // at 0.5 the frame is a circle and loses its corners. Cap the drawing, keep
  // the control's range. See MAX_EYE_RADIUS.
  const radius = Math.min(clamp(eyeRadius, 0, MAX_MODULE_RADIUS) * EYE_SPAN, MAX_EYE_RADIUS);
  const frameR = frameRadii(frame, radius);
  const parts: string[] = [];

  for (const [row, col] of finderOrigins(grid.size)) {
    const x = quiet + col;
    const y = quiet + row;
    parts.push(eyeShape(x, y, EYE_SPAN, frameR, style.colors.eye));
    parts.push(eyeShape(x + 1, y + 1, EYE_SPAN - 2, insetRadii(frameR, 1), style.colors.bg));
    parts.push(centreShape(centre, x + 2, y + 2, style.colors.eyeCenter));
  }
  return parts.join('');
}

// --- logo -------------------------------------------------------------------

/** A rounded rect that restores the modules a mark covers. */
function plateMarkup(logo: QrLogo, box: number, cx: number, cy: number, colors: QrColors): string {
  if (!logo.plate) return '';
  const padding = clamp(logo.padding, 0, 0.5);
  const size = box * (1 + padding * 2);
  // `radius` is a fraction of the plate's side (see `QrLogo.radius`), so a 0.18
  // corner on a 10-module plate is 1.8 modules, not 0.18 of one.
  const radius = clamp(logo.radius, 0, 0.5) * size;
  const fill = logo.plateColor.trim() === '' ? colors.bg : logo.plateColor;
  return `<rect x="${num(cx - size / 2)}" y="${num(cy - size / 2)}" width="${num(size)}" height="${num(size)}" rx="${num(radius)}" ry="${num(radius)}" fill="${escapeXml(fill)}"/>`;
}

/** The mark itself: an uploaded image, a preset glyph, or a short text fallback. */
function markMarkup(logo: QrLogo, box: number, x: number, y: number, colors: QrColors): string {
  const ink = escapeXml(colors.eye);
  if (logo.kind === 'image') {
    if (logo.value.trim() === '') return '';
    // A data URL stays in the document: nothing is fetched, nothing is uploaded.
    const href = escapeXml(logo.value);
    return `<image href="${href}" xlink:href="${href}" x="${num(x)}" y="${num(y)}" width="${num(box)}" height="${num(box)}" preserveAspectRatio="xMidYMid meet"/>`;
  }
  if (logo.kind === 'text') {
    const text = logo.value.slice(0, 3);
    if (text === '') return '';
    return `<text x="${num(x + box / 2)}" y="${num(y + box / 2)}" fill="${ink}" font-family="system-ui, -apple-system, Segoe UI, sans-serif" font-size="${num(box * 0.62)}" font-weight="700" text-anchor="middle" dominant-baseline="central">${escapeXml(text)}</text>`;
  }
  const glyph = glyphById(logo.value);
  if (!glyph) return '';
  const scale = box / 24;
  return `<path fill="${ink}" transform="translate(${num(x)} ${num(y)}) scale(${num(scale)})" d="${glyph.path}"/>`;
}

/**
 * The centre mark.
 *
 * Sized against the **symbol box**, not the quiet-zone-included extent: a
 * 28% mark must cover 28% of the code, because that is what `LOGO_MAX_SCALE`
 * was measured against. Centred on the same box for the same reason.
 */
function renderLogo(logo: QrLogo, gridSize: number, quiet: number, colors: QrColors): string {
  const scale = Number.isFinite(logo.scale)
    ? clamp(logo.scale, MIN_LOGO_SCALE, LOGO_MAX_SCALE)
    : 0.2;
  const box = scale * gridSize;
  const cx = quiet + gridSize / 2;
  const cy = quiet + gridSize / 2;
  const x = cx - box / 2;
  const y = cy - box / 2;
  const plate = plateMarkup(logo, box, cx, cy, colors);
  const mark = markMarkup(logo, box, x, y, colors);
  return plate + mark;
}

// --- public API -------------------------------------------------------------

/** Builds the module-unit markup for a symbol. */
export function qrSymbolMarkup(grid: QrGrid, style: QrStyle, options: SymbolOptions): SymbolMarkup {
  const { size } = grid;
  if (!Number.isFinite(size) || size <= 0) {
    throw new Error(`Cannot draw a symbol from a grid of size ${size}.`);
  }

  const prefix = safeId(options.idPrefix);
  const gradientId = `${prefix}-grad`;
  const quiet = Math.max(2, clamp(QUIET_MODULES[style.quietZone], 2, 12));
  const extent = size + quiet * 2;
  const colors = style.colors;

  const defs: string[] = [];
  if (style.colorMode === 'gradient') {
    const angle = Number.isFinite(style.gradientAngle) ? style.gradientAngle : 90;
    const radians = (angle * Math.PI) / 180;
    const dx = Math.cos(radians);
    const dy = Math.sin(radians);
    // Project the square box onto the angle so the stops reach both corners.
    const half = (size / 2) * (Math.abs(dx) + Math.abs(dy));
    const cx = quiet + size / 2;
    const cy = quiet + size / 2;
    defs.push(
      `<linearGradient id="${escapeXml(gradientId)}" gradientUnits="userSpaceOnUse" x1="${num(cx - dx * half)}" y1="${num(cy - dy * half)}" x2="${num(cx + dx * half)}" y2="${num(cy + dy * half)}">`
      + `<stop offset="0" stop-color="${escapeXml(colors.fg)}"/>`
      + `<stop offset="1" stop-color="${escapeXml(colors.fg2)}"/>`
      + '</linearGradient>',
    );
  }

  const parts: string[] = [];
  if (options.paintBackground !== false) {
    parts.push(`<rect x="0" y="0" width="${num(extent)}" height="${num(extent)}" fill="${escapeXml(colors.bg)}"/>`);
  }
  parts.push(renderModules(grid, style, quiet, gradientId));
  parts.push(renderEyes(grid, style, quiet));
  if (style.logo) {
    parts.push(renderLogo(style.logo, size, quiet, colors));
  }

  return { extent, markup: parts.join(''), defs: defs.join('') };
}

/** Builds a standalone `<svg>` element string. */
export function qrSymbolSvg(grid: QrGrid, style: QrStyle, options: SymbolSvgOptions): string {
  const { extent, markup, defs } = qrSymbolMarkup(grid, style, options);
  const sizePx = Math.max(1, Math.round(Number.isFinite(options.sizePx) ? options.sizePx : extent));
  const label = escapeXml(options.title?.trim() || 'QR code');
  const className = options.className ? ` class="${escapeXml(options.className)}"` : '';
  const title = options.title?.trim() ? `<title>${label}</title>` : '';
  const defsElement = defs === '' ? '' : `<defs>${defs}</defs>`;

  return `<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" viewBox="0 0 ${num(extent)} ${num(extent)}" width="${sizePx}" height="${sizePx}" role="img" aria-label="${label}"${className}>`
    + title
    + defsElement
    + markup
    + '</svg>';
}

/** Escapes text for use in SVG text nodes and attribute values. */
export function escapeXml(value: string): string {
  return value
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&apos;');
}
