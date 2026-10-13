import { Navigate, Route, Routes } from 'react-router-dom';
import Layout from './components/Layout';
import LoginPage from './pages/LoginPage';
import DashboardPage from './pages/DashboardPage';
import SessionsPage from './pages/SessionsPage';
import PricingPage from './pages/PricingPage';
import DjsPage from './pages/DjsPage';
import EarningsPage from './pages/EarningsPage';

export default function App() {
  return (
    <Routes>
      {/* Login is standalone (no sidebar) until the session cookie exists. */}
      <Route path="/login" element={<LoginPage />} />

      <Route element={<Layout />}>
        <Route path="/dashboard" element={<DashboardPage />} />
        <Route path="/sessions" element={<SessionsPage />} />
        <Route path="/pricing" element={<PricingPage />} />
        <Route path="/djs" element={<DjsPage />} />
        <Route path="/earnings" element={<EarningsPage />} />
      </Route>

      <Route path="*" element={<Navigate to="/dashboard" replace />} />
    </Routes>
  );
}
