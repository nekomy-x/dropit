/**
 * Board formatting helpers.
 *
 * Money is stored in minor units by the API (`*_minor`), so every figure on the
 * board goes through one formatter: no `toFixed`, no hand-rolled concatenation.
 * Numbers, money and clock times follow the language the user picked, so a
 * Spanish panel reads `1.284,50 €` and an English one `€1,284.50` — the value is
 * the same, only the presentation changes.
 *
 * Every function takes the BCP-47 locale from `useLocale()`. Intl formatters are
 * cached per locale: constructing one per cell on a live table is measurable.
 */
import type { Language } from '../../../i18n';
import { localeFor } from '../../../i18n';

/** `Language` or any BCP-47 tag; the app passes `locale` from the provider. */
export type LocaleInput = Language | string;

function resolveLocale(locale: LocaleInput | undefined): string {
  if (!locale) return localeFor('en');
  return locale.length <= 3 ? localeFor(locale as Language) : locale;
}

const MONEY_CACHE = new Map<string, Intl.NumberFormat>();
function moneyFormatter(locale: string): Intl.NumberFormat {
  const cached = MONEY_CACHE.get(locale);
  if (cached) return cached;
  const formatter = new Intl.NumberFormat(locale, {
    style: 'currency',
    currency: 'EUR',
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  });
  MONEY_CACHE.set(locale, formatter);
  return formatter;
}

const COUNT_CACHE = new Map<string, Intl.NumberFormat>();
function countFormatter(locale: string): Intl.NumberFormat {
  const cached = COUNT_CACHE.get(locale);
  if (cached) return cached;
  const formatter = new Intl.NumberFormat(locale, { maximumFractionDigits: 0 });
  COUNT_CACHE.set(locale, formatter);
  return formatter;
}

/** `1234` minor → `€12.34` (`12,34 €` in Spanish). Null when there is no reading. */
export function formatMoneyMinor(minor: number | null | undefined, locale?: LocaleInput): string | null {
  if (typeof minor !== 'number' || !Number.isFinite(minor)) return null;
  return moneyFormatter(resolveLocale(locale)).format(minor / 100);
}

/** `1284` → `1,284` (`1.284` in Spanish). Null when there is no reading. */
export function formatCount(value: number | null | undefined, locale?: LocaleInput): string | null {
  if (typeof value !== 'number' || !Number.isFinite(value)) return null;
  return countFormatter(resolveLocale(locale)).format(value);
}

/** 0.62 → `62%`. Clamped so a broken ratio never prints `-4%` or `812%`. */
export function formatPercent(ratio: number | null | undefined): string | null {
  if (typeof ratio !== 'number' || !Number.isFinite(ratio)) return null;
  return `${Math.round(Math.min(1, Math.max(0, ratio)) * 100)}%`;
}

/** ISO timestamp → `23:45` in the viewer's zone; null when unparseable. */
export function formatClock(iso: string | null | undefined, locale?: LocaleInput): string | null {
  if (!iso) return null;
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return null;
  return new Intl.DateTimeFormat(resolveLocale(locale), { hour: '2-digit', minute: '2-digit' }).format(date);
}

/**
 * Honest freshness label derived from the last SUCCESSFUL request
 * (01-contract.md §7: "updated Ns ago", never a self-ticking counter).
 * `null` means no request has ever succeeded, which is said in words.
 *
 * The translator is passed in so the sentence comes from the catalog; this
 * function only decides which bucket the elapsed time falls into.
 */
export function formatFreshness(
  lastFetchedAt: number | null,
  now: number,
  t: (key: string, vars?: Record<string, string | number>) => string,
): string {
  if (lastFetchedAt === null) return t('fresh.none');
  const seconds = Math.max(0, Math.round((now - lastFetchedAt) / 1000));
  if (seconds < 5) return t('fresh.now');
  if (seconds < 60) return t('fresh.seconds', { count: seconds });
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return t('fresh.minutes', { count: minutes });
  const hours = Math.round(minutes / 60);
  return t('fresh.hours', { count: hours });
}

/** First 8 characters of a token/ID — enough to compare two screens, not a secret dump. */
export function shortId(id: string | null | undefined): string | null {
  if (!id) return null;
  return id.slice(0, 8);
}
