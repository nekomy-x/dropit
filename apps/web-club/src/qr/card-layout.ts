/**
 * Card layout engine — from "the operator typed a caption" to "positioned
 * primitives in millimetres".
 *
 * `card.ts` is the public module; this file is the composition engine behind it
 * and the only place that decides *where* anything sits on the sign. It is
 * deliberately free of QR knowledge: a layout receives a square box for the
 * symbol and never asks what version or style it is, which is what lets one
 * layout serve both a version-2 token and a version-12 room URL.
 *
 * Three ideas carry the file:
 *
 * 1. **One unit is one millimetre, and type is sized in millimetres too.** A
 *    card's ramp is derived from `min(width, height)`, never from px, so the
 *    80mm sticker and the A4 are the same design at two scales. Every card has
 *    a floor (`MIN_TYPE_MM`) because 1.2mm type is not readable print, it is
 *    texture.
 *
 * 2. **Text is measured before it is placed.** `estimateTextWidth()` sums real
 *    advance widths, in em, so wrapping, truncation and column fitting are
 *    decided against a number instead of a guess. Two things made the table
 *    non-obvious and both are load-bearing:
 *
 *      - the app ships `public/fonts/inter-latin-var.woff2`, whose
 *        `unicode-range` (declared in `src/index.css`) covers Latin-1 and
 *        general punctuation but **not Cyrillic**. A Russian caption therefore
 *        renders in the *fallback* stack, and Inter's metric is the wrong
 *        number for it: Inter's `а` is 0.444em while the Arial/Helvetica
 *        fallback's is 0.556em — 25% wider. Measuring Cyrillic with Inter's
 *        table would under-estimate by exactly enough to run a line off the
 *        paper, which is the one failure a printed card cannot recover from.
 *      - a fallback stack is not one font. For anything outside the Inter
 *        subset the table holds the **widest** advance across the plausible
 *        system faces (system-ui, sans-serif, Arial, Helvetica, Helvetica Neue,
 *        Tahoma, Trebuchet MS), so the estimator over-estimates on every
 *        machine rather than being right only on mine.
 *
 *    The estimator adds the trailing letter-space (as CSS does) and ignores
 *    kerning, so it is conservative by construction.
 *
 * 3. **A composition that does not fit is not a composition.** Every layout
 *    reports the slack it leaves (mm, negative when it overflows) and
 *    `composeCard()` searches the ramp from large to small, keeping the biggest
 *    scale that fits. "Long Russian title on an 80mm sticker" is therefore
 *    resolved by the engine — a smaller ramp, fewer subtitle lines — instead of
 *    by a truncation the operator never sees.
 *
 * Contrast is a print concern here, not a screen one: `contrastRatio()` is the
 * WCAG 2.1 ratio, used to decide whether the operator's accent may carry the
 * eyebrow and the rule or must fall back to ink, and to keep secondary text at
 * or above 4.5:1 against the card surface.
 */
import { escapeXml } from './geometry';
import type { CardLayout, QrCaption, QrDoc } from './types';

/* ========================================================================== */
/* Colour and contrast                                                        */
/* ========================================================================== */

interface Rgb {
  r: number;
  g: number;
  b: number;
}

const NAMED_COLORS: Record<string, Rgb> = {
  black: { r: 0, g: 0, b: 0 },
  white: { r: 255, g: 255, b: 255 },
};

function clampChannel(value: number): number {
  return Math.min(255, Math.max(0, value));
}

/**
 * Parses the colour forms a `QrDoc` realistically carries: `#rgb`, `#rgba`,
 * `#rrggbb`, `#rrggbbaa`, `rgb()` / `rgba()` with numbers or percentages, and
 * `black` / `white`.
 *
 * Anything else — `currentColor`, `transparent`, a CSS variable — returns
 * `null`, and every caller then treats the colour as unknown, which means "fall
 * back to ink", never "assume it is fine".
 */
export function parseColor(value: string): Rgb | null {
  const text = value.trim().toLowerCase();
  const named = NAMED_COLORS[text];
  if (named) return named;

  if (text.startsWith('#')) {
    const hex = text.slice(1);
    if (hex.length === 3 || hex.length === 4) {
      if (!/^[0-9a-f]{3,4}$/.test(hex)) return null;
      const [r, g, b] = [hex[0], hex[1], hex[2]];
      if (r === undefined || g === undefined || b === undefined) return null;
      const expand = (digit: string) => Number.parseInt(`${digit}${digit}`, 16);
      return { r: expand(r), g: expand(g), b: expand(b) };
    }
    if (hex.length === 6 || hex.length === 8) {
      if (!/^[0-9a-f]{6}([0-9a-f]{2})?$/.test(hex)) return null;
      return {
        r: Number.parseInt(hex.slice(0, 2), 16),
        g: Number.parseInt(hex.slice(2, 4), 16),
        b: Number.parseInt(hex.slice(4, 6), 16),
      };
    }
    return null;
  }

  const functional = /^rgba?\(([^)]+)\)$/.exec(text);
  if (functional) {
    const parts = (functional[1] ?? '').split(/[,/\s]+/).filter(Boolean);
    const [rawR, rawG, rawB] = [parts[0], parts[1], parts[2]];
    if (rawR === undefined || rawG === undefined || rawB === undefined) return null;
    const channel = (part: string) => (part.endsWith('%')
      ? (Number.parseFloat(part) / 100) * 255
      : Number.parseFloat(part));
    const [r, g, b] = [channel(rawR), channel(rawG), channel(rawB)];
    if ([r, g, b].some((value) => !Number.isFinite(value))) return null;
    return { r: clampChannel(r), g: clampChannel(g), b: clampChannel(b) };
  }

  return null;
}

function linearChannel(value: number): number {
  const c = clampChannel(value) / 255;
  return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
}

/** WCAG relative luminance; `NaN` when the colour cannot be parsed. */
export function relativeLuminance(color: string): number {
  const rgb = parseColor(color);
  if (!rgb) return Number.NaN;
  return 0.2126 * linearChannel(rgb.r) + 0.7152 * linearChannel(rgb.g) + 0.0722 * linearChannel(rgb.b);
}

/**
 * WCAG 2.1 contrast ratio between two colours, 1–21.
 *
 * `NaN` when either colour is unknown, so that every `>= 4.5` test is false and
 * the caller takes the ink fallback: an unparseable accent must not win a
 * colour decision by default.
 */
export function contrastRatio(a: string, b: string): number {
  const la = relativeLuminance(a);
  const lb = relativeLuminance(b);
  if (Number.isNaN(la) || Number.isNaN(lb)) return Number.NaN;
  const [high, low] = la >= lb ? [la, lb] : [lb, la];
  return (high + 0.05) / (low + 0.05);
}

/** Minimum ratio for text a person reads (WCAG AA, normal text). */
export const MIN_READABLE_CONTRAST = 4.5;

/** True when `ratio` is a number at or above the readable minimum. */
export function isReadable(ratio: number): boolean {
  return Number.isFinite(ratio) && ratio >= MIN_READABLE_CONTRAST;
}

function toHex(rgb: Rgb): string {
  const part = (value: number) => Math.round(clampChannel(value)).toString(16).padStart(2, '0');
  return `#${part(rgb.r)}${part(rgb.g)}${part(rgb.b)}`;
}

