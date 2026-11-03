/**
 * Live request queue for the DJ console.
 *
 * Transport: `POST /v1/ws/ticket` (one-shot, 30 s TTL) → `GET /v1/ws?ticket=` →
 * first client frame `{"subscribe":"queue","id":"<sessionId>"}` — the protocol
 * of `crates/gateway-api/src/ws.rs`, documented in `docs/openapi.yaml` under
 * `/v1/ws` and `/v1/ws/ticket`. Server frames are `{type, ...}`; a `request.*`
 * or `session.*` frame is only a *hint* that the list changed, so each one
 * triggers a regular `GET /v1/dj/queue?sort=`. The frames carry a title and a
 * status only, while a row needs cover/preview/ISRC as well — rendering the
 * payload would replace a complete row with a partial one.
 *
 * ## Connection state machine (`DjLiveConnection`)
 *
 *   timer   ← the first paint: the mount read plus the 12 s poll keep the list
 *             current while the socket is still handshaking.
 *   timer → live    socket open, `subscribed` frame received.
 *   live  → timer   socket dropped: the 12 s poll owns the list again until
 *                   `subscribed` hands it back.
 *   *     → offline a **read** failed (not 401): the list on screen is the last
 *                   one the API confirmed; a 401 also stops the layer.
 *
 * `live` therefore means "the socket is open and subscribed", never "we tried",
 * and `offline` only ever talks about a read — a reconnect is `timer`.
 * Reconnect delay: 1 s after the first failure, 2 s after the second. A socket
 * that has worked before then walks 4 → 8 → 15 → 30 s, capped; a socket that
 * never opened is probed once a minute from the second failure on. Every attempt
 * mints a fresh ticket, and no path retries faster than once a second, so there
 * is no hot loop.
 *
 * All timers are paused while `document.visibilityState === 'hidden'`; coming
 * back reads immediately and resumes.
 */
import { useCallback, useEffect, useRef, useState } from 'react';
import { ApiRequestError, getDjQueue, getWsTicket, wsUrl, type DjQueueItem } from '../api/client';
import { IS_DEMO } from '../app-mode';
import type { DjConsoleRequest, DjLiveConnection } from './types';

/** Server → client frames the console acts on (`WsEvent.kind` in ws.rs). */
export type DjLiveEventType =
  | 'request.created'
  | 'request.accepted'
  | 'request.rejected'
  | 'request.played'
  | 'session.paused'
  | 'session.resumed';

/**
 * One queue event, as far as it can be trusted: an id and a status.
 *
 * Deliberately not a request row — the event has no cover, preview or ISRC, so
 * consumers use it for feedback (a toast, a counter) and let the refetch below
 * own the list.
 */
export interface DjLiveEvent {
  type: DjLiveEventType;
  /** Affected request; `null` for `session.*` frames. */
  requestId: string | null;
  /** `pending` | `accepted` | `rejected` | `played` | `paused` | `active`. */
  status: string | null;
  /** Reject reason, only on `request.rejected`. */
  reason: string | null;
  /** Badge counter of the same track, only on `request.created`. */
  sameTrackCount: number | null;
}

export interface DjLiveQueueOptions {
  /** `false` in demo mode: the hook then owns no timer, socket or request. */
  enabled: boolean;
  /** Shift whose queue topic is subscribed. `null` means timer only. */
  sessionId: string | null;
  sort: 'new' | 'hot';
  /** Called for every `request.*` / `session.*` frame, before the refetch. */
  onEvent?: (event: DjLiveEvent) => void;
}

export interface DjLiveQueue {
  queue: DjConsoleRequest[];
  connection: DjLiveConnection;
  /** Epoch ms of the last successful queue read; `null` before the first one. */
  lastSyncAt: number | null;
  /**
   * Ids that arrived after the surface opened — highlighted, never hidden.
   * A row that leaves the queue drops out at once; a badge ages out after
   * `NEW_ID_TTL_MS` so an hour-old arrival is not still "new".
   */
  newIds: ReadonlySet<string>;
  /** Read the queue now (manual refresh). A no-op in demo mode. */
  refresh: () => void;
}

