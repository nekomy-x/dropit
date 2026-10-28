import { useCallback, useMemo, useRef, useState } from 'react';
import { createMockDjConsoleState } from './mockData';
import type {
  DjConsoleMockApi,
  DjConsoleMockOptions,
  DjConsoleRequest,
  DjConsoleState,
  DjHistoryEntry,
  DjNowPlaying,
  DjRequestAction,
  DjTip,
} from './types';

function copyState(state: DjConsoleState): DjConsoleState {
  return {
    ...state,
    session: state.session ? { ...state.session } : null,
    nowPlaying: { ...state.nowPlaying, track: state.nowPlaying.track ? { ...state.nowPlaying.track } : null },
    requests: state.requests.map((item) => ({ ...item })),
    history: state.history.map((item) => ({ ...item, track: { ...item.track } })),
    tips: state.tips.map((item) => ({ ...item })),
    telemetry: { ...state.telemetry },
    stats: { ...state.stats },
  };
}

function mergeInitial(initial: Partial<DjConsoleState> | undefined): DjConsoleState {
  const base = createMockDjConsoleState();
  if (!initial) return base;
  return copyState({
    ...base,
    ...initial,
    nowPlaying: initial.nowPlaying ?? base.nowPlaying,
    requests: initial.requests ?? base.requests,
    history: initial.history ?? base.history,
    tips: initial.tips ?? base.tips,
    telemetry: { ...base.telemetry, ...initial.telemetry },
    stats: { ...base.stats, ...initial.stats },
  });
}

const requestTrack = (request: DjConsoleRequest) => ({
  id: request.id,
  title: request.title,
  artist: request.artist,
  style: request.style,
  coverUrl: request.coverUrl,
  bpm: request.bpm,
  key: request.key,
});