/** Mixes `color` toward `toward` by `amount` (0 keeps it, 1 becomes `toward`). */
export function mixColor(color: string, toward: string, amount: number): string | null {
  const a = parseColor(color);
  const b = parseColor(toward);
  if (!a || !b) return null;
  const t = Math.min(1, Math.max(0, amount));
  return toHex({
    r: a.r + (b.r - a.r) * t,
    g: a.g + (b.g - a.g) * t,
    b: a.b + (b.b - a.b) * t,
  });
}

/**
 * Resolves the accent for a print card.
 *
 * The operator may pick any accent, but it is allowed to carry the eyebrow and
 * the rule only when it clears 4.5:1 on the card surface. Otherwise both fall
 * back to ink, so a neon green never becomes unreadable copy on white paper.
 * This is the whole of "the accent is never body text": body text never even
 * sees the accent, and the one place the accent does appear is contrast-checked.
 */
export function accentOrInk(accent: string, surface: string, ink: string): string {
  return isReadable(contrastRatio(accent, surface)) ? accent : ink;
}

/**
 * A quieter member of the ink for secondary lines, obtained by mixing toward
 * the surface — but only as far as 4.5:1 survives. The mix is reduced step by
 * step and finally abandoned, so hierarchy never costs legibility.
 */
export function mutedInk(ink: string, surface: string, preferredMix = 0.16): string {
  for (const step of [preferredMix, preferredMix * 0.5, preferredMix * 0.25]) {
    const mixed = mixColor(ink, surface, step);
    if (mixed !== null && isReadable(contrastRatio(mixed, surface))) return mixed;
  }
  return ink;
}

/* ========================================================================== */
/* Type metrics                                                               */
/* ========================================================================== */

/**
 * Advance widths in em, one value per code point, space separated.
 *
 * Measured with Chromium's canvas `measureText` at 1000px and divided by 1000,
 * from the stack the card actually declares (`Inter, system-ui, sans-serif`)
 * with the shipped Inter face installed exactly as `src/index.css` declares it,
 * `unicode-range` included. Each weight has its own row because Inter 700 is a
 * real second design, not a scaled 400: the average ratio is 1.058, but `I`
 * goes 0.269 → 0.281 while `"` goes 0.466 → 0.551.
 */
const ADVANCE_TABLES: ReadonlyArray<{ from: number; w400: string; w700: string }> = [
  /* U+0020 … U+007E — space, punctuation, digits, A–Z, a–z */
  {
    from: 0x20,
    w400: `
        0.281 0.288 0.466 0.633 0.642 0.982 0.644 0.300 0.365 0.365 0.501 0.662
        0.288 0.460 0.288 0.360 0.631 0.407 0.610 0.618 0.646 0.593 0.620 0.566
        0.619 0.620 0.288 0.302 0.662 0.662 0.662 0.511 0.966 0.690 0.654 0.731
        0.722 0.601 0.590 0.746 0.743 0.269 0.571 0.672 0.565 0.903 0.753 0.765
        0.639 0.765 0.644 0.642 0.645 0.744 0.690 0.985 0.682 0.679 0.629 0.365
        0.360 0.365 0.471 0.456 0.323 0.561 0.612 0.571 0.612 0.583 0.370 0.613
        0.591 0.242 0.242 0.549 0.242 0.876 0.591 0.600 0.612 0.612 0.377 0.528
        0.327 0.591 0.562 0.818 0.546 0.562 0.552 0.426 0.333 0.426 0.662
      `,
    w700: `
        0.281 0.338 0.551 0.649 0.655 1.015 0.672 0.339 0.377 0.377 0.559 0.679
        0.334 0.468 0.334 0.388 0.674 0.431 0.630 0.645 0.676 0.622 0.650 0.582
        0.651 0.650 0.334 0.343 0.679 0.679 0.679 0.559 1.016 0.747 0.662 0.740
        0.722 0.607 0.590 0.751 0.747 0.281 0.584 0.719 0.565 0.932 0.762 0.770
        0.648 0.777 0.657 0.655 0.668 0.744 0.747 1.038 0.738 0.731 0.664 0.377
        0.388 0.377 0.487 0.476 0.365 0.581 0.630 0.588 0.630 0.596 0.398 0.632
        0.623 0.271 0.271 0.580 0.271 0.912 0.623 0.613 0.630 0.630 0.407 0.560
        0.366 0.623 0.600 0.850 0.580 0.602 0.573 0.469 0.372 0.469 0.679
      `,
  },
  /* U+0400 … U+045F — Cyrillic capitals and lowercase (fallback stack only) */
  {
    from: 0x400,
    w400: `
        0.678 0.667 0.865 0.550 0.722 0.667 0.373 0.373 0.519 1.105 1.124 0.854
        0.667 0.741 0.667 0.722 0.678 0.675 0.685 0.550 0.729 0.667 1.011 0.666
        0.730 0.730 0.667 0.703 0.871 0.722 0.778 0.722 0.667 0.722 0.619 0.667
        0.861 0.667 0.744 0.667 1.011 1.032 0.833 0.885 0.656 0.722 1.020 0.722
        0.556 0.573 0.548 0.433 0.606 0.556 0.783 0.506 0.584 0.584 0.548 0.584
        0.688 0.563 0.574 0.564 0.593 0.537 0.500 0.500 0.832 0.518 0.573 0.543
        0.802 0.831 0.625 0.722 0.534 0.537 0.778 0.558 0.565 0.556 0.578 0.433
        0.537 0.500 0.282 0.282 0.305 0.906 0.876 0.578 0.548 0.566 0.500 0.564
      `,
    w700: `
        0.678 0.669 0.885 0.578 0.736 0.667 0.483 0.483 0.565 1.125 1.124 0.877
        0.722 0.796 0.669 0.766 0.722 0.719 0.722 0.578 0.772 0.667 1.061 0.674
        0.775 0.775 0.722 0.776 0.907 0.764 0.778 0.764 0.667 0.741 0.650 0.669
        0.923 0.693 0.781 0.719 1.089 1.103 0.870 0.979 0.719 0.738 1.127 0.725
        0.599 0.628 0.615 0.498 0.647 0.594 0.886 0.542 0.652 0.652 0.611 0.654
        0.757 0.649 0.617 0.646 0.629 0.574 0.520 0.576 0.888 0.605 0.656 0.616
        0.923 0.934 0.729 0.854 0.615 0.576 0.923 0.611 0.594 0.594 0.635 0.498
        0.576 0.556 0.302 0.317 0.363 0.969 0.915 0.635 0.611 0.652 0.576 0.648
      `,
  },
];

/** Marks a caption really uses outside the two blocks above. */
const EXTRA_CODES: ReadonlyArray<number> = [
  0x00a0, 0x00a7, 0x00a9, 0x00ab, 0x00ae, 0x00b0, 0x00b7, 0x00bb, 0x00d7, 0x2013, 0x2014,
  0x2018, 0x2019, 0x201a, 0x201c, 0x201d, 0x2022, 0x2026, 0x20ac, 0x2116, 0x2122,
];
const EXTRA_W400 = '0.281 0.568 0.914 0.583 0.666 0.456 0.288 0.583 0.662 0.500 1.000 0.261 0.261 0.241 0.440 0.440 0.563 0.864 0.666 1.141 0.611';
const EXTRA_W700 = '0.281 0.568 0.914 0.660 0.666 0.460 0.334 0.660 0.679 0.500 1.000 0.311 0.311 0.288 0.540 0.532 0.563 1.002 0.685 1.233 0.640';

/** Used for glyphs outside the measured set (emoji, CJK). Deliberately wide. */
export const FALLBACK_ADVANCE = 1.05;

