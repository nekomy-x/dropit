/**
 * QR Studio presets — the styles an operator picks from, and the only place the
 * studio's look is decided.
 *
 * Two rules shape this file:
 *
 * 1. **A sign is print, a panel is product.** The palette below is not the token
 *    layer: a printed card is paper and ink, and it must survive a phone camera
 *    under club lighting. Every pair here keeps the symbol ink/paper ratio at
 *    roughly 8:1 or better, which is what scanners actually need; the panel
 *    around the studio keeps using `--c-*` tokens.
 *
 * 2. **A preset is a starting point, not a lock.** Every value a preset sets is
 *    editable in the studio; the preset only saves the user from assembling a
 *    look from six controls. `preset: ''` therefore means "hand-made", and the
 *    studio reports it as such instead of pretending it is still Ink.
 *
 * Nothing here inverts the symbol (light ink on dark paper): measured with zbar,
 * an inverted symbol did not decode on the tested scanners even at high
 * correction, so "dark mode QR" is offered only as a *card* background with a
 * light quiet zone, never as an inverted symbol.
 */
import type { QrStyle, QrDoc } from './types';

export interface Preset {
  id: string;
  /** Shown as the tile label. Kept short — it sits under a 96px preview. */
  name: string;
  /** One line of what it is for. */
  hint: string;
  /** Style applied when selected. Copied, never shared. */
  style: QrStyle & { logo: null };
}

const base = {
  moduleRadius: 0.5,
  moduleGap: 0,
  eyeFrame: 'square',
  eyeCenter: 'square',
  eyeRadius: 0.42,
  quietZone: 'standard',
  colorMode: 'single',
  gradientAngle: 90,
} as const;

export const PRESETS: ReadonlyArray<Preset> = [
  {
    id: 'ink',
    name: 'Ink',
    hint: 'Hard squares, one colour. The safest print and the cheapest to reproduce at any size.',
    style: {
      ...base,
      modules: 'square',
      moduleRadius: 0,
      moduleGap: 0,
      eyeFrame: 'square',
      eyeCenter: 'square',
      eyeRadius: 0,
      colors: { fg: '#0B1210', fg2: '#0B1210', bg: '#FFFFFF', eye: '#0B1210', eyeCenter: '#0B1210' },
      logo: null,
    },
  },
  {
    id: 'soft',
    name: 'Soft',
    hint: 'Rounded modules and eyes. Reads modern on a table card without losing contrast.',
    style: {
      ...base,
      modules: 'rounded',
      moduleRadius: 0.5,
      moduleGap: 0.04,
      eyeFrame: 'rounded',
      eyeCenter: 'rounded',
      eyeRadius: 0.46,
      colors: { fg: '#101816', fg2: '#101816', bg: '#FFFFFF', eye: '#101816', eyeCenter: '#101816' },
      logo: null,
    },
  },
  {
    id: 'liquid',
    name: 'Liquid',
    hint: 'Modules join into strokes at their edges. The most "designed" look that still prints clean.',
    style: {
      ...base,
      modules: 'liquid',
      moduleRadius: 0.5,
      moduleGap: 0.03,
      eyeFrame: 'leaf',
      eyeCenter: 'dot',
      eyeRadius: 0.5,
      colors: { fg: '#0A1F18', fg2: '#0E7A54', bg: '#FFFFFF', eye: '#0A1F18', eyeCenter: '#0E7A54' },
      logo: null,
      colorMode: 'gradient',
      gradientAngle: 115,
    },
  },
  {
    id: 'dots',
    name: 'Dots',
    hint: 'Circular modules. Friendlier for a sticker; keep the print size above 40mm.',
    style: {
      ...base,
      modules: 'dot',
      moduleRadius: 0.5,
      moduleGap: 0.02,
      eyeFrame: 'circle',
      eyeCenter: 'dot',
      eyeRadius: 0.5,
      colors: { fg: '#0B1210', fg2: '#0B1210', bg: '#FFFFFF', eye: '#0B1210', eyeCenter: '#0B1210' },
      logo: null,
    },
  },
  {
    id: 'connected',
    name: 'Woven',
    hint: 'Neighbours touch, so the symbol reads as a woven field. Best at A5 and above.',
    style: {
      ...base,
      modules: 'connected',
      moduleRadius: 0.5,
      moduleGap: 0,
      eyeFrame: 'rounded',
      eyeCenter: 'square',
      eyeRadius: 0.3,
      colors: { fg: '#121A2B', fg2: '#121A2B', bg: '#FFFFFF', eye: '#121A2B', eyeCenter: '#121A2B' },
      logo: null,
    },
  },
  {
    id: 'mint',
    name: 'Mint',
    hint: 'DROP IT green accented eyes on white. Reads as the product, not as a guest card.',
    style: {
      ...base,
      modules: 'rounded',
      moduleRadius: 0.4,
      moduleGap: 0.05,
      eyeFrame: 'rounded',
      eyeCenter: 'dot',
      eyeRadius: 0.44,
      colors: { fg: '#08130D', fg2: '#08130D', bg: '#FFFFFF', eye: '#0B6B44', eyeCenter: '#0B6B44' },
      logo: null,
    },
  },
  {
    id: 'espresso',
    name: 'Espresso',
    hint: 'Warm ink on cream for a bar or a lounge. Print on warm stock, not on bright white.',
    style: {
      ...base,
      modules: 'rounded',
      moduleRadius: 0.5,
      moduleGap: 0.04,
      eyeFrame: 'leaf',
      eyeCenter: 'rounded',
      eyeRadius: 0.5,
      colors: { fg: '#2A1B12', fg2: '#6B3F22', bg: '#F8F1E5', eye: '#2A1B12', eyeCenter: '#6B3F22' },
      logo: null,
      colorMode: 'gradient',
      gradientAngle: 135,
    },
  },
  {
    id: 'cobalt',
    name: 'Cobalt',
    hint: 'Cool blue ink. Reads well photographed under a phone flash on white paper.',
    style: {
      ...base,
      modules: 'rounded',
      moduleRadius: 0.32,
      moduleGap: 0.04,
      eyeFrame: 'square',
      eyeCenter: 'rounded',
      eyeRadius: 0.2,
      colors: { fg: '#0B2A6B', fg2: '#1B57C9', bg: '#FFFFFF', eye: '#0B2A6B', eyeCenter: '#1B57C9' },
      logo: null,
      colorMode: 'dual',
    },
  },
  {
    id: 'ember',
    name: 'Ember',
    hint: 'Warm ink on a bone surface for a poster. Keep the quiet zone clean.',
    style: {
      ...base,
      modules: 'connected',
      moduleRadius: 0.36,
      moduleGap: 0,
      eyeFrame: 'circle',
      eyeCenter: 'dot',
      eyeRadius: 0.5,
      colors: { fg: '#3B1206', fg2: '#A93B16', bg: '#FFF6EE', eye: '#3B1206', eyeCenter: '#A93B16' },
      logo: null,
      colorMode: 'gradient',
      gradientAngle: 100,
    },
  },
  {
    id: 'acid',
    name: 'Acid',
    hint: 'For a poster or a backstage door. Loud by design; never use it on a table card.',
    style: {
      ...base,
      modules: 'liquid',
      moduleRadius: 0.5,
      moduleGap: 0.05,
      eyeFrame: 'leaf',
      eyeCenter: 'dot',
      eyeRadius: 0.5,
      colors: { fg: '#0A1A05', fg2: '#2F7A00', bg: '#F4FFE8', eye: '#0A1A05', eyeCenter: '#3D8E00' },
      logo: null,
      colorMode: 'gradient',
      gradientAngle: 70,
    },
  },
];

