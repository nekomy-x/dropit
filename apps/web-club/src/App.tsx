import { Navigate, Route, Routes } from 'react-router-dom';
import Layout, { DjLayout, RequireRole } from './components/Layout';
import LoginPage from './pages/LoginPage';
import DashboardPage from './pages/DashboardPage';
import SessionsPage from './pages/SessionsPage';
import PricingPage from './pages/PricingPage';
import DjsPage from './pages/DjsPage';
import EarningsPage from './pages/EarningsPage';
import QrStudioPage from './pages/QrStudioPage';
import StartPage from './pages/connect/StartPage';
import ClubStepPage from './pages/connect/ClubStepPage';
import VenueStepPage from './pages/connect/VenueStepPage';
import QrStepPage from './pages/connect/QrStepPage';
import ConnectedPage from './pages/connect/ConnectedPage';
import DjConsolePage from './pages/DjConsolePage';
import DjLoginPage from './pages/DjLoginPage';
import DjInvitePage from './pages/DjInvitePage';
import RegisterPage from './pages/RegisterPage';
import ForgotPasswordPage from './pages/ForgotPasswordPage';
import ResetPasswordPage from './pages/ResetPasswordPage';
import { IS_DEMO } from './app-mode';

export default function App() {
  if (IS_DEMO) {
    return (
      <Routes>
        <Route path="/" element={<Navigate to="/dj" replace />} />
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
      {/* Public "Connect" flow (no auth): landing + 3-step onboarding wizard.
          It is the marketing entry point, so it owns the site root. */}
      <Route path="/" element={<Navigate to={IS_DEMO ? '/dj' : '/connect'} replace />} />
      {IS_DEMO ? <Route path="/login" element={<Navigate to="/dashboard" replace />} /> : null}
      {IS_DEMO ? <Route path="/dj/login" element={<Navigate to="/dj" replace />} /> : null}
      <Route path="/connect" element={<StartPage />} />
      <Route path="/connect/club" element={<ClubStepPage />} />
      <Route path="/connect/venue" element={<VenueStepPage />} />
      <Route path="/connect/qr" element={<QrStepPage />} />
      <Route path="/connect/connected" element={<ConnectedPage />} />

      {/* Login is standalone (no sidebar) — Bearer token is stored in memory
          after the password/2FA steps (see api/client.ts). */}
      <Route path="/login" element={<LoginPage />} />
      {/* Open registration (Test Mode stand): one click creates the account
          and returns a TokenPair, so no login round-trip is needed. */}
      <Route path="/register" element={<RegisterPage />} />
      <Route path="/dj/login" element={<DjLoginPage />} />
      <Route path="/dj/invite/:token" element={<DjInvitePage />} />

      {/* Password recovery (skin B): request a link, then choose a password.
          Public: a locked-out account cannot authenticate first. */}
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
        </Route>
      </Route>

      <Route element={<RequireRole role="dj" />}>
        <Route element={<DjLayout />}>
          <Route path="/dj" element={<DjConsolePage />} />
        </Route>
      </Route>

      {/* Sessions is the only fully live club flow (F2) — the fallback. */}
      <Route path="*" element={<Navigate to="/sessions" replace />} />
    </Routes>
  );
}