/** Queue poll cadence while the socket is not carrying the list. */
const POLL_MS = 12_000;
/** Frames of one burst collapse into a single read. */
const REFRESH_DEBOUNCE_MS = 300;
/** Reconnect ladder for a socket that has worked at least once. */
const BACKOFF_MS = [1_000, 2_000, 4_000, 8_000, 15_000, 30_000] as const;
/** Cold socket (never opened) is retried at this pace. */
const BROKEN_SOCKET_RETRY_MS = 60_000;
/** Consecutive failures after which the cold socket is only probed per minute. */
const FALLBACK_AFTER_FAILURES = 2;
/** How long an arrival keeps its "new" badge. */
const NEW_ID_TTL_MS = 120_000;
/** How often expired badges are collected, while any new id is alive. */
const NEW_ID_SWEEP_MS = 30_000;

const EMPTY_QUEUE: DjConsoleRequest[] = [];
const EMPTY_IDS: ReadonlySet<string> = new Set<string>();

const LIVE_EVENT_TYPES: ReadonlySet<string> = new Set<string>([
  'request.created',
  'request.accepted',
  'request.rejected',
  'request.played',
  'session.paused',
  'session.resumed',
]);

/** The queue topic id is a shift UUID; anything else cannot be subscribed. */
const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

function isLiveEventType(value: string): value is DjLiveEventType {
  return LIVE_EVENT_TYPES.has(value);
}

function isUuid(value: string): boolean {
  return UUID_RE.test(value);
}

function toRequest(item: DjQueueItem): DjConsoleRequest {
  return {
    id: item.id,
    title: item.title,
    artist: item.artist,
    style: item.style,
    createdAt: item.created_at,
    status: item.status === 'accepted' ? 'accepted' : item.status === 'played' ? 'played' : item.status === 'rejected' ? 'rejected' : 'pending',
    sameTrackCount: item.same_track_count,
    djShareMinor: 0,
    coverUrl: item.cover_url,
    // `null` when the provider reported none; the column says so instead of
    // offering a control that cannot work.
    previewUrl: item.preview_url ?? null,
    isrc: item.isrc ?? null,
  };
}

/** Fields a row renders; two reads that agree on all of them are the same list. */
function sameRequestRow(a: DjConsoleRequest, b: DjConsoleRequest): boolean {
  return a.id === b.id
    && a.title === b.title
    && a.artist === b.artist
    && a.style === b.style
    && a.createdAt === b.createdAt
    && a.status === b.status
    && a.sameTrackCount === b.sameTrackCount
    && a.djShareMinor === b.djShareMinor
    && a.coverUrl === b.coverUrl
    && a.previewUrl === b.previewUrl
    && a.isrc === b.isrc;
}

function sameQueue(previous: DjConsoleRequest[], next: DjConsoleRequest[]): boolean {
  if (previous === next) return true;
  if (previous.length !== next.length) return false;
  for (let index = 0; index < next.length; index += 1) {
    const before = previous[index];
    const after = next[index];
    if (before === undefined || after === undefined || !sameRequestRow(before, after)) return false;
  }
  return true;
}

