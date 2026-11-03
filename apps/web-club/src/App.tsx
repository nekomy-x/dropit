import { Navigate, Route, Routes, useSearchParams } from 'react-router-dom';
import Layout, { DjLayout, RequireRole } from './components/Layout';
import LoginPage from './pages/LoginPage';
import DashboardPage from './pages/DashboardPage';
import SessionsPage from './pages/SessionsPage';
import PricingPage from './pages/PricingPage';
import DjsPage from './pages/DjsPage';
import EarningsPage from './pages/EarningsPage';
import QrStudioPage from './pages/QrStudioPage';
import DeveloperPage from './pages/DeveloperPage';
import HubPage from './pages/HubPage';
import DownloadPage from './pages/DownloadPage';
import StartPage from './pages/connect/StartPage';
import ClubStepPage from './pages/connect/ClubStepPage';
import VenueStepPage from './pages/connect/VenueStepPage';
import QrStepPage from './pages/connect/QrStepPage';
import ConnectedPage from './pages/connect/ConnectedPage';
import DjConsolePage from './pages/DjConsolePage';
import DjConsoleEntry from './components/DjConsoleEntry';
import DjLoginPage from './pages/DjLoginPage';
import DjInvitePage from './pages/DjInvitePage';
import ClubInvitePage from './pages/ClubInvitePage';
import RegisterPage from './pages/RegisterPage';
import ForgotPasswordPage from './pages/ForgotPasswordPage';
import ResetPasswordPage from './pages/ResetPasswordPage';
import { IS_DEMO } from './app-mode';

/**
 * Старая форма ссылки-приглашения DJ: `/dj/invite?token=<hex>`.
 *
 * Письма клубов собираются сервером и содержат токен в query
 * (docs/audit/web-club-gaps.md §3 №26, §6.10), а SPA ждёт путь
 * `/dj/invite/:token`. Вместо того чтобы ломать уже отправленные письма,
 * query-форма переводится в путь: обе ссылки открывают одну форму.
 * Токен копируется без изменений — invite/accept принимает его как есть.
 * Ссылка без токена ведёт на вход: показать форму, которую нельзя отправить,
 * было бы тупиком.
 */
function DjInviteFromQuery() {
  const [params] = useSearchParams();
  const token = params.get('token')?.trim();
  if (!token) return <Navigate to="/dj/login" replace />;
  return <Navigate to={`/dj/invite/${encodeURIComponent(token)}`} replace />;
}

/**
 * Старая форма ссылки-приглашения клуба: `/club/invite?token=<hex>`.
 *
 * Письма `club_invite` собираются сервером из `invite_url` и уже разосланы в
 * путь-форме; query-форма остаётся только для ссылок, скопированных из более
 * ранних писем и стендов. Обе открывают одну страницу активации. Ссылка без
 * токена ведёт на вход: пустая форма активации — тупик.
 */
function ClubInviteFromQuery() {
  const [params] = useSearchParams();
  const token = params.get('token')?.trim();
  if (!token) return <Navigate to="/login" replace />;
  return <Navigate to={`/club/invite/${encodeURIComponent(token)}`} replace />;
}