function buildAdvanceMap(row: 'w400' | 'w700'): Map<number, number> {
  const map = new Map<number, number>();
  for (const table of ADVANCE_TABLES) {
    const values = table[row].trim().split(/\s+/);
    values.forEach((value, index) => {
      const advance = Number.parseFloat(value);
      if (Number.isFinite(advance) && advance > 0) map.set(table.from + index, advance);
    });
  }
  const extras = (row === 'w400' ? EXTRA_W400 : EXTRA_W700).trim().split(/\s+/);
  EXTRA_CODES.forEach((code, index) => {
    const advance = Number.parseFloat(extras[index] ?? '');
    if (Number.isFinite(advance) && advance > 0) map.set(code, advance);
  });
  return map;
}

const ADVANCE_400 = buildAdvanceMap('w400');
const ADVANCE_700 = buildAdvanceMap('w700');

/** Advance of one character in em at `weight`, interpolated between 400 and 700. */
export function advanceEm(character: string, weight = 400): number {
  const code = character.codePointAt(0) ?? 0;
  const regular = ADVANCE_400.get(code) ?? FALLBACK_ADVANCE;
  const semibold = ADVANCE_700.get(code) ?? regular;
  const t = Math.min(1, Math.max(0, (weight - 400) / 300));
  return regular + (semibold - regular) * t;
}

/**
 * Em-box budgeting, measured rather than assumed.
 *
 * The number that matters is not Inter's cap height (0.727em) or its ink
 * descender (0.216em): it is the **font bounding box**, because that is what an
 * SVG renderer reports for a `<text>` element and therefore what a print shop's
 * imposition, a PDF bounding-box check and a browser crop will all consider to
 * be the element's extent. Measured in Chromium from the shipped Inter face and
 * from the fallback stack:
 *
 *     Inter       ascent 0.969em   descent 0.241em
 *     system-ui   ascent 0.967em   descent 0.211em
 *     sans-serif  ascent 0.920em   descent 0.230em
 *
 * The constants below are the maxima (0.97 / 0.25), rounded up. Reserving only
 * the ink box would leave a line's *reported* box hanging over the paper edge
 * on the last line — the card would look fine and fail a print preflight.
 */
export const FONT_ASCENT_EM = 0.97;
export const FONT_DESCENT_EM = 0.25;

export interface MeasureOptions {
  /** Letter spacing in em, added after every character, exactly as CSS does. */
  trackingEm?: number;
  weight?: number;
}

/**
 * How much wider a browser's hinted text is than the unhinted em sum.
 *
 * SVG text is laid out in user units and rasterised at whatever pixel size the
 * device gives it, and a rasteriser cannot place a stem on a fractional pixel:
 * it rounds every glyph advance to the pixel grid. At large sizes the rounding
 * is invisible, but print sizes are small — 6.6mm type is ~24px on a 760px A4
 * preview and ~78px at 300dpi — and there the error is a systematic **widening**
 * of the whole line. Measured in Chromium for the stack this card declares:
 *
 *     px   12    18    24    39    78   104+
 *     ×  1.118 1.061 1.028 1.020 1.001 1.000
 *
 * So the estimator is corrected by the table below. It is a *minimum* width,
 * not an exact one: underestimating is what puts a printed line off the paper,
 * so this rounds in the reader's favour and the layout keeps the slack.
 */
const HINTING_FACTOR: ReadonlyArray<{ maxPx: number; factor: number }> = [
  { maxPx: 14, factor: 1.13 },
  { maxPx: 20, factor: 1.07 },
  { maxPx: 28, factor: 1.04 },
  { maxPx: 44, factor: 1.025 },
  { maxPx: 64, factor: 1.012 },
  { maxPx: 96, factor: 1.004 },
  { maxPx: Number.POSITIVE_INFINITY, factor: 1 },
];

/**
 * Pixels per millimetre assumed when applying the hinting correction to a
 * length in mm.
 *
 * A vector sign has no intrinsic pixel size, so the correction is evaluated at
 * the smallest realistic rasterisation: 150dpi (5.9px/mm), the low end of the
 * export presets in `export.ts`. Using the smallest scale keeps the estimate
 * conservative for larger exports too, where hinting matters less.
 */
const PX_PER_MM = 150 / 25.4;

/** The hinting factor for a font size in millimetres. */
export function hintingFactor(sizeMm: number): number {
  const px = sizeMm * PX_PER_MM;
  for (const step of HINTING_FACTOR) {
    if (px <= step.maxPx) return step.factor;
  }
  return 1;
}

/** Width of `text` at `sizeMm`, in millimetres. */
export function estimateTextWidth(text: string, sizeMm: number, options: MeasureOptions = {}): number {
  const tracking = options.trackingEm ?? 0;
  const weight = options.weight ?? 400;
  let em = 0;
  let count = 0;
  for (const character of text) {
    em += advanceEm(character, weight);
    count += 1;
  }
  return (em + tracking * count) * sizeMm * hintingFactor(sizeMm);
}

/** Height of a `lineCount`-line block at `size`, including leading. */
export function textBlockHeight(size: number, lineHeight: number, lineCount: number): number {
  if (lineCount <= 0) return 0;
  return (FONT_ASCENT_EM + FONT_DESCENT_EM) * size + (lineCount - 1) * lineHeight * size;
}

/**
 * Width-aware word wrapping.
 *
 * Words are never split unless a single word is wider than the box; then it is
 * hard-broken, because a printed card has no scrollbar and no hover to reveal
 * the rest of a bare token.
 */
export function wrapText(
  text: string,
  maxWidth: number,
  sizeMm: number,
  options: MeasureOptions = {},
): string[] {
  const trimmed = text.trim();
  if (trimmed === '' || maxWidth <= 0) return [];
  const width = (value: string) => estimateTextWidth(value, sizeMm, options);
  const lines: string[] = [];
  let current = '';

  const breakWord = (word: string): string[] => {
    const chunks: string[] = [];
    let rest = word;
    let guard = 0;
    while (rest !== '' && guard < 128) {
      guard += 1;
      if (width(rest) <= maxWidth) {
        chunks.push(rest);
        break;
      }
      let take = rest.length - 1;
      while (take > 1 && width(rest.slice(0, take)) > maxWidth) take -= 1;
      chunks.push(rest.slice(0, take));
      rest = rest.slice(take);
    }
    return chunks;
  };

  for (const word of trimmed.split(/\s+/).filter(Boolean)) {
    const candidate = current === '' ? word : `${current} ${word}`;
    if (width(candidate) <= maxWidth) {
      current = candidate;
      continue;
    }
    if (current !== '') {
      lines.push(current);
      current = '';
    }
    if (width(word) <= maxWidth) {
      current = word;
      continue;
    }
    const chunks = breakWord(word);
    const tail = chunks.pop() ?? '';
    for (const chunk of chunks) lines.push(chunk);
    current = tail;
  }
  if (current !== '') lines.push(current);
  return lines;
}

/** Cuts `text` to fit `maxWidth`, appending an ellipsis when it had to cut. */
export function truncateToWidth(
  text: string,
  maxWidth: number,
  sizeMm: number,
  options: MeasureOptions = {},
): string {
  const width = (value: string) => estimateTextWidth(value, sizeMm, options);
  if (width(text) <= maxWidth) return text;
  const ellipsis = '…';
  if (width(ellipsis) > maxWidth) return '';
  let low = 0;
  let high = text.length;
  while (low < high) {
    const middle = Math.ceil((low + high) / 2);
    if (width(`${text.slice(0, middle).trimEnd()}${ellipsis}`) <= maxWidth) low = middle;
    else high = middle - 1;
  }
  const head = text.slice(0, low).trimEnd();
  return head === '' ? ellipsis : `${head}${ellipsis}`;
}

