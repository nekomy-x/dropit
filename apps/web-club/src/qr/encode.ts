/**
 * The encoder boundary — the only file that knows how a string becomes a grid.
 *
 * It wraps `qrcode`'s core (no renderer): the studio draws the symbol itself so
 * that module shapes, eyes and the logo plate are its own decisions rather than
 * a post-processing filter. Keeping the import in one module means the rest of
 * the feature never sees a third-party shape, and the decoder-facing contract
 * (`size`, `isDark`) stays stable if the encoder is ever swapped.
 */
import { create } from 'qrcode/lib/core/qrcode';
import type { Ecc } from './types';

export interface QrGrid {
  /** Modules per side: 21 for version 1, +4 per version. */
  size: number;
  /** 1–40. */
  version: number;
  /** 0–7; the mask the encoder applied. */
  maskPattern: number;
  /** `dark(row, col)` — 1 when the module is dark. */
  dark: (row: number, col: number) => boolean;
}

export class QrEncodeError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'QrEncodeError';
  }
}

/**
 * Encodes `value` into a module grid.
 *
 * Throws `QrEncodeError` when the payload cannot fit: the studio reports the
 * length and the version limit instead of rendering an empty box, because a
 * blank QR on a printed sign is worse than a refused export.
 */
export function encodeQr(
  value: string,
  { ecc, maskPattern }: { ecc: Ecc; maskPattern?: number },
): QrGrid {
  if (value.trim() === '') {
    throw new QrEncodeError('Empty value: there is nothing to encode.');
  }
  const mask = typeof maskPattern === 'number' && maskPattern >= 0 && maskPattern <= 7
    ? (maskPattern as 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7)
    : undefined;
  try {
    const symbol = create(value, { errorCorrectionLevel: ecc, maskPattern: mask });
    const { size } = symbol.modules;
    return {
      size,
      version: symbol.version,
      maskPattern: symbol.maskPattern,
      dark: (row: number, col: number) => symbol.modules.get(row, col) === 1,
    };
  } catch (error) {
    const reason = error instanceof Error ? error.message : String(error);
    throw new QrEncodeError(
      reason.includes('too big')
        ? `This value is ${value.length} characters and does not fit at error correction ${ecc}. Shorten the link or lower the correction level.`
        : reason,
    );
  }
}

/**
 * The three finder patterns, as the top-left corner of a 7×7 block.
 *
 * Separators and the format-information strip around an eye are *not* part of
 * the eye: they are data-bearing and must be drawn with the module shape, or a
 * scanner loses the quiet ring that tells it where the eye ends.
 */
export function finderOrigins(size: number): ReadonlyArray<[number, number]> {
  return [
    [0, 0],
    [0, size - 7],
    [size - 7, 0],
  ];
}

/** True when `(row, col)` is inside one of the 7×7 finder blocks. */
export function inFinder(size: number, row: number, col: number): boolean {
  const last = size - 7;
  return (row < 7 && col < 7)
    || (row < 7 && col >= last)
    || (row >= last && col < 7);
}