export default function App() {
  if (IS_DEMO) {
    return (
      <Routes>
        <Route path="/" element={<Navigate to="/dj" replace />} />
        {/* Public front door, reachable in the demo build for design review. */}
        <Route path="/hub" element={<HubPage />} />
        <Route path="/download" element={<DownloadPage />} />
        <Route path="/connect" element={<StartPage />} />
        <Route path="/connect/club" element={<ClubStepPage />} />
        <Route path="/connect/venue" element={<VenueStepPage />} />
        <Route path="/connect/qr" element={<QrStepPage />} />
        <Route path="/connect/connected" element={<ConnectedPage />} />
        <Route path="/login" element={<Navigate to="/dashboard" replace />} />
        <Route path="/register" element={<Navigate to="/dashboard" replace />} />
        <Route path="/reset-password" element={<Navigate to="/dashboard" replace />} />
        <Route path="/reset-password/:token" element={<ResetPasswordPage />} />
        <Route path="/dj/login" element={<Navigate to="/dj" replace />} />
        <Route element={<Layout />}>
          <Route path="/dashboard" element={<DashboardPage />} />
          <Route path="/sessions" element={<SessionsPage />} />
          <Route path="/pricing" element={<PricingPage />} />
          <Route path="/djs" element={<DjsPage />} />
          <Route path="/earnings" element={<EarningsPage />} />
          <Route path="/qr" element={<QrStudioPage />} />
          {/* API v1: scoped keys, the generated reference and the guides. */}
          <Route path="/developers" element={<DeveloperPage />} />
          <Route path="*" element={<Navigate to="/dashboard" replace />} />
        </Route>
        <Route element={<DjLayout />}>
          <Route path="/dj" element={<DjConsolePage />} />
        </Route>
      </Routes>
    );
  }

  return (
    <Routes>
      {/* Public front door: the guest/club gate at the domain root. The club
          side of the gate hands over to the existing `/connect` landing. */}
      <Route path="/" element={<HubPage />} />
      <Route path="/hub" element={<Navigate to="/" replace />} />
      <Route path="/download" element={<DownloadPage />} />
      <Route path="/connect" element={<StartPage />} />
      <Route path="/connect/club" element={<ClubStepPage />} />
      <Route path="/connect/venue" element={<VenueStepPage />} />
      <Route path="/connect/qr" element={<QrStepPage />} />
      <Route path="/connect/connected" element={<ConnectedPage />} />

      {/* Login is standalone (no sidebar) — Bearer token is stored in memory
          after the password/2FA steps (see api/client.ts). */}
      <Route path="/login" element={<LoginPage />} />
      {/* Club application, not open registration: the form submits an
          application and the account is created only after approval, on the
          invitation page below (docs/onboarding-contract.md). */}
      <Route path="/register" element={<RegisterPage />} />
      <Route path="/dj/login" element={<DjLoginPage />} />
      {/* Both invite forms: the path the SPA builds and the `?token=`
          query already sitting in mailboxes. */}
      <Route path="/dj/invite" element={<DjInviteFromQuery />} />
      <Route path="/dj/invite/:token" element={<DjInvitePage />} />
      <Route path="/club/invite" element={<ClubInviteFromQuery />} />
      <Route path="/club/invite/:token" element={<ClubInvitePage />} />

      {/* Password recovery: request a link, then choose a password. Public: a
          locked-out account cannot authenticate first. */}
      <Route path="/reset-password" element={<ForgotPasswordPage />} />
      <Route path="/reset-password/:token" element={<ResetPasswordPage />} />

      <Route element={<RequireRole role="club" />}>
        <Route element={<Layout />}>
          <Route path="/dashboard" element={<DashboardPage />} />
          <Route path="/sessions" element={<SessionsPage />} />
          <Route path="/pricing" element={<PricingPage />} />
          <Route path="/djs" element={<DjsPage />} />
          <Route path="/earnings" element={<EarningsPage />} />
          <Route path="/qr" element={<QrStudioPage />} />
          {/* API v1: the club's scoped keys and the in-panel documentation. */}
          <Route path="/developers" element={<DeveloperPage />} />
        </Route>
      </Route>

      {/* The console is the DJ role's surface, but a club account reaches it
          through a server-issued handoff (DjConsoleEntry) instead of a second
          login: the club stays signed in, the console runs as the shift's DJ. */}
      <Route element={<DjConsoleEntry />}>
        <Route element={<DjLayout />}>
          <Route path="/dj" element={<DjConsolePage />} />
        </Route>
      </Route>

      {/* Sessions is the only fully live club flow (F2) — the fallback. */}
      <Route path="*" element={<Navigate to="/sessions" replace />} />
    </Routes>
  );
}
