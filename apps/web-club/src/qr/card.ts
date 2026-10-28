/**
 * Card composition — the printed sign around a symbol.
 *
 * A club does not print a naked QR: it prints a small sign that says what the
 * code does, so a guest who cannot scan still knows where it points. This
 * module composes the whole sign (surface, symbol, caption block, foot line)
 * in one SVG document, sized in millimetres, so the same document is the screen
 * preview, the PNG export and the print source.
 *
 * Coordinate system: **one unit is one millimetre**. A card declares its own
 * `width`/`height` in mm and every measurement below is in those units, which
 * is what makes a "table tent 100×150mm" actually print at 100×150mm.
 *
 * The figure/sign pair is the contract with `geometry.ts`: the symbol is asked
 * for module-unit markup and placed with one `translate` + `scale`, so a symbol
 * restyle never requires a change here.
 *
 * This file is the public surface (the studio imports only this); the measuring,
 * layout and contrast logic lives in `card-layout.ts`.
 */
import type { QrGrid } from './encode';
import type { CardLayout, QrDoc } from './types';
import { qrSymbolMarkup, QUIET_MODULES, escapeXml } from './geometry';
import { composeCard, renderScene, ruleWidth, type CardScene } from './card-layout';

/** Print sizes in millimetres. `story` is a 9:16 social export, not a print. */
export const CARD_SIZES: Record<QrDoc['card']['size'], { width: number; height: number; label: string }> = {
  a6: { width: 105, height: 148, label: 'A6 · 105 × 148 mm' },
  a5: { width: 148, height: 210, label: 'A5 · 148 × 210 mm' },
  a4: { width: 210, height: 297, label: 'A4 · 210 × 297 mm' },
  'table-tent': { width: 100, height: 150, label: 'Table tent · 100 × 150 mm' },
  sticker: { width: 80, height: 80, label: 'Sticker · 80 × 80 mm' },
  story: { width: 108, height: 192, label: 'Story · 9:16' },
};

export interface CardOptions {
  idPrefix: string;
  /** Emit the card surface fill; off when the caller paints it (print CSS). */
  paintSurface?: boolean;
}

export interface CardMarkup {
  width: number;
  height: number;
  markup: string;
  defs: string;
}

/**
 * Composes the whole sign for a document.
 *
 * Returns markup in millimetres plus the symbol's `<defs>` (gradients, clips),
 * which the caller must emit inside the same `<svg>` — the symbol's ids are
 * prefixed with `idPrefix` so two signs on one page cannot collide.
 */
export function qrCardMarkup(grid: QrGrid, doc: QrDoc, options: CardOptions): CardMarkup {
  const size = CARD_SIZES[doc.card.size];
  const scene = composeCard(doc, size, { paintSurface: options.paintSurface });
  const symbol = qrSymbolMarkup(grid, doc.style, {
    idPrefix: options.idPrefix,
    paintBackground: options.paintSurface !== false,
  });
  const rendered = renderScene(scene, symbol, { size: doc.card.size, layout: doc.card.layout });
  return {
    width: size.width,
    height: size.height,
    markup: rendered.markup,
    defs: rendered.defs,
  };
}

/**
 * The aria label for a sign: the caption's own words, so a screen reader user
 * gets the same instruction a sighted guest reads off the paper.
 */
export function cardAriaLabel(doc: QrDoc, title?: string): string {
  if (title !== undefined && title.trim() !== '') return title.trim();
  const parts = [
    doc.caption.title,
    doc.caption.subtitle,
    doc.caption.url === '' ? '' : doc.caption.url,
  ];
  const label = parts.map((part) => part.trim()).filter((part) => part !== '').join('. ');
  return label === '' ? 'QR code card' : label;
}

/**
 * Composes the sign as a standalone `<svg>` element.
 *
 * The element carries `width="Wmm" height="Hmm"` **and** a `viewBox`, because
 * the two consumers need different halves of that: a browser rasterising it for
 * PNG/PDF needs the intrinsic size to exist at all (a zero-size SVG paints a
 * 0×0 bitmap), and the `viewBox` is what keeps the artwork vector and
 * scale-free. There are no external references — no font file, no image — so the
 * string is the whole document and can be saved, printed or decoded on its own.
 */
export function qrCardSvg(
  grid: QrGrid,
  doc: QrDoc,
  options: CardOptions & { title?: string; className?: string },
): string {
  const card = qrCardMarkup(grid, doc, options);
  const label = escapeXml(cardAriaLabel(doc, options.title));
  const className = options.className === undefined ? '' : ` class="${escapeXml(options.className)}"`;
  const defs = card.defs.trim() === '' ? '' : `<defs>${card.defs}</defs>`;
  return `<svg xmlns="http://www.w3.org/2000/svg"${className} width="${card.width}mm" height="${card.height}mm"`
    + ` viewBox="0 0 ${card.width} ${card.height}" role="img" aria-label="${label}"`
    + ` preserveAspectRatio="xMidYMid meet" data-card-size="${escapeXml(doc.card.size)}"`
    + ` data-card-layout="${escapeXml(doc.card.layout)}">${defs}${card.markup}</svg>`;
}

/**
 * Layout intent per card shape, kept here so the studio can explain the
 * difference to the user instead of offering four unlabelled buttons.
 */
export const CARD_LAYOUTS: ReadonlyArray<{ id: CardLayout; label: string; hint: string }> = [
  { id: 'qr-only', label: 'QR only', hint: 'The symbol alone, with nothing to read. For a frame or a wall where the copy already exists.' },
  { id: 'stacked', label: 'Stacked', hint: 'Symbol on top, caption under it. The default for a table card.' },
  { id: 'side', label: 'Side by side', hint: 'Symbol beside the copy. Reads left-to-right and fits a wide surface.' },
  { id: 'poster', label: 'Poster', hint: 'Large caption, small symbol. For a door or a wall, read from a distance.' },
];

/** Re-exported for the studio: the layout may want the scene for hit-testing. */
export type { CardScene };
export { ruleWidth };

export { escapeXml, QUIET_MODULES };
