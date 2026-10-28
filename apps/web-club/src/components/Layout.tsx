import { Navigate, NavLink, Outlet, useLocation, useNavigate } from 'react-router-dom';
import { useCallback, useEffect, useRef, useState } from 'react';
import { clearTokens, getProfile, isAuthenticated } from '../api/client';
import Icon from './Icon';
import BrandLogo from './BrandLogo';
import { AccountMenu, AccountSettings, type AccountFact } from '../account';
import { ThemeToggle } from '../theme';
import { DeploymentPill } from './DeploymentStatus';
import { useLocale } from '../locale';
import { IS_DEMO } from '../app-mode';
import { NavDrawer } from '../nav-drawer';

/**
 * The shell owns identity, not the page.
 *
 * `GET /v1/club/profile` is read once per page load and cached at module level:
 * every club screen shows the real club name in the rail, and a page that also
 * fetched the profile does not cause a second request. A failed read leaves the
 * generic label in place — the rail never invents a club name.
 */
/**
 * Identity is one account block in the rail foot, and nothing else.
 *
 * There used to be a second `workspace` badge above the nav. In every panel it
 * restated the account row's own two strings in the opposite order, and since
 * it could not switch to anything it read as a broken account picker. The
 * club name it carried is still shown where it belongs: the page identity.
 */
interface ShellIdentity {
  /** The signed-in account. */
  account: { name: string; meta: string; initials: string; facts: AccountFact[] };
}

let profilePromise: Promise<{ name: string } | null> | null = null;
function loadShellProfile(): Promise<{ name: string } | null> {
  if (profilePromise) return profilePromise;
  profilePromise = getProfile()
    .then((profile) => ({ name: profile.name }))
    .catch(() => null);
  return profilePromise;
}

function useShellProfile(): { name: string } | null {
  const [profile, setProfile] = useState<{ name: string } | null>(null);
  useEffect(() => {
    // The read is attempted in every mode: the demo build has no backend, so it
    // fails silently and the generic label stays. Pretending to know the name in
    // demo would be a lie about data the shell does not have.
    let active = true;
    void loadShellProfile().then((value) => {
      if (active) setProfile(value);
    });
    return () => { active = false; };
  }, []);
  return profile;
}

/** Two letters from the club name, used for the rail avatar. */
function initialsOf(name: string): string {
  const words = name.trim().split(/\s+/).filter(Boolean);
  if (words.length >= 2) return `${words[0]![0] ?? ''}${words[1]![0] ?? ''}`.toUpperCase();
  return name.trim().slice(0, 2).toUpperCase() || 'CL';
}

export function RequireRole({ role }: { role: 'club' | 'dj' }) {
  const location = useLocation();
  if (!IS_DEMO && !isAuthenticated(role)) {
    const loginPath = role === 'dj' ? '/dj/login' : '/login';
    return <Navigate to={loginPath} replace state={{ from: `${location.pathname}${location.search}${location.hash}` }} />;
  }
  return <Outlet />;
}

type NavItem = { to: string; labelKey: string; icon: `lucide:${string}` };

const NAV_GROUPS: ReadonlyArray<{ labelKey: string; items: ReadonlyArray<NavItem> }> = [
  {
    labelKey: 'nav.workspace',
    items: [
      { to: '/dashboard', labelKey: 'nav.overview', icon: 'lucide:layout-dashboard' },
      { to: '/sessions', labelKey: 'nav.liveRoom', icon: 'lucide:radio-tower' },
    ],
  },
  {
    labelKey: 'nav.manage',
    items: [
      { to: '/djs', labelKey: 'nav.djs', icon: 'lucide:users' },
      { to: '/pricing', labelKey: 'nav.pricing', icon: 'lucide:badge-euro' },
      { to: '/earnings', labelKey: 'nav.earnings', icon: 'lucide:wallet-cards' },
    ],
  },
  {
    labelKey: 'nav.tools',
    items: [
      { to: '/qr', labelKey: 'nav.qrStudio', icon: 'lucide:qr-code' },
      { to: '/dj', labelKey: 'nav.djConsole', icon: 'lucide:disc-3' },
    ],
  },
];

const NAV_ITEMS = NAV_GROUPS.flatMap((group) => group.items);

/** DJ console views. `key` is the hash the console page answers to. */
type DjTab = 'queue' | 'room' | 'session';

const DJ_NAV_ITEMS: ReadonlyArray<{ key: DjTab; labelKey: string; icon: `lucide:${string}` }> = [
  { key: 'queue', labelKey: 'nav.requestQueue', icon: 'lucide:list-music' },
  { key: 'room', labelKey: 'nav.roomControl', icon: 'lucide:radio-tower' },
  { key: 'session', labelKey: 'nav.sessionDetails', icon: 'lucide:history' },
];

