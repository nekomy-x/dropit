/**
 * DeploymentStatus — the one strip that says what this deployment is.
 *
 * Three states, one component, mounted once at the app root:
 *
 *   test         a stand — open signup, mocked payments, demo data.
 *   maintenance  the platform is being worked on; faults and interruptions
 *                are possible.
 *   normal       production: the strip is not rendered at all.
 *
 * It is fixed to the bottom edge of the viewport, above every panel surface,
 * and it cannot be dismissed: hiding it would let demo data or a degraded
 * service read as production. It is the only deployment indicator in the
 * product — there is no second corner badge, pill or inline footnote saying
 * the same thing anywhere else. The bottom offsets of the shell live in
 * `status-bar.css` and key off `<html data-deployment-bar="…">`, which this
 * component maintains.
 *
 * Chrome plus a chip
 * ------------------
 * The band is persistent chrome — the same job as the rail and the mobile
 * navigation — so it is drawn on `--c-rail` behind a single top hairline, not
 * on a tinted surface of its own. Two earlier versions invented a register the
 * product does not have: a full-bleed saturated fill outranked the live state
 * it reported on, and a 10% tone wash turned muddy brown in dark theme and read
 * as a smudge along the bottom edge.
 *
 * The state itself is the product's one status primitive: `.chip`. `test` uses
 * `chip-warn`, `maintenance` uses `chip-error`, so a deployment state looks
 * exactly like a DJ status, a session state or a failed payout. Words carry the
 * state; the chip's tone is the second signal, as everywhere else. The facts
 * sit beside it on the muted text ramp, separated by a hairline rule rather
 * than a dot.
 *
 * Where the fact comes from
 * -------------------------
 * `/v1/status` is the resolved mode from the gateway: an admin setting
 * (`platform_settings.runtime_mode`), the operator's `DROPIT_MAINTENANCE`
 * flag, or the adaptive default (open signup / mock payments mean `test`,
 * otherwise `normal`). One probe per page load is shared by the whole app.
 *
 * An older server without `/v1/status` falls back to `/readyz`, which has
 * reported open-signup and payment facts since the first test stand; that
 * keeps a stand from being silently labelled production. A failed probe
 * renders nothing — an unknown environment is not a licence to claim either
 * "production" or "test".
 *
 * The demo build (`npm run dev:demo`) never has a backend and is a stand by
 * definition, so it starts at `test` without waiting for the network.
 */
import { useEffect, useRef, useState } from 'react';
import { useLocale } from '../locale';

export type DeploymentMode = 'test' | 'maintenance' | 'normal';
export type DeploymentSource = 'demo-build' | 'admin' | 'env' | 'adaptive' | 'fallback' | 'unknown';

export interface DeploymentStatusInfo {
  mode: DeploymentMode;
  source: DeploymentSource;
  openSignup: boolean;
  payments: 'mock' | 'live';
}

const UNKNOWN: DeploymentStatusInfo = {
  mode: 'normal',
  source: 'unknown',
  openSignup: false,
  payments: 'live',
};

const DEMO: DeploymentStatusInfo = {
  mode: 'test',
  source: 'demo-build',
  openSignup: true,
  payments: 'mock',
};

function isMode(value: unknown): value is DeploymentMode {
  return value === 'test' || value === 'maintenance' || value === 'normal';
}

function isSource(value: unknown): value is DeploymentSource {
  return value === 'admin' || value === 'env' || value === 'adaptive';
}

function isPaymentMode(value: unknown): value is 'mock' | 'live' {
  return value === 'mock' || value === 'live';
}

async function readJson(path: string): Promise<Record<string, unknown> | null> {
  try {
    const response = await fetch(path, { headers: { Accept: 'application/json' } });
    if (!response.ok) return null;
    return (await response.json()) as Record<string, unknown>;
  } catch {
    return null;
  }
}

/** One probe per page load, shared by every consumer of the hook. */
let probe: Promise<DeploymentStatusInfo | null> | null = null;

async function readDeploymentStatus(): Promise<DeploymentStatusInfo | null> {
  if (!probe) {
    probe = (async () => {
      // The resolved answer first: only the gateway knows about an admin-set
      // maintenance window, and `auto` has already been resolved there.
      const status = await readJson('/v1/status');
      if (status && isMode(status.mode) && isSource(status.source)) {
        return {
          mode: status.mode,
          source: status.source,
          openSignup: status.open_signup === true,
          payments: isPaymentMode(status.payments) ? status.payments : 'mock',
        } satisfies DeploymentStatusInfo;
      }

      // Fallback for a gateway that predates `/v1/status`. The two facts are
      // enough to recognise a stand; the absence of `maintenance` is honest —
      // an old server cannot be in a mode it does not know about.
      const ready = await readJson('/readyz');
      if (ready) {
        const payments: 'mock' | 'live' = ready.payments === 'live' ? 'live' : 'mock';
        const openSignup = ready.open_signup === true;
        return {
          mode: openSignup || payments === 'mock' ? 'test' : 'normal',
          source: 'fallback',
          openSignup,
          payments,
        } satisfies DeploymentStatusInfo;
      }

      return null;
    })();
  }
  return probe;
}

/**
 * @param demoBuild pass the app's `IS_DEMO` — a demo build is a stand by
 *   definition and does not wait for the network.
 */
