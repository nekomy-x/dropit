import { useEffect } from 'react';
import { Navigate, Route, Routes, useLocation } from 'react-router-dom';
import './admin-theme.css';
import './control-center.css';
import './admin-shell.css';
import './overview.css';
import Layout from './components/Layout';
import LoginPage from './pages/LoginPage';
import DashboardPage from './pages/DashboardPage';
import ClubsPage from './pages/ClubsPage';
import DjsPage from './pages/DjsPage';
import GuestsPage from './pages/GuestsPage';
import PaymentsPage from './pages/PaymentsPage';
import PayoutsPage from './pages/PayoutsPage';
import IncidentsPage from './pages/IncidentsPage';
import AuditLogPage from './pages/AuditLogPage';
import PlatformSettingsPage from './pages/PlatformSettingsPage';
import MailPage from './pages/MailPage';
import OnboardingPage from './pages/OnboardingPage';
import ApiKeysPage from './pages/ApiKeysPage';
import AdminOverviewPage from './pages/AdminOverviewPage';
import AdminManagementView from './pages/AdminManagementView';
import AdminActivityPage from './pages/AdminActivityPage';
import AdminAccessPage from './pages/AdminAccessPage';
import AdminSettingsPage from './pages/AdminSettingsPage';
import { IS_DEMO } from './app-mode';
import { isAuthenticated } from './api/client';

/** Section navigation for the control center, driven by the route hash.
 * The rail in `Layout` is the only navigation control — this list exists so a
 * hash that does not name a section falls back to the overview. */
const CONTROL_SECTIONS: ReadonlyArray<{ key: string; label: string }> = [
  { key: 'overview', label: 'Overview' },
  { key: 'users', label: 'Users' },
  { key: 'djs', label: 'DJs' },
  { key: 'clubs', label: 'Clubs' },
  { key: 'transactions', label: 'Transactions' },
  { key: 'activity', label: 'Audit log' },
  { key: 'access', label: 'Roles' },
  { key: 'settings', label: 'Settings' },
];

/** The operations control center is a local-first preview surface. Each section
 * is a real route (`/manage/users`), not a hash: the rail state survives a
 * reload, links are shareable, and a screenshot tool can address them directly.
 *
 * The `manage/` prefix is deliberate — the live panel routes (`/clubs`, `/djs`,
 * `/payouts`, ...) are real screens, and a bare `/:section` route would quietly
 * render those instead of the management view for the two names they share.
 */
function ControlCenter() {
  const location = useLocation();
  const rawSection = location.pathname.split('/')[2] || 'overview';
  const section = CONTROL_SECTIONS.some(({ key }) => key === rawSection) ? rawSection : 'overview';

  useEffect(() => { window.scrollTo({ top: 0 }); }, [section]);

  return (
    <div className="control-center">
      {section === 'overview' ? <AdminOverviewPage /> : null}
      {section === 'users' ? <AdminManagementView view="users" /> : null}
      {section === 'djs' ? <AdminManagementView view="djs" /> : null}
      {section === 'clubs' ? <AdminManagementView view="clubs" /> : null}
      {section === 'transactions' ? <AdminManagementView view="transactions" /> : null}
      {section === 'activity' ? <AdminActivityPage /> : null}
      {section === 'access' ? <AdminAccessPage /> : null}
      {section === 'settings' ? <AdminSettingsPage /> : null}
    </div>
  );
}

function RequireAdmin() {
  const location = useLocation();
  if (IS_DEMO) return <Navigate to="/" replace />;
  if (!isAuthenticated()) return <Navigate to="/login" replace state={{ from: `${location.pathname}${location.search}${location.hash}` }} />;
  return <Layout />;
}

export default function App() {
  if (IS_DEMO) {
    // Demo mode keeps the real shell: the rail is the navigation, so the
    // control center renders inside it instead of owning a second tab bar.
    return (
      <Routes>
        <Route element={<Layout />}>
          <Route path="/" element={<ControlCenter />} />
          <Route path="/manage/:section" element={<ControlCenter />} />
          {/* Mail reads the live API, so a demo build shows its real answer or
              an honest error state — never a seeded queue. */}
          <Route path="/mail" element={<MailPage />} />
          {/* Key oversight is a live API surface too: the demo build shows the
              real answer or an honest error, never a seeded list. */}
          <Route path="/api-keys" element={<ApiKeysPage />} />
          {/* The onboarding queue is a live API surface too: the demo build
              shows the real answer or an honest error, never a seeded list. */}
          <Route path="/onboarding" element={<OnboardingPage />} />
          <Route path="*" element={<Navigate to="/" replace />} />
        </Route>
      </Routes>
    );
  }

  return (
    <Routes>
      {/* Login is standalone; admin 2FA is mandatory (Part I §3) — the
          Bearer token is stored in memory after the TOTP step. */}
      <Route path="/login" element={<LoginPage />} />

      <Route element={<RequireAdmin />}>
        <Route path="/dashboard" element={<DashboardPage />} />
        <Route path="/clubs" element={<ClubsPage />} />
        <Route path="/djs" element={<DjsPage />} />
        <Route path="/guests" element={<GuestsPage />} />
        <Route path="/payments" element={<PaymentsPage />} />
        {/* The two LIVE admin functions (F6 backend). */}
        <Route path="/payouts" element={<PayoutsPage />} />
        <Route path="/incidents" element={<IncidentsPage />} />
        <Route path="/audit-log" element={<AuditLogPage />} />
        <Route path="/platform-settings" element={<PlatformSettingsPage />} />
        {/* Developer keys: read-only oversight of every club, plus revocation. */}
        <Route path="/api-keys" element={<ApiKeysPage />} />
        {/* Mail is a LIVE API surface (docs/mail-infra-contract.md §8). */}
        <Route path="/mail" element={<MailPage />} />
        {/* Club applications and their invite links
            (docs/onboarding-contract.md). */}
        <Route path="/onboarding" element={<OnboardingPage />} />

        {/* Operations control center: local-first preview with full CRUD.
            Paths are relative to the router basename (/admin/), so `/admin/users`
            in the browser is `/users` here. */}
        <Route path="/" element={<ControlCenter />} />
        <Route path="/manage/:section" element={<ControlCenter />} />
        <Route path="/requests" element={<AdminManagementView view="transactions" />} />
        <Route path="/moderation" element={<AdminActivityPage />} />
        <Route path="/roles" element={<AdminAccessPage />} />
        <Route path="/feature-flags" element={<AdminSettingsPage />} />
        <Route path="/integrations" element={<PlatformSettingsPage />} />
      </Route>

      {/* Payouts is the only fully live admin surface (F6) — the default. */}
      <Route path="*" element={<Navigate to="/payouts" replace />} />
    </Routes>
  );
}