export function useDjLiveQueue({ enabled, sessionId, sort, onEvent }: DjLiveQueueOptions): DjLiveQueue {
  const active = enabled && !IS_DEMO;

  const [queue, setQueue] = useState<DjConsoleRequest[]>(EMPTY_QUEUE);
  const [connection, setConnection] = useState<DjLiveConnection>('offline');
  const [lastSyncAt, setLastSyncAt] = useState<number | null>(null);
  const [newIds, setNewIds] = useState<ReadonlySet<string>>(EMPTY_IDS);

  /** The caller may pass an inline callback; a new identity must not reconnect. */
  const onEventRef = useRef<((event: DjLiveEvent) => void) | undefined>(onEvent);
  useEffect(() => { onEventRef.current = onEvent; }, [onEvent]);

  /** `refresh()` is stable, so a page can pass it into its own callbacks. */
  const refreshRef = useRef<() => void>(() => undefined);
  const refresh = useCallback(() => { refreshRef.current(); }, []);

  /** Bumped when an effect run ends: a late response must not touch the UI. */
  const runTokenRef = useRef(0);
  /** Previous confirmed id list — the baseline for `newIds`, `null` before the first read. */
  const knownIdsRef = useRef<Set<string> | null>(null);
  /** Arrival time of every id still carrying a "new" badge (badge ageing). */
  const arrivedAtRef = useRef<Map<string, number>>(new Map());
  /** Last confirmed read, readable from callbacks that must not see stale state. */
  const syncedAtRef = useRef<number | null>(null);
  /** Which shift `knownIdsRef` belongs to; a new shift starts a new baseline. */
  const sessionKeyRef = useRef<string | null>(null);

  useEffect(() => {
    const runToken = ++runTokenRef.current;

    // Demo mode and `enabled: false` are inert by contract: no ticket, no
    // socket, no timer, no request — and an empty list.
    if (!active) {
      refreshRef.current = () => undefined;
      sessionKeyRef.current = null;
      knownIdsRef.current = null;
      arrivedAtRef.current = new Map();
      syncedAtRef.current = null;
      setQueue(EMPTY_QUEUE);
      setConnection('offline');
      setLastSyncAt(null);
      setNewIds(EMPTY_IDS);
      return undefined;
    }

    // A different shift is a different list: the previous one must not stay
    // on screen under the new session id. Null → id is the normal boot
    // (the shift arrives after the first render), so it keeps what it has.
    if (sessionKeyRef.current !== sessionId) {
      if (sessionKeyRef.current !== null) {
        syncedAtRef.current = null;
        arrivedAtRef.current = new Map();
        setQueue(EMPTY_QUEUE);
        setLastSyncAt(null);
        setNewIds(EMPTY_IDS);
      }
      sessionKeyRef.current = sessionId;
      knownIdsRef.current = null;
    }

    let disposed = false;
    let socket: WebSocket | null = null;
    let subscribed = false;
    let everSubscribed = false;
    let failures = 0;
    let authFailed = false;
    let connecting = false;
    let pollActive = true;
    let lastReadFailed = false;
    let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
    let pollTimer: ReturnType<typeof setTimeout> | null = null;
    let refreshTimer: ReturnType<typeof setTimeout> | null = null;
    let pruneTimer: ReturnType<typeof setTimeout> | null = null;
    let fetchInFlight = false;
    let rerun = false;

    const hidden = () => document.visibilityState === 'hidden';

    const channelLabel = (): DjLiveConnection => {
      if (lastReadFailed) return 'offline';
      // The view's rule: before the first confirmed read the list is kept by
      // the timer, whatever the socket is doing.
      if (syncedAtRef.current === null) return 'timer';
      if (subscribed) return 'live';
      return pollActive ? 'timer' : 'offline';
    };

    const detach = (ws: WebSocket) => {
      ws.onopen = null;
      ws.onmessage = null;
      ws.onerror = null;
      ws.onclose = null;
    };

    const closeSocket = (reason: string) => {
      const ws = socket;
      socket = null;
      if (!ws) return;
      detach(ws);
      try {
        ws.close(1000, reason);
      } catch {
        // Already closed by the other side.
      }
    };

    const schedulePoll = () => {
      if (disposed || !pollActive || pollTimer !== null) return;
      if (hidden()) return; // resumed by visibilitychange
      pollTimer = setTimeout(() => {
        pollTimer = null;
        void readQueue().finally(() => { schedulePoll(); });
      }, POLL_MS);
    };

    const startPolling = () => {
      pollActive = true;
      schedulePoll();
    };

    const stopPolling = () => {
      pollActive = false;
      if (pollTimer !== null) {
        clearTimeout(pollTimer);
        pollTimer = null;
      }
    };

    const clearReconnect = () => {
      if (reconnectTimer !== null) {
        clearTimeout(reconnectTimer);
        reconnectTimer = null;
      }
    };

    const clearRefresh = () => {
      if (refreshTimer !== null) {
        clearTimeout(refreshTimer);
        refreshTimer = null;
      }
    };

    const clearPrune = () => {
      if (pruneTimer !== null) {
        clearTimeout(pruneTimer);
        pruneTimer = null;
      }
    };

    const schedulePrune = () => {
      if (disposed || pruneTimer !== null || hidden()) return;
      if (arrivedAtRef.current.size === 0) {
        clearPrune();
        return;
      }
      pruneTimer = setTimeout(() => {
        pruneTimer = null;
        pruneExpired();
      }, NEW_ID_SWEEP_MS);
    };

    /** Drops badges older than `NEW_ID_TTL_MS`; one timer while any is alive. */
    const pruneExpired = () => {
      if (disposed) return;
      const now = Date.now();
      let expired = false;
      for (const [id, at] of arrivedAtRef.current) {
        if (now - at >= NEW_ID_TTL_MS) {
          arrivedAtRef.current.delete(id);
          expired = true;
        }
      }
      if (expired) {
        setNewIds((previous) => {
          let changed = false;
          const kept = new Set<string>();
          for (const id of previous) {
            if (arrivedAtRef.current.has(id)) kept.add(id);
            else changed = true;
          }
          return changed ? kept : previous;
        });
      }
      schedulePrune();
    };

    /** Everything off: used on 401, and by the effect cleanup. */
    const stopChannels = () => {
      subscribed = false;
      stopPolling();
      clearReconnect();
      clearRefresh();
      clearPrune();
      closeSocket('console closed');
    };

    const applyQueue = (items: DjQueueItem[]) => {
      const next = items.map(toRequest);
      const nextIds = new Set(next.map((item) => item.id));
      const known = knownIdsRef.current;
      knownIdsRef.current = nextIds;
      // A poll that returned the same rows must not replace the array: the DJ
      // may be reading it, and a stable identity keeps the list from
      // re-rendering under them.
      setQueue((previous) => (sameQueue(previous, next) ? previous : next));
      // The arrival map exists only to age badges out; an id that left cannot
      // keep an entry.
      for (const id of arrivedAtRef.current.keys()) {
        if (!nextIds.has(id)) arrivedAtRef.current.delete(id);
      }
      if (known === null) return; // the first read is the baseline, not arrivals
      const now = Date.now();
      const fresh: string[] = [];
      for (const id of nextIds) {
        if (known.has(id)) continue;
        fresh.push(id);
        arrivedAtRef.current.set(id, now);
      }
      if (fresh.length > 0) schedulePrune();
      // Prune what left the queue, add what arrived, and keep the previous
      // instance when nothing changed — the request list is read mid-set and
      // must not re-render for a poll that returned the same rows.
      setNewIds((previous) => {
        let changed = false;
        const merged = new Set<string>();
        for (const id of previous) {
          if (nextIds.has(id)) merged.add(id);
          else changed = true;
        }
        for (const id of fresh) {
          if (!merged.has(id)) {
            merged.add(id);
            changed = true;
          }
        }
        return changed ? merged : previous;
      });
    };

    const readQueue = async (): Promise<void> => {
      if (disposed || authFailed || hidden()) return;
      if (fetchInFlight) {
        // A read is already in flight; the caller wants the state after it,
        // not a second parallel request.
        rerun = true;
        return;
      }
      fetchInFlight = true;
      try {
        const result = await getDjQueue(sort);
        if (disposed || authFailed || runToken !== runTokenRef.current) return;
        const now = Date.now();
        applyQueue(result.items);
        syncedAtRef.current = now;
        lastReadFailed = false;
        setLastSyncAt(now);
        setConnection(channelLabel());
      } catch (err) {
        if (disposed || authFailed || runToken !== runTokenRef.current) return;
        if (err instanceof ApiRequestError && err.status === 401) {
          // The session is gone; the console is about to be redirected.
          // Retrying would only mint 401s, so the whole layer stops.
          authFailed = true;
          stopChannels();
          setConnection('offline');
          return;
        }
        lastReadFailed = true;
        setConnection('offline');
        // The last confirmed list stays on screen. Recovery is the socket's
        // next frame, the next poll (when the timer owns the list), or the
        // DJ's refresh — no retry storm.
      } finally {
        fetchInFlight = false;
        if (rerun && !disposed && !authFailed) {
          rerun = false;
          void readQueue();
        }
      }
    };

    const refreshNow = () => {
      if (disposed || authFailed) return;
      clearRefresh();
      void readQueue(); // a refresh asked from a hidden tab is a no-op there
    };

    const requestRefresh = (delay: number) => {
      if (disposed || authFailed || hidden()) return;
      if (refreshTimer !== null) return; // one read per burst, trailing edge
      refreshTimer = setTimeout(() => {
        refreshTimer = null;
        void readQueue();
      }, delay);
    };

    const noteFailure = () => {
      if (disposed || authFailed) return;
      failures += 1;
      // The timer is the floor: from the moment a socket attempt fails, a poll
      // is what keeps the list current, so a dropped socket reads `timer`, not
      // `offline` — the latter is reserved for a failed read.
      startPolling();
      setConnection(channelLabel());
      scheduleReconnect();
    };

    const scheduleReconnect = () => {
      if (disposed || authFailed || reconnectTimer !== null) return;
      if (hidden()) return; // resumed on becoming visible
      let delay: number;
      if (failures < FALLBACK_AFTER_FAILURES) {
        delay = BACKOFF_MS[failures - 1] ?? BACKOFF_MS[0];
      } else if (everSubscribed) {
        // The channel works in this environment; walk the ladder, capped so
        // it can never become a hot loop.
        delay = BACKOFF_MS[Math.min(failures - 1, BACKOFF_MS.length - 1)] ?? BROKEN_SOCKET_RETRY_MS;
      } else {
        // It never opened. The timer is the primary channel now and the
        // socket is only probed once a minute.
        delay = BROKEN_SOCKET_RETRY_MS;
      }
      reconnectTimer = setTimeout(() => {
        reconnectTimer = null;
        void connect();
      }, delay);
    };

    const onSubscribed = () => {
      subscribed = true;
      everSubscribed = true;
      failures = 0;
      stopPolling();
      setConnection(channelLabel());
      // The topic is a broadcast, not a log: events between the last read and
      // this subscription are gone, so close the gap with one read.
      void readQueue();
    };

    const onFrame = (raw: string, failSocket: () => void) => {
      let frame: Record<string, unknown>;
      try {
        const parsed: unknown = JSON.parse(raw);
        if (parsed === null || typeof parsed !== 'object') return;
        frame = parsed as Record<string, unknown>;
      } catch {
        return; // an unreadable frame is not a reason to drop the channel
      }
      const type = typeof frame.type === 'string' ? frame.type : '';
      if (type === 'subscribed') {
        onSubscribed();
        return;
      }
      if (type === 'error') {
        // The server rejected the topic after the upgrade (used or expired
        // ticket, a shift that is not this DJ's) and closes the socket.
        // Immediate retry would only repeat the rejection: use the backoff.
        failSocket();
        return;
      }
      if (!isLiveEventType(type)) return; // a newer server may send more types
      const frameSession = typeof frame.session_id === 'string' ? frame.session_id : null;
      if (frameSession !== null && frameSession !== sessionId) return;
      onEventRef.current?.({
        type,
        requestId: typeof frame.request_id === 'string' ? frame.request_id : null,
        status: typeof frame.status === 'string' ? frame.status : null,
        reason: typeof frame.reason === 'string' ? frame.reason : null,
        sameTrackCount: typeof frame.same_track_count === 'number' ? frame.same_track_count : null,
      });
      requestRefresh(REFRESH_DEBOUNCE_MS);
    };

    const openSocket = (ticket: string) => {
      let ws: WebSocket;
      try {
        ws = new WebSocket(wsUrl(ticket));
      } catch {
        noteFailure();
        return;
      }
      socket = ws;
      let settled = false;

      const fail = () => {
        if (settled) return;
        settled = true;
        detach(ws);
        if (socket === ws) socket = null;
        try {
          ws.close();
        } catch {
          // Already closing.
        }
        if (disposed) return;
        subscribed = false;
        noteFailure();
      };

      ws.onopen = () => {
        if (disposed || socket !== ws) {
          detach(ws);
          try {
            ws.close();
          } catch {
            // Already closing.
          }
          return;
        }
        // First client frame; the server waits 10 s for it and then refuses.
        ws.send(JSON.stringify({ subscribe: 'queue', id: sessionId }));
      };
      ws.onmessage = (event: MessageEvent) => {
        if (disposed || socket !== ws) return;
        if (typeof event.data !== 'string') return;
        onFrame(event.data, fail);
      };
      // A failed handshake fires `error` and then `close`; `fail` is settled
      // once, so either path counts as one failure.
      ws.onerror = () => undefined;
      ws.onclose = () => { fail(); };
    };

    const connect = async () => {
      if (disposed || authFailed || connecting || socket !== null) return;
      // The topic id is parsed as a UUID by the server; a shift we cannot
      // name would earn an `error` frame and a pointless retry loop, so the
      // timer is the only channel in that case.
      if (sessionId === null || !isUuid(sessionId)) return;
      connecting = true;
      try {
        let ticket: string;
        try {
          ticket = (await getWsTicket()).ticket;
        } catch (err) {
          if (disposed) return;
          if (err instanceof ApiRequestError && err.status === 401) {
            authFailed = true;
            stopChannels();
            setConnection('offline');
            return;
          }
          noteFailure();
          return;
        }
        if (disposed || authFailed) return;
        openSocket(ticket);
      } finally {
        connecting = false;
      }
    };

    const onVisibilityChange = () => {
      if (disposed || authFailed) return;
      if (hidden()) {
        // A background tab is not a console: every timer pauses. The socket
        // is left alone — the browser may close it, and the close path
        // defers the reconnect to the next visible transition.
        if (pollTimer !== null) {
          clearTimeout(pollTimer);
          pollTimer = null;
        }
        clearReconnect();
        clearRefresh();
        clearPrune();
        return;
      }
      // Back in front of the DJ: the list is the likeliest thing to be stale.
      // A badge that aged while the tab was in the background goes now.
      pruneExpired();
      if (pollActive) schedulePoll();
      void readQueue();
      if (!subscribed && socket === null) void connect();
    };

    document.addEventListener('visibilitychange', onVisibilityChange);
    refreshRef.current = refreshNow;
    // The timer is the floor from the first paint; the socket takes over the
    // moment it proves itself, and gives it back when it drops.
    startPolling();
    schedulePrune();
    setConnection(channelLabel());
    void readQueue();
    void connect();

    return () => {
      disposed = true;
      runTokenRef.current += 1;
      refreshRef.current = () => undefined;
      document.removeEventListener('visibilitychange', onVisibilityChange);
      clearReconnect();
      clearRefresh();
      clearPrune();
      if (pollTimer !== null) {
        clearTimeout(pollTimer);
        pollTimer = null;
      }
      closeSocket('console closed');
    };
  }, [active, sessionId, sort]);

  return { queue, connection, lastSyncAt, newIds, refresh };
}
