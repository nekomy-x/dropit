/// <reference types="vite/client" />
/**
 * Export — one document, four outputs.
 *
 * The studio never re-renders for export: it takes the same SVG string the
 * preview shows and converts it, so what an operator checked on screen is what
 * the printer gets. All paths are synchronous except the raster ones, which
 * wait on an image decode.
 *
 * Formats and why each exists:
 *   `svg`  the vector master — what a print shop asks for;
 *   `png`  raster at a chosen DPI, for a screen, a slide or a phone photo;
 *   `pdf`  a one-page document sized in millimetres, for office printers;
 *   `print` the browser's own dialog, which is the only way to hit the real
 *           printer margins of a club's printer.
 *
 * Everything is client-side. There is no upload endpoint and the room token
 * must not leave the operator's browser.
 *
 * Implementation notes that are decisions rather than accidents:
 *
 * 1. **The raster honours the requested pixel size, not the SVG's units.** A
 *    card declares `width="105mm" height="148mm"`, which Chromium decodes to
 *    ~397×559 CSS px at 96dpi — but 96dpi is a screen assumption, not a print
 *    one. The caller passes the exact `widthPx`/`heightPx` it wants for the
 *    chosen DPI (`rasterSize()` above computes them), so the image is stretched
 *    to those dimensions. Neither dimension is distorted, because the caller
 *    derives both from the same card's millimetre aspect ratio.
 *
 * 2. **A blob URL is released after the draw, never before it.** Chromium keeps
 *    reading from the `Image` while `drawImage` runs, so revoking the URL in the
 *    `load` handler races the first paint and yields a blank canvas. It is
 *    released in `finally` instead.
 *
 * 3. **`toBlob` reports failure by passing `null`.** At very large sizes the
 *    browser refuses to encode and calls back with `null` rather than throwing.
 *    That becomes a rejected promise with a readable message; an `undefined`
 *    blob would otherwise fail later, inside the download handler, where the
 *    reason is lost.
 *
 * 4. **The PDF is written by hand.** There is no PDF library in this app and
 *    adding one for a single image page would cost more than the ~90 lines
 *    below. The only thing that must not be approximate is the cross-reference
 *    table: it stores the byte offset of every object, so offsets are measured
 *    while the file is assembled rather than computed from string lengths that
 *    change once bytes are encoded.
 */
import type { CardSize } from './types';
import { CARD_SIZES } from './card';
import printStylesheet from './export.css?raw';

export type ExportFormat = 'svg' | 'png' | 'jpg' | 'pdf';

export interface ExportResult {
  blob: Blob;
  filename: string;
  /** Raster outputs report it so the UI can warn about a small file. */
  widthPx?: number;
  heightPx?: number;
}

/** Dots per inch a raster export is rendered at. */
export const DPI_PRESETS = [150, 300, 600] as const;

/**
 * A PNG at `dpi` needs `mm / 25.4 * dpi` pixels. A4 at 600dpi is 4961×7016,
 * which is a 34 MP canvas — over the browser's safe canvas cap on mobile, so
 * the caller is expected to check `rasterSize()` against `MAX_CANVAS_SIDE`.
 */
export const MAX_CANVAS_SIDE = 8192;

/** One millimetre in PostScript points: 72 / 25.4. */
const MM_TO_PT = 2.834645669291339;

/** JPEG quality. High enough that module edges stay crisp through the DCT. */
const JPEG_QUALITY = 0.92;

/** Paper for formats that cannot carry transparency (JPEG inside the PDF). */
const PAPER = '#FFFFFF';

/** How long `printCard()` waits for `afterprint` before settling anyway. */
const PRINT_TIMEOUT_MS = 60_000;

/** Fallback when a caller passes a `dpi` that is not a usable number. */
const DEFAULT_DPI = 300;

export function rasterSize(size: CardSize, dpi: number): { width: number; height: number } {
  const card = CARD_SIZES[size];
  return {
    width: Math.round((card.width / 25.4) * dpi),
    height: Math.round((card.height / 25.4) * dpi),
  };
}