export function presetById(id: string): Preset | undefined {
  return PRESETS.find((preset) => preset.id === id);
}

/**
 * The studio opens on Ink: a new operator should meet the safest symbol first
 * and discover the styling after the code already works.
 */
export const DEFAULT_PRESET = 'ink';

/** Presets for the centre mark, drawn in a 24×24 box and scaled to the plate. */
export const LOGO_GLYPHS: ReadonlyArray<{ id: string; label: string; path: string }> = [
  {
    id: 'drop',
    label: 'Drop',
    path: 'M12 2.6c3.6 4.3 6.4 7.9 6.4 11.2A6.4 6.4 0 0 1 12 20.2a6.4 6.4 0 0 1-6.4-6.4C5.6 10.5 8.4 6.9 12 2.6Z',
  },
  {
    id: 'disc',
    label: 'Vinyl',
    path: 'M12 2.2a9.8 9.8 0 1 0 0 19.6 9.8 9.8 0 0 0 0-19.6Zm0 2.6a7.2 7.2 0 1 1 0 14.4 7.2 7.2 0 0 1 0-14.4Zm0 5.1a2.1 2.1 0 1 0 0 4.2 2.1 2.1 0 0 0 0-4.2Z',
  },
  {
    id: 'wave',
    label: 'Wave',
    path: 'M3 12h2.2v0h1.4l1.5-5.2 2 10.4 2-13.2 2 13.6 1.8-6.2 1.3 2.2H21v-1.6h-1.6l-2.3-3.8-1 3.8h-2.6L11.5 3 9.6 14 8 5.8 5.8 13.6H3Z',
  },
  {
    id: 'bolt',
    label: 'Bolt',
    path: 'M13.4 2 4.8 13.3h5.3L9.4 22l8.9-11.6h-5.4L13.4 2Z',
  },
  {
    id: 'spark',
    label: 'Spark',
    path: 'M12 2l1.9 6.3L20 10l-6.1 1.7L12 18l-1.9-6.3L4 10l6.1-1.7L12 2Z',
  },
  {
    id: 'ring',
    label: 'Ring',
    path: 'M12 3.2a8.8 8.8 0 1 0 0 17.6 8.8 8.8 0 0 0 0-17.6Zm0 2.2a6.6 6.6 0 1 1 0 13.2 6.6 6.6 0 0 1 0-13.2Z',
  },
];

export function glyphById(id: string) {
  return LOGO_GLYPHS.find((glyph) => glyph.id === id);
}

/** A minimal document that renders and encodes immediately, with no backend. */
export function emptyDoc(value: string, url: string): QrDoc {
  const preset = presetById(DEFAULT_PRESET);
  const style: QrStyle = {
    ...(preset?.style ?? {
      modules: 'square',
      moduleRadius: 0,
      moduleGap: 0,
      eyeFrame: 'square',
      eyeCenter: 'square',
      eyeRadius: 0,
      quietZone: 'standard',
      colors: { fg: '#0B1210', fg2: '#0B1210', bg: '#FFFFFF', eye: '#0B1210', eyeCenter: '#0B1210' },
      colorMode: 'single',
      gradientAngle: 90,
    }),
    logo: null,
  };
  return {
    value,
    url,
    style,
    caption: {
      eyebrow: '',
      title: 'Scan to request',
      subtitle: 'Open the room and send a track from your phone.',
      url,
      footer: '',
    },
    card: {
      layout: 'stacked',
      size: 'table-tent',
      surface: '#FFFFFF',
      ink: '#0B1210',
      accent: '#0B6B44',
      preset: DEFAULT_PRESET,
      brandMark: true,
      divider: false,
    },
    encode: { ecc: 'M', maskPattern: -1 },
  };
}
