import { useEffect, useId, useRef, useState } from 'react';
import { Link } from 'react-router-dom';
import Icon from './Icons';
import Select from '../ui/Select';
import BrandLogo from '../BrandLogo';
import { LANGUAGES, useLocale } from '../../connect/locale';
import { CLUB_JOIN_CTA } from '../../connect/cta';
import { IS_DEMO } from '../../app-mode';
import './connect-nav.css';
import './public-layout.css';

/**
 * Logo lockup shared by the nav and the footer.
 * Frame h7akdu / Nav / Logo lockup — 36px blue mark, Inter 20 wordmark,
 * 10px mono pill outlined in the accent.
 *
 * `nav` tags the link for the header's tap-target rules; the footer lockup is
 * unchanged.
 */
export function LogoLockup({ compact = false, nav = false }: { compact?: boolean; nav?: boolean }) {
  void compact;
  const { t } = useLocale();
  return (
    <Link
      to="/connect"
      className={`${nav ? 'connect-nav-logo ' : ''}flex min-w-0 items-center gap-2.5`}
      aria-label={`DROP IT — ${t('home')}`}
    >
      <BrandLogo size="medium" />
    </Link>
  );
}

interface ConnectNavProps {
  /** `landing` shows the full link set, `wizard` the onboarding subset. */
  variant?: 'landing' | 'wizard';
}

