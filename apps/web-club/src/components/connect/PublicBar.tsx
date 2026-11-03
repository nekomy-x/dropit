import { Link } from 'react-router-dom';
import BrandLogo from '../BrandLogo';
import Select from '../ui/Select';
import { LANGUAGES, useLocale } from '../../connect/locale';
import { IS_DEMO } from '../../app-mode';
import './public-layout.css';

/**
 * The header of the two public front-door pages (`/` and `/download`).
 *
 * It is deliberately smaller than `ConnectNav`: those pages have two
 * destinations and nothing to anchor-link to, so the landing's section menu
 * would be six dead anchors. Same height (72px), same hairline, same language
 * control and the same lockup as the rest of the public site, so a visitor
 * crossing from `/` to `/connect` does not cross into a different header.
 */
export default function PublicBar({ active }: { active?: 'guests' | 'clubs' }) {
  const { language, setLanguage, t } = useLocale();

  const linkClass = (isActive: boolean) =>
    `inline-flex min-h-11 items-center whitespace-nowrap text-[13px] transition-colors hover:text-drop-accent focus:outline-none focus-visible:ring-2 focus-visible:ring-drop-accent/50 ${
      isActive ? 'text-drop-text' : 'text-drop-muted'
    }`;

  return (
    <header className="public-bar sticky top-0 z-50 h-[72px] border-b border-drop-line bg-drop-rail">
      <div className="mx-auto flex h-full w-full max-w-[1440px] items-center justify-between gap-3 px-4 sm:px-8 lg:px-12">
        <Link to={IS_DEMO ? '/hub' : '/'} className="flex min-h-11 min-w-0 items-center gap-3" aria-label="DROP IT">
          <BrandLogo size="medium" />
          <span className="hidden min-w-0 truncate font-mono text-[11px] text-drop-muted sm:inline">
            {t('hubTag')}
          </span>
        </Link>

        <div className="flex shrink-0 items-center gap-2 sm:gap-3">
          <nav className="hidden items-center gap-6 md:flex" aria-label={t('hubSwitchHint')}>
            <Link to="/download" className={linkClass(active === 'guests')}>{t('hubNavGuests')}</Link>
            <Link to="/connect" className={linkClass(active === 'clubs')}>{t('hubNavClubs')}</Link>
            <Link to="/login" className={linkClass(false)}>{t('signIn')}</Link>
          </nav>
          <Select
            id="public-language"
            className="connect-select connect-select-nav"
            menuClassName="connect-select-menu connect-select-menu-nav"
            ariaLabel={t('language')}
            value={language}
            onChange={(next) => setLanguage(next as typeof language)}
            options={LANGUAGES.map((item) => ({ value: item.code, label: `${item.flag} ${item.native}` }))}
          />
        </div>
      </div>
    </header>
  );
}
