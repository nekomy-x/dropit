declare module 'qrcode/lib/core/qrcode' {
  /**
   * The encoder only — `qrcode`'s public entry point is a canvas/SVG renderer,
   * which the studio does not use (it draws the symbol itself so the module
   * shapes, eyes and logo are under its control). The core module has no types
   * of its own, and it is the same file the package resolves in a browser.
   */
  export interface QrMatrix {
    size: number;
    /** 1 = dark module. The API is `get(row, col)` whether or not called. */
    get(row: number, col: number): number;
  }
  export interface QrSymbol {
    /** 1–40: the QR version, derived from the data length and ECC. */
    version: number;
    /** 0–7: the mask that was applied. */
    maskPattern: number;
    modules: QrMatrix;
  }
  export interface QrCoreOptions {
    errorCorrectionLevel?: 'L' | 'M' | 'Q' | 'H';
    /** Pin the version (`1`–`40`) instead of letting the encoder choose. */
    version?: number;
    maskPattern?: 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7;
  }
  export function create(data: string, options?: QrCoreOptions): QrSymbol;
}

declare module 'qrcode/lib/core/bit-matrix' {
  export default class BitMatrix {
    size: number;
    constructor(size: number);
    set(row: number, col: number, value: boolean, reserved?: boolean): void;
    get(row: number, col: number): number;
    xor(row: number, col: number, value: boolean): void;
  }
}
