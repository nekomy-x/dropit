/**
 * Test entry point for `geometry.test.mjs`.
 *
 * Exists so esbuild can bundle the real modules into one browser script without
 * the test reaching into `window` globals the app never defines. It exports the
 * *public* surface only — if a test needs something that is not exported here,
 * that is a signal the renderer is being tested through a private door.
 */
export { encodeQr, finderOrigins, inFinder } from './encode';
export { qrSymbolMarkup, qrSymbolSvg, QUIET_MODULES, LOGO_MAX_SCALE, escapeXml } from './geometry';
export { PRESETS, LOGO_GLYPHS, presetById } from './presets';
