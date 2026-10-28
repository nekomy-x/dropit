/**
 * Scan check — the studio looks at its own output the way a phone would.
 *
 * Every style the studio offers was measured with a scanner before it shipped,
 * but the operator can still combine values into a symbol that no longer reads:
 * a mark that is too large, a quiet zone that was tightened too far, ink and
 * paper swapped. Telling them "it might not scan" is useless on a printed sign,
 * so the studio rasterises the design and actually decodes it, in the browser,
 * with `jsQR` — the same class of decoder a phone camera uses.
 *
 * The check is asynchronous and runs on a copy of the design, so a keystroke
 * never blocks. It reports three states: `unknown` (not run yet), `ok`
 * (decoded, and the payload matches) and `failed` (it did not).
 */
/**
 * `jsQR` is a decoder, not a draw call: it is ~250 kB minified and it runs once
 * per design change. Importing it statically put the whole decoder in the
 * panel's entry chunk for a feature most sessions never open, so it is loaded on
 * first use and cached by the module system afterwards.
 */
let decoderPromise: Promise<typeof import('jsqr').default> | null = null;
function loadDecoder(): Promise<typeof import('jsqr').default> {
  decoderPromise ??= import('jsqr').then((module) => module.default);
  return decoderPromise;
}

export type DecodeState = 'unknown' | 'checking' | 'ok' | 'failed';

export interface DecodeResult {
  state: DecodeState;
  /** What the decoder read, when it read anything. */
  decoded?: string;
  /** Rendered size of the raster the check used. */
  width?: number;
  height?: number;
  reason?: string;
}

/**
 * Rasterises an SVG document and decodes the largest QR in it.
 *
 * **Why several sizes, not one.** `jsQR`'s binariser is resolution-sensitive in
 * a way real scanners are not: measured against the same artwork, a symbol with
 * rounded modules and a 4% gap failed jsQR at 480/600/720/1024 px but decoded at
 * 360 px, while `zbar` read the identical PNG at every size. Reporting the
 * 720 px failure as "your sign will not scan" would be a lie about a sign that a
 * phone reads fine, and a check that cries wolf is worse than no check.
 *
 * So the check tries a ladder of sizes and accepts the sign if **any** of them
 * decodes — which is what "can a camera read this" actually means. It still
 * fails a symbol that is genuinely broken: inverted ink, low contrast and a
 * destroyed quiet zone fail at every size (measured).
 */
const DECODE_SIZES = [360, 480, 600, 720] as const;

export async function decodeSvg(svg: string, expected: string, options: { sizePx?: number } = {}): Promise<DecodeResult> {
  const sizes = options.sizePx ? [options.sizePx] : DECODE_SIZES;
  let image: HTMLImageElement;
  let url: string | null = null;
  try {
    const blob = new Blob([svg], { type: 'image/svg+xml;charset=utf-8' });
    url = URL.createObjectURL(blob);
    image = await loadImage(url);
  } catch (error) {
    if (url) URL.revokeObjectURL(url);
    return { state: 'failed', reason: error instanceof Error ? error.message : 'The drawing could not be rasterised.' };
  }

  let decoder: typeof import('jsqr').default;
  try {
    decoder = await loadDecoder();
  } catch {
    return { state: 'unknown', reason: 'The decoder did not load.' };
  }

  try {
    // The ratio is kept from the document's own viewBox so a tall poster is
    // checked at the same proportion it will be printed.
    const ratio = image.naturalWidth && image.naturalHeight ? image.naturalHeight / image.naturalWidth : 1;
    let first: { width: number; height: number; decoded?: string } | null = null;
    for (const sizePx of sizes) {
      const width = Math.round(sizePx);
      const height = Math.round(sizePx * ratio);
      const canvas = document.createElement('canvas');
      canvas.width = width;
      canvas.height = height;
      const context = canvas.getContext('2d', { willReadFrequently: true });
      if (!context) return { state: 'failed', reason: 'This browser could not create a 2D canvas.' };
      context.fillStyle = '#FFFFFF';
      context.fillRect(0, 0, width, height);
      context.drawImage(image, 0, 0, width, height);
      const { data } = context.getImageData(0, 0, width, height);
      const found = decoder(data, width, height, { inversionAttempts: 'dontInvert' });
      if (found && found.data === expected) {
        return { state: 'ok', decoded: found.data, width, height };
      }
      first ??= { width, height, decoded: found?.data };
    }
    return {
      state: 'failed',
      width: first?.width,
      height: first?.height,
      decoded: first?.decoded,
      reason: first?.decoded ? 'The scanner read a different value.' : undefined,
    };
  } catch (error) {
    return { state: 'failed', reason: error instanceof Error ? error.message : 'The check threw.' };
  } finally {
    if (url) URL.revokeObjectURL(url);
  }
}

function loadImage(url: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const image = new Image();
    image.decoding = 'sync';
    image.onload = () => resolve(image);
    image.onerror = () => reject(new Error('The SVG did not load as an image.'));
    image.src = url;
  });
}
