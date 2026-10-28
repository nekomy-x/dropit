/**
 * Honest data hook for the club Overview board.
 *
 * The public shape is FIXED by docs/ui/club-redesign/01-contract.md §7 — the
 * board page is written against it, so do not rename or add fields without the
 * Lead. The one rule this file exists for: **never present fabricated or stale
 * data as live**.
 *
 * Backing endpoints (all real, see crates/gateway-api/src/panel_routes.rs):
 *   - GET /v1/club/dashboard  → open session, rolling-24h request count,
 *                               all-time settled club splits
 *   - GET /v1/club/profile    → club name
 *   - GET /v1/club/djs        → lineup
 *   - GET /v1/club/pricing    → price history; the active row is `validTo === null`
 *   - POST /v1/club/sessions/:id/qr/generate → one-time QR token (on demand only)
 *
 * Honesty rules enforced here:
 *   - `lastFetchedAt` is stamped only after a real await resolved; it is never
 *     advanced by a timer.
 *   - No `setInterval` issues network requests. There is no polling at all.
 *   - On failure a metric becomes `unavailable` with `value: null`; stale
 *     numbers are NOT relabelled live.
 *   - No `Math.random()`, no synthetic crowd/energy/tips series.
 *   - Zones have no endpoint, so they stay a local `preview` block and are
 *     never presented as live.
 *
 * Captions mirror what the SQL actually computes:
 *   - netSettledMinor: `split.amount_minor - reversed_minor` for payee_type
 *     'club', with NO time filter → "all time, settled".
 *   - requests24h: `request.created_at > now() - interval '24 hours'` joined to
 *     the club's sessions, with NO payment filter → "last 24 h" (never "paid").
 */

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useLocale } from '../locale';
import {
  ApiRequestError,
  generateSessionQr,
  getClubDashboard,
  getDjs,
  getPricing,
  getProfile,
  type Dj,
  type PricingPolicy,
  type SessionStatus,
} from '../api/client';

// ---------------------------------------------------------------------------
// Contract types — 01-contract.md §7 (shape fixed by the Lead)
// ---------------------------------------------------------------------------

export type MetricSource = 'live' | 'preview' | 'unavailable';

export interface BoardMetric<T> {
  value: T | null;
  source: MetricSource;
  /** Фактический период/фильтр для честной подписи, напр. "all time, settled". */
  caption: string;
}

export interface BoardSession {
  id: string;
  status: 'created' | 'active' | 'paused_acceptance' | 'closed';
}

export interface BoardDj {
  id: string;
  displayName: string;
  email: string;
  status: 'invited' | 'active' | 'suspended' | 'blocked';
  /** true, когда у DJ есть незакрытая смена (считает сервер). */
  isOnShift: boolean;
  /** Рабочая заметка клуба; null — заметки нет. */
  note: string | null;
  /** Heartbeat DJ-консоли моложе TTL — «в сети» по факту, не по догадке. */
  online: boolean;
}

export interface BoardZone {
  id: string;
  name: string;
  capacity: number;
  inside: number;
  qrActive: boolean;
}

export interface ClubBoard {
  clubName: string;
  clubNameSource: MetricSource;

  session: BoardSession | null;
  sessionSource: MetricSource;
  /** ms epoch последнего УСПЕШНОГО запроса; null если ни одного */
  lastFetchedAt: number | null;
  refreshing: boolean;
  /** одна честная ошибка на доску; null когда всё ок */
  error: { message: string; code?: string } | null;
  reload: () => void;

  netSettledMinor: BoardMetric<number>;
  requests24h: BoardMetric<number>;
  requestPriceMinor: BoardMetric<number>;
  djs: BoardDj[];
  djsSource: MetricSource;

  zones: BoardZone[];
  zonesSource: MetricSource;

  /** реальный токен сессии; null если смены нет или ошибка */
  sessionQr: { token: string; expiresAt: string } | null;
  sessionQrSource: MetricSource;
  rotateSessionQr: () => Promise<void>;
  qrBusy: boolean;

  /** сколько preview-метрик на доске; >0 => показывать data-preview-banner */
  previewCount: number;
}

// ---------------------------------------------------------------------------
// Preview zone model — the ONLY local data allowed on the board (§5.1.2/§9.4)
// ---------------------------------------------------------------------------
// The backend has no zones/tables/occupancy tables (migrations 0001-0011), so
// this block cannot be live. It is deliberately small and every value is
// labelled `preview` by the board. Do not add fabricated telemetry here.

