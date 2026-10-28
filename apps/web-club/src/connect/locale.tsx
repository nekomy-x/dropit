/**
 * Connect locale — a compatibility layer over the app's single provider.
 *
 * The Connect flow used to own the only language state in the app. Language is
 * now a product-wide preference (`src/i18n.tsx` + `src/locale.tsx`), so this
 * module keeps the call sites unchanged (`useLocale`, `LANGUAGES`,
 * `translateValidationMessage`) while reading the shared provider: switching
 * the language in the panel changes the public site, and vice versa.
 */
import { useLocale as usePanelLocale, type TranslateVars } from '../i18n';
import { connectMessages, type MessageKey } from './messages';

export { LANGUAGES, type Language } from '../i18n';
export { translateValidationMessage, type MessageKey } from './messages';

export interface ConnectLocaleValue {
  language: ReturnType<typeof usePanelLocale>['language'];
  setLanguage: ReturnType<typeof usePanelLocale>['setLanguage'];
  /** BCP-47 tag for `Intl` formatters. */
  locale: string;
  t: (key: MessageKey, vars?: TranslateVars) => string;
}

export function useLocale(): ConnectLocaleValue {
  const { language, setLanguage, locale, t } = usePanelLocale();
  return {
    language,
    setLanguage,
    locale,
    // The provider already falls back to English; this narrows the key space to
    // the Connect catalog, which is what these call sites use.
    t: (key: MessageKey, vars?: TranslateVars) => t(key, vars),
  };
}

/** Every Connect key, for the i18n coverage gate. */
export const connectKeys = Object.keys(connectMessages.en) as MessageKey[];
