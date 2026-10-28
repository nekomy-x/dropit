import { useCallback, useEffect, useRef, useState } from 'react';
import { NavLink, Outlet, useLocation, useNavigate } from 'react-router-dom';
import { clearTokens } from '../api/client';
import { NavDrawer } from '../nav-drawer';
import Icon from './Icon';
import { ThemeToggle } from '../theme';
import { DeploymentPill } from './DeploymentStatus';
import { AccountMenu, AccountSettings } from '../account';
import { useLocale } from '../locale';
import { IS_DEMO } from '../app-mode';

type IconName = `lucide:${string}`;
type NavItem = { to: string; labelKey: string; descriptionKey?: string; live?: boolean; icon: IconName };
type NavSection = { titleKey: string; items: ReadonlyArray<NavItem> };

const NAV_SECTIONS: ReadonlyArray<NavSection> = [
  {
    titleKey: 'adminNav.sectionCommand',
    items: [
      { to: '/', labelKey: 'nav.overview', descriptionKey: 'adminNav.overviewDescription', icon: 'lucide:layout-dashboard' },
      { to: '/manage/users', labelKey: 'adminNav.users', descriptionKey: 'adminNav.usersDescription', icon: 'lucide:users-round' },
      { to: '/manage/djs', labelKey: 'nav.djs', descriptionKey: 'adminNav.djsDescription', icon: 'lucide:disc-3' },
      { to: '/manage/clubs', labelKey: 'adminNav.clubs', descriptionKey: 'adminNav.clubsDescription', icon: 'lucide:building-2' },
      { to: '/manage/transactions', labelKey: 'adminNav.transactions', descriptionKey: 'adminNav.transactionsDescription', icon: 'lucide:credit-card' },
    ],
  },
  {
    titleKey: 'adminNav.sectionOperations',
    items: [
      { to: '/dashboard', labelKey: 'adminNav.liveOverview', descriptionKey: 'adminNav.liveOverviewDescription', icon: 'lucide:activity' },
      { to: '/guests', labelKey: 'adminNav.guests', descriptionKey: 'adminNav.guestsDescription', icon: 'lucide:user-round-search' },
      { to: '/payouts', labelKey: 'adminNav.payouts', descriptionKey: 'adminNav.payoutsDescription', live: true, icon: 'lucide:arrow-down-to-line' },
      { to: '/incidents', labelKey: 'adminNav.incidents', descriptionKey: 'adminNav.incidentsDescription', live: true, icon: 'lucide:triangle-alert' },
      { to: '/mail', labelKey: 'adminNav.mail', descriptionKey: 'adminNav.mailDescription', live: true, icon: 'lucide:radio-tower' },
      { to: '/manage/activity', labelKey: 'adminNav.moderation', descriptionKey: 'adminNav.moderationDescription', icon: 'lucide:shield-check' },
      { to: '/manage/transactions', labelKey: 'adminNav.requests', descriptionKey: 'adminNav.requestsDescription', icon: 'lucide:music-2' },
    ],
  },
  {
    titleKey: 'adminNav.sectionPlatform',
    items: [
      { to: '/manage/access', labelKey: 'adminNav.roles', descriptionKey: 'adminNav.rolesDescription', icon: 'lucide:key-round' },
      { to: '/manage/settings', labelKey: 'adminNav.featureFlags', descriptionKey: 'adminNav.featureFlagsDescription', icon: 'lucide:toggle-right' },
      { to: '/platform-settings', labelKey: 'adminNav.economics', descriptionKey: 'adminNav.economicsDescription', icon: 'lucide:sliders-horizontal' },
      { to: '/audit-log', labelKey: 'adminNav.systemAudit', descriptionKey: 'adminNav.systemAuditDescription', icon: 'lucide:list-checks' },
      { to: '/platform-settings', labelKey: 'adminNav.integrations', descriptionKey: 'adminNav.integrationsDescription', icon: 'lucide:plug-zap' },
    ],
  },
];

const NAV_ITEMS = NAV_SECTIONS.flatMap((section) => section.items);

/** A nav item is active when the router path matches it exactly. Section pages
 * are real routes (`/admin/users`), so the rail survives a reload on any of
 * them and a screenshot tool can address them directly. */
function isItemActive(item: NavItem, location: ReturnType<typeof useLocation>): boolean {
  const [path] = item.to.split('#');
  return location.pathname === path;
}

