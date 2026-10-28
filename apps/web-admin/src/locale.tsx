/**
 * The one locale provider for the admin panel.
 *
 * The admin surface shares the shell catalog with the club and DJ panels
 * (`packages/ui/messages.ts`: navigation, account menu, common vocabulary), so
 * switching the language there and here has the same effect. Admin screen copy
 * lives in `./messages/` and is merged in the same way.
 */
import { LocaleProvider as Provider, type MessageCatalog, LANGUAGE_CODES } from './i18n';
import { shellMessages } from './shell-messages';
import { buildCatalog } from './messages';

const { catalog: adminCatalog, problems } = buildCatalog();
if (problems.length > 0 && typeof console !== 'undefined') {
  console.error('[i18n] catalog problems', problems);
}

const merge = (): MessageCatalog => {
  const merged = {} as MessageCatalog;
  for (const code of LANGUAGE_CODES) {
    merged[code] = { ...shellMessages[code], ...adminCatalog[code] };
  }
  return merged;
};

const catalog = merge();

export function LocaleProvider({ children }: { children: React.ReactNode }) {
  return <Provider catalog={catalog}>{children}</Provider>;
}

export { useLocale, LANGUAGES, LOCALE_STORAGE_KEY, localeFor } from './i18n';
export type { Language, Translate } from './i18n';
