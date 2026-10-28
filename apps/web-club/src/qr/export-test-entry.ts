/**
 * Test entry point for `export.test.mjs`.
 *
 * Bundles the export module together with the card composer, so the test drives
 * the export paths with the document the studio actually produces rather than a
 * hand-written SVG stub.
 */
export {
  exportSvg, exportRaster, exportPdf, printCard,
  rasterSize, exceedsCanvas, DPI_PRESETS, MAX_CANVAS_SIDE, downloadBlob,
} from './export';
export { encodeQr } from './encode';
export { qrCardSvg, qrCardMarkup, CARD_SIZES } from './card';
export { emptyDoc, PRESETS } from './presets';
