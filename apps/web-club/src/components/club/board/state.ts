/**
 * Board copy derived from the hook's session state.
 *
 * The board never invents a venue mode: the only shift states that exist are
 * the ones the API returns (01-contract.md §7 `BoardSession['status']`). Every
 * sentence below is a statement about data the board actually has.
 */
import type { MetricSource } from '../SourceChip';

/** Translation function shape; `board.*` keys live in `messages/dashboard.ts`. */
type Translate = (key: string) => string;

export type BoardSessionStatus = 'created' | 'active' | 'paused_acceptance' | 'closed';

export interface SessionState {
  /** h1 of the board — the answer to "what is happening in the room". */
  headline: string;
  /** One sentence that explains the headline without repeating it. */
  detail: string;
  /** Chip word. Normal case; never colour alone. */
  chip: string;
  tone: 'live' | 'warn' | 'error' | 'neutral';
  /** The single next step, shown inside "Room now". */
  next: string;
  /** Label for the control that performs it. */
  action: string;
  to: string;
  /** True when guests can actually send requests right now. */
  accepting: boolean;
}

export function describeSession(
  session: { id: string; status: BoardSessionStatus } | null,
  source: MetricSource,
  t: Translate,
): SessionState {
  if (source === t('board.chipUnavailable')) {
    return {
      headline: t('board.headlineUnavailable'),
      detail: t('board.detailUnavailable'),
      chip: t('board.chipUnavailable'),
      tone: 'error',
      next: t('board.nextUnavailable'),
      action: t('board.actionOpenLiveRoom'),
      to: '/sessions',
      accepting: false,
    };
  }
  if (!session) {
    return {
      headline: t('board.headlineNoShift'),
      detail: t('board.detailNoShift'),
      chip: t('board.chipNoShift'),
      tone: 'neutral',
      next: t('board.nextNoShift'),
      action: t('board.actionOpenLiveRoom'),
      to: '/sessions',
      accepting: false,
    };
  }
  switch (session.status) {
    case 'active':
      return {
        headline: t('board.headlineActive'),
        detail: t('board.detailActive'),
        chip: t('board.chipActive'),
        tone: 'live',
        next: t('board.nextActive'),
        action: t('board.actionOpenLiveRoom'),
        to: '/sessions',
        accepting: true,
      };
    case 'paused_acceptance':
      return {
        headline: t('board.headlinePaused'),
        detail: t('board.detailPaused'),
        chip: t('board.chipPaused'),
        tone: 'warn',
        next: t('board.nextPaused'),
        action: t('board.actionResume'),
        to: '/sessions',
        accepting: false,
      };
    case 'closed':
      return {
        headline: t('board.headlineClosed'),
        detail: t('board.detailClosed'),
        chip: t('board.chipClosed'),
        tone: 'neutral',
        next: t('board.nextClosed'),
        action: t('board.actionOpenLiveRoom'),
        to: '/sessions',
        accepting: false,
      };
    case 'created':
    default:
      return {
        headline: t('board.headlineCreated'),
        detail: t('board.detailCreated'),
        chip: t('board.chipCreated'),
        tone: 'neutral',
        next: t('board.nextCreated'),
        action: t('board.actionStart'),
        to: '/sessions',
        accepting: false,
      };
  }
}