/** The rail's own three groups, rendered from one source for every width.
 *
 * The mobile drawer is the rail, not a summary of it — same groups, same order,
 * same labels, same live badges — so the markup lives here once instead of
 * being copied into the drawer, where it would drift. `closeOnNavigate` marks
 * each destination with `data-nav-close`, the drawer's signal to close itself
 * once a choice is made. */
function NavGroups({ closeOnNavigate = false }: { closeOnNavigate?: boolean }) {
  const location = useLocation();
  const { t } = useLocale();
  return (
    <>
      {NAV_SECTIONS.map((section) => (
        <div key={section.titleKey} className="panel-nav-group">
          <p className="panel-nav-label">{t(section.titleKey)}</p>
          <ul className="panel-nav-list">
            {section.items.map((item) => (
              <li key={`${section.titleKey}-${item.to}-${item.labelKey}`}>
                <NavLink
                  to={item.to}
                  className={`panel-nav-link ${isItemActive(item, location) ? 'is-active' : ''}`}
                  data-nav-close={closeOnNavigate ? '' : undefined}
                >
                  <span className="panel-nav-icon"><Icon name={item.icon} size={17} /></span>
                  {t(item.labelKey)}
                  {item.live ? <span className="panel-nav-trailing">{t('adminNav.liveBadge')}</span> : null}
                </NavLink>
              </li>
            ))}
          </ul>
        </div>
      ))}
    </>
  );
}