/**
 * The club destinations, exactly as the rail lists them.
 *
 * One renderer for the rail and the drawer. The old bottom bar was a second,
 * five-slot list with its own abbreviations ("Room", "Price", "Income"), which
 * is how two destinations became unreachable on a phone and how one link ended
 * up with two names. `closeOnNavigate` stamps `data-nav-close` on each
 * destination — the one thing the drawer needs and the rail does not.
 */
function NavGroups({ closeOnNavigate = false }: { closeOnNavigate?: boolean }) {
  const { t } = useLocale();
  return (
    <>
      {NAV_GROUPS.map((group) => (
        <div key={group.labelKey} className="panel-nav-group">
          <p className="panel-nav-label">{t(group.labelKey)}</p>
          <ul className="panel-nav-list">
            {group.items.map((item) => (
              <li key={item.to}>
                <NavLink
                  to={item.to}
                  data-nav-close={closeOnNavigate ? true : undefined}
                  className={({ isActive }) => `panel-nav-link ${isActive ? 'is-active' : ''}`}
                >
                  <span className="panel-nav-icon"><Icon name={item.icon} size={17} /></span>
                  {t(item.labelKey)}
                </NavLink>
              </li>
            ))}
          </ul>
        </div>
      ))}
    </>
  );
}

/**
 * The DJ console's rail list: buttons, because the console has no routes to
 * navigate to — the hash selects a view. The drawer renders these same two
 * groups rather than a shortened copy, so the queue-monitor note travels too.
 */
function DjNavGroups({ activeTab, onSelect, closeOnNavigate = false }: {
  activeTab: DjTab;
  onSelect: (tab: DjTab) => void;
  closeOnNavigate?: boolean;
}) {
  const { t } = useLocale();
  return (
    <>
      <div className="panel-nav-group">
        <p className="panel-nav-label">{t('nav.workspace')}</p>
        <ul className="panel-nav-list">
          {DJ_NAV_ITEMS.map((item) => (
            <li key={item.key}>
              <button
                type="button"
                aria-current={activeTab === item.key ? 'page' : undefined}
                className={`panel-nav-link ${activeTab === item.key ? 'is-active' : ''}`}
                data-nav-close={closeOnNavigate ? true : undefined}
                onClick={() => onSelect(item.key)}
              >
                <span className="panel-nav-icon"><Icon name={item.icon} size={17} /></span>
                {t(item.labelKey)}
              </button>
            </li>
          ))}
        </ul>
      </div>
      <div className="panel-nav-group">
        <p className="panel-nav-label">{t('nav.queueMonitor')}</p>
        <div className="dj-rail-note">
          <span>{t('nav.refreshNote')}</span>
        </div>
      </div>
    </>
  );
}

const FOCUSABLE = [
  'a[href]',
  'button:not([disabled])',
  'input:not([disabled])',
  'select:not([disabled])',
  'textarea:not([disabled])',
  '[tabindex]:not([tabindex="-1"])',
].join(',');

