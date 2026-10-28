import { useState } from 'react';
import { Link } from 'react-router-dom';
import Icon from './Icons';
import Select from '../ui/Select';
import BrandLogo from '../BrandLogo';
import { LANGUAGES, useLocale } from '../../connect/locale';
import { CLUB_JOIN_CTA } from '../../connect/cta';
import { IS_DEMO } from '../../app-mode';

/**
 * Logo lockup shared by the nav and the footer.
 * Frame h7akdu / Nav / Logo lockup — 36px blue mark, Inter 20 wordmark,
 * 10px mono pill outlined in the accent.
 */
export function LogoLockup({ compact = false }: { compact?: boolean }) {
  void compact;
  return (
    <Link to="/connect" className="flex min-w-0 items-center gap-2.5" aria-label="DROP IT — на главную">
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
  const { language, setLanguage, t } = useLocale();
  const isDevelopment = window.location.hostname === 'localhost' || window.location.hostname === '127.0.0.1';
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

  return (
    <header className="sticky top-0 z-50 h-[72px] border-b border-drop-line bg-[#0B0D0BD9] backdrop-blur-[12px]">
      <div className="mx-auto flex h-full w-full max-w-[1440px] items-center justify-between gap-2 px-4 sm:gap-3 sm:px-8 lg:px-12">
        <LogoLockup />

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

        <div className="flex shrink-0 items-center gap-2 sm:gap-3">
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
          {/* Test stand: open registration lives at /register, so a visitor can
              create an account without going through the onboarding wizard. */}
          {IS_DEMO ? null : (
            <Link
              to="/register"
              className="connect-nav-signup hidden min-h-11 items-center px-2 text-[13px] text-drop-text transition-colors hover:text-drop-accent focus:outline-none focus-visible:ring-2 focus-visible:ring-drop-accent/50 md:inline-flex lg:px-3"
            >
              {t('signUp')}
            </Link>
          )}
          <Link
            to={CLUB_JOIN_CTA}
            className="connect-nav-join inline-flex min-h-11 items-center justify-center rounded-field bg-drop-accent px-3 text-[12px] font-semibold leading-[1.2] text-drop-ink transition hover:brightness-110 focus:outline-none focus-visible:ring-2 focus-visible:ring-drop-accent/50 sm:px-4 sm:text-[13px] xl:px-[18px]"
          >
            <span className="sm:hidden">{t('join')}</span>
            <span className="hidden sm:inline">{t('join')}</span>
          </Link>
          <button
            type="button"
            onClick={() => setOpen((v) => !v)}
            aria-expanded={open}
            aria-label={open ? 'Закрыть меню' : 'Открыть меню'}
            className="flex h-11 w-11 shrink-0 items-center justify-center rounded-field border border-drop-line bg-white/[0.025] text-drop-muted transition-colors hover:border-drop-accent/40 hover:bg-drop-tint hover:text-drop-text focus:outline-none focus-visible:ring-2 focus-visible:ring-drop-accent/50 min-[1280px]:hidden"
          >
            <span className="flex flex-col gap-[5px]" aria-hidden="true">
              <span className={`h-px w-[18px] bg-current transition-transform ${open ? 'translate-y-[3px] rotate-45' : ''}`} />
              <span className={`h-px w-[18px] bg-current transition-opacity ${open ? 'opacity-0' : ''}`} />
              <span className={`h-px w-[18px] bg-current transition-transform ${open ? '-translate-y-[3px] -rotate-45' : ''}`} />
            </span>
          </button>
        </div>
      </div>

      {open && (
        <div className="absolute inset-x-0 top-full border-t border-drop-line bg-[#0B0D0B]/[0.98] shadow-[0_24px_48px_-24px_rgba(0,0,0,0.9)] backdrop-blur-xl min-[1280px]:hidden">
          <nav className="mx-auto w-full max-w-[1440px] px-4 py-5 sm:px-8 lg:px-12" aria-label="Мобильная навигация">
            <p className="connect-mono-label mb-3 px-1">{t('menu')}</p>
            <div className="grid gap-1 sm:grid-cols-2">
              {links.map((link, index) => {
                if (link.href === '#panel') {
                  return (
                    <div key={`${index}-${link.label}`} className="connect-mobile-panel-links border-b border-drop-line/70">
                      <p className="px-3 pb-1 pt-3 text-[10px] uppercase tracking-[.12em] text-drop-dim">{link.label}</p>
                      <Link to="/login" onClick={() => setOpen(false)} className="connect-mobile-panel-link">{t('panelClubLogin')}<Icon name="arrowUpRight" size={15} /></Link>
                      <Link to="/dj/login" onClick={() => setOpen(false)} className="connect-mobile-panel-link">{t('panelDjLogin')}<Icon name="arrowUpRight" size={15} /></Link>
                      {isDevelopment ? <a href="http://localhost:5174/login" onClick={() => setOpen(false)} className="connect-mobile-panel-link">{t('panelAdminLocal')}<Icon name="arrowUpRight" size={15} /></a> : null}
                      <a href="#panel" onClick={() => setOpen(false)} className="connect-mobile-panel-link">{t('panelPreview')}<Icon name="arrowUpRight" size={15} /></a>
                    </div>
                  );
                }
                const row = link.href.startsWith('#') ? (
                  <a key={link.label} href={link.href} onClick={() => setOpen(false)} className="flex min-h-12 items-center justify-between rounded-field px-3 text-sm text-drop-muted transition-colors hover:bg-white/[0.04] hover:text-drop-text focus:outline-none focus-visible:ring-2 focus-visible:ring-drop-accent/50">
                    <span>{link.label}</span><Icon name="arrowUpRight" size={15} />
                  </a>
                ) : (
                  <Link key={link.label} to={link.href} onClick={() => setOpen(false)} className="flex min-h-12 items-center justify-between rounded-field px-3 text-sm text-drop-muted transition-colors hover:bg-white/[0.04] hover:text-drop-text focus:outline-none focus-visible:ring-2 focus-visible:ring-drop-accent/50">
                    <span>{link.label}</span><Icon name="arrowUpRight" size={15} />
                  </Link>
                );
                return <div key={`${index}-${link.label}`} className="border-b border-drop-line/70">{row}</div>;
              })}
            </div>
            <div className="mt-4 flex flex-col gap-2 border-t border-drop-line pt-4 sm:flex-row">
              <Link to="/login" onClick={() => setOpen(false)} className="connect-btn-ghost w-full sm:flex-1">{t('signIn')}</Link>
              {IS_DEMO ? null : (
                <Link to="/register" onClick={() => setOpen(false)} className="connect-btn-ghost w-full sm:flex-1">{t('signUp')}</Link>
              )}
              <Link to={CLUB_JOIN_CTA} onClick={() => setOpen(false)} className="connect-btn-primary w-full sm:flex-1">{t('join')} <Icon name="arrowRight" size={15} /></Link>
            </div>
          </nav>
        </div>
      )}
    </header>
  );
}