export default function Layout() {
  const navigate = useNavigate();
  const { t } = useLocale();
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [commandOpen, setCommandOpen] = useState(false);
  const [query, setQuery] = useState('');
  const dialogRef = useRef<HTMLElement | null>(null);
  const triggerRef = useRef<HTMLButtonElement | null>(null);
  const previousFocusRef = useRef<HTMLElement | null>(null);

  const signOut = () => {
    clearTokens();
    navigate(IS_DEMO ? '/' : '/login', { replace: true });
  };

  // One identity for the whole admin shell: the operator account. Pages do not
  // report anything here — the platform operator is the only role this app has.
  const identity = {
    initials: 'OP',
    name: t('shell.platformOperator'),
    meta: t('shell.workspacePlatform'),
    facts: [
      { label: t('account.role'), value: t('shell.platformOperator') },
      { label: t('account.workspace'), value: t('shell.workspacePlatform') },
    ],
  };

  const filtered = NAV_ITEMS.filter((item) => `${t(item.labelKey)} ${item.descriptionKey ? t(item.descriptionKey) : ''}`.toLowerCase().includes(query.toLowerCase()));

  const closeCommand = useCallback(() => {
    setCommandOpen(false);
    const previous = previousFocusRef.current ?? triggerRef.current;
    previous?.focus({ preventScroll: true });
  }, []);

  // Two triggers (the field at ≥641px, the icon below it) open one palette and
  // return focus to whichever one was used.
  const openCommand = () => {
    previousFocusRef.current = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    setCommandOpen(true);
  };

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'k') {
        event.preventDefault();
        setCommandOpen((open) => {
          if (open) {
            const previous = previousFocusRef.current ?? triggerRef.current;
            previous?.focus({ preventScroll: true });
            return false;
          }
          previousFocusRef.current = document.activeElement instanceof HTMLElement ? document.activeElement : null;
          return true;
        });
      }
      if (event.key === 'Escape') {
        setCommandOpen((open) => {
          if (!open) return open;
          const previous = previousFocusRef.current ?? triggerRef.current;
          previous?.focus({ preventScroll: true });
          return false;
        });
      }
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, []);

  useEffect(() => {
    if (!commandOpen) return undefined;
    const { body } = document;
    const previousOverflow = body.style.overflow;
    body.style.overflow = 'hidden';
    const container = dialogRef.current;
    if (container) {
      const focusable = () => Array.from(container.querySelectorAll<HTMLElement>('a[href], button:not([disabled]), input:not([disabled])'))
        .filter((node) => node.offsetParent !== null);
      (focusable()[0] ?? container).focus({ preventScroll: true });
    }
    return () => { body.style.overflow = previousOverflow; };
  }, [commandOpen]);

  const choose = (to: string) => {
    setCommandOpen(false);
    setQuery('');
    navigate(to);
  };

  return (
    <div className="panel-shell">
      <aside className="panel-rail">
        <div className="panel-rail-brand">
          <span className="admin-brand">
            <Icon name="lucide:headphones" size={18} />
            <span className="admin-brand-word">DROP IT</span>
            <span className="admin-brand-role">{t('nav.admin')}</span>
          </span>
        </div>

        {/* No workspace badge: the platform has one operator and one workspace,
            and the rail foot already names both. A block above the nav that
            repeats them, and cannot switch to anything, only reads as a broken
            account picker. */}
        <nav className="panel-rail-nav" aria-label={t('nav.adminNav')}>
          <NavGroups />
        </nav>

        <div className="panel-rail-foot">
          <AccountMenu
            identity={identity}
            onOpenSettings={() => setSettingsOpen(true)}
            onSignOut={signOut}
          />
        </div>
      </aside>

      <div className="panel-main">
        <header className="panel-topbar">
          {/* Below 1024px the rail is gone, and this drawer is the only way to
              the other sixteen destinations. It is first in the row so the
              burger sits where a phone expects the way back to navigation. */}
          <NavDrawer
            label={t('nav.adminNav')}
            triggerLabel={t('nav.drawerOpen')}
            subtitle={t('shell.workspacePlatform')}
          >
            <NavGroups closeOnNavigate />
          </NavDrawer>

          <button
            ref={triggerRef}
            type="button"
            className="panel-search"
            aria-haspopup="dialog"
            aria-expanded={commandOpen}
            onClick={openCommand}
          >
            <Icon name="lucide:search" size={15} />
            <span>{t('shell.searchPlatform')}</span>
            <kbd aria-hidden="true">⌘K</kbd>
          </button>
          {/* <=640px: the field costs more width than it earns, so it collapses
              to this icon. Same command palette, same shortcut, same ref. */}
          <button
            type="button"
            className="panel-icon-button panel-search-compact"
            aria-haspopup="dialog"
            aria-expanded={commandOpen}
            aria-label={t('shell.searchWorkspace')}
            title={t('shell.searchWorkspace')}
            onClick={openCommand}
          >
            <Icon name="lucide:search" size={17} />
          </button>

          <div className="panel-topbar-actions">
            <DeploymentPill demoBuild={IS_DEMO} className="test-badge-inline" />
            <span className="chip chip-accent admin-status-chip panel-topbar-status">{t('adminNav.allOperational')}</span>
            <ThemeToggle />
            <span className="panel-topbar-account">
              <AccountMenu
                variant="compact"
                identity={identity}
                onOpenSettings={() => setSettingsOpen(true)}
                onSignOut={signOut}
              />
            </span>
          </div>
        </header>

        {commandOpen ? (
          <div className="command-backdrop" role="presentation" onMouseDown={closeCommand}>
            <section
              ref={dialogRef}
              className="command-dialog"
              role="dialog"
              aria-modal="true"
              aria-labelledby="admin-command-title"
              tabIndex={-1}
              onMouseDown={(event) => event.stopPropagation()}
            >
              <h2 id="admin-command-title" className="sr-only">{t('shell.commandMenu')}</h2>
              <div className="command-search-row">
                <Icon name="lucide:search" size={17} />
                <input
                  autoFocus
                  value={query}
                  onChange={(event) => setQuery(event.target.value)}
                  placeholder={t('adminNav.commandPlaceholder')}
                  aria-label={t('adminNav.commandLabel')}
                />
                <kbd>ESC</kbd>
              </div>
              <div className="command-results">
                {filtered.length ? filtered.map((item) => (
                  <button type="button" key={`${item.to}-${item.labelKey}`} className="command-result" onClick={() => choose(item.to)}>
                    <Icon name={item.icon} size={16} />
                    <span>{t(item.labelKey)}</span>
                    <span className="command-result-group">{item.descriptionKey ? t(item.descriptionKey) : ''}</span>
                  </button>
                )) : <p className="command-empty">{t('adminNav.commandEmpty')}</p>}
              </div>
            </section>
          </div>
        ) : null}

        <div className="panel-content"><Outlet /></div>
      </div>

      <AccountSettings
        open={settingsOpen}
        identity={identity}
        facts={identity.facts}
        onClose={() => setSettingsOpen(false)}
        onSignOut={signOut}
      />
    </div>
  );
}
