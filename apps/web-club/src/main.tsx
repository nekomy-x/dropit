import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { BrowserRouter } from 'react-router-dom';
import App from './App';
import DeploymentStatusBar, { useDeploymentStatus } from './components/DeploymentStatus';
import { IS_DEMO } from './app-mode';
import { LocaleProvider } from './locale';
import { installPointerSelectionGuard } from './pointer-guard';
import './index.css';
import './operations.css';

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
        <App />
      </LocaleProvider>
    </BrowserRouter>
  </StrictMode>,
);
