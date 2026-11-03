/** UI-independent DJ console domain types for local and future live adapters. */

export type DjRequestStatus = 'pending' | 'accepted' | 'played' | 'rejected';
export type DjSessionStatus = 'created' | 'active' | 'paused_acceptance' | 'closed';
export type DjRequestAction = 'accept' | 'reject' | 'played';
export type DjTipStatus = 'received' | 'acknowledged';
export type DjConnectionStatus = 'connected' | 'degraded' | 'offline';

export interface DjTrack {
  id: string;
  title: string;
  artist: string;
  style: string | null;
  durationSeconds?: number;
  bpm?: number;
  key?: string;
  coverUrl?: string;
}

export interface DjNowPlaying {
  track: DjTrack | null;
  startedAt: string | null;
  elapsedSeconds: number;
  source?: 'rekordbox' | 'pioneer' | 'local' | 'unknown';
  deck?: 'A' | 'B' | null;
}

export interface DjConsoleRequest {
  id: string;
  title: string;
  artist: string;
  style: string | null;
  createdAt: string;
  status: DjRequestStatus;
  sameTrackCount: number;
  guestName?: string;
  note?: string;
  priceMinor?: number;
  tipMinor?: number;
  /** DJ share in minor currency units. */
  djShareMinor: number;
  coverUrl?: string;
  bpm?: number;
  key?: string;
  vip?: boolean;
  /** Catalog 30-second preview (`track_cache.preview_url`). Absent means the
   *  provider had none — the console says so instead of offering a dead
   *  control. */
  previewUrl?: string | null;
  /** Recording code from the provider, used to copy a track into DJ software. */
  isrc?: string | null;
}

/**
 * How the request list is being kept current.
 *
 * `live` is the WebSocket channel (`/v1/ws`, queue topic); `timer` is the
 * documented fallback when the socket cannot be established — it still updates
 * on its own, just later; `offline` means the last read failed and the list on
 * screen is the last one the API confirmed.
 */
export type DjLiveConnection = 'live' | 'timer' | 'offline';

/** What a view needs to know about freshness, without reaching into the adapter. */
export interface DjLiveStatus {
  connection: DjLiveConnection;
  /** Epoch ms of the last successful queue read. */
  lastSyncAt: number | null;
  /** Requests that arrived after the surface opened — highlighted, never hidden. */
  newIds: ReadonlySet<string>;
}

export interface DjHistoryEntry {
  id: string;
  requestId?: string;
  track: DjTrack;
  status: 'played' | 'rejected';
  occurredAt: string;
  djShareMinor?: number;
}

export interface DjTip {
  id: string;
  requestId?: string;
  guestName?: string;
  amountMinor: number;
  status: DjTipStatus;
  receivedAt: string;
}

export interface DjTelemetry {
  connection: DjConnectionStatus;
  lastSyncedAt: string | null;
  queueLatencyMs: number | null;
  requestsPerMinute: number;
  eventsReceived: number;
  lastEvent: 'request_received' | 'request_updated' | 'tip_received' | 'player_changed' | null;
}

export interface DjConsoleSession {
  id: string;
  status: DjSessionStatus;
  timezone: string;
  clubId: string | null;
}

export interface DjConsoleStats {
  requests: number;
  accepted: number;
  played: number;
  rejected: number;
  earnedMinor: number;
}

export interface DjConsoleState {
  session: DjConsoleSession | null;
  nowPlaying: DjNowPlaying;
  requests: DjConsoleRequest[];
  history: DjHistoryEntry[];
  tips: DjTip[];
  telemetry: DjTelemetry;
  stats: DjConsoleStats;
  lastUpdatedAt: string;
}

export interface DjConsoleMockOptions {
  /** Partial data is merged with mock fixtures, allowing focused scenarios. */
  initialState?: Partial<DjConsoleState>;
  now?: () => Date;
}

export interface DjConsoleMockApi {
  state: DjConsoleState;
  /** Pending and accepted requests, sorted newest first. */
  queue: DjConsoleRequest[];
  history: DjHistoryEntry[];
  tips: DjTip[];
  telemetry: DjTelemetry;
  /** `reason` is only meaningful for `reject` and is sent to
   *  POST /v1/dj/requests/{id}/reject (max 200 chars). */
  action: (requestId: string, action: DjRequestAction, reason?: string) => void;
  pinRequest: (requestId: string) => void;
  setNowPlaying: (nowPlaying: DjNowPlaying) => void;
  addRequest: (request: DjConsoleRequest) => void;
  addTip: (tip: DjTip) => void;
  acknowledgeTip: (tipId: string) => void;
  setAcceptance: (paused: boolean) => void;
  toggleAcceptance: () => void;
  replaceState: (next: Partial<DjConsoleState>) => void;
  reset: () => void;
}