/** True when the raster export would exceed a safe canvas. */
export function exceedsCanvas(size: CardSize, dpi: number): boolean {
  const { width, height } = rasterSize(size, dpi);
  return width > MAX_CANVAS_SIDE || height > MAX_CANVAS_SIDE;
}

export function downloadBlob(blob: Blob, filename: string): void {
  const url = URL.createObjectURL(blob);
  const link = document.createElement('a');
  link.href = url;
  link.download = filename;
  link.rel = 'noopener';
  document.body.append(link);
  link.click();
  link.remove();
  // Revoking immediately can cancel the download in Safari; one frame is enough.
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

// --- shared plumbing --------------------------------------------------------

/** Decodes an SVG string into a loaded `Image`, with a handle to release it. */
function loadSvgImage(svg: string): Promise<{ image: HTMLImageElement; release: () => void }> {
  return new Promise((resolve, reject) => {
    const url = URL.createObjectURL(new Blob([svg], { type: 'image/svg+xml;charset=utf-8' }));
    const image = new Image();
    image.decoding = 'sync';
    let settled = false;

    image.onload = () => {
      if (settled) return;
      settled = true;
      resolve({ image, release: () => URL.revokeObjectURL(url) });
    };
    image.onerror = () => {
      if (settled) return;
      settled = true;
      URL.revokeObjectURL(url);
      reject(new Error('The drawing could not be read as an image. The SVG may be empty or malformed.'));
    };
    image.src = url;
  });
}

/** Rasterises the SVG into a canvas of exactly `width`×`height` CSS px. */
async function rasterise(
  svg: string,
  width: number,
  height: number,
  background: string | null,
): Promise<HTMLCanvasElement> {
  if (!Number.isFinite(width) || !Number.isFinite(height) || width < 1 || height < 1) {
    throw new Error('The requested image size is not usable. Pick another resolution.');
  }
  const { image, release } = await loadSvgImage(svg);
  try {
    const canvas = document.createElement('canvas');
    canvas.width = Math.round(width);
    canvas.height = Math.round(height);
    const context = canvas.getContext('2d');
    if (!context) throw new Error('This browser could not create a 2D canvas for the export.');
    // A JPEG has no alpha, and a transparent PNG comes out black in some
    // viewers: both are painted onto the surface the caller asked for.
    if (background !== null) {
      context.fillStyle = background;
      context.fillRect(0, 0, canvas.width, canvas.height);
    }
    context.imageSmoothingEnabled = true;
    context.imageSmoothingQuality = 'high';
    context.drawImage(image, 0, 0, canvas.width, canvas.height);
    return canvas;
  } finally {
    release();
  }
}

/** `canvas.toBlob` as a promise; rejects instead of resolving `null`. */
function canvasToBlob(canvas: HTMLCanvasElement, type: string, quality?: number): Promise<Blob> {
  return new Promise((resolve, reject) => {
    canvas.toBlob(
      (blob) => {
        if (blob) {
          resolve(blob);
        } else {
          reject(new Error('The image is too large for this browser to encode. Lower the resolution, or download the SVG.'));
        }
      },
      type,
      quality,
    );
  });
}

// --- SVG --------------------------------------------------------------------

export function exportSvg(svg: string, filename: string): ExportResult {
  return {
    blob: new Blob([svg], { type: 'image/svg+xml;charset=utf-8' }),
    filename,
  };
}

// --- raster -----------------------------------------------------------------

export async function exportRaster(svg: string, options: {
  format: 'png' | 'jpg';
  widthPx: number;
  heightPx: number;
  filename: string;
  background: string;
}): Promise<ExportResult> {
  const { format, widthPx, heightPx, filename, background } = options;
  const canvas = await rasterise(svg, widthPx, heightPx, background);
  const isJpeg = format === 'jpg';
  const blob = await canvasToBlob(
    canvas,
    isJpeg ? 'image/jpeg' : 'image/png',
    isJpeg ? JPEG_QUALITY : undefined,
  );
  return {
    blob,
    filename,
    // The PDF and the UI both report the real pixel count, so it is measured
    // from the canvas rather than echoed from the request.
    widthPx: canvas.width,
    heightPx: canvas.height,
  };
}

// --- PDF --------------------------------------------------------------------

/**
 * Reads the real pixel size and colour component count out of a JPEG's SOF
 * marker.
 *
 * The PDF dictionary has to describe the image exactly — a wrong `/Width` makes
 * a viewer lay the scan lines out on the wrong stride and the page renders as
 * diagonal noise — so the values are parsed from the bytes that are actually
 * embedded instead of being assumed from the canvas that produced them.
 */
function jpegInfo(bytes: Uint8Array): { width: number; height: number; components: number } {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  if (view.byteLength < 4 || view.getUint8(0) !== 0xff || view.getUint8(1) !== 0xd8) {
    throw new Error('The PDF image is not a JPEG.');
  }
  let offset = 2;
  while (offset + 3 < view.byteLength) {
    if (view.getUint8(offset) !== 0xff) {
      offset += 1;
      continue;
    }
    const marker = view.getUint8(offset + 1);
    // Standalone markers carry no length field.
    if (marker === 0xd8 || marker === 0xd9 || marker === 0x01 || (marker >= 0xd0 && marker <= 0xd7)) {
      offset += 2;
      continue;
    }
    const length = view.getUint16(offset + 2);
    // SOF0…SOF15, excluding DHT (0xC4), JPG (0xC8) and DAC (0xCC).
    const isStartOfFrame = marker >= 0xc0 && marker <= 0xcf
      && marker !== 0xc4 && marker !== 0xc8 && marker !== 0xcc;
    if (isStartOfFrame) {
      if (offset + 9 >= view.byteLength) break;
      return {
        height: view.getUint16(offset + 5),
        width: view.getUint16(offset + 7),
        components: view.getUint8(offset + 9),
      };
    }
    offset += 2 + length;
  }
  throw new Error('The PDF image has no frame header.');
}

/** Escapes a string for a PDF literal string, or switches to UTF-16BE hex. */
function pdfText(value: string): string {
  if (/^[\x20-\x7e]*$/.test(value)) {
    return `(${value.replace(/[\\()]/g, (match) => `\\${match}`)})`;
  }
  // Non-ASCII: a UTF-16BE hex string with a byte-order mark is unambiguous.
  let hex = 'FEFF';
  for (const unit of value) {
    const code = unit.codePointAt(0) ?? 0x20;
    if (code > 0xffff) {
      const shifted = code - 0x10000;
      hex += (0xd800 + (shifted >> 10)).toString(16).toUpperCase().padStart(4, '0');
      hex += (0xdc00 + (shifted & 0x3ff)).toString(16).toUpperCase().padStart(4, '0');
    } else {
      hex += code.toString(16).toUpperCase().padStart(4, '0');
    }
  }
  return `<${hex}>`;
}

/** One numbered PDF object: a dictionary head, plus optional binary stream. */
interface PdfObject {
  id: number;
  head: string;
  stream?: Uint8Array;
}

/**
 * Assembles a PDF 1.4 file from numbered objects.
 *
 * The cross-reference table stores the **byte offset of each object** from the
 * start of the file, and a viewer rejects the document outright when those
 * offsets are wrong. They are therefore measured while the file is being
 * written — the only reliable way, since text bytes, JPEG bytes and the xref
 * itself all contribute and are not interchangeable by string length once a
 * `Uint8Array` is in play.
 */
function buildPdf(objects: readonly PdfObject[]): Blob {
  const encoder = new TextEncoder();
  // Typed as `BlobPart[]` on purpose: a `Uint8Array` may be backed by a
  // `SharedArrayBuffer` in the DOM lib's types, which `Blob` does not accept.
  // `Blob` copies the bytes, so no view into a shared buffer escapes.
  const chunks: BlobPart[] = [];
  let offset = 0;

  const write = (part: string | Uint8Array): void => {
    if (typeof part === 'string') {
      // `Blob` encodes a string as UTF-8, which is exactly what `TextEncoder`
      // measures, so the offset stays byte-accurate without a second copy.
      chunks.push(part);
      offset += encoder.encode(part).byteLength;
      return;
    }
    // `slice()` yields a `Uint8Array<ArrayBuffer>` whose buffer this code owns.
    // A bare `Uint8Array` is typed over `ArrayBufferLike` and could be a view
    // into a `SharedArrayBuffer`, which `BlobPart` does not accept.
    const bytes = part.slice();
    chunks.push(bytes);
    offset += bytes.byteLength;
  };

  write('%PDF-1.4\n');

  const offsets = new Map<number, number>();
  const ordered = [...objects].sort((a, b) => a.id - b.id);
  for (const object of ordered) {
    offsets.set(object.id, offset);
    const length = object.stream ? `\n/Length ${object.stream.byteLength}` : '';
    write(`${object.id} 0 obj\n<< ${object.head}${length} >>\n`);
    if (object.stream) {
      write('stream\n');
      write(object.stream);
      write('\nendstream\n');
    }
    write('endobj\n');
  }

  const xrefOffset = offset;
  const maxId = ordered.reduce((max, object) => Math.max(max, object.id), 0);
  let xref = `xref\n0 ${maxId + 1}\n0000000000 65535 f \n`;
  for (let id = 1; id <= maxId; id += 1) {
    const at = offsets.get(id);
    xref += at === undefined
      ? '0000000000 65535 f \n'
      : `${String(at).padStart(10, '0')} 00000 n \n`;
  }
  xref += `trailer\n<< /Size ${maxId + 1} /Root 1 0 R /Info 4 0 R >>\nstartxref\n${xrefOffset}\n%%EOF\n`;
  write(xref);

  return new Blob(chunks, { type: 'application/pdf' });
}

export async function exportPdf(svg: string, options: {
  size: CardSize;
  filename: string;
  title: string;
  /** Rasterise the card inside the PDF (vector PDF is not available without a library). */
  dpi: number;
}): Promise<ExportResult> {
  const { size, filename, title } = options;
  const dpi = Number.isFinite(options.dpi) && options.dpi > 0 ? options.dpi : DEFAULT_DPI;
  const card = CARD_SIZES[size];
  const { width: widthPx, height: heightPx } = rasterSize(size, dpi);

  const canvas = await rasterise(svg, widthPx, heightPx, PAPER);
  const jpegBlob = await canvasToBlob(canvas, 'image/jpeg', JPEG_QUALITY);
  const jpeg = new Uint8Array(await jpegBlob.arrayBuffer());
  const info = jpegInfo(jpeg);

  // The page is the card, in points; the image fills it exactly.
  const pageWidth = +(card.width * MM_TO_PT).toFixed(4);
  const pageHeight = +(card.height * MM_TO_PT).toFixed(4);
  // `cm` scales the unit square to the full page and `Do` paints the image
  // into it: the JPEG fills the page with no margin, which is what a card is.
  const content = new TextEncoder().encode(`q\n${pageWidth} 0 0 ${pageHeight} 0 0 cm\n/Im0 Do\nQ\n`);

  const objects: PdfObject[] = [
    { id: 1, head: '/Type /Catalog /Pages 2 0 R' },
    { id: 2, head: '/Type /Pages /Kids [3 0 R] /Count 1' },
    {
      id: 3,
      head: `/Type /Page /Parent 2 0 R /MediaBox [0 0 ${pageWidth} ${pageHeight}] `
        + '/Resources << /XObject << /Im0 5 0 R >> >> /Contents 6 0 R',
    },
    {
      id: 4,
      head: `/Title ${pdfText(title)} /Producer (DROP IT QR studio) /Creator (DROP IT)`,
    },
    {
      id: 5,
      // `/Width`, `/Height` and `/ColorSpace` describe the JPEG that is actually
      // embedded, parsed above, so they cannot disagree with the stream.
      head: `/Type /XObject /Subtype /Image /Width ${info.width} /Height ${info.height} `
        + `/ColorSpace ${info.components === 1 ? '/DeviceGray' : '/DeviceRGB'} `
        + '/BitsPerComponent 8 /Filter /DCTDecode',
      stream: jpeg,
    },
    {
      id: 6,
      head: '',
      stream: content,
    },
  ];

  return {
    blob: buildPdf(objects),
    filename,
    widthPx,
    heightPx,
  };
}

// --- print ------------------------------------------------------------------

/**
 * Opens the browser print dialog on a document that contains only the card,
 * sized to the page. Returns a promise that settles when the dialog closes.
 *
 * An **iframe** is used rather than `window.open`: a popup can be blocked, and
 * when it is not, it leaves the operator with a second tab they have to close
 * after printing. An iframe is same-document-synchronous — the SVG is parsed
 * inline, so there is no network load to wait on and no chance of printing an
 * empty page — and it is removed in `finally` whether printing succeeded, was
 * cancelled, or threw.
 *
 * The promise always settles: `afterprint` is the real signal, but a browser
 * that never fires it (or a dialog the user leaves open) must not strand the
 * caller's "preparing…" state, so a timeout resolves it as well.
 */
export function printCard(svg: string, size: CardSize): Promise<void> {
  return new Promise<void>((resolve, reject) => {
    const card = CARD_SIZES[size];
    const width = Number.isFinite(card.width) ? card.width : 100;
    const height = Number.isFinite(card.height) ? card.height : 150;

    const iframe = document.createElement('iframe');
    // The frame must be on-screen-sized to lay out at the card's millimetres;
    // `display:none` makes printers emit a blank page in Chromium.
    iframe.setAttribute('aria-hidden', 'true');
    iframe.setAttribute('title', 'Print preview');
    iframe.style.position = 'fixed';
    iframe.style.left = '-10000px';
    iframe.style.top = '0';
    iframe.style.width = `${width}mm`;
    iframe.style.height = `${height}mm`;
    iframe.style.border = '0';
    iframe.style.visibility = 'hidden';

    let settled = false;
    let timer: ReturnType<typeof setTimeout> | null = null;
    let frameWindow: Window | null = null;

    const cleanup = (): void => {
      if (timer !== null) {
        clearTimeout(timer);
        timer = null;
      }
      if (frameWindow) {
        frameWindow.removeEventListener('afterprint', onAfterPrint);
        frameWindow = null;
      }
      iframe.remove();
    };

    const finish = (error?: Error): void => {
      if (settled) return;
      settled = true;
      cleanup();
      if (error) reject(error);
      else resolve();
    };

    function onAfterPrint(): void {
      finish();
    }

    try {
      document.body.append(iframe);
      const frameDocument = iframe.contentDocument;
      const frame = iframe.contentWindow;
      if (!frameDocument || !frame) {
        finish(new Error('The print view could not be created. Allow this page to open print previews and try again.'));
        return;
      }
      frameWindow = frame;

      const pageRule = `@page { size: ${width}mm ${height}mm; margin: 0; }`;
      frameDocument.open();
      frameDocument.write(
        '<!doctype html><html><head><meta charset="utf-8">'
        + `<style>${pageRule}\n${printStylesheet}</style>`
        + '</head><body><div class="qr-print-card">'
        + svg
        + '</div></body></html>',
      );
      frameDocument.close();

      frame.addEventListener('afterprint', onAfterPrint);
      timer = setTimeout(() => finish(), PRINT_TIMEOUT_MS);

      // Give the frame one layout pass so the inline SVG has measured itself
      // before the dialog snapshots the page.
      frame.requestAnimationFrame(() => {
        frame.requestAnimationFrame(() => {
          if (settled) return;
          try {
            frame.focus();
            frame.print();
          } catch (error) {
            finish(error instanceof Error ? error : new Error('The print dialog could not be opened.'));
          }
        });
      });
    } catch (error) {
      finish(error instanceof Error ? error : new Error('The print dialog could not be opened.'));
    }
  });
}
