import { useCallback, useEffect, useMemo, useState } from 'react';
import {
  ApiRequestError,
  djRequestAction,
  getDjHistory,
  getDjSessionStats,
  getDjShift,
  pingDjPresence,
  setDjAcceptance,
  type DjHistoryItem,
} from '../api/client';
import { IS_DEMO } from '../app-mode';
import type { Translate } from '../i18n';
import { useDjConsoleMock } from './useDjConsoleMock';
import { useDjLiveQueue, type DjLiveEvent } from './useDjLiveQueue';
import type { DjConsoleLiveData, DjHistoryAvailability } from './liveData';
import type {
  DjConsoleMockApi,
  DjConsoleSession,
  DjConsoleState,
  DjConsoleStats,
  DjHistoryEntry,
  DjLiveStatus,
  DjRequestAction,
  DjTelemetry,
} from './types';

export interface DjConsoleData {
  dj: DjConsoleMockApi;
  loading: boolean;
  error: string | null;
  live: boolean;
  reload: (sort?: 'new' | 'hot') => void;
  sort: 'new' | 'hot';
  /** Freshness of the request list — the one thing a view may say about it. */
  status: DjLiveStatus;
}

/** Presence heartbeat interval. The server calls a DJ online for 90 s, so 30 s
 *  leaves room for one dropped request before the club shows them offline. */
const DJ_HEARTBEAT_MS = 30_000;

/** Page size for the shift log — the server's default, asked explicitly so the
 *  console and the API never disagree about what "one page" means. */
const HISTORY_PAGE_SIZE = 50;

/** API row → view row. `occurredAt` stays empty when the server does not know
 *  the play time: the console then renders "unavailable", not a made-up clock. */
function toHistoryEntry(item: DjHistoryItem): DjHistoryEntry {
  return {
    id: item.id,
    requestId: item.id,
    track: {
      id: item.id,
      title: item.title,
      artist: item.artist,
      style: item.style,
      coverUrl: item.cover_url ?? undefined,
    },
    status: 'played',
    occurredAt: item.played_at ?? '',
  };
}

/**
 * Demo mode has no live channel, but the view still asks `status` for a
 * connection. The values are placeholders the view ignores (`live === false`,
 * `lastSyncAt === null`); both objects are module-level so the placeholder does
 * not change identity on every render.
 */
const DEMO_NEW_IDS: ReadonlySet<string> = new Set<string>();
const DEMO_STATUS: DjLiveStatus = { connection: 'live', lastSyncAt: null, newIds: DEMO_NEW_IDS };

/**
 * Live-mode adapter. It receives the page's translator so every toast it emits —
 * and every fallback error string — is in the user's language. The demo adapter
 * (`useDjConsoleMock`) owns its own copy; this one owns only server responses.
 *
 * In live mode the request list belongs to `useDjLiveQueue` (WebSocket with a
 * 12 s timer fallback); this hook owns the shift, the shift-scoped counters and
 * the shift log (`GET /v1/dj/history`), which change on a human action rather
 * than on a stream, and joins them into the `DjConsoleMockApi` shape the views
 * already speak. Every failed read is reported through `liveData` so the view
 * can say "unavailable" instead of printing a zero.
 */
