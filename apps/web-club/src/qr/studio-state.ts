/**
 * Studio state — one document, one reducer, one place that knows what a change
 * means.
 *
 * The screen has three jobs (place a code, style it, print it). Sharing one
 * reducer keeps them from disagreeing: raising a centre mark also raises the
 * error correction, choosing a preset records which preset the design is, and
 * editing a value after that marks the design as hand-made instead of leaving a
 * stale preset name on the screen. Those are rules, not UI details, so they live
 * here rather than in the page.
 *
 * Persistence is deliberately plain (`localStorage`, one key, versioned): an
 * operator who spent ten minutes on a sign should not lose it to a reload, and
 * there is no account-level place to store a *design* yet.
 */
import type { CardLayout, CardSize, ColorMode, Ecc, EyeCenter, EyeFrame, ModuleShape, QrDoc, QrStyle, QuietZone } from './types';
import { LOGO_MAX_SCALE } from './geometry';
import { DEFAULT_PRESET, PRESETS, emptyDoc, presetById, type Preset } from './presets';

const STORAGE_KEY = 'dropit.qr.studio.v1';

/** How many characters a printed caption may carry before the sign shortens it. */
export const CAPTION_LIMITS = { eyebrow: 40, title: 44, subtitle: 90, footer: 60, url: 120 } as const;

export type QrAction =
  | { type: 'load'; doc: QrDoc }
  | { type: 'value'; value: string }
  | { type: 'url'; url: string }
  | { type: 'preset'; id: string }
  | { type: 'style'; patch: Partial<QrStyle> }
  | { type: 'colors'; patch: Partial<QrStyle['colors']> }
  | { type: 'moduleShape'; shape: ModuleShape }
  | { type: 'eyeFrame'; frame: EyeFrame }
  | { type: 'eyeCenter'; center: EyeCenter }
  | { type: 'quietZone'; zone: QuietZone }
  | { type: 'colorMode'; mode: ColorMode }
  | { type: 'logo'; logo: QrStyle['logo'] }
  | { type: 'caption'; patch: Partial<QrDoc['caption']> }
  | { type: 'card'; patch: Partial<QrDoc['card']> }
  | { type: 'ecc'; ecc: Ecc }
  | { type: 'resetDoc' }
  | { type: 'clearCaption' };

/** A preset sets the whole look; the logo is the user's, so it survives. */
export function applyPreset(style: QrStyle, preset: Preset): QrStyle {
  const next: QrStyle = { ...preset.style, logo: style.logo };
  // A centre mark is only safe at H; picking a preset must not undo that.
  return style.logo ? { ...next } : next;
}

/**
 * Error correction that the current style *needs*.
 *
 * A centre mark consumes the symbol's redundancy, so it forces H. The function
 * returns the minimum level rather than silently raising the field, so the page
 * can tell the operator why the control changed.
 */
export function requiredEcc(style: QrStyle): Ecc | null {
  return style.logo ? 'H' : null;
}

const ECC_ORDER: Ecc[] = ['L', 'M', 'Q', 'H'];

export function atLeastEcc(current: Ecc, floor: Ecc): Ecc {
  return ECC_ORDER.indexOf(current) >= ECC_ORDER.indexOf(floor) ? current : floor;
}

/**
 * After any style edit, the design is no longer exactly the preset it started
 * from. Comparing the values (not tracking a flag) means re-editing the value
 * back to the preset's own makes the preset name return, which is what an
 * operator expects.
 */
function syncPreset(doc: QrDoc): QrDoc {
  const preset = presetById(doc.card.preset);
  const stillPreset = preset ? sameStyle(doc.style, preset.style) : false;
  if (preset && stillPreset) return doc;
  // Find any preset the style now matches, so "hand-made" is not sticky.
  const match = PRESETS.find((candidate) => sameStyle(doc.style, candidate.style));
  return { ...doc, card: { ...doc.card, preset: match?.id ?? '' } };
}

function sameStyle(a: QrStyle, b: QrStyle): boolean {
  return a.modules === b.modules
    && a.moduleRadius === b.moduleRadius
    && a.moduleGap === b.moduleGap
    && a.eyeFrame === b.eyeFrame
    && a.eyeCenter === b.eyeCenter
    && a.eyeRadius === b.eyeRadius
    && a.quietZone === b.quietZone
    && a.colorMode === b.colorMode
    && a.gradientAngle === b.gradientAngle
    && a.colors.fg.toLowerCase() === b.colors.fg.toLowerCase()
    && a.colors.fg2.toLowerCase() === b.colors.fg2.toLowerCase()
    && a.colors.bg.toLowerCase() === b.colors.bg.toLowerCase()
    && a.colors.eye.toLowerCase() === b.colors.eye.toLowerCase()
    && a.colors.eyeCenter.toLowerCase() === b.colors.eyeCenter.toLowerCase();
}

