/**
 * i18n — one language choice for every DROP IT web surface.
 *
 * The panel shell, the public Connect flow and the admin panel all read the
 * same preference (`localStorage['dropit.language']`) and the same six
 * languages, so switching the language in the account menu also switches the
 * public site and survives a reload. There is exactly one provider per app,
 * mounted above the router in `main.tsx`.
 *
 * Catalog contract
 * ----------------
 * Each app owns one catalog: `Record<Language, Record<string, string>>`.
 * Keys are namespaced by surface (`nav.*`, `account.*`, `dashboard.*`, …) so a
 * panel catalog can be merged with the Connect catalog without collisions.
 * A language that has no entry for a key falls back to English — an unfinished
 * translation renders English copy, never a raw key, and `scripts/i18n-check.mjs`
 * fails the build gate listing every key a language is missing.
 *
 * The provider also writes `<html lang>` and exposes the BCP-47 tag used by
 * `Intl` formatters, so money, dates and numbers follow the chosen language.
 */
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from 'react';

export interface LanguageOption {
  code: string;
  /** English name, used in accessible descriptions. */
  label: string;
  /** Endonym, the only label shown to the user. */
  native: string;
  flag: string;
  /** BCP-47 tag for `Intl.NumberFormat` / `Intl.DateTimeFormat`. */
  locale: string;
}

export const LANGUAGES = [
  { code: 'en', label: 'English', native: 'English', flag: '🇬🇧', locale: 'en-IE' },
  { code: 'es', label: 'Spanish', native: 'Español', flag: '🇪🇸', locale: 'es-ES' },
  { code: 'fr', label: 'French', native: 'Français', flag: '🇫🇷', locale: 'fr-FR' },
  { code: 'de', label: 'German', native: 'Deutsch', flag: '🇩🇪', locale: 'de-DE' },
  { code: 'pt', label: 'Portuguese', native: 'Português', flag: '🇵🇹', locale: 'pt-PT' },
  { code: 'ru', label: 'Russian', native: 'Русский', flag: '🇷🇺', locale: 'ru-RU' },
] as const satisfies ReadonlyArray<LanguageOption>;

export type Language = (typeof LANGUAGES)[number]['code'];

export const LANGUAGE_CODES = LANGUAGES.map((entry) => entry.code) as readonly Language[];

/** One catalog per language. Partial translations are allowed; English is the fallback. */
export type MessageCatalog = Record<Language, Record<string, string>>;

export type TranslateVars = Record<string, string | number>;
export type Translate = (key: string, vars?: TranslateVars) => string;

export const LOCALE_STORAGE_KEY = 'dropit.language';

export function isLanguage(value: unknown): value is Language {
  return typeof value === 'string' && (LANGUAGE_CODES as readonly string[]).includes(value);
}

export function localeFor(language: Language): string {
  return LANGUAGES.find((entry) => entry.code === language)?.locale ?? 'en-IE';
}

export function nativeNameFor(language: Language): string {
  return LANGUAGES.find((entry) => entry.code === language)?.native ?? 'English';
}

/** `{name}` placeholders; an unknown placeholder is left visible rather than blank. */
export function interpolate(template: string, vars?: TranslateVars): string {
  if (!vars) return template;
  return template.replace(/\{(\w+)\}/g, (match, name: string) => {
    const value = vars[name];
    return value === undefined ? match : String(value);
  });
}

function deviceLanguage(): Language {
  if (typeof navigator === 'undefined') return 'ru';
  const browserLanguages = navigator.languages?.length ? navigator.languages : [navigator.language];
  for (const language of browserLanguages) {
    const base = language.toLowerCase().split('-')[0];
    if (isLanguage(base)) return base;
  }
  return 'ru';
}

export function readStoredLanguage(): Language {
  try {
    const saved = window.localStorage.getItem(LOCALE_STORAGE_KEY);
    if (isLanguage(saved)) return saved;
  } catch {
    /* private mode: the device language is still a valid choice */
  }
  return deviceLanguage();
}

export interface LocaleValue {
  language: Language;
  setLanguage: (next: Language) => void;
  /** BCP-47 tag for Intl formatters. */
  locale: string;
  t: Translate;
  /** Whether the active (non-English) catalog actually carries the key. */
  has: (key: string) => boolean;
}

const LocaleContext = createContext<LocaleValue | null>(null);

export function LocaleProvider({ catalog, children }: { catalog: MessageCatalog; children: ReactNode }) {
  const [language, setLanguageState] = useState<Language>(readStoredLanguage);

  useEffect(() => {
    document.documentElement.lang = language;
  }, [language]);

  const setLanguage = useCallback((next: Language) => {
    setLanguageState(next);
    try {
      window.localStorage.setItem(LOCALE_STORAGE_KEY, next);
    } catch {
      /* Storage is optional (private browsing or browser policy may deny it). */
    }
  }, []);

  // A second tab or window changing the language is a real state change.
  useEffect(() => {
    const onStorage = (event: StorageEvent) => {
      if (event.key !== LOCALE_STORAGE_KEY) return;
      setLanguageState(isLanguage(event.newValue) ? event.newValue : deviceLanguage());
    };
    window.addEventListener('storage', onStorage);
    return () => window.removeEventListener('storage', onStorage);
  }, []);

  const value = useMemo<LocaleValue>(() => {
    const english = catalog.en ?? {};
    const current = catalog[language] ?? {};
    return {
      language,
      setLanguage,
      locale: localeFor(language),
      t: (key, vars) => interpolate(current[key] ?? english[key] ?? key, vars),
      has: (key) => typeof current[key] === 'string',
    };
  }, [catalog, language, setLanguage]);

  return <LocaleContext.Provider value={value}>{children}</LocaleContext.Provider>;
}

export function useLocale(): LocaleValue {
  const value = useContext(LocaleContext);
  if (!value) throw new Error('useLocale must be used inside LocaleProvider');
  return value;
}
