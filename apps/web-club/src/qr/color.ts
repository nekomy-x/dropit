/**
 * Colour maths for the studio — the part that decides whether a design is
 * printable, not just whether it is pretty.
 *
 * Two rules from the measured decode tests drive this file:
 *
 *   - a symbol needs **dark modules on light paper**. An inverted symbol failed
 *     to decode on every scanner tried, so the studio treats "ink lighter than
 *     paper" as an error it explains, not a theme it offers;
 *   - club lighting costs contrast, so the working target for the symbol is
 *     7:1, not the 4.5:1 that suffices for body text on a screen. Body copy on
 *     the sign keeps the ordinary 4.5:1 AA floor.
 */

export interface Rgb {
  r: number;
  g: number;
  b: number;
}

/** `#abc`, `#aabbcc` and a leading-`#`-less form are all accepted. */
export function parseHex(value: string): Rgb | null {
  const hex = value.trim().replace(/^#/, '');
  const full = hex.length === 3 ? hex.split('').map((char) => char + char).join('') : hex;
  if (!/^[0-9a-f]{6}$/i.test(full)) return null;
  return {
    r: parseInt(full.slice(0, 2), 16),
    g: parseInt(full.slice(2, 4), 16),
    b: parseInt(full.slice(4, 6), 16),
  };
}

export function toHex({ r, g, b }: Rgb): string {
  const channel = (v: number) => Math.max(0, Math.min(255, Math.round(v))).toString(16).padStart(2, '0');
  return `#${channel(r)}${channel(g)}${channel(b)}`.toUpperCase();
}

/** WCAG 2.1 relative luminance. */
export function luminance(color: Rgb): number {
  const channel = (value: number) => {
    const s = value / 255;
    return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * channel(color.r) + 0.7152 * channel(color.g) + 0.0722 * channel(color.b);
}

/** WCAG 2.1 contrast ratio, 1–21. Returns 1 when either colour is unreadable. */
export function contrastRatio(a: string, b: string): number {
  const left = parseHex(a);
  const right = parseHex(b);
  if (!left || !right) return 1;
  const la = luminance(left);
  const lb = luminance(right);
  return (Math.max(la, lb) + 0.05) / (Math.min(la, lb) + 0.05);
}

/** True when the first colour is darker than the second. */
export function isDarker(a: string, b: string): boolean {
  const left = parseHex(a);
  const right = parseHex(b);
  if (!left || !right) return true;
  return luminance(left) < luminance(right);
}

/**
 * Picks whichever of `ink` / `surface` reads better on `surface`.
 *
 * Used by the card composer when a user's accent colour cannot carry text: the
 * sign falls back to plain ink instead of printing an unreadable line.
 */
export function readableOn(ink: string, surface: string, minimum = 4.5): string {
  if (contrastRatio(ink, surface) >= minimum) return ink;
  return isDarker('#000000', surface) ? '#0B1210' : '#FFFFFF';
}

/** Rounds a ratio for display: `18.9`, never `18.950000001`. */
export function formatRatio(ratio: number): string {
  return ratio.toFixed(1);
}
