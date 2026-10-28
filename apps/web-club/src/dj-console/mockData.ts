import type { DjConsoleRequest, DjConsoleState, DjHistoryEntry, DjTip } from './types';

const minutesAgo = (minutes: number): string => new Date(Date.now() - minutes * 60_000).toISOString();

export const MOCK_DJ_REQUESTS: DjConsoleRequest[] = [
  { id: 'req_mock_01', title: 'One More Time', artist: 'Daft Punk', style: 'French house', createdAt: minutesAgo(3), status: 'pending', sameTrackCount: 3, guestName: 'Alex', djShareMinor: 210, priceMinor: 300, tipMinor: 500 },
  { id: 'req_mock_02', title: 'Show Me Love', artist: 'Robin S', style: 'House', createdAt: minutesAgo(8), status: 'accepted', sameTrackCount: 1, guestName: 'Mia', note: 'When the room is ready', djShareMinor: 140, priceMinor: 200 },
  { id: 'req_mock_03', title: 'Music Sounds Better With You', artist: 'Stardust', style: 'Filter house', createdAt: minutesAgo(14), status: 'pending', sameTrackCount: 2, guestName: 'Sam', djShareMinor: 140, priceMinor: 200 },
  { id: 'req_mock_04', title: 'Finally', artist: 'Kings of Tomorrow', style: 'Classic house', createdAt: minutesAgo(22), status: 'played', sameTrackCount: 1, djShareMinor: 210, priceMinor: 300, tipMinor: 500 },
];

export const MOCK_DJ_HISTORY: DjHistoryEntry[] = [
  { id: 'hist_mock_01', requestId: 'req_mock_04', track: { id: 'track_mock_04', title: 'Finally', artist: 'Kings of Tomorrow', style: 'Classic house', bpm: 126, key: '4A' }, status: 'played', occurredAt: minutesAgo(22), djShareMinor: 210 },
  { id: 'hist_mock_02', track: { id: 'track_mock_05', title: 'Music Is the Answer', artist: 'Danny Tenaglia', style: 'House', bpm: 125, key: '8A' }, status: 'rejected', occurredAt: minutesAgo(31) },
];

export const MOCK_DJ_TIPS: DjTip[] = [
  { id: 'tip_mock_01', guestName: 'Jordan', amountMinor: 500, status: 'received', receivedAt: minutesAgo(5) },
  { id: 'tip_mock_02', requestId: 'req_mock_02', guestName: 'Mia', amountMinor: 200, status: 'acknowledged', receivedAt: minutesAgo(18) },
];

export const MOCK_DJ_STATE: DjConsoleState = {
  session: { id: 'session_mock_01', status: 'active', timezone: 'Europe/Madrid', clubId: 'club_mock_01' },
  nowPlaying: {
    track: { id: 'track_mock_now', title: 'Music Sounds Better With You', artist: 'Stardust', style: 'Filter house', durationSeconds: 395, bpm: 124, key: '8A' },
    startedAt: minutesAgo(2),
    elapsedSeconds: 132,
    source: 'rekordbox',
    deck: 'A',
  },
  requests: MOCK_DJ_REQUESTS,
  history: MOCK_DJ_HISTORY,
  tips: MOCK_DJ_TIPS,
  telemetry: { connection: 'connected', lastSyncedAt: new Date().toISOString(), queueLatencyMs: 86, requestsPerMinute: 2, eventsReceived: 42, lastEvent: 'request_received' },
  stats: { requests: 18, accepted: 11, played: 7, rejected: 2, earnedMinor: 2_940 },
  lastUpdatedAt: new Date().toISOString(),
};

export function createMockDjConsoleState(): DjConsoleState {
  return {
    ...MOCK_DJ_STATE,
    session: MOCK_DJ_STATE.session ? { ...MOCK_DJ_STATE.session } : null,
    nowPlaying: { ...MOCK_DJ_STATE.nowPlaying, track: MOCK_DJ_STATE.nowPlaying.track ? { ...MOCK_DJ_STATE.nowPlaying.track } : null },
    requests: MOCK_DJ_REQUESTS.map((item) => ({ ...item })),
    history: MOCK_DJ_HISTORY.map((item) => ({ ...item, track: { ...item.track } })),
    tips: MOCK_DJ_TIPS.map((item) => ({ ...item })),
    telemetry: { ...MOCK_DJ_STATE.telemetry },
    stats: { ...MOCK_DJ_STATE.stats },
  };
}