export function useDjConsoleMock(options: DjConsoleMockOptions = {}): DjConsoleMockApi {
  const nowRef = useRef(options.now ?? (() => new Date()));
  const initialRef = useRef(mergeInitial(options.initialState));
  const [state, setState] = useState<DjConsoleState>(() => copyState(initialRef.current));

  const action = useCallback((requestId: string, actionType: DjRequestAction) => {
    setState((previous) => {
      const target = previous.requests.find((item) => item.id === requestId);
      if (!target || (target.status !== 'pending' && target.status !== 'accepted')) return previous;
      if (actionType === 'accept' && target.status !== 'pending') return previous;

      const occurredAt = nowRef.current().toISOString();
      const nextStatus = actionType === 'accept' ? 'accepted' : actionType === 'reject' ? 'rejected' : 'played';
      const stats = { ...previous.stats };
      const wasAccepted = target.status === 'accepted';
      if (actionType === 'accept') {
        stats.accepted += 1;
        stats.earnedMinor += target.djShareMinor;
      } else if (actionType === 'reject') {
        stats.rejected += 1;
        if (wasAccepted) {
          stats.accepted = Math.max(0, stats.accepted - 1);
          stats.earnedMinor = Math.max(0, stats.earnedMinor - target.djShareMinor);
        }
      } else {
        stats.played += 1;
        if (!wasAccepted) stats.earnedMinor += target.djShareMinor;
      }

      let history = previous.history;
      if (actionType !== 'accept') {
        const entry: DjHistoryEntry = {
          id: `history_${requestId}_${occurredAt}`,
          requestId,
          track: requestTrack(target),
          status: actionType === 'played' ? 'played' : 'rejected',
          occurredAt,
          djShareMinor: actionType === 'played' ? target.djShareMinor : undefined,
        };
        history = [entry, ...history];
      }

      return {
        ...previous,
        requests: previous.requests.map((item) => item.id === requestId ? { ...item, status: nextStatus } : item),
        nowPlaying: actionType === 'played' ? { track: requestTrack(target), startedAt: occurredAt, elapsedSeconds: 0, source: 'local', deck: 'A' } : previous.nowPlaying,
        history,
        stats,
        telemetry: { ...previous.telemetry, lastSyncedAt: occurredAt, eventsReceived: previous.telemetry.eventsReceived + 1, lastEvent: 'request_updated' },
        lastUpdatedAt: occurredAt,
      };
    });
  }, []);

  const pinRequest = useCallback((requestId: string) => {
    setState((previous) => {
      const target = previous.requests.find((item) => item.id === requestId);
      if (!target || target.status !== 'pending') return previous;
      const occurredAt = nowRef.current().toISOString();
      return { ...previous, requests: previous.requests.map((item) => item.id === requestId ? { ...item, status: 'pending', vip: true, createdAt: occurredAt } : item), lastUpdatedAt: occurredAt };
    });
  }, []);

  const setNowPlaying = useCallback((nowPlaying: DjNowPlaying) => {
    setState((previous) => {
      const stamp = nowRef.current().toISOString();
      return {
        ...previous,
        nowPlaying: { ...nowPlaying, track: nowPlaying.track ? { ...nowPlaying.track } : null },
        telemetry: { ...previous.telemetry, lastSyncedAt: stamp, eventsReceived: previous.telemetry.eventsReceived + 1, lastEvent: 'player_changed' },
        lastUpdatedAt: stamp,
      };
    });
  }, []);

  const addRequest = useCallback((request: DjConsoleRequest) => {
    setState((previous) => {
      if (previous.requests.some((item) => item.id === request.id)) return previous;
      const stamp = nowRef.current().toISOString();
      return {
        ...previous,
        requests: [{ ...request }, ...previous.requests],
        stats: { ...previous.stats, requests: previous.stats.requests + 1 },
        telemetry: { ...previous.telemetry, lastSyncedAt: stamp, eventsReceived: previous.telemetry.eventsReceived + 1, lastEvent: 'request_received' },
        lastUpdatedAt: stamp,
      };
    });
  }, []);

  const addTip = useCallback((tip: DjTip) => {
    setState((previous) => {
      if (previous.tips.some((item) => item.id === tip.id)) return previous;
      const stamp = nowRef.current().toISOString();
      return {
        ...previous,
        tips: [{ ...tip }, ...previous.tips],
        telemetry: { ...previous.telemetry, lastSyncedAt: stamp, eventsReceived: previous.telemetry.eventsReceived + 1, lastEvent: 'tip_received' },
        lastUpdatedAt: stamp,
      };
    });
  }, []);

  const acknowledgeTip = useCallback((tipId: string) => {
    setState((previous) => {
      if (!previous.tips.some((tip) => tip.id === tipId && tip.status === 'received')) return previous;
      const stamp = nowRef.current().toISOString();
      return { ...previous, tips: previous.tips.map((tip) => tip.id === tipId ? { ...tip, status: 'acknowledged' } : tip), lastUpdatedAt: stamp };
    });
  }, []);

  const setAcceptance = useCallback((paused: boolean) => {
    setState((previous) => {
      if (!previous.session) return previous;
      const status = paused ? 'paused_acceptance' : 'active';
      if (previous.session.status === status) return previous;
      return { ...previous, session: { ...previous.session, status }, lastUpdatedAt: nowRef.current().toISOString() };
    });
  }, []);

  const toggleAcceptance = useCallback(() => {
    setState((previous) => {
      if (!previous.session) return previous;
      const paused = previous.session.status === 'paused_acceptance';
      return { ...previous, session: { ...previous.session, status: paused ? 'active' : 'paused_acceptance' }, lastUpdatedAt: nowRef.current().toISOString() };
    });
  }, []);

  const replaceState = useCallback((next: Partial<DjConsoleState>) => {
    setState((previous) => mergeInitial({ ...previous, ...next }));
  }, []);
  const reset = useCallback(() => setState(copyState(initialRef.current)), []);
  const queue = useMemo(() => state.requests
    .filter((item) => item.status === 'pending' || item.status === 'accepted')
    .sort((a, b) => Date.parse(b.createdAt) - Date.parse(a.createdAt)), [state.requests]);

  return { state, queue, history: state.history, tips: state.tips, telemetry: state.telemetry, action, pinRequest, setNowPlaying, addRequest, addTip, acknowledgeTip, setAcceptance, toggleAcceptance, replaceState, reset };
}
