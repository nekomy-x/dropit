import { useCallback, useEffect, useState } from 'react';
import {
  ApiRequestError,
  djRequestAction,
  getDjQueue,
  getDjShift,
  getDjStats,
  pingDjPresence,
  setDjAcceptance,
  type DjQueueItem,
} from '../api/client';
import { IS_DEMO } from '../app-mode';
import type { Translate } from '../i18n';
import { useDjConsoleMock } from './useDjConsoleMock';
import type { DjConsoleMockApi, DjConsoleRequest, DjConsoleSession, DjConsoleState } from './types';

export interface DjConsoleData {
  dj: DjConsoleMockApi;
  loading: boolean;
  error: string | null;
  live: boolean;
  reload: (sort?: 'new' | 'hot') => void;
  sort: 'new' | 'hot';
}

/** Presence heartbeat interval. The server calls a DJ online for 90 s, so 30 s
 *  leaves room for one dropped request before the club shows them offline. */
const DJ_HEARTBEAT_MS = 30_000;

const toRequest = (item: DjQueueItem): DjConsoleRequest => ({
  id: item.id,
  title: item.title,
  artist: item.artist,
  style: item.style,
  createdAt: item.created_at,
  status: item.status === 'accepted' ? 'accepted' : item.status === 'played' ? 'played' : item.status === 'rejected' ? 'rejected' : 'pending',
  sameTrackCount: item.same_track_count,
  djShareMinor: 0,
  coverUrl: item.cover_url,
});

/**
 * Live-mode adapter. It receives the page's translator so every toast it emits —
 * and every fallback error string — is in the user's language. The demo adapter
 * (`useDjConsoleMock`) owns its own copy; this one owns only server responses.
 */
export function useDjConsoleData(onNotice: (message: string) => void, t: Translate): DjConsoleData {
  const mock = useDjConsoleMock();
  const [queue, setQueue] = useState<DjConsoleRequest[]>([]);
  const [session, setSession] = useState<DjConsoleSession | null>(null);
  const [stats, setStats] = useState({ requests: 0, accepted: 0, played: 0, rejected: 0, earnedMinor: 0 });
  const [loading, setLoading] = useState(!IS_DEMO);
  const [error, setError] = useState<string | null>(null);
  const [refreshKey, setRefreshKey] = useState(0);
  const [sort, setSort] = useState<'new' | 'hot'>('new');

  const reload = useCallback((nextSort?: 'new' | 'hot') => {
    if (nextSort) setSort(nextSort);
    setRefreshKey((key) => key + 1);
  }, []);

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
    if (IS_DEMO) return;
    let cancelled = false;
    setLoading(true);
    setError(null);
    Promise.all([getDjShift(), getDjQueue(sort), getDjStats()]).then(([shift, queueResult, statsResult]) => {
      if (cancelled) return;
      setSession(shift.session ? { id: shift.session.id, status: shift.session.status, timezone: shift.session.timezone, clubId: shift.session.club_id } : null);
      setQueue(queueResult.items.map(toRequest));
      setStats(statsResult);
    }).catch((err: unknown) => {
      if (cancelled) return;
      if (err instanceof ApiRequestError && err.status === 401) return;
      setError(err instanceof Error ? err.message : t('dj.errorLoad'));
    }).finally(() => { if (!cancelled) setLoading(false); });
    return () => { cancelled = true; };
  }, [refreshKey, sort]);

  const liveAction = useCallback((id: string, action: 'accept' | 'reject' | 'played') => {
    void djRequestAction(id, action).then(() => {
      onNotice(action === 'accept' ? t('dj.noticeAccepted') : action === 'reject' ? t('dj.noticeRejected') : t('dj.noticePlayed'));
      reload();
    }).catch((err: unknown) => setError(err instanceof Error ? err.message : t('dj.errorAction')));
  }, [onNotice, reload, t]);
  const liveAcceptance = useCallback((paused: boolean) => {
    void setDjAcceptance(paused).then(() => { onNotice(paused ? t('dj.noticeIntakePaused') : t('dj.noticeIntakeResumed')); reload(); })
      .catch((err: unknown) => setError(err instanceof Error ? err.message : t('dj.errorIntake')));
  }, [onNotice, reload, t]);

  if (IS_DEMO) return { dj: mock, loading: false, error: null, live: false, reload, sort };

  const state: DjConsoleState = {
    session,
    nowPlaying: { track: null, startedAt: null, elapsedSeconds: 0, source: 'unknown', deck: null },
    requests: queue,
    history: [],
    tips: [],
    telemetry: { connection: error ? 'offline' : loading ? 'degraded' : 'connected', lastSyncedAt: null, queueLatencyMs: null, requestsPerMinute: 0, eventsReceived: 0, lastEvent: null },
    stats,
    lastUpdatedAt: new Date().toISOString(),
  };
  const liveDj: DjConsoleMockApi = {
    ...mock,
    state,
    queue,
    history: [],
    tips: [],
    telemetry: state.telemetry,
    action: liveAction,
    pinRequest: () => onNotice(t('dj.noticePinDemo')),
    acknowledgeTip: () => undefined,
    setAcceptance: liveAcceptance,
    toggleAcceptance: () => liveAcceptance(session?.status !== 'paused_acceptance'),
    reset: () => undefined,
  };
  return { dj: liveDj, loading, error, live: true, reload, sort };
}