const PREVIEW_ZONES: BoardZone[] = [
  { id: 'main-stage', name: 'Main Stage', capacity: 420, inside: 316, qrActive: true },
  { id: 'rooftop', name: 'Rooftop', capacity: 160, inside: 74, qrActive: true },
  { id: 'vip-lounge', name: 'VIP Lounge', capacity: 90, inside: 61, qrActive: false },
];

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function metric<T>(value: T | null, source: MetricSource, caption: string): BoardMetric<T> {
  // `value === null` is only allowed together with `unavailable` (§7).
  if (value === null && source !== 'unavailable') {
    return { value: null, source: 'unavailable', caption };
  }
  return { value, source, caption };
}

function errorOf(reason: unknown): { message: string; code?: string } {
  if (reason instanceof ApiRequestError) {
    return { message: reason.message, code: reason.code };
  }
  if (reason instanceof Error) return { message: reason.message };
  return { message: 'Request failed' };
}

/** Empty dashboard snapshot: every API metric is honestly unavailable. */
interface Snapshot {
  clubName: string;
  session: BoardSession | null;
  netSettledMinor: number | null;
  requests24h: number | null;
  requestPriceMinor: number | null;
  requestPriceCaption: string;
  djs: BoardDj[];
  dashboardOk: boolean;
  profileOk: boolean;
  pricingOk: boolean;
  djsOk: boolean;
  fetchedAt: number | null;
  error: { message: string; code?: string } | null;
}

const INITIAL_SNAPSHOT: Snapshot = {
  clubName: '',
  session: null,
  netSettledMinor: null,
  requests24h: null,
  requestPriceMinor: null,
  requestPriceCaption: 'current guest price',
  djs: [],
  dashboardOk: false,
  profileOk: false,
  pricingOk: false,
  djsOk: false,
  fetchedAt: null,
  error: null,
};

function activePolicy(items: PricingPolicy[]): PricingPolicy | null {
  // GET /v1/club/pricing returns history newest-first; the active row has no
  // valid_to. If several are open (should not happen), the newest wins.
  const open = items.find((item) => item.validTo === null) ?? null;
  // A row whose amount did not survive the DTO mapping (missing field, NaN) is
  // not a reading: the board must say `unavailable`, never show a live chip
  // next to a dash.
  if (!open || typeof open.priceMinor !== 'number' || !Number.isFinite(open.priceMinor)) return null;
  return open;
}

function toBoardDj(dj: Dj): BoardDj {
  return {
    id: dj.id,
    displayName: dj.displayName,
    email: dj.email,
    status: dj.status,
    // Both are server facts now: `on_shift` is the open session assigned to
    // this DJ and `online` is a heartbeat within the presence TTL
    // (GET /v1/club/djs). The panel no longer guesses either of them.
    isOnShift: dj.onShift,
    note: dj.note,
    online: dj.online,
  };
}

// ---------------------------------------------------------------------------
// Hook
// ---------------------------------------------------------------------------