/* ========================================================================== */
/* The type ramp                                                              */
/* ========================================================================== */

/** Nothing on a printed card is set below this (~4.8pt). */
export const MIN_TYPE_MM = 1.7;

/** One hundredth of the card's shorter side, with a floor. */
export function typeUnit(width: number, height: number): number {
  return Math.max(0.8, Math.min(width, height) / 100);
}

export type RampLayout = Exclude<CardLayout, 'qr-only'>;

export interface TypeRamp {
  eyebrow: number;
  title: number;
  subtitle: number;
  url: number;
  footer: number;
  brand: number;
}

/**
 * Sizes per role, as multiples of `t = min(width, height) / 100`.
 *
 * The ratios inside a ramp are fixed on purpose: `poster` keeps its title at
 * least 3× its subtitle whatever the card, because that contrast *is* the
 * poster. When the engine shrinks a card it multiplies the whole ramp by one
 * factor and never a single role, so a card never loses its hierarchy.
 */
const RAMPS: Record<RampLayout, TypeRamp> = {
  stacked: { eyebrow: 2.7, title: 7.2, subtitle: 3.2, url: 2.9, footer: 2.7, brand: 2.7 },
  side: { eyebrow: 2.7, title: 7.6, subtitle: 3.3, url: 3.0, footer: 2.7, brand: 2.7 },
  poster: { eyebrow: 3.0, title: 13.5, subtitle: 4.0, url: 5.6, footer: 2.9, brand: 2.9 },
};

/** The ramp in millimetres for a layout at a given scale. */
export function typeSizes(layout: RampLayout, t: number, scale = 1): TypeRamp {
  const ramp = RAMPS[layout];
  const at = (value: number) => Math.max(MIN_TYPE_MM, value * t * scale);
  return {
    eyebrow: at(ramp.eyebrow),
    title: at(ramp.title),
    subtitle: at(ramp.subtitle),
    url: at(ramp.url),
    footer: at(ramp.footer),
    brand: at(ramp.brand),
  };
}

/* ========================================================================== */
/* Scene                                                                      */
/* ========================================================================== */

export type TextPart = 'eyebrow' | 'title' | 'subtitle' | 'url' | 'footer';
export type TextAnchor = 'start' | 'middle' | 'end';

export interface SceneLine {
  part: TextPart;
  text: string;
  /** Anchor x, in mm. */
  x: number;
  /** Baseline y, in mm. */
  baseline: number;
  /** Font size, in mm. */
  size: number;
  weight: number;
  anchor: TextAnchor;
  trackingEm: number;
  fill: string;
  /** Width the line was wrapped to; carried for tests and diagnostics. */
  boxWidth: number;
}

export interface SceneRule {
  x1: number;
  y1: number;
  x2: number;
  y2: number;
  width: number;
  fill: string;
}

export interface SceneBrand {
  /** Baseline of the wordmark. */
  baseline: number;
  /** Left edge of the mark, dot included. */
  left: number;
  size: number;
  fill: string;
  trackingEm: number;
  /** x the "DROP IT" text starts at. */
  textX: number;
  dot: { cx: number; cy: number; r: number; fill: string } | null;
}

export interface CardScene {
  width: number;
  height: number;
  surface: string;
  paintSurface: boolean;
  symbol: { x: number; y: number; size: number };
  lines: SceneLine[];
  rules: SceneRule[];
  brand: SceneBrand | null;
  /** The ramp scale the composition settled at, and the slack it left in mm. */
  fit: { scale: number; slack: number };
  /** Paper between the symbol box and the nearest card edge, in mm. */
  clearance: number;
}

export interface ComposeOptions {
  paintSurface?: boolean;
}

interface Ink {
  body: string;
  muted: string;
  accent: string;
}

interface Context {
  width: number;
  height: number;
  minSide: number;
  t: number;
  pad: number;
  contentW: number;
  contentH: number;
  surface: string;
  ink: Ink;
  caption: QrCaption;
  brandMark: boolean;
  divider: boolean;
  layout: RampLayout;
}

const FONT_FAMILY = 'Inter, system-ui, sans-serif';

const LINE_HEIGHT: Record<TextPart, number> = {
  eyebrow: 1.25,
  title: 1.14,
  subtitle: 1.3,
  url: 1.22,
  footer: 1.25,
};

const TRACKING: Record<TextPart, number> = {
  eyebrow: 0.1,
  title: 0,
  subtitle: 0,
  url: 0.02,
  footer: 0.02,
};

const WEIGHT: Record<TextPart, number> = {
  eyebrow: 500,
  title: 700,
  subtitle: 400,
  url: 500,
  footer: 400,
};

const BRAND_TEXT = 'DROP IT';
const BRAND_WEIGHT = 600;
const BRAND_TRACKING = 0.16;
const BRAND_DOT_RATIO = 0.3;
const BRAND_GAP_RATIO = 0.44;

interface Paragraph {
  lines: SceneLine[];
  height: number;
  /**
   * True when the field had more text than its box allowed and lost its tail
   * to an ellipsis. This is an overflow signal, not a cosmetic flag: a printed
   * headline that reads "СКАНИРУЙ И ЗАКАЗЫ…" is a broken sign, so a truncated
   * field makes the whole composition overflow and the engine keeps shrinking
   * the ramp until nothing has to be cut.
   */
  truncated: boolean;
}

interface TextBox {
  x: number;
  width: number;
  size: number;
  anchor: TextAnchor;
  fill: string;
  maxLines: number;
}

/**
 * Wraps and truncates one field into at most `maxLines` lines.
 *
 * Truncation only ever touches the last allowed line and happens after
 * wrapping, so a long title loses its tail rather than its middle and always
 * ends in a visible ellipsis.
 */
function paragraph(part: TextPart, text: string, box: TextBox): Paragraph {
  const value = text.trim();
  if (value === '') return { lines: [], height: 0, truncated: false };
  const measure = { trackingEm: TRACKING[part], weight: WEIGHT[part] };
  let size = box.size;

  /*
   * Shrink before cutting.
   *
   * A printed headline that reads "СКАНИРУЙ И ЗАКАЗЫ…" is not a headline, it is
   * a defect: the guest cannot read what the sign is for. So every field first
   * steps its size down (in 0.5% increments of itself, floored at
   * `MIN_TYPE_MM`) until it fits its box, and only a field that still does not
   * fit at the floor is allowed to lose its tail to an ellipsis. The caller gets
   * `truncated` back so the composition can keep shrinking the rest of the ramp
   * rather than quietly printing the cut.
   */
  let wrapped = wrapText(value, box.width, size, measure);
  let guard = 0;
  while (wrapped.length > box.maxLines && size > MIN_TYPE_MM && guard < 512) {
    size = Math.max(MIN_TYPE_MM, size * 0.995);
    wrapped = wrapText(value, box.width, size, measure);
    guard += 1;
  }

  let truncated = false;
  if (wrapped.length > box.maxLines) {
    const keep = Math.max(1, box.maxLines);
    const head = wrapped.slice(0, keep - 1);
    head.push(truncateToWidth(wrapped.slice(keep - 1).join(' '), box.width, size, measure));
    wrapped = head;
    truncated = true;
  }
  if (wrapped.length === 0) return { lines: [], height: 0, truncated: false };

  const lineHeight = LINE_HEIGHT[part];
  const lines = wrapped.map<SceneLine>((line, index) => ({
    part,
    text: line,
    x: box.x,
    baseline: FONT_ASCENT_EM * size + index * lineHeight * size,
    size,
    weight: WEIGHT[part],
    anchor: box.anchor,
    trackingEm: TRACKING[part],
    fill: box.fill,
    boxWidth: box.width,
  }));

  return { lines, height: textBlockHeight(size, lineHeight, lines.length), truncated };
}

