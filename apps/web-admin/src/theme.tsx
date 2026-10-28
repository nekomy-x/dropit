/**
 * Theme — one stored preference, one attribute, two themes.
 *
 * The attribute is written before first paint by the boot script in
 * index.html, so this module never causes a flash: it only reads what is
 * already on <html> and keeps it in sync.
 *
 * Contract (same in web-club and web-admin, mirrored by
 * scripts/sync-ui-tokens.mjs):
 *   <html data-theme="dark|light">
 *   localStorage['dropit-theme'] = 'dark' | 'light'
 */
import { useCallback, useEffect, useState } from 'react';
import Icon from './components/Icon';

export type Theme = 'dark' | 'light';
export const THEME_STORAGE_KEY = 'dropit-theme';

function currentTheme(): Theme {
  if (typeof document === 'undefined') return 'dark';
  return document.documentElement.dataset.theme === 'light' ? 'light' : 'dark';
}

export function useTheme() {
  const [theme, setThemeState] = useState<Theme>(currentTheme);

  const setTheme = useCallback((next: Theme) => {
    document.documentElement.dataset.theme = next;
    try {
      window.localStorage.setItem(THEME_STORAGE_KEY, next);
    } catch {
      /* private mode: the theme still applies for this page */
    }
    setThemeState(next);
  }, []);

  useEffect(() => {
    const onStorage = (event: StorageEvent) => {
      if (event.key !== THEME_STORAGE_KEY) return;
      const next: Theme = event.newValue === 'light' ? 'light' : 'dark';
      document.documentElement.dataset.theme = next;
      setThemeState(next);
    };
    window.addEventListener('storage', onStorage);
    return () => window.removeEventListener('storage', onStorage);
  }, []);

  return { theme, setTheme };
}

/** Two explicit states, both labelled, keyboard operable. */
export function ThemeToggle({ className = '' }: { className?: string }) {
  const { theme, setTheme } = useTheme();
  return (
    <div className={`theme-toggle ${className}`.trim()} role="group" aria-label="Colour theme">
      <button
        type="button"
        aria-pressed={theme === 'light'}
        aria-label="Light theme"
        title="Light theme"
        onClick={() => setTheme('light')}
      >
        <Icon name="lucide:sun" size={15} />
      </button>
      <button
        type="button"
        aria-pressed={theme === 'dark'}
        aria-label="Dark theme"
        title="Dark theme"
        onClick={() => setTheme('dark')}
      >
        <Icon name="lucide:moon" size={15} />
      </button>
    </div>
  );
}
