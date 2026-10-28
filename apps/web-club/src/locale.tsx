/**
 * The one locale provider for the club panel.
 *
 * Two catalogs share one language choice:
 *   - `panelMessages`  shell, club pages and the DJ console (`nav.*`, `dj.*`, …)
 *   - `connectMessages` the public Connect flow (`heroTitle`, `wizardName`, …)
 *
 * They are merged here rather than nested, because a nested provider would mean
 * two languages could disagree between the panel and the public site. Connect
 * modules reach this provider through the compatibility hook in
 * `connect/locale.tsx`.
 */
import { LocaleProvider as Provider, type MessageCatalog, LANGUAGE_CODES } from './i18n';
import { connectMessages } from './connect/messages';
import { buildCatalog } from './messages';

// A duplicate key across area files is a copy-paste mistake that would show the
// wrong sentence; it fails loudly in development instead of silently winning.
const { catalog: panelCatalog, problems } = buildCatalog();
if (problems.length > 0 && typeof console !== 'undefined') {
  console.error('[i18n] catalog problems', problems);
}

const merge = (): MessageCatalog => {
  const merged = {} as MessageCatalog;
  for (const code of LANGUAGE_CODES) {
    merged[code] = { ...connectMessages[code], ...panelCatalog[code] };
  }
  return merged;
};

const catalog = merge();

export function LocaleProvider({ children }: { children: React.ReactNode }) {
  return <Provider catalog={catalog}>{children}</Provider>;
}

export { useLocale, LANGUAGES, LOCALE_STORAGE_KEY, localeFor } from './i18n';
export type { Language, Translate } from './i18n';