export function useClubBoard(): ClubBoard {
  // Captions are user-facing copy ("all time, settled"), so they come from the
  // catalog like every other sentence; the metric shape itself is unchanged.
  const { t } = useLocale();
  const [snapshot, setSnapshot] = useState<Snapshot>(INITIAL_SNAPSHOT);
  const [refreshing, setRefreshing] = useState(true);
  const [tick, setTick] = useState(0);

  const [sessionQr, setSessionQr] = useState<{ token: string; expiresAt: string } | null>(null);
  const [qrBusy, setQrBusy] = useState(false);
  // Guards double-submit synchronously: state updates are async, a ref is not.
  const qrInFlight = useRef(false);
  // Board-wide error may also be raised by the QR mutation.
  const [mutationError, setMutationError] = useState<{ message: string; code?: string } | null>(null);

  useEffect(() => {
    let cancelled = false;
    setRefreshing(true);

    const load = async () => {
      const [dashboard, profile, djs, pricing] = await Promise.allSettled([
        getClubDashboard(),
        getProfile(),
        getDjs(),
        getPricing(),
      ]);
      if (cancelled) return;

      const parse = <T,>(result: PromiseSettledResult<T>): [T | null, unknown] =>
        result.status === 'fulfilled' ? [result.value, null] : [null, result.reason];

      const [dashValue, dashError] = parse(dashboard);
      const [profileValue, profileError] = parse(profile);
      const [djsValue, djsError] = parse(djs);
      const [pricingValue, pricingError] = parse(pricing);

      const firstFailure = dashError ?? profileError ?? pricingError ?? djsError;

      const session: BoardSession | null = dashValue?.session
        ? {
            id: dashValue.session.id,
            status: dashValue.session.status as BoardSession['status'],
          }
        : null;

      const policy = pricingValue ? activePolicy(pricingValue.items) : null;
      // A row that exists but carries no readable amount is a different fact
      // from "the club has no policy", and the caption says which one it is.
      const unreadablePolicy = pricingValue !== null && policy === null
        && pricingValue.items.some((item) => item.validTo === null);
      const sessionStatus: SessionStatus | undefined = session?.status;
      const shiftExists = session !== null && sessionStatus !== 'closed';

      setSnapshot({
        clubName: profileValue?.name ?? '',
        session: shiftExists ? session : null,
        netSettledMinor: dashValue ? dashValue.earnedTotalMinor : null,
        requests24h: dashValue ? dashValue.requestsToday : null,
        requestPriceMinor: policy ? policy.priceMinor : null,
        requestPriceCaption: policy
          ? t('dashboard.requestPricePolicy')
          : unreadablePolicy
            ? t('dashboard.requestPriceUnreadable')
            : t('dashboard.requestPriceNoPolicy'),
        djs: djsValue ? djsValue.items.map(toBoardDj) : [],
        dashboardOk: dashValue !== null,
        profileOk: profileValue !== null,
        pricingOk: pricingValue !== null,
        djsOk: djsValue !== null,
        // Stamped only after the awaits above actually resolved. Partial
        // success still means real successful requests happened.
        fetchedAt:
          dashValue || profileValue || djsValue || pricingValue ? Date.now() : null,
        error: firstFailure ? errorOf(firstFailure) : null,
      });

      if (!dashValue) setSessionQr(null);
      setRefreshing(false);
    };

    void load();
    return () => {
      cancelled = true;
    };
  }, [tick]);

  const reload = useCallback(() => setTick((value) => value + 1), []);

  const rotateSessionQr = useCallback(async () => {
    if (qrInFlight.current) return; // no second request while one is in flight
    const session = snapshot.session;
    if (!session) return;
    qrInFlight.current = true;
    setQrBusy(true);
    setMutationError(null);
    try {
      const qr = await generateSessionQr(session.id);
      setSessionQr({ token: qr.qr_token, expiresAt: qr.expires_at });
    } catch (reason) {
      setMutationError(errorOf(reason));
    } finally {
      qrInFlight.current = false;
      setQrBusy(false);
    }
  }, [snapshot.session]);

  const netSettledMinor = useMemo(
    () =>
      metric(
        snapshot.dashboardOk ? snapshot.netSettledMinor : null,
        snapshot.dashboardOk ? 'live' : 'unavailable',
        t('dashboard.netSettledCaption'),
      ),
    [snapshot.dashboardOk, snapshot.netSettledMinor],
  );

  const requests24h = useMemo(
    () =>
      metric(
        snapshot.dashboardOk ? snapshot.requests24h : null,
        snapshot.dashboardOk ? 'live' : 'unavailable',
        t('dashboard.requests24hCaption'),
      ),
    [snapshot.dashboardOk, snapshot.requests24h],
  );

  const requestPriceMinor = useMemo(
    () =>
      metric(
        snapshot.requestPriceMinor,
        snapshot.pricingOk && snapshot.requestPriceMinor !== null ? 'live' : 'unavailable',
        snapshot.requestPriceCaption,
      ),
    [snapshot.pricingOk, snapshot.requestPriceMinor, snapshot.requestPriceCaption],
  );

  const zones = PREVIEW_ZONES;

  const previewCount = useMemo(() => {
    const sources: MetricSource[] = [
      snapshot.profileOk ? 'live' : 'unavailable',
      snapshot.dashboardOk ? 'live' : 'unavailable',
      snapshot.dashboardOk ? 'live' : 'unavailable',
      snapshot.pricingOk && snapshot.requestPriceMinor !== null ? 'live' : 'unavailable',
      snapshot.djsOk ? 'live' : 'unavailable',
      'preview', // zones — the only preview block on the board
    ];
    return sources.filter((source) => source === 'preview').length;
  }, [
    snapshot.profileOk,
    snapshot.dashboardOk,
    snapshot.pricingOk,
    snapshot.requestPriceMinor,
    snapshot.djsOk,
  ]);

  return {
    clubName: snapshot.clubName,
    clubNameSource: snapshot.profileOk && snapshot.clubName ? 'live' : 'unavailable',

    session: snapshot.session,
    sessionSource: snapshot.dashboardOk ? 'live' : 'unavailable',
    lastFetchedAt: snapshot.fetchedAt,
    refreshing,
    error: snapshot.error ?? mutationError,
    reload,

    netSettledMinor,
    requests24h,
    requestPriceMinor,
    djs: snapshot.djs,
    djsSource: snapshot.djsOk ? 'live' : 'unavailable',

    zones,
    zonesSource: 'preview',

    sessionQr,
    sessionQrSource: sessionQr !== null ? 'live' : 'unavailable',
    rotateSessionQr,
    qrBusy,

    previewCount,
  };
}