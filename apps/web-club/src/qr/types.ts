/**
 * QR Studio — the shared vocabulary between the encoder, the renderers and the
 * studio screen.
 *
 * The studio is a *placing* tool, not a vanity card generator: a club prints one
 * sign per zone, puts it on a bar or a table, and the guest scans it. So every
 * style exists to be scannable first and recognisable second, and the shapes
 * below are the only knobs the screen may expose.
 *
 * Geometry is expressed in **modules** (the QR grid cell), never in pixels: the
 * same design is previewed at 240px and exported at 2048px, and a radius or a
 * gap that were stored in pixels would change the artwork between the two.
 *
 * Decode limits, measured with zbar against generated matrices (see
 * `docs/ui/qr-studio.md`):
 *   - rounded modules (radius ≤ 0.5 of a module) and dots decode at every ECC;
 *   - a logo over the centre decodes up to 28% of the symbol at ECC H;
 *     at 34% it stops decoding, so 0.28 is the ceiling, not a preference;
 *   - light-on-dark inverted symbols do NOT decode on the tested scanners, so
 *     the studio never ships one as a "style" — only the print frame may be
 *     dark, never the symbol.
 */

/** Error correction. H is the only level that tolerates a logo; see above. */
export type Ecc = 'L' | 'M' | 'Q' | 'H';

/** How a single data module is drawn. */
export type ModuleShape =
  /** Square block, hard corners — the classic. */
  | 'square'
  /** Square with 0.5-module corners: the "soft" look. */
  | 'rounded'
  /** Circle inscribed in the module cell. */
  | 'dot'
  /** Square whose outer corners follow the neighbour: joins runs into strokes. */
  | 'connected'
  /** Rounded on the outside of a run, square where it meets a neighbour. */
  | 'liquid';

/** How the three finder patterns (the eyes) are drawn. */
export type EyeFrame =
  | 'square'
  | 'rounded'
  | 'circle'
  /** 7×7 ring replaced by a rounded square with a leaf-shaped cut. */
  | 'leaf';

export type EyeCenter = 'square' | 'rounded' | 'dot';

/** How the data area sits inside the symbol box. */
export type QuietZone = 'tight' | 'standard' | 'air';

export type ColorMode = 'single' | 'dual' | 'gradient';

export interface QrColors {
  /** Data modules (or the start of the gradient). */
  fg: string;
  /** Second colour for `dual` / `gradient`. Ignored for `single`. */
  fg2: string;
  /** Quiet zone and the gaps between modules. Must stay light for scanning. */
  bg: string;
  /** Finder patterns; defaults to `fg` and is kept separate on purpose. */
  eye: string;
  /** Inner 3×3 of the finder patterns. */
  eyeCenter: string;
}

export interface QrStyle {
  modules: ModuleShape;
  /** 0–0.5 of a module; only used by shapes that have a corner to round. */
  moduleRadius: number;
  /** 0–0.15 of a module: the gap between modules. */
  moduleGap: number;
  eyeFrame: EyeFrame;
  eyeCenter: EyeCenter;
  /**
   * Corner rounding of the eye frame, 0–0.5 of the eye's 7-module span.
   *
   * The control offers the full range; the renderer clamps the *drawn* radius to
   * 0.45 because a frame rounded past that stops decoding: at 0.5 the ring
   * becomes a circle, and a circle keeps the finder's 1:1:3:1:1 run only on the
   * two lines through its centre (measured on zbar, jsQR and Apple Vision).
   * `circle` therefore draws as the furthest-rounded squircle, not a true circle.
   */
  eyeRadius: number;
  quietZone: QuietZone;
  colors: QrColors;
  colorMode: ColorMode;
  /** Gradient angle in degrees (0 = left→right, 90 = top→bottom). */
  gradientAngle: number;
  /** Optional centre mark. `null` draws nothing. */
  logo: QrLogo | null;
}

export interface QrLogo {
  /** A data URL (uploaded image) or a preset glyph id. */
  kind: 'image' | 'glyph' | 'text';
  /** For `image`: a data URL. For `glyph`: an id from `LOGO_GLYPHS`. */
  value: string;
  /** Fraction of the symbol's width, 0.1–0.28. Enforced by `logoLimit()`. */
  scale: number;
  /** Fraction of the logo box used as padding inside a clear plate. */
  padding: number;
  /**
   * Corner radius of the clear plate, as a fraction of its side (0–0.5).
   *
   * Expressed as a fraction for the same reason `moduleRadius` and `eyeRadius`
   * are: the same design is previewed at 240px and exported at 2048px, and a
   * radius stored in modules would change the corner between the two. A fraction
   * of the plate also means "round it like the mark", which is what the control
   * actually offers.
   */
  radius: number;
  /** Draw a light plate behind the mark, which restores the modules it covers. */
  plate: boolean;
  plateColor: string;
}

/** The label text that sits beside or under the QR on a printed sign. */
export interface QrCaption {
  /** Small line above the title, e.g. the club name. */
  eyebrow: string;
  /** The headline: "SCAN TO REQUEST". */
  title: string;
  /** The instruction under it. */
  subtitle: string;
  /** The destination printed for people who cannot scan. */
  url: string;
  /** Foot line, e.g. opening hours or "1 request = €5". */
  footer: string;
}

/** How the QR and the caption are composed on the sign. */
export type CardLayout = 'qr-only' | 'stacked' | 'side' | 'poster';

/** Print sizes, in millimetres, that a club can actually put on a table. */
export type CardSize = 'a6' | 'a5' | 'a4' | 'table-tent' | 'sticker' | 'story';

export interface QrDoc {
  /** What the symbol encodes: the room URL or a one-time session token. */
  value: string;
  /** Human-readable destination, kept even when `value` is a bare token. */
  url: string;
  style: QrStyle;
  caption: QrCaption;
  card: {
    layout: CardLayout;
    size: CardSize;
    /** Card surface and ink, independent from the symbol's own colours. */
    surface: string;
    ink: string;
    accent: string;
    /** A saved preset id, or empty for a hand-made look. */
    preset: string;
    /** Show the DROP IT mark on the sign. */
    brandMark: boolean;
    /** Show a thin rule between the symbol and the caption. */
    divider: boolean;
  };
  /** Encoding details that are not cosmetic. */
  encode: {
    ecc: Ecc;
    /** 0 = let the encoder choose the smallest version. */
    maskPattern: number;
  };
}