/** Shifts a paragraph down by `dy` mm, returning its lines. */
function place(block: Paragraph, dy: number): SceneLine[] {
  return block.lines.map((line) => ({ ...line, baseline: line.baseline + dy }));
}

/* ========================================================================== */
/* Brand mark                                                                 */
/* ========================================================================== */

/** Width of the wordmark including its dot, in mm. */
export function brandMarkWidth(size: number): number {
  return size * (BRAND_DOT_RATIO * 2 + BRAND_GAP_RATIO)
    + estimateTextWidth(BRAND_TEXT, size, { trackingEm: BRAND_TRACKING, weight: BRAND_WEIGHT });
}

/**
 * Height the wordmark reserves in a stack, in mm.
 *
 * It is exactly the ink box — ascent above the baseline plus the dot, which
 * sits inside the cap height — so a reserved box and the measured bounding box
 * are the same rectangle. Getting that wrong by a couple of tenths of an em is
 * what silently eats into the 3mm the symbol needs around it.
 */
export function brandMarkHeight(size: number): number {
  // A `<text>` reports its font box whatever the glyphs are, so the reserved
  // box is the font box: "DROP IT" is capitals, but the browser still reports
  // the descender, and a reserved box that disagrees with the reported box is
  // how a gap stops being the gap you asked for.
  return size * (FONT_ASCENT_EM + FONT_DESCENT_EM);
}

/** Baseline inside a `brandMarkHeight()` box, in mm from its top. */
export function brandBaselineOffset(size: number): number {
  return size * FONT_ASCENT_EM;
}

function brandMark(options: {
  anchorX: number;
  baseline: number;
  size: number;
  anchor: TextAnchor;
  fill: string;
  dotFill: string;
}): SceneBrand {
  const total = brandMarkWidth(options.size);
  const left = options.anchor === 'start'
    ? options.anchorX
    : options.anchor === 'middle'
      ? options.anchorX - total / 2
      : options.anchorX - total;
  const radius = options.size * BRAND_DOT_RATIO;
  return {
    baseline: options.baseline,
    left,
    size: options.size,
    fill: options.fill,
    trackingEm: BRAND_TRACKING,
    textX: left + radius * 2 + options.size * BRAND_GAP_RATIO,
    dot: {
      cx: left + radius,
      cy: options.baseline - options.size * 0.34,
      r: radius,
      fill: options.dotFill,
    },
  };
}

/* ========================================================================== */
/* Spacing helpers                                                            */
/* ========================================================================== */

/**
 * Minimum paper between the symbol box and any text, in mm.
 *
 * An absolute floor rather than a ratio, because the requirement is about the
 * reader's eye and the printer's edge, not about the card's proportions: 3mm is
 * the number below which a descender starts to look like it touches the code on
 * every card, including the A4.
 */
export const SYMBOL_TEXT_GAP = 3;

function spacing(context: Context, ratio: number, floor: number): number {
  return Math.max(floor, context.minSide * ratio);
}

/** A gap that also has to respect the symbol's own breathing room. */
function symbolGap(context: Context, ratio: number): number {
  return Math.max(SYMBOL_TEXT_GAP, context.minSide * ratio);
}

/** Card margin. Never below 4mm: a home printer's unprintable edge is ~3mm. */
export function cardPadding(minSide: number): number {
  return Math.max(4, minSide * 0.075);
}

/** Hairline rule width. Capped at 0.35mm so it prints as a rule, not a band. */
export function ruleWidth(minSide: number): number {
  return Math.min(0.35, Math.max(0.12, minSide * 0.003));
}

/* ========================================================================== */
/* Layouts                                                                    */
/* ========================================================================== */

interface Built {
  symbol: { x: number; y: number; size: number };
  lines: SceneLine[];
  rules: SceneRule[];
  brand: SceneBrand | null;
  /** Free vertical space left by the composition; negative means overflow. */
  slack: number;
  /** True when some field had to be cut; treated as overflow by the driver. */
  truncated: boolean;
}

/**
 * Turns a layout's vertical slack plus its cut-text flags into the single number
 * the driver shrinks against.
 *
 * A cut field counts as overflow with a large, fixed penalty rather than a
 * small one: "there is a bit of room left and also the headline lost its last
 * word" is still a broken sign, so the ramp must keep shrinking.
 */
function overflowSlack(slack: number, truncated: boolean): number {
  return truncated ? Math.min(slack, -1) : slack;
}

/** True when any of these fields had to be cut to fit. */
function cutText(...blocks: Paragraph[]): boolean {
  return blocks.some((block) => block.truncated);
}

/**
 * `qr-only` — the symbol and nothing else.
 *
 * This layout exists for the case where the copy is already on the wall or in
 * the frame, so any text added here would compete with the symbol. The margin is
 * generous (13% of the shorter side) both to give the symbol air and to keep a
 * trim line out of the quiet zone.
 */
function layoutQrOnly(context: Context): Built {
  const margin = Math.max(4, context.minSide * 0.13);
  const size = Math.max(1, Math.min(context.width, context.height) - margin * 2);
  return {
    symbol: { x: (context.width - size) / 2, y: (context.height - size) / 2, size },
    lines: [],
    rules: [],
    brand: null,
    slack: 0,
    truncated: false,
  };
}

/**
 * `stacked` — eyebrow, symbol, rule, title, subtitle, url, then the foot.
 *
 * The symbol takes whatever the caption leaves, between a floor (it must stay
 * scannable) and a ceiling (it must not swamp the copy). Leftover paper is
 * absorbed by the symbol rather than dumped in one band, which is what keeps a
 * table tent from having a dead third at the bottom.
 */