export default function Layout() {
  const navigate = useNavigate();
  const { t } = useLocale();
  const [commandOpen, setCommandOpen] = useState(false);
  const [commandQuery, setCommandQuery] = useState('');
  const [settingsOpen, setSettingsOpen] = useState(false);
  const profile = useShellProfile();
  const dialogRef = useRef<HTMLElement | null>(null);
  const triggerRef = useRef<HTMLButtonElement | null>(null);
  const previousFocusRef = useRef<HTMLElement | null>(null);

  const signOut = () => {
    clearTokens();
    navigate(IS_DEMO ? '/dashboard' : '/login', { replace: true });
  };

  const clubName = profile?.name?.trim() ?? '';
  const identity: ShellIdentity = {
    account: {
      name: t('account.ownerName'),
      meta: clubName || t('shell.workspaceClub'),
      initials: clubName ? initialsOf(clubName) : 'CL',
      facts: [
        { label: t('account.club'), value: clubName },
        { label: t('account.role'), value: t('account.ownerName') },
      ],
    },
  };

  const filteredCommands = NAV_ITEMS.filter((item) =>
    t(item.labelKey).toLowerCase().includes(commandQuery.toLowerCase()));

  const closeCommand = useCallback(() => {
    setCommandOpen(false);
    // Focus returns to the control that opened the palette.
    const previous = previousFocusRef.current ?? triggerRef.current;
    previous?.focus({ preventScroll: true });
  }, []);

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

  // Background scroll lock: an aria-modal dialog that lets the page behind it
  // scroll is not modal. Restores the previous overflow on close/unmount.
  useEffect(() => {
    if (!commandOpen) return undefined;
    const { body } = document;
    const previousOverflow = body.style.overflow;
    const previousPadding = body.style.paddingRight;
    const scrollbar = window.innerWidth - document.documentElement.clientWidth;
    body.style.overflow = 'hidden';
    if (scrollbar > 0) body.style.paddingRight = `${scrollbar}px`;
    return () => {
      body.style.overflow = previousOverflow;
      body.style.paddingRight = previousPadding;
    };
  }, [commandOpen]);

  // Focus trap: the palette is an aria-modal dialog, so Tab must not escape it.
  useEffect(() => {
    if (!commandOpen) return undefined;
    const container = dialogRef.current;
    if (!container) return undefined;
    const focusable = () => Array.from(container.querySelectorAll<HTMLElement>(FOCUSABLE))
      .filter((node) => node.offsetParent !== null || node === document.activeElement);
    const first = focusable()[0];
    (first ?? container).focus({ preventScroll: true });
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== 'Tab') return;
      const nodes = focusable();
      if (nodes.length === 0) {
        event.preventDefault();
        return;
      }
      const head = nodes[0];
      const tail = nodes[nodes.length - 1];
      if (!head || !tail) return;
      if (event.shiftKey && document.activeElement === head) {
        event.preventDefault();
        tail.focus();
      } else if (!event.shiftKey && document.activeElement === tail) {
        event.preventDefault();
        head.focus();
      }
    };
    document.addEventListener('keydown', onKeyDown);
    return () => document.removeEventListener('keydown', onKeyDown);
  }, [commandOpen]);

  const chooseCommand = (to: string) => {
    setCommandOpen(false);
    setCommandQuery('');
    navigate(to);
  };

  return (
    <div className="panel-shell club-app-shell">
      <aside className="panel-rail">
        <div className="panel-rail-brand">
          <BrandLogo size="small" />
        </div>

        {/* No workspace badge. The club name is already the page identity on
            every screen, and the rail foot names the owner and the club again;
            a third block above the nav was the same two strings twice, and it
            read as an account picker that could not pick anything. */}
        <nav className="panel-rail-nav" aria-label={t('nav.main')}>
          <NavGroups />
        </nav>

        <div className="panel-rail-foot">
          <AccountMenu
            identity={identity.account}
            onOpenSettings={() => setSettingsOpen(true)}
            onSignOut={signOut}
          />
        </div>
      </aside>

      <div className="panel-main">
        <header className="panel-topbar" role="banner">
          {/* The burger is the first control in the top bar and the only way
              into navigation below the rail breakpoint. It renders the rail's
              own groups inside the drawer, so nothing has to be abbreviated to
              fit a fixed slot count and the bottom edge stays free. */}
          <NavDrawer
            label={t('nav.main')}
            triggerLabel={t('nav.drawerOpen')}
            subtitle={clubName || t('shell.workspaceClub')}
          >
            <NavGroups closeOnNavigate />
          </NavDrawer>
          {/* On touch widths the wide field would only push the account out of
              reach, so it collapses to the icon button below. The key shortcut
              still opens the palette. */}
          <button
            ref={triggerRef}
            type="button"
            onClick={() => {
              previousFocusRef.current = document.activeElement instanceof HTMLElement ? document.activeElement : null;
              setCommandOpen(true);
            }}
            className="panel-search club-topbar-search"
            aria-haspopup="dialog"
            aria-expanded={commandOpen}
          >
            <Icon name="lucide:search" size={15} />
            <span>{t('shell.jumpTo')}</span>
            <kbd aria-hidden="true">⌘K</kbd>
          </button>
          <button
            type="button"
            onClick={() => {
              previousFocusRef.current = document.activeElement instanceof HTMLElement ? document.activeElement : null;
              setCommandOpen(true);
            }}
            className="panel-icon-button club-topbar-search-compact"
            aria-haspopup="dialog"
            aria-expanded={commandOpen}
            aria-label={t('shell.searchWorkspace')}
          >
            <Icon name="lucide:search" size={17} />
          </button>

          <div className="panel-topbar-actions">
            <DeploymentPill demoBuild={IS_DEMO} className="test-badge-inline" />
            <ThemeToggle />
            {/* Below the rail breakpoint the rail is gone, so the account block
                reappears here as a compact button — the same menu, not a second
                control with its own behaviour. */}
            <span className="panel-topbar-account">
              <AccountMenu
                variant="compact"
                identity={identity.account}
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
              aria-labelledby="command-dialog-title"
              data-surface="dialog"
              tabIndex={-1}
              onMouseDown={(event) => event.stopPropagation()}
            >
              <h2 id="command-dialog-title" className="sr-only">{t('shell.commandMenu')}</h2>
              <div className="command-search-row">
                <Icon name="lucide:search" size={17} />
                <input
                  autoFocus
                  value={commandQuery}
                  onChange={(event) => setCommandQuery(event.target.value)}
                  placeholder={t('shell.commandSearch')}
                  aria-label={t('shell.searchWorkspace')}
                  data-control="field"
                />
                <kbd>ESC</kbd>
              </div>
              <div className="command-results">
                {filteredCommands.length ? filteredCommands.map((item) => (
                  <button type="button" key={item.to} className="command-result" data-control="button" onClick={() => chooseCommand(item.to)}>
                    <Icon name={item.icon} size={16} />
                    <span>{t(item.labelKey)}</span>
                    <span className="command-result-group">{t('nav.workspace')}</span>
                  </button>
                )) : <p className="command-empty">{t('shell.noMatch')}</p>}
              </div>
            </section>
          </div>
        ) : null}

        <main className="panel-content"><Outlet /></main>
      </div>

      <AccountSettings
        open={settingsOpen}
        identity={identity.account}
        facts={identity.account.facts}
        onClose={() => setSettingsOpen(false)}
        onSignOut={signOut}
      />
    </div>
  );
}