export default function ConnectNav({ variant = 'landing' }: ConnectNavProps) {
  const [open, setOpen] = useState(false);
  const menuId = useId();
  const languageLabelId = useId();
  const burgerRef = useRef<HTMLButtonElement>(null);
  const drawerRef = useRef<HTMLDivElement>(null);
  const { language, setLanguage, t } = useLocale();
  const isDevelopment = window.location.hostname === 'localhost' || window.location.hostname === '127.0.0.1';

  // The drawer is a modal surface on a phone: lock the page behind it, close it
  // on Escape, and drop it when the viewport grows back to the desktop nav —
  // otherwise the burger disappears while the locked body cannot scroll.
  //
  // Focus enters the panel on the first destination and comes back to the
  // burger on Escape; Tab cycles inside the panel, because `aria-modal` tells
  // assistive tech the rest of the page is inert.
  useEffect(() => {
    if (!open) return undefined;
    const previous = document.body.style.overflow;
    const drawer = drawerRef.current;
    const focusFirst = window.requestAnimationFrame(() => {
      drawer?.querySelector<HTMLElement>('a[href], button:not([disabled])')?.focus({ preventScroll: true });
    });
    const focusables = () => {
      if (!drawer) return [];
      return Array.from(
        drawer.querySelectorAll<HTMLElement>('a[href], button:not([disabled]), [tabindex]:not([tabindex="-1"])'),
      ).filter((element) => element.offsetParent !== null);
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.preventDefault();
        setOpen(false);
        burgerRef.current?.focus();
        return;
      }
      if (event.key !== 'Tab' || !drawer) return;
      const items = focusables();
      if (items.length === 0) return;
      const first = items[0];
      const last = items[items.length - 1];
      if (!first || !last) return;
      const active = document.activeElement;
      if (event.shiftKey && (active === first || !drawer.contains(active))) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && active === last) {
        event.preventDefault();
        first.focus();
      }
    };
    const desktop = window.matchMedia('(min-width: 1280px)');
    const onBreakpoint = (event: MediaQueryListEvent) => {
      if (event.matches) setOpen(false);
    };
    document.body.style.overflow = 'hidden';
    window.addEventListener('keydown', onKeyDown);
    desktop.addEventListener('change', onBreakpoint);
    return () => {
      window.cancelAnimationFrame(focusFirst);
      document.body.style.overflow = previous;
      window.removeEventListener('keydown', onKeyDown);
      desktop.removeEventListener('change', onBreakpoint);
    };
  }, [open]);

  const links = variant === 'landing'
    ? [
        { label: t('home'), href: '#hero' },
        { label: t('features'), href: '#benefits' },
        { label: t('revenue'), href: '#revenue' },
        { label: t('steps'), href: '#steps' },
        { label: t('panel'), href: '#panel' },
        { label: t('guest'), href: '#guest' },
      ]
    : [
        { label: t('features'), href: '/connect#benefits' },
        { label: t('revenue'), href: '/connect#revenue' },
        { label: t('steps'), href: '/connect#steps', active: true },
      ];

  const closeDrawer = () => setOpen(false);

  return (
    <>
      <header
        className="connect-nav sticky top-0 z-50 h-[72px] border-b border-drop-line bg-[#0B0D0BD9] backdrop-blur-[12px]"
        data-drawer-open={open ? 'true' : undefined}
      >
        <div className="connect-nav-row mx-auto flex h-full w-full max-w-[1440px] items-center justify-between gap-2 px-4 sm:gap-3 sm:px-8 lg:px-12">
          <LogoLockup nav />

          <nav className="hidden items-center gap-2 min-[1280px]:flex 2xl:gap-4" aria-label={t('menu')}>
            {links.map((link, index) => {
              const isActive = 'active' in link && link.active;
              const isFirst = index === 0 && variant === 'landing';
              const className = `whitespace-nowrap text-[11px] 2xl:text-[12px] transition-colors hover:text-drop-accent ${
                isActive || isFirst ? 'text-drop-accent' : 'text-drop-muted'
              }`;
              if (link.href === '#panel') {
                return (
                  <div key={link.label} className="connect-panel-menu group relative">
                    <a href="#panel" className={`${className} inline-flex items-center gap-1`} aria-haspopup="true">
                      {link.label}<Icon name="chevronDown" size={13} />
                    </a>
                    <div className="connect-panel-menu-list invisible absolute left-0 top-full z-10 mt-1.5 min-w-52 translate-y-1 rounded-[12px] bg-[#191919] p-1 opacity-0 shadow-[0_18px_40px_-14px_rgba(0,0,0,0.8),0_0_0_1px_rgba(255,255,255,0.07)] transition duration-150 group-hover:visible group-hover:translate-y-0 group-hover:opacity-100 group-focus-within:visible group-focus-within:translate-y-0 group-focus-within:opacity-100">
                      <Link to="/login" className="connect-panel-menu-link">{t('panelClubLogin')}</Link>
                      <Link to="/dj/login" className="connect-panel-menu-link">{t('panelDjLogin')}</Link>
                      {isDevelopment ? <a href="http://localhost:5174/login" target="_blank" rel="noreferrer" className="connect-panel-menu-link">{t('panelAdminLocal')}</a> : null}
                      <span className="connect-panel-menu-divider" />
                      <a href="#panel" className="connect-panel-menu-link">{t('panelPreview')}</a>
                    </div>
                  </div>
                );
              }
              return link.href.startsWith('#') ? (
                <a key={link.label} href={link.href} className={className}>
                  {link.label}
                </a>
              ) : (
                <Link key={link.label} to={link.href} className={className}>
                  {link.label}
                </Link>
              );
            })}
          </nav>

          <div className="connect-nav-actions flex shrink-0 items-center gap-2 sm:gap-3">
            <Select
              id="site-language"
              className="connect-select connect-select-nav"
              menuClassName="connect-select-menu connect-select-menu-nav"
              ariaLabel={t('language')}
              value={language}
              onChange={(next) => setLanguage(next as typeof language)}
              options={LANGUAGES.map((item) => ({ value: item.code, label: `${item.flag} ${item.native}` }))}
            />
            <Link
              to="/login"
              className="hidden min-h-11 items-center px-2 text-[13px] text-drop-text transition-colors hover:text-drop-accent focus:outline-none focus-visible:ring-2 focus-visible:ring-drop-accent/50 md:inline-flex lg:px-3"
            >
              {t('signIn')}
            </Link>
            {/* /register is the club application (signup is closed): the
                visitor applies here instead of going through the wizard. */}
            {IS_DEMO ? null : (
              <Link
                to="/register"
                className="connect-nav-signup hidden min-h-11 items-center px-2 text-[13px] text-drop-text transition-colors hover:text-drop-accent focus:outline-none focus-visible:ring-2 focus-visible:ring-drop-accent/50 md:inline-flex lg:px-3"
              >
                {t('applyClub')}
              </Link>
            )}
            <Link
              to={CLUB_JOIN_CTA}
              className="connect-nav-join inline-flex min-h-11 items-center justify-center rounded-field bg-drop-accent px-3 text-[12px] font-semibold leading-[1.2] text-drop-ink transition hover:brightness-110 focus:outline-none focus-visible:ring-2 focus-visible:ring-drop-accent/50 sm:px-4 sm:text-[13px] xl:px-[18px]"
            >
              {/* The phone header has one job for this control: stay clear of the
                  burger. The short label (joinShort) does that at 320px in every
                  language; the drawer below keeps the full "Connect a club". */}
              <span className="md:hidden">{t('joinShort')}</span>
              <span className="hidden md:inline">{t('join')}</span>
            </Link>
            <button
              ref={burgerRef}
              type="button"
              onClick={() => setOpen((v) => !v)}
              aria-expanded={open}
              aria-controls={menuId}
              aria-label={open ? t('menuClose') : t('menuOpen')}
              className="connect-nav-burger flex h-11 w-11 shrink-0 items-center justify-center rounded-field border border-drop-line bg-white/[0.025] text-drop-muted transition-colors hover:border-drop-accent/40 hover:bg-drop-tint hover:text-drop-text focus:outline-none focus-visible:ring-2 focus-visible:ring-drop-accent/50 min-[1280px]:hidden"
            >
              <span className="flex flex-col gap-[5px]" aria-hidden="true">
                <span className={`h-px w-[18px] bg-current transition-transform ${open ? 'translate-y-[3px] rotate-45' : ''}`} />
                <span className={`h-px w-[18px] bg-current transition-opacity ${open ? 'opacity-0' : ''}`} />
                <span className={`h-px w-[18px] bg-current transition-transform ${open ? '-translate-y-[3px] -rotate-45' : ''}`} />
              </span>
            </button>
          </div>
        </div>
      </header>

      {/* The drawer is the header's sibling on purpose: the sticky bar carries
          `backdrop-filter`, which turns it into the containing block for fixed
          descendants — inside it the panel could never reach the viewport. */}
      {open ? (
        <div
          ref={drawerRef}
          id={menuId}
          className="connect-nav-drawer"
          role="dialog"
          aria-modal="true"
          aria-label={t('menu')}
        >
          <nav className="connect-nav-drawer-inner" aria-label={t('menu')}>
            <p className="connect-nav-drawer-eyebrow">{t('menu')}</p>
            <div className="connect-nav-drawer-links">
              {links.map((link, index) => {
                if (link.href === '#panel') {
                  return (
                    <div key={`${index}-${link.label}`} className="connect-nav-drawer-panel">
                      <p className="connect-nav-drawer-group-label">{link.label}</p>
                      <Link to="/login" onClick={closeDrawer} className="connect-nav-drawer-sublink">{t('panelClubLogin')}<Icon name="arrowUpRight" size={15} /></Link>
                      <Link to="/dj/login" onClick={closeDrawer} className="connect-nav-drawer-sublink">{t('panelDjLogin')}<Icon name="arrowUpRight" size={15} /></Link>
                      {isDevelopment ? <a href="http://localhost:5174/login" onClick={closeDrawer} className="connect-nav-drawer-sublink">{t('panelAdminLocal')}<Icon name="arrowUpRight" size={15} /></a> : null}
                      <a href="#panel" onClick={closeDrawer} className="connect-nav-drawer-sublink">{t('panelPreview')}<Icon name="arrowUpRight" size={15} /></a>
                    </div>
                  );
                }
                const row = link.href.startsWith('#') ? (
                  <a key={link.label} href={link.href} onClick={closeDrawer} className="connect-nav-drawer-link">
                    <span>{link.label}</span><Icon name="arrowUpRight" size={15} />
                  </a>
                ) : (
                  <Link key={link.label} to={link.href} onClick={closeDrawer} className="connect-nav-drawer-link">
                    <span>{link.label}</span><Icon name="arrowUpRight" size={15} />
                  </Link>
                );
                return row;
              })}
            </div>

            <div className="connect-nav-drawer-lang">
              <p className="connect-nav-drawer-label" id={languageLabelId}>{t('language')}</p>
              <Select
                id="site-language-drawer"
                className="connect-select connect-select-drawer"
                menuClassName="connect-select-menu connect-select-menu-drawer"
                labelledBy={languageLabelId}
                value={language}
                onChange={(next) => setLanguage(next as typeof language)}
                options={LANGUAGES.map((item) => ({ value: item.code, label: `${item.flag} ${item.native}` }))}
              />
            </div>

            <div className="connect-nav-drawer-actions">
              <Link to="/login" onClick={closeDrawer} className="connect-btn-ghost">{t('signIn')}</Link>
              {IS_DEMO ? null : (
                <Link to="/register" onClick={closeDrawer} className="connect-btn-ghost">{t('applyClub')}</Link>
              )}
              <Link to={CLUB_JOIN_CTA} onClick={closeDrawer} className="connect-btn-primary">{t('join')} <Icon name="arrowRight" size={15} /></Link>
            </div>
          </nav>
        </div>
      ) : null}
    </>
  );
}