function layoutStacked(context: Context, sizes: TypeRamp, minSymbol: number): Built {
  const { pad, contentW, contentH, caption, ink, width } = context;
  const cx = width / 2;

  const gapEyebrow = symbolGap(context, 0.022);
  const gapSymbol = symbolGap(context, 0.05);
  const gapRule = spacing(context, 0.02, 2);
  const gapCaption = spacing(context, 0.03, 2.5);
  const gapFoot = spacing(context, 0.035, 3);

  const eyebrow = paragraph('eyebrow', caption.eyebrow, {
    x: cx, width: contentW * 0.84, size: sizes.eyebrow, anchor: 'middle',
    fill: ink.accent, maxLines: 1,
  });
  const title = paragraph('title', caption.title, {
    x: cx, width: contentW, size: sizes.title, anchor: 'middle',
    fill: ink.body, maxLines: 3,
  });
  const subtitle = paragraph('subtitle', caption.subtitle, {
    x: cx, width: contentW * 0.9, size: sizes.subtitle, anchor: 'middle',
    fill: ink.muted, maxLines: 2,
  });
  const url = paragraph('url', caption.url, {
    x: cx, width: contentW * 0.94, size: sizes.url, anchor: 'middle',
    fill: ink.muted, maxLines: 1,
  });
  const footer = paragraph('footer', caption.footer, {
    x: cx, width: contentW * 0.9, size: sizes.footer, anchor: 'middle',
    fill: ink.muted, maxLines: 1,
  });
  const brandH = context.brandMark ? brandMarkHeight(sizes.brand) : 0;
  const ruleH = context.divider && (title.height > 0 || subtitle.height > 0 || url.height > 0)
    ? ruleWidth(context.minSide)
    : 0;

  const captionBlock = title.height
    + (subtitle.height > 0 ? gapCaption + subtitle.height : 0)
    + (url.height > 0 ? gapCaption + url.height : 0);
  const topBlock = eyebrow.height > 0 ? eyebrow.height + gapEyebrow : 0;
  const brandGap = brandH > 0 && footer.height > 0 ? gapCaption * 0.6 : 0;
  const bottomBlock = (footer.height > 0 ? gapFoot : 0) + footer.height + brandGap + brandH;

  const required = topBlock + gapSymbol + minSymbol
    + (ruleH > 0 ? gapRule * 2 + ruleH : 0) + captionBlock + bottomBlock;
  const slack = contentH - required;

  const symbolSize = Math.max(
    minSymbol,
    Math.min(
      Math.min(context.minSide * 0.62, contentW),
      minSymbol + Math.max(0, slack) * 0.72,
    ),
  );

  const lines: SceneLine[] = [];
  const rules: SceneRule[] = [];

  let y = pad;
  if (eyebrow.height > 0) {
    lines.push(...place(eyebrow, y));
    y += eyebrow.height + gapEyebrow;
  }
  const symbol = { x: cx - symbolSize / 2, y, size: symbolSize };
  y += symbolSize + gapSymbol;

  if (ruleH > 0) {
    y += gapRule;
    rules.push({
      x1: cx - contentW / 2, y1: y, x2: cx + contentW / 2, y2: y,
      width: ruleH, fill: ink.accent,
    });
    y += gapRule;
  }

  lines.push(...place(title, y));
  y += title.height;
  if (subtitle.height > 0) {
    y += gapCaption;
    lines.push(...place(subtitle, y));
    y += subtitle.height;
  }
  if (url.height > 0) {
    y += gapCaption;
    lines.push(...place(url, y));
  }

  // The foot is pinned to the bottom margin. On a tall card that is what stops
  // the last line from floating in the middle of the paper.
  let brand: SceneBrand | null = null;
  if (brandH > 0) {
    brand = brandMark({
      anchorX: cx,
      baseline: context.height - pad - brandMarkHeight(sizes.brand) + brandBaselineOffset(sizes.brand),
      size: sizes.brand,
      anchor: 'middle',
      fill: ink.body,
      dotFill: ink.accent,
    });
  }
  if (footer.height > 0) {
    const footerBottom = brandH > 0
      ? context.height - pad - brandH - brandGap
      : context.height - pad;
    lines.push(...place(footer, footerBottom - footer.height));
  }

  return {
    symbol,
    lines,
    rules,
    brand,
    slack: overflowSlack(slack, cutText(eyebrow, title, subtitle, url, footer)),
    truncated: cutText(eyebrow, title, subtitle, url, footer),
  };
}

/**
 * `side` — symbol on one side, caption column on the other, left-aligned.
 *
 * A square card refolds into `stacked`: at 80×80 two columns would be two ~30mm
 * slivers, which is not a layout, it is a squeeze. Reporting the square case as
 * the stacked composition is the honest answer, and the test asserts that the
 * square card really does switch to the stacked geometry.
 */
function isSquareCard(context: Context): boolean {
  const ratio = context.width / context.height;
  return ratio > 0.92 && ratio < 1.08;
}

function layoutSide(context: Context, sizes: TypeRamp, minSymbol: number): Built {
  const { pad, contentW, contentH, caption, ink } = context;
  const columnGap = symbolGap(context, 0.055);
  const captionGap = spacing(context, 0.028, 2);
  /*
   * The symbol takes a *little under half* the width, not as much as it can.
   *
   * On a portrait card the caption column is the scarce resource: three lines of
   * a Russian headline need roughly 40 characters at 7.6% of the short side, and
   * every millimetre taken by the symbol is a millimetre the headline cannot
   * use — which is how a column ends up wrapping to five lines and truncating
   * its subtitle. 0.42 is where an A6 keeps its full headline and still gives
   * the symbol more width than a phone needs at arm's length.
   */
  const symbolSize = Math.max(
    minSymbol,
    Math.min(context.minSide * 0.44, contentW * 0.42, contentH * 0.58),
  );
  const columnX = pad + symbolSize + columnGap;
  const columnW = contentW - symbolSize - columnGap;

  const eyebrow = paragraph('eyebrow', caption.eyebrow, {
    x: columnX, width: columnW, size: sizes.eyebrow, anchor: 'start',
    fill: ink.accent, maxLines: 1,
  });
  const title = paragraph('title', caption.title, {
    x: columnX, width: columnW, size: sizes.title, anchor: 'start',
    fill: ink.body, maxLines: 3,
  });
  const subtitle = paragraph('subtitle', caption.subtitle, {
    x: columnX, width: columnW, size: sizes.subtitle, anchor: 'start',
    fill: ink.muted, maxLines: 2,
  });
  const url = paragraph('url', caption.url, {
    x: columnX, width: columnW, size: sizes.url, anchor: 'start',
    fill: ink.muted, maxLines: 1,
  });
  const footer = paragraph('footer', caption.footer, {
    x: columnX, width: columnW, size: sizes.footer, anchor: 'start',
    fill: ink.muted, maxLines: 1,
  });
  const brandH = context.brandMark ? brandMarkHeight(sizes.brand) : 0;

  const blocks = [eyebrow, title, subtitle, url, footer].filter((block) => block.height > 0);
  const captionH = blocks.reduce((sum, block) => sum + block.height, 0)
    + captionGap * Math.max(0, blocks.length - 1)
    + (brandH > 0 ? (blocks.length > 0 ? captionGap : 0) + brandH : 0);
  const groupH = Math.max(symbolSize, captionH);
  const slack = contentH - groupH;
  const top = pad + Math.max(0, (contentH - groupH) / 2);

  const lines: SceneLine[] = [];
  let y = top + (groupH - captionH) / 2;
  for (const block of blocks) {
    lines.push(...place(block, y));
    y += block.height + captionGap;
  }

  let brand: SceneBrand | null = null;
  if (brandH > 0) {
    brand = brandMark({
      anchorX: columnX,
      baseline: y + brandBaselineOffset(sizes.brand),
      size: sizes.brand,
      anchor: 'start',
      fill: ink.body,
      dotFill: ink.accent,
    });
  }

  const rules: SceneRule[] = [];
  if (context.divider && captionH > 0) {
    const x = pad + symbolSize + columnGap / 2;
    rules.push({
      x1: x,
      y1: top + (groupH - captionH) / 2,
      x2: x,
      y2: top + (groupH + captionH) / 2,
      width: ruleWidth(context.minSide),
      fill: ink.accent,
    });
  }

  const cut = cutText(eyebrow, title, subtitle, url, footer);
  return {
    symbol: { x: pad, y: top + (groupH - symbolSize) / 2, size: symbolSize },
    lines,
    rules,
    brand,
    slack: overflowSlack(slack, cut),
    truncated: cut,
  };
}

/**
 * `poster` — the title is the point and the symbol is the tool.
 *
 * Copy reads from the top at a big ramp (title ≈ 3.4× subtitle), the url is set
 * large because someone standing at the door will type it, and the symbol sits
 * in the lower-right corner, small enough not to compete with the words and
 * large enough to scan from a step back.
 */
