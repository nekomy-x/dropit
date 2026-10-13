import { Navigate, Route, Routes } from 'react-router-dom';
import Layout from './components/Layout';
import LoginPage from './pages/LoginPage';
import DashboardPage from './pages/DashboardPage';
import ClubsPage from './pages/ClubsPage';
import DjsPage from './pages/DjsPage';
import GuestsPage from './pages/GuestsPage';
import PaymentsPage from './pages/PaymentsPage';
import IncidentsPage from './pages/IncidentsPage';
import AuditLogPage from './pages/AuditLogPage';
import PlatformSettingsPage from './pages/PlatformSettingsPage';

export default function App() {
  return (
    <Routes>
      {/* Login is standalone; admin 2FA is mandatory (Part I §3). */}
      <Route path="/login" element={<LoginPage />} />

      <Route element={<Layout />}>
        <Route path="/dashboard" element={<DashboardPage />} />
        <Route path="/clubs" element={<ClubsPage />} />
        <Route path="/djs" element={<DjsPage />} />
        <Route path="/guests" element={<GuestsPage />} />
        <Route path="/payments" element={<PaymentsPage />} />
        <Route path="/incidents" element={<IncidentsPage />} />
        <Route path="/audit-log" element={<AuditLogPage />} />
        <Route path="/platform-settings" element={<PlatformSettingsPage />} />
      </Route>

      <Route path="*" element={<Navigate to="/dashboard" replace />} />
    </Routes>
  );
}
