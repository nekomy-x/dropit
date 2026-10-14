import { Navigate, Route, Routes } from 'react-router-dom';
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

export default function App() {
  return (
    <Routes>
      {/* Login is standalone; admin 2FA is mandatory (Part I §3) — the
          Bearer token is stored in memory after the TOTP step. */}
      <Route path="/login" element={<LoginPage />} />

      <Route element={<Layout />}>
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
      </Route>

      {/* Payouts is the only fully live admin surface (F6) — the default. */}
      <Route path="*" element={<Navigate to="/payouts" replace />} />
    </Routes>
  );
}