function layoutPoster(context: Context, sizes: TypeRamp, minSymbol: number): Built {
  const { pad, contentW, contentH, caption, ink } = context;
  const gapSymbol = symbolGap(context, 0.055);
  const columnGap = symbolGap(context, 0.045);
  const captionGap = spacing(context, 0.03, 2.5);
  // The band holds the symbol, so both of its outer gaps are symbol-to-text.
  const gapFoot = symbolGap(context, 0.03);

  const symbolSize = Math.max(minSymbol, Math.min(context.minSide * 0.32, contentW * 0.36));
  const urlWidth = contentW - symbolSize - columnGap;

  const eyebrow = paragraph('eyebrow', caption.eyebrow, {
    x: pad, width: contentW * 0.8, size: sizes.eyebrow, anchor: 'start',
    fill: ink.accent, maxLines: 1,
  });
  const title = paragraph('title', caption.title, {
    x: pad, width: contentW, size: sizes.title, anchor: 'start',
    fill: ink.body, maxLines: 3,
  });
  const subtitle = paragraph('subtitle', caption.subtitle, {
    x: pad, width: contentW * 0.94, size: sizes.subtitle, anchor: 'start',
    fill: ink.muted, maxLines: 2,
  });
  const url = paragraph('url', caption.url, {
    x: pad, width: urlWidth, size: sizes.url, anchor: 'start',
    fill: ink.body, maxLines: 1,
  });
  /*
   * The wordmark is a signature, not a headline: it is capped well below the
   * card's own URL ramp so a grown poster still reads as DROP IT signing the
   * bottom corner rather than as a second title. Capping it here (instead of in
   * the ramp) is what lets the *title* grow into an A4 without the mark growing
   * with it.
   */
  const brandSize = Math.min(sizes.brand, context.minSide * 0.032);
  const brandH = context.brandMark ? brandMarkHeight(brandSize) : 0;
  const brandW = context.brandMark ? brandMarkWidth(brandSize) : 0;
  // The foot row's two occupants must be separated by the same absolute floor
  // the symbol gets: a 3mm minimum in the estimator is a *target*, and the
  // browser's real glyph advance can be a tenth of a millimetre wider, so the
  // gap is sized from the paper it has to protect rather than from a ratio.
  const footerGap = brandW > 0 ? symbolGap(context, 0.03) : 0;
  const footer = paragraph('footer', caption.footer, {
    x: pad, width: contentW - brandW - footerGap, size: sizes.footer, anchor: 'start',
    fill: ink.muted, maxLines: 1,
  });
  const ruleH = context.divider ? ruleWidth(context.minSide) : 0;

  const copyH = eyebrow.height
    + (title.height > 0 ? captionGap * 0.7 + title.height : 0)
    + (subtitle.height > 0 ? captionGap * 0.9 + subtitle.height : 0);

  const bandHeight = Math.max(symbolSize, url.height);
  // The foot row holds the footer on the left and the wordmark on the right, so
  // its height is the taller of the two and the footer's column is narrowed by
  // the mark's actual width. Without that, a long foot line and a grown ramp
  // overlap — which is what a "measured" layout is supposed to prevent.
  const footH = Math.max(footer.height, brandH);

  /*
   * The poster's copy is distributed over the whole paper, not stacked at the
   * top with a hole under it.
   *
   * A poster is read standing up, so its pieces occupy zones: the eyebrow sits
   * on the top margin, the title hangs just under it, and the subtitle is pushed
   * down toward the band that carries the url and the symbol. The air therefore
   * lands *between* the headline and its explanation, where it reads as
   * separation, instead of as a dead band under everything.
   */
  const fixedHeight = gapSymbol
    + (ruleH > 0 ? captionGap + ruleH : 0)
    + bandHeight
    + (footH > 0 ? gapFoot + footH : 0);
  const subtitlePush = subtitle.height > 0
    ? Math.max(0, (contentH - copyH - fixedHeight) * 0.62)
    : 0;
  const required = copyH
    + subtitlePush
    + gapSymbol
    + (ruleH > 0 ? captionGap + ruleH : 0)
    + bandHeight
    + (footH > 0 ? gapFoot + footH : 0);
  const slack = contentH - required;

  const lines: SceneLine[] = [];
  const rules: SceneRule[] = [];

  let y = pad;
  if (eyebrow.height > 0) {
    lines.push(...place(eyebrow, y));
    y += eyebrow.height + captionGap * 0.7;
  }
  if (title.height > 0) {
    lines.push(...place(title, y));
    y += title.height;
  }
  if (subtitle.height > 0) {
    y += captionGap * 0.9 + subtitlePush;
    lines.push(...place(subtitle, y));
  }

  // The band and the foot hang from the bottom margin, so a poster reads as
  // anchored and the leftover becomes air between the copy and the symbol.
  const footY = context.height - pad;
  const footTop = footY - footH;
  const bandBottom = footH > 0 ? footTop - gapFoot : footY;
  const bandTop = bandBottom - bandHeight;

  if (ruleH > 0) {
    const ruleY = bandTop - captionGap;
    rules.push({
      x1: pad, y1: ruleY, x2: pad + contentW, y2: ruleY,
      width: ruleH, fill: ink.accent,
    });
  }

  lines.push(...place(url, bandTop + (bandHeight - url.height) / 2));

  if (footer.height > 0) lines.push(...place(footer, footTop));

  let brand: SceneBrand | null = null;
  if (brandH > 0) {
    brand = brandMark({
      anchorX: pad + contentW,
      baseline: footTop + brandBaselineOffset(brandSize),
      size: brandSize,
      anchor: 'end',
      fill: ink.body,
      dotFill: ink.accent,
    });
  }

  const cut = cutText(eyebrow, title, subtitle, url, footer);
  return {
    symbol: {
      x: pad + contentW - symbolSize,
      y: bandTop + (bandHeight - symbolSize) / 2,
      size: symbolSize,
    },
    lines,
    rules,
    brand,
    slack: overflowSlack(slack, cut),
    truncated: cut,
  };
}

/* ========================================================================== */
/* Driver                                                                     */
/* ========================================================================== */

/**
 * Symbol floor per layout, as a fraction of the card's shorter side.
 *
 * `stacked` needs the largest floor of the three because it is the layout that
 * spends the most paper on copy: on an 80mm sticker the caption wants ~44mm and
 * a symbol sized "whatever is left" came out at 17mm, which is a code you have
 * to hunt for. 0.3 of the short side (24mm on the sticker, 63mm on an A6) is
 * both comfortably scannable and leaves the caption room to shrink into.
 */
const SYMBOL_FLOOR: Record<RampLayout, number> = {
  stacked: 0.3,
  side: 0.28,
  poster: 0.16,
};

function build(context: Context, sizes: TypeRamp): Built {
  const minSymbol = context.minSide * SYMBOL_FLOOR[context.layout];
  switch (context.layout) {
    case 'stacked':
      return layoutStacked(context, sizes, minSymbol);
    case 'side':
      return isSquareCard(context)
        ? layoutStacked(context, sizes, minSymbol)
        : layoutSide(context, sizes, minSymbol);
    case 'poster':
      return layoutPoster(context, sizes, minSymbol);
  }
}

function clearanceOf(
  symbol: { x: number; y: number; size: number },
  width: number,
  height: number,
): number {
  return Math.min(
    symbol.x,
    symbol.y,
    width - symbol.x - symbol.size,
    height - symbol.y - symbol.size,
  );
}

