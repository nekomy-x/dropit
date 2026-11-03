/**
 * Live-only additions to the console adapter contract.
 *
 * `DjConsoleMockApi` describes what a view can render, and the demo adapter
 * plus the live adapter both satisfy it. Only the live adapter can know
 * whether a server read *failed*, so the honesty flags are declared here as an
 * optional extension instead of widening `types.ts` (the demo fixtures do not
 * need them, and an absent `liveData` already means "demo").
 *
 * The rule these flags exist for is `docs/ui/PANEL-DESIGN.md` §Data honesty:
 * a failed read renders the word `unavailable` with no value — never `0`.
 */

/** How the shift log on screen was obtained. */
export type DjHistoryAvailability = 'loading' | 'ready' | 'unavailable';

/** Read state of the shift-scoped counters (`GET /v1/dj/stats/session`). */
export type DjStatsAvailability = 'ok' | 'unavailable';

/** What the live console knows about its own data, for honest rendering. */
export interface DjConsoleLiveData {
  /** `unavailable` — the shift counters could not be read. */
  stats: DjStatsAvailability;
  /** `unavailable` — the shift log could not be read. */
  history: DjHistoryAvailability;
  /** The log has another page (`next_cursor` came back non-null). */
  hasMoreHistory: boolean;
  /** A further page is being read right now. */
  historyLoading: boolean;
  /** Reads the next page; a no-op while a page is already in flight. */
  loadMoreHistory: () => void;
}

declare module './types' {
  interface DjConsoleMockApi {
    /**
     * Present only on the live adapter. Views use it to distinguish "the
     * server said zero" from "the server did not answer", and demo mode keeps
     * rendering its fixtures.
     */
    liveData?: DjConsoleLiveData;
  }
}