export function DjLayout() {
  const navigate = useNavigate();
  const { t } = useLocale();
  const [settingsOpen, setSettingsOpen] = useState(false);
  // Hash is the small integration contract with DjConsolePage: the shell owns
  // navigation state, while the page maps requests/queue/history to its view.
  const [activeTab, setActiveTab] = useState<DjTab>(() => {
    const value = window.location.hash.slice(1);
    return value === 'room' ? 'room' : value === 'session' || value === 'history' ? 'session' : 'queue';
  });
  useEffect(() => {
    const syncHash = () => {
      const value = window.location.hash.slice(1);
      if (value === 'room') setActiveTab('room');
      else if (value === 'session' || value === 'history') setActiveTab('session');
      else if (value === 'queue' || value === 'requests') setActiveTab('queue');
    };
    window.addEventListener('hashchange', syncHash);
    return () => window.removeEventListener('hashchange', syncHash);
  }, []);

  const followTab = (tab: DjTab) => {
    setActiveTab(tab);
    if (window.location.hash !== `#${tab}`) window.history.pushState(null, '', `#${tab}`);
    window.dispatchEvent(new Event('hashchange'));
  };
  const signOut = () => {
    clearTokens();
    navigate(IS_DEMO ? '/dj' : '/dj/login', { replace: true });
  };

  // No workspace badge: the DJ console is one DJ on one shift, and the page
  // head already says which. A block that names the only possible workspace,
  // and cannot switch to another, only reads as a broken account picker.
  const identity: ShellIdentity = {
    account: {
      initials: 'DJ',
      name: t('account.djName'),
      meta: t('shell.tonightShift'),
      facts: [
        { label: t('account.role'), value: t('account.djName') },
        { label: t('account.workspace'), value: t('shell.workspaceDj') },
      ],
    },
  };

  return (
    <div className="panel-shell dj-app-shell">
      <aside className="panel-rail">
        <div className="panel-rail-brand">
          <BrandLogo size="small" />
        </div>

        <nav className="panel-rail-nav" aria-label={t('nav.djNav')}>
          <DjNavGroups activeTab={activeTab} onSelect={followTab} />
        </nav>

        <div className="panel-rail-foot">
          <AccountMenu
            identity={identity.account}
            onOpenSettings={() => setSettingsOpen(true)}
            onSignOut={signOut}
          />
        </div>
      </aside>

      <div className="panel-main">
        <header className="panel-topbar">
          {/* Same drawer as the club shell, carrying this console's own two
              groups — the workspace views AND the queue-monitor note — so the
              rail is not a wider version of a shorter phone list. */}
          <NavDrawer
            label={t('nav.djNav')}
            triggerLabel={t('nav.drawerOpen')}
            subtitle={t('shell.tonightShift')}
          >
            <DjNavGroups activeTab={activeTab} onSelect={followTab} closeOnNavigate />
          </NavDrawer>
          <span className="panel-search dj-topbar-status-pill" aria-hidden="true" style={{ cursor: 'default' }}>
            <Icon name="lucide:radio-tower" size={15} />
            <span>{t('nav.liveRequests')}</span>
          </span>
          <div className="panel-topbar-actions">
            <DeploymentPill demoBuild={IS_DEMO} className="test-badge-inline" />
            <ThemeToggle />
            <span className="panel-topbar-account">
              <AccountMenu
                variant="compact"
                identity={identity.account}
                onOpenSettings={() => setSettingsOpen(true)}
                onSignOut={signOut}
              />
            </span>
          </div>
        </header>
        <main className="panel-content"><Outlet /></main>
      </div>

      <AccountSettings
        open={settingsOpen}
        identity={identity.account}
        facts={identity.account.facts}
        onClose={() => setSettingsOpen(false)}
        onSignOut={signOut}
      />
    </div>
  );
}