function makeContext(
  doc: QrDoc,
  width: number,
  height: number,
  layout: RampLayout,
  ink: Ink,
): Context {
  const minSide = Math.min(width, height);
  const pad = cardPadding(minSide);
  return {
    width,
    height,
    minSide,
    t: typeUnit(width, height),
    pad,
    contentW: width - pad * 2,
    contentH: height - pad * 2,
    surface: doc.card.surface,
    ink,
    caption: doc.caption,
    brandMark: doc.card.brandMark,
    divider: doc.card.divider,
    layout,
  };
}

/**
 * Composes the scene for a document on a card of the given size.
 *
 * Searches the ramp from the layout's ceiling down in `SCALE_STEP` steps and
 * keeps the largest scale that both fits and cuts nothing, so a card comes back
 * as the biggest complete composition its own paper can hold.
 */
export function composeCard(
  doc: QrDoc,
  card: { width: number; height: number },
  options: ComposeOptions = {},
): CardScene {
  const { width, height } = card;
  const paintSurface = options.paintSurface !== false;
  const ink: Ink = {
    body: doc.card.ink,
    muted: mutedInk(doc.card.ink, doc.card.surface),
    accent: accentOrInk(doc.card.accent, doc.card.surface, doc.card.ink),
  };

  if (doc.card.layout === 'qr-only') {
    const built = layoutQrOnly(makeContext(doc, width, height, 'stacked', ink));
    return {
      width,
      height,
      surface: doc.card.surface,
      paintSurface,
      symbol: built.symbol,
      lines: [],
      rules: [],
      brand: null,
      fit: { scale: 1, slack: 0 },
      clearance: clearanceOf(built.symbol, width, height),
    };
  }

  const layout = doc.card.layout;
  const context = makeContext(doc, width, height, layout, ink);

  /*
   * Search the ramp from big to small and keep the largest scale that fits.
   *
   * The search starts *above* 1.0, not at it, because the ramp is a ratio, not a
   * size: on an A4 the same proportions are far too timid, and a poster whose
   * title is 20mm on 297mm of paper is not a poster. Starting high and stepping
   * down means every card gets the largest type its own paper can hold, with the
   * ratios intact — which is exactly the "80mm sticker and A4 both look right"
   * requirement, achieved by one rule instead of two ramps.
   *
   * The ceiling differs by layout because the layouts want different things: a
   * poster is supposed to be dominated by its words, while `stacked` has to keep
   * room for a scannable symbol and stops growing sooner.
   */
  const ceiling = SCALE_CEILING[layout];
  let chosen: { built: Built; scale: number } | null = null;
  const steps = Math.round((ceiling - MIN_SCALE) / SCALE_STEP);
  for (let index = 0; index <= steps; index += 1) {
    const scale = ceiling - index * SCALE_STEP;
    const built = build(context, typeSizes(layout, context.t, scale));
    if (built.slack >= -0.001 && !built.truncated) {
      chosen = { built, scale };
      break;
    }
    if (chosen === null) chosen = { built, scale };
  }
  const { built, scale } = chosen ?? {
    built: build(context, typeSizes(layout, context.t, MIN_SCALE)),
    scale: MIN_SCALE,
  };

  return {
    width,
    height,
    surface: doc.card.surface,
    paintSurface,
    symbol: built.symbol,
    lines: built.lines,
    rules: built.rules,
    brand: built.brand,
    fit: { scale, slack: built.slack },
    clearance: clearanceOf(built.symbol, width, height),
  };
}

/** Smallest ramp multiplier the search will accept before giving up. */
const MIN_SCALE = 0.35;

/** Granularity of the ramp search: 1.5% steps are finer than the eye resolves. */
const SCALE_STEP = 0.015;

/**
 * How far above the nominal ramp each layout may grow.
 *
 * Measured, not chosen: at these ceilings the largest card in the set (A4) is
 * using its paper, and one step further starts either to push the symbol below
 * its scannable floor or to crowd the caption against the margin.
 */
const SCALE_CEILING: Record<RampLayout, number> = {
  stacked: 1.5,
  side: 1.5,
  poster: 2.4,
};

/* ========================================================================== */
/* Rendering                                                                  */
/* ========================================================================== */

/** Rounds to a thousandth of a millimetre — finer than any printer resolves. */
export function rnd(value: number): number {
  const rounded = Math.round(value * 1000) / 1000;
  return Object.is(rounded, -0) ? 0 : rounded;
}

function num(value: number): string {
  return String(rnd(value));
}

export interface RenderedScene {
  markup: string;
  defs: string;
}

/**
 * Turns a scene into SVG in millimetres.
 *
 * The symbol is placed with one `translate` + `scale`, exactly as the geometry
 * contract requires, so its module-unit markup is untouched: `k = box / extent`
 * maps the quiet-zone-inclusive square onto the box the layout reserved. That
 * is also what keeps "respect the symbol's quiet zone" true by construction —
 * the box *is* the quiet zone, so the printed modules sit further from the paper
 * edge than the reported clearance.
 */
export function renderScene(
  scene: CardScene,
  symbol: { extent: number; markup: string; defs: string },
  meta: { size: string; layout: string },
): RenderedScene {
  const parts: string[] = [];

  if (scene.paintSurface) {
    parts.push(
      `<rect data-part="surface" x="0" y="0" width="${num(scene.width)}" height="${num(scene.height)}" fill="${escapeXml(scene.surface)}"/>`,
    );
  }

  const scale = symbol.extent > 0 ? scene.symbol.size / symbol.extent : 0;
  parts.push(
    `<g data-part="symbol" transform="translate(${num(scene.symbol.x)} ${num(scene.symbol.y)}) scale(${num(scale)})">${symbol.markup}</g>`,
  );

  for (const rule of scene.rules) {
    parts.push(
      `<line data-part="rule" x1="${num(rule.x1)}" y1="${num(rule.y1)}" x2="${num(rule.x2)}" y2="${num(rule.y2)}" stroke="${escapeXml(rule.fill)}" stroke-width="${num(rule.width)}" stroke-linecap="round"/>`,
    );
  }

  for (const line of scene.lines) {
    const tracking = line.trackingEm > 0
      ? ` letter-spacing="${num(line.trackingEm * line.size)}"`
      : '';
    parts.push(
      `<text data-part="${line.part}" x="${num(line.x)}" y="${num(line.baseline)}"`
      + ` font-family="${FONT_FAMILY}" font-size="${num(line.size)}" font-weight="${line.weight}"${tracking}`
      + ` fill="${escapeXml(line.fill)}" text-anchor="${line.anchor}">${escapeXml(line.text)}</text>`,
    );
  }

  if (scene.brand !== null) {
    const mark = scene.brand;
    const dot = mark.dot === null
      ? ''
      : `<circle data-part="brand-dot" cx="${num(mark.dot.cx)}" cy="${num(mark.dot.cy)}" r="${num(mark.dot.r)}" fill="${escapeXml(mark.dot.fill)}"/>`;
    parts.push(
      `<g data-part="brand">${dot}`
      + `<text x="${num(mark.textX)}" y="${num(mark.baseline)}" font-family="${FONT_FAMILY}"`
      + ` font-size="${num(mark.size)}" font-weight="${BRAND_WEIGHT}" letter-spacing="${num(mark.trackingEm * mark.size)}"`
      + ` fill="${escapeXml(mark.fill)}" text-anchor="start">${BRAND_TEXT}</text></g>`,
    );
  }

  return {
    markup: `<g class="qr-card" data-card-size="${escapeXml(meta.size)}"`
      + ` data-card-layout="${escapeXml(meta.layout)}">${parts.join('')}</g>`,
    defs: symbol.defs,
  };
}