export function useDjConsoleData(onNotice: (message: string) => void, t: Translate): DjConsoleData {
  const mock = useDjConsoleMock();
  const [session, setSession] = useState<DjConsoleSession | null>(null);
  const [stats, setStats] = useState<DjConsoleStats>({ requests: 0, accepted: 0, played: 0, rejected: 0, earnedMinor: 0 });
  const [statsAvailable, setStatsAvailable] = useState(true);
  const [history, setHistory] = useState<DjHistoryEntry[]>([]);
  const [historyStatus, setHistoryStatus] = useState<DjHistoryAvailability>('loading');
  const [historyCursor, setHistoryCursor] = useState<string | null>(null);
  const [historyLoading, setHistoryLoading] = useState(false);
  const [metaLoading, setMetaLoading] = useState(!IS_DEMO);
  const [error, setError] = useState<string | null>(null);
  const [refreshKey, setRefreshKey] = useState(0);
  const [sort, setSort] = useState<'new' | 'hot'>('new');
  // Live frames are counted for the session view's telemetry. The vocabulary in
  // `DjTelemetry` predates the session frames, so a pause/resume is reported as
  // the queue update it is.
  const [frames, setFrames] = useState(0);
  const [lastFrame, setLastFrame] = useState<DjTelemetry['lastEvent']>(null);

  const handleLiveEvent = useCallback((event: DjLiveEvent) => {
    setFrames((count) => count + 1);
    setLastFrame(event.type === 'request.created' ? 'request_received' : 'request_updated');
  }, []);

  const { queue, connection, lastSyncAt, newIds, refresh } = useDjLiveQueue({
    enabled: !IS_DEMO,
    sessionId: session?.id ?? null,
    sort,
    onEvent: handleLiveEvent,
  });

  const status = useMemo<DjLiveStatus>(
    () => ({ connection, lastSyncAt, newIds }),
    [connection, lastSyncAt, newIds],
  );

  const reload = useCallback((nextSort?: 'new' | 'hot') => {
    // Shift and counters are cheap and human-triggered; the queue is read by
    // the live layer, and a sort change makes it re-read (and re-subscribe).
    setRefreshKey((key) => key + 1);
    if (nextSort && nextSort !== sort) {
      setSort(nextSort);
      return;
    }
    refresh();
  }, [refresh, sort]);

  // Presence heartbeat: the club roster shows "online" while this DJ's
  // `last_seen_at` is younger than the server's TTL. One request every 30 s,
  // only while the tab is visible — a background tab is not "in the console".
  // Failures are silent on purpose: presence is a convenience signal, and a
  // toast about it would interrupt the DJ's actual work.
  useEffect(() => {
    if (IS_DEMO) return undefined;
    let timer: ReturnType<typeof setTimeout> | undefined;
    let stopped = false;

    const beat = async () => {
      if (document.visibilityState === 'visible') {
        try {
          await pingDjPresence();
        } catch {
          // 401 is handled by the request layer (token refresh/redirect);
          // any other failure just means the club sees an older heartbeat.
        }
      }
      if (!stopped) timer = setTimeout(() => { void beat(); }, DJ_HEARTBEAT_MS);
    };

    const onVisibility = () => {
      // Coming back to the tab is the one moment the club's view is most
      // likely stale, so beat immediately instead of waiting out the interval.
      if (document.visibilityState === 'visible') {
        if (timer) clearTimeout(timer);
        void beat();
      }
    };

    void beat();
    document.addEventListener('visibilitychange', onVisibility);
    return () => {
      stopped = true;
      if (timer) clearTimeout(timer);
      document.removeEventListener('visibilitychange', onVisibility);
    };
  }, []);

  useEffect(() => {
    if (IS_DEMO) return undefined;
    let cancelled = false;
    setMetaLoading(true);
    setError(null);
    setHistoryStatus('loading');
    // The shift, its counters and the first page of its log are three readings
    // of the same object, so they are issued together and settle together.
    // A failure in one must not blank the others: each `catch` marks only its
    // own source unavailable, and the queue keeps streaming regardless.
    const shiftRead = getDjShift().then((shift) => {
      if (!cancelled) {
        setSession(shift.session ? { id: shift.session.id, status: shift.session.status, timezone: shift.session.timezone, clubId: shift.session.club_id } : null);
      }
    });
    const statsRead = getDjSessionStats().then((result) => {
      if (cancelled) return;
      // The counters are session-scoped: a shift that is not the open one
      // would put another night's money under "Session total".
      setStats({ requests: result.requests, accepted: result.accepted, played: result.played, rejected: result.rejected, earnedMinor: result.earnedMinor });
      setStatsAvailable(true);
    }).catch(() => {
      // A failed read is reported as unavailable, so the view prints the word
      // instead of these zeros — the values stay as a safe default only.
      if (!cancelled) {
        setStatsAvailable(false);
        setStats({ requests: 0, accepted: 0, played: 0, rejected: 0, earnedMinor: 0 });
      }
    });
    const historyRead = getDjHistory({ limit: HISTORY_PAGE_SIZE }).then((page) => {
      if (cancelled) return;
      setHistory(page.items.map(toHistoryEntry));
      setHistoryCursor(page.nextCursor);
      setHistoryStatus('ready');
    }).catch(() => {
      if (!cancelled) {
        setHistory([]);
        setHistoryCursor(null);
        setHistoryStatus('unavailable');
      }
    });
    Promise.all([shiftRead, statsRead, historyRead]).catch((err: unknown) => {
      if (cancelled) return;
      if (err instanceof ApiRequestError && err.status === 401) return;
      setError(err instanceof Error ? err.message : t('dj.errorLoad'));
    }).finally(() => { if (!cancelled) setMetaLoading(false); });
    return () => { cancelled = true; };
  }, [refreshKey]);

  /** Next page of the shift log. Failures leave the rows already on screen. */
  const loadMoreHistory = useCallback(() => {
    if (!historyCursor || historyLoading) return;
    setHistoryLoading(true);
    getDjHistory({ limit: HISTORY_PAGE_SIZE, cursor: historyCursor })
      .then((page) => {
        setHistory((previous) => [...previous, ...page.items.map(toHistoryEntry)]);
        setHistoryCursor(page.nextCursor);
        setHistoryStatus('ready');
      })
      .catch((err: unknown) => setError(err instanceof Error ? err.message : t('dj.errorLoad')))
      .finally(() => setHistoryLoading(false));
  }, [historyCursor, historyLoading, t]);

  const liveAction = useCallback((id: string, action: DjRequestAction, reason?: string) => {
    void djRequestAction(id, action, reason).then(() => {
      onNotice(action === 'accept' ? t('dj.noticeAccepted') : action === 'reject' ? t('dj.noticeRejected') : t('dj.noticePlayed'));
      reload();
    }).catch((err: unknown) => setError(err instanceof Error ? err.message : t('dj.errorAction')));
  }, [onNotice, reload, t]);
  const liveAcceptance = useCallback((paused: boolean) => {
    void setDjAcceptance(paused).then(() => { onNotice(paused ? t('dj.noticeIntakePaused') : t('dj.noticeIntakeResumed')); reload(); })
      .catch((err: unknown) => setError(err instanceof Error ? err.message : t('dj.errorIntake')));
  }, [onNotice, reload, t]);

  if (IS_DEMO) return { dj: mock, loading: false, error: null, live: false, reload, sort, status: DEMO_STATUS };

  // The queue read is the hook's; until it confirms once there is nothing
  // honest to render, and an offline first read must not keep the page in a
  // loading state forever.
  const loading = metaLoading || (lastSyncAt === null && connection !== 'offline');
  const state: DjConsoleState = {
    session,
    nowPlaying: { track: null, startedAt: null, elapsedSeconds: 0, source: 'unknown', deck: null },
    requests: queue,
    history,
    tips: [],
    telemetry: {
      connection: connection === 'live' ? 'connected' : connection === 'timer' ? 'degraded' : 'offline',
      lastSyncedAt: lastSyncAt === null ? null : new Date(lastSyncAt).toISOString(),
      queueLatencyMs: null,
      requestsPerMinute: 0,
      eventsReceived: frames,
      lastEvent: lastFrame,
    },
    stats,
    lastUpdatedAt: lastSyncAt === null ? new Date().toISOString() : new Date(lastSyncAt).toISOString(),
  };
  const liveData: DjConsoleLiveData = {
    stats: statsAvailable ? 'ok' : 'unavailable',
    history: historyStatus,
    hasMoreHistory: historyCursor !== null,
    historyLoading,
    loadMoreHistory,
  };
  const liveDj: DjConsoleMockApi = {
    ...mock,
    state,
    queue,
    history,
    tips: [],
    telemetry: state.telemetry,
    action: liveAction,
    pinRequest: () => onNotice(t('dj.noticePinDemo')),
    acknowledgeTip: () => undefined,
    setAcceptance: liveAcceptance,
    toggleAcceptance: () => liveAcceptance(session?.status !== 'paused_acceptance'),
    reset: () => undefined,
    liveData,
  };
  return { dj: liveDj, loading, error, live: true, reload, sort, status };
}
