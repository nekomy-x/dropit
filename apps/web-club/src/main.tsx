import { lazy, StrictMode, Suspense } from 'react';
import { createRoot } from 'react-dom/client';
import { BrowserRouter, Route, Routes } from 'react-router-dom';
import DeploymentStatusBar, { useDeploymentStatus } from './components/DeploymentStatus';
import { IS_DEMO } from './app-mode';
import { LocaleProvider } from './locale';
import { installPointerSelectionGuard } from './pointer-guard';
import './index.css';
import './operations.css';

// A QR guest must not download the club/admin tooling before ordering.
const App = lazy(() => import('./App'));
const GuestCheckoutPage = lazy(() => import('./pages/GuestCheckoutPage'));

// A click on a control is a click, not a selection: clear any selection left
// over from the page before the control runs. See pointer-guard.ts.
installPointerSelectionGuard();

/**
 * The bottom band, for `maintenance` only. The test-stand fact is the corner
 * pill (`DeploymentPill`), which the product owner asked for on every screen;
 * two indicators for one fact read as two facts, and the band pushed content up
 * on short viewports.
 */
function MaintenanceBand({ demoBuild }: { demoBuild: boolean }) {
  const info = useDeploymentStatus(demoBuild);
  if (info.mode !== 'maintenance') return null;
  return <DeploymentStatusBar info={info} />;
}

const rootElement = document.getElementById('root');
if (!rootElement) {
  throw new Error('Root element #root not found in index.html');
}

createRoot(rootElement).render(
  <StrictMode>
    <BrowserRouter>
      <LocaleProvider>
        {/* "Test stand" is the corner pill on every screen; the bottom band is
            reserved for an announced maintenance window. */}
        <MaintenanceBand demoBuild={IS_DEMO} />
        <Suspense fallback={<div className="panel-loading" role="status">DROP IT…</div>}>
          <Routes>
            <Route path="/join" element={<GuestCheckoutPage />} />
            <Route path="/room" element={<GuestCheckoutPage />} />
            <Route path="/guest/payment-return" element={<GuestCheckoutPage />} />
            <Route path="*" element={<App />} />
          </Routes>
        </Suspense>
      </LocaleProvider>
    </BrowserRouter>
  </StrictMode>,
);