export function useDeploymentStatus(demoBuild: boolean): DeploymentStatusInfo {
  const [info, setInfo] = useState<DeploymentStatusInfo>(() => (demoBuild ? DEMO : UNKNOWN));

  useEffect(() => {
    if (demoBuild) return undefined;
    let cancelled = false;
    void readDeploymentStatus().then((server) => {
      if (!cancelled && server) setInfo(server);
    });
    return () => { cancelled = true; };
  }, [demoBuild]);

  return info;
}

/** The facts as separate phrases, so a wrap breaks between them, never inside. */
function testFacts(info: DeploymentStatusInfo, t: (key: string) => string): string[] {
  return [
    info.openSignup ? t('deployment.signupOpen') : t('deployment.signupClosed'),
    info.payments === 'mock' ? t('deployment.paymentsMock') : t('deployment.paymentsLive'),
  ];
}

/**
 * App-level mount: reads the mode once and renders the strip when there is
 * something to say. Mount it above the router (inside the locale provider) so
 * every screen — panel, auth, public Connect — carries the same state.
 */
export function DeploymentStatus({ demoBuild }: { demoBuild: boolean }) {
  const info = useDeploymentStatus(demoBuild);
  return <DeploymentStatusBar info={info} />;
}

/**
 * The strip. `position: fixed` and the bottom offsets belong to the stylesheet;
 * this component owns the words, the tone, the document flag — and the measured
 * height, because the sentence wraps differently per language and per width.
 *
 * The wrappers reserve `--deployment-bar-h` from CSS, which cannot know how many
 * lines the copy takes. So the band publishes its real height into
 * `--deployment-bar-pad` and every offset below writes
 * `max(--deployment-bar-h, --deployment-bar-pad)`: the CSS guess covers the
 * first paint, the measurement keeps the last row of content above the strip
 * after it.
 */
export default function DeploymentStatusBar({ info }: { info: DeploymentStatusInfo }) {
  const { t } = useLocale();
  const visible = info.mode !== 'normal';
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const root = document.documentElement;
    if (!visible) {
      delete root.dataset.deploymentBar;
      root.style.removeProperty('--deployment-bar-pad');
      return undefined;
    }
    root.dataset.deploymentBar = info.mode;
    return () => {
      delete root.dataset.deploymentBar;
      root.style.removeProperty('--deployment-bar-pad');
    };
  }, [info.mode, visible]);

  useEffect(() => {
    const node = ref.current;
    if (!node) return undefined;
    const root = document.documentElement;
    const publish = () => {
      root.style.setProperty('--deployment-bar-pad', `${node.offsetHeight}px`);
    };
    publish();
    if (typeof ResizeObserver === 'undefined') return undefined;
    const observer = new ResizeObserver(publish);
    observer.observe(node);
    return () => { observer.disconnect(); root.style.removeProperty('--deployment-bar-pad'); };
  }, [visible, info.mode, info.openSignup, info.payments]);

  if (!visible) return null;

  const label = info.mode === 'test' ? t('deployment.testLabel') : t('deployment.maintenanceLabel');
  const facts = info.mode === 'test' ? testFacts(info, t) : [t('deployment.maintenanceDetail')];
  const detail = facts.join(' · ');
  const chipTone = info.mode === 'test' ? 'chip-warn' : 'chip-error';

  return (
    <div
      ref={ref}
      className={`deployment-bar deployment-bar--${info.mode}`}
      role="status"
      title={`${label} · ${detail}`}
      data-deployment-mode={info.mode}
      data-deployment-source={info.source}
    >
      <span className={`chip ${chipTone} deployment-bar-chip`}>{label}</span>
      <span className="deployment-bar-rule" aria-hidden="true" />
      <span className="deployment-bar-detail">
        {facts.map((fact, index) => (
          <span key={`${index}-${fact}`} className="deployment-bar-fact">{fact}</span>
        ))}
      </span>
    </div>
  );
}

/**
 * DeploymentPill — "this deployment is not production" as one small label.
 *
 * The product owner asked for exactly this shape: a small pill with an orange
 * outline in the corner of every screen, not a band across the bottom. It sits
 * in the corner cluster of an auth screen and inline in a panel top bar, so the
 * fact is present on every route without taking a row away from the content.
 *
 * One fact, one shape: the bottom band (`DeploymentStatusBar`) is kept for
 * `maintenance` only, which is a different fact and needs the room for words.
 * See `MaintenanceBand`.
 */
export function DeploymentPill({
  demoBuild,
  className = '',
}: {
  demoBuild: boolean;
  className?: string;
}) {
  const info = useDeploymentStatus(demoBuild);
  const { t } = useLocale();
  if (info.mode !== 'test') return null;
  const facts = [
    info.openSignup ? t('deployment.signupOpen') : t('deployment.signupClosed'),
    info.payments === 'mock' ? t('deployment.paymentsMock') : t('deployment.paymentsLive'),
  ].join(' · ');
  return (
    <span
      className={`test-badge ${className}`.trim()}
      title={`${t('deployment.testLabel')} · ${facts}`}
      data-test-stand=""
      data-payments={info.payments}
    >
      <span className="test-badge-dot" aria-hidden="true" />
      <span>{t('deployment.testLabel')}</span>
    </span>
  );
}