/** Clamps everything a slider or a colour field can produce. */
function clampStyle(style: QrStyle): QrStyle {
  return {
    ...style,
    moduleRadius: Math.min(0.5, Math.max(0, style.moduleRadius)),
    moduleGap: Math.min(0.15, Math.max(0, style.moduleGap)),
    eyeRadius: Math.min(0.5, Math.max(0, style.eyeRadius)),
    gradientAngle: ((style.gradientAngle % 360) + 360) % 360,
    logo: style.logo
      ? { ...style.logo, scale: Math.min(LOGO_MAX_SCALE, Math.max(0.1, style.logo.scale)) }
      : null,
  };
}

export function qrReducer(doc: QrDoc, action: QrAction): QrDoc {
  switch (action.type) {
    case 'load':
      return action.doc;

    case 'value':
      return { ...doc, value: action.value };

    case 'url':
      return { ...doc, url: action.url, caption: { ...doc.caption, url: action.url } };

    case 'preset': {
      const preset = presetById(action.id);
      if (!preset) return doc;
      const style = applyPreset(doc.style, preset);
      return { ...doc, style, card: { ...doc.card, preset: preset.id } };
    }

    case 'style':
      return syncPreset({ ...doc, style: clampStyle({ ...doc.style, ...action.patch }) });

    case 'colors':
      return syncPreset({ ...doc, style: clampStyle({ ...doc.style, colors: { ...doc.style.colors, ...action.patch } }) });

    case 'moduleShape':
      return syncPreset({ ...doc, style: { ...doc.style, modules: action.shape } });

    case 'eyeFrame':
      return syncPreset({ ...doc, style: { ...doc.style, eyeFrame: action.frame } });

    case 'eyeCenter':
      return syncPreset({ ...doc, style: { ...doc.style, eyeCenter: action.center } });

    case 'quietZone':
      return syncPreset({ ...doc, style: { ...doc.style, quietZone: action.zone } });

    case 'colorMode':
      return syncPreset({ ...doc, style: { ...doc.style, colorMode: action.mode } });

    case 'logo': {
      const style = clampStyle({ ...doc.style, logo: action.logo });
      // A mark forces H — the studio states this instead of letting an operator
      // discover it as a scanner failure on the printed sign.
      const floor = requiredEcc(style);
      const encode = floor ? { ...doc.encode, ecc: atLeastEcc(doc.encode.ecc, floor) } : doc.encode;
      return syncPreset({ ...doc, style, encode });
    }

    case 'caption':
      return { ...doc, caption: { ...doc.caption, ...action.patch } };

    case 'card':
      return { ...doc, card: { ...doc.card, ...action.patch } };

    case 'ecc': {
      const floor = requiredEcc(doc.style);
      return { ...doc, encode: { ...doc.encode, ecc: floor ? atLeastEcc(action.ecc, floor) : action.ecc } };
    }

    case 'clearCaption':
      return {
        ...doc,
        caption: { eyebrow: '', title: '', subtitle: '', url: doc.url, footer: '' },
      };

    case 'resetDoc':
      return emptyDoc(doc.value, doc.url);

    default:
      return doc;
  }
}

export interface PersistedDoc {
  doc: QrDoc;
  savedAt: number;
}

/** Reads the last design. Anything unreadable is discarded, never half-applied. */
export function loadPersistedDoc(value: string, url: string): PersistedDoc | null {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as PersistedDoc;
    if (!parsed || typeof parsed !== 'object' || !parsed.doc || typeof parsed.doc !== 'object') return null;
    const base = emptyDoc(value, url);
    // Merge onto a fresh document so a stored design from an older version
    // cannot leave a field undefined and crash the renderer.
    const doc: QrDoc = {
      ...base,
      ...parsed.doc,
      // The encoded value is never restored: a token from last night must not
      // reappear on a sign for tonight.
      value,
      url,
      style: { ...base.style, ...parsed.doc.style, colors: { ...base.style.colors, ...parsed.doc.style?.colors } },
      caption: { ...base.caption, ...parsed.doc.caption },
      card: { ...base.card, ...parsed.doc.card },
      encode: { ...base.encode, ...parsed.doc.encode },
    };
    return { doc, savedAt: typeof parsed.savedAt === 'number' ? parsed.savedAt : 0 };
  } catch {
    return null;
  }
}

export function persistDoc(doc: QrDoc): PersistedDoc | null {
  try {
    const payload: PersistedDoc = { doc: { ...doc, value: '' }, savedAt: Date.now() };
    localStorage.setItem(STORAGE_KEY, JSON.stringify(payload));
    return payload;
  } catch {
    // A full or blocked storage is not an error the operator can act on; the
    // studio simply keeps working without saving.
    return null;
  }
}

/** Default document for the studio, with the shipped preset. */
export function createDoc(value: string, url: string): QrDoc {
  const doc = emptyDoc(value, url);
  const preset = presetById(DEFAULT_PRESET);
  return preset ? { ...doc, style: { ...preset.style, logo: null }, card: { ...doc.card, preset: preset.id } } : doc;
}

export const CARD_SIZE_IDS: readonly CardSize[] = ['table-tent', 'sticker', 'a6', 'a5', 'a4', 'story'];
export const CARD_LAYOUT_IDS: readonly CardLayout[] = ['stacked', 'side', 'poster', 'qr-only'];
