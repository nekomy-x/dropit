/**
 * One venue, one set of numbers.
 *
 * The public Connect page quotes the same club in three places: the panel
 * preview, the revenue calculator and the night-by-night stats card. Every
 * figure in those blocks is derived from this file, so a visitor can check the
 * arithmetic of any card against the others:
 *
 *   price 2,00 € − DROP IT fee 0,30 € = 1,70 € to the club per payment
 *   140 payments a night × 1,70 € = 238,00 € a night
 *   26 nights a month × 238,00 € = 6 188,00 € a month
 *
 * The weekday weights sum to 7, which is what makes the average night equal
 * the scan control instead of drifting above it. The sample week is the
 * default scenario: 980 payments, 1 666,00 € to the club. The week before is
 * 903 payments, so the comparison chart shows a real +8.5 %, not a decoration.
 *
 * This is sample content for a marketing preview — the panel preview carries a
 * footnote saying so — but sample does not mean arbitrary: every figure the
 * page prints is computed from this file, and no translated string carries a
 * revenue, price or fee number.
 */
import type { Language } from '../../../i18n';

/** The only money DROP IT keeps: a flat fee on every paid request. */
export const SERVICE_FEE_MINOR = 30;

export const DEFAULT_PRICE_MINOR = 200;
export const DEFAULT_SCANS = 140;
export const DEFAULT_NIGHTS = 26;

/**
 * Relative demand per weekday. The seven weights sum to exactly 7, so the
 * mean night is `DEFAULT_SCANS` and the weekday chart's average line lands on
 * the value the control promises.
 */
export const WEEKDAY_SHARE = [0.45, 0.55, 0.65, 0.85, 1.55, 1.95, 1.0] as const;

/** `round(DEFAULT_SCANS × WEEKDAY_SHARE)` — 980 payments in the sample week. */
export const SAMPLE_WEEK_REQUESTS = [63, 77, 91, 119, 217, 273, 140] as const;

/** The week before: 903 payments, +8.5 % week over week. */
export const PREVIOUS_WEEK_REQUESTS = [58, 71, 84, 110, 200, 251, 129] as const;

/** Top requests in the sample week — 312 of 980 payments. */
export const TOP_TRACKS = [
  { title: 'Midnight City', artist: 'M83', requests: 96 },
  { title: 'Blue Monday', artist: 'New Order', requests: 78 },
  { title: 'Around The World', artist: 'Daft Punk', requests: 64 },
  { title: 'Teardrop', artist: 'Massive Attack', requests: 41 },
  { title: 'Losing It', artist: 'Fisher', requests: 33 },
] as const;

/**
 * The panel preview in `#panel` is the same venue on a different night, on the
 * same price model. Only that night's own counts are inputs: the revenue it
 * shows is computed from them (87 × 1,70 € = 147,90 €), so the preview cannot
 * drift away from the calculator's arithmetic.
 */
export const PANEL_REQUESTS_TODAY = 87;
export const PANEL_QUEUE_NOW = 6;
export const PANEL_SCANS_HOUR = 42;

/** Club share of one payment, in minor units. Never negative. */
export function clubShareMinor(priceMinor: number): number {
  return Math.max(0, priceMinor - SERVICE_FEE_MINOR);
}

/** Sum of a series in minor units. */
export function sumMinor(values: readonly number[]): number {
  return values.reduce((total, value) => total + value, 0);
}

const WEEKDAYS: Record<Language, readonly string[]> = {
  en: ['MON', 'TUE', 'WED', 'THU', 'FRI', 'SAT', 'SUN'],
  es: ['LU', 'MA', 'MI', 'JU', 'VI', 'SÁ', 'DO'],
  fr: ['LU', 'MA', 'ME', 'JE', 'VE', 'SA', 'DI'],
  de: ['MO', 'DI', 'MI', 'DO', 'FR', 'SA', 'SO'],
  pt: ['SEG', 'TER', 'QUA', 'QUI', 'SEX', 'SÁB', 'DOM'],
  ru: ['ПН', 'ВТ', 'СР', 'ЧТ', 'ПТ', 'СБ', 'ВС'],
};

export function weekdayLabels(language: Language): readonly string[] {
  return WEEKDAYS[language];
}