/**
 * "Next actions" is derived, not authored.
 *
 * Every entry is a consequence of a value the board already has, and every
 * entry is a link to the page that can actually perform it. Nothing here
 * promises an action the product cannot do (no "take over", no "save rules"),
 * and no entry contains a figure that is not also a labelled metric elsewhere
 * on the board — so the list never becomes a second, unlabelled data source.
 */
import type { MetricSource } from '../SourceChip';
import type { BoardSessionStatus } from './state';

export interface NextAction {
  id: string;
  title: string;
  detail: string;
  to: string;
  /** Right-aligned destination label. */
  meta: string;
}

export interface NextActionInput {
  session: { id: string; status: BoardSessionStatus } | null;
  sessionSource: MetricSource;
  djs: ReadonlyArray<{ id: string }>;
  djsSource: MetricSource;
  requestPriceSource: MetricSource;
  requestPriceMinor: number | null;
  requests24h: number | null;
  sessionQrSource: MetricSource;
}

const MAX_ACTIONS = 3;

/** Translation function shape; `action.*` keys live in `messages/dashboard.ts`. */
type Translate = (key: string) => string;

export function deriveNextActions(input: NextActionInput, t: Translate): NextAction[] {
  const { session, sessionSource, djs, djsSource, requestPriceSource, requestPriceMinor, requests24h } =
    input;
  const actions: NextAction[] = [];

  if (sessionSource === 'unavailable') {
    actions.push({
      id: 'shift-unknown',
      title: t('action.checkShift'),
      detail: t('action.checkShiftDetail'),
      to: '/sessions',
      meta: t('action.liveRoom'),
    });
  } else if (!session) {
    actions.push({
      id: 'open-shift',
      title: t('action.openShift'),
      detail: t('action.openShiftDetail'),
      to: '/sessions',
      meta: t('action.liveRoom'),
    });
  } else if (session.status === 'paused_acceptance') {
    actions.push({
      id: 'resume-acceptance',
      title: t('action.resumeAcceptance'),
      detail: t('action.resumeAcceptanceDetail'),
      to: '/sessions',
      meta: t('action.liveRoom'),
    });
  } else if (session.status === 'created') {
    actions.push({
      id: 'start-acceptance',
      title: t('action.startAcceptance'),
      detail: t('action.startAcceptanceDetail'),
      to: '/sessions',
      meta: t('action.liveRoom'),
    });
  } else if (session.status === 'closed') {
    actions.push({
      id: 'reopen-shift',
      title: t('action.reopenShift'),
      detail: t('action.reopenShiftDetail'),
      to: '/sessions',
      meta: t('action.liveRoom'),
    });
  }

  if (djsSource === 'unavailable') {
    actions.push({
      id: 'lineup-unknown',
      title: t('action.reloadLineup'),
      detail: t('action.reloadLineupDetail'),
      to: '/djs',
      meta: t('action.djsPage'),
    });
  } else if (djs.length === 0) {
    actions.push({
      id: 'invite-dj',
      title: t('action.inviteDj'),
      detail: t('action.inviteDjDetail'),
      to: '/djs',
      meta: t('action.djsPage'),
    });
  }
  // No "put a DJ on shift" entry: the club API does not report which DJ is on
  // shift (see DecksPanel), so the board cannot know that one is missing.

  if (requestPriceSource === 'unavailable' || requestPriceMinor === null) {
    actions.push({
      id: 'set-price',
      title: t('action.setPrice'),
      detail: t('action.setPriceUnreadDetail'),
      to: '/pricing',
      meta: t('action.pricingPage'),
    });
  }

  // No door-QR entry. The board no longer issues or rotates the door token
  // (5.1.8: rotation lives in Live room), so it cannot know whether one exists;
  // an alert saying it "could not be generated" would describe a state the board
  // never observed (RT-01). The Guest access tile carries the honest pointer.

  // Steady-state work that is always honest and always available, used only to
  // fill the panel to three rows so the tile never ends in a void.
  const steady: NextAction[] = [
    {
      id: 'review-money',
      title: t('action.reviewNet'),
      detail: requests24h && requests24h > 0
        ? t('action.reviewNetDetailActive')
        : t('action.reviewNetDetailIdle'),
      to: '/earnings',
      meta: t('action.earningsPage'),
    },
    {
      id: 'review-lineup',
      title: t('action.reviewRoster'),
      detail: t('action.reviewRosterDetail'),
      to: '/djs',
      meta: t('action.djsPage'),
    },
    {
      id: 'review-price',
      title: t('action.reviewPrice'),
      detail: t('action.reviewPriceDetail'),
      to: '/pricing',
      meta: t('action.pricingPage'),
    },
    {
      id: 'review-room',
      title: t('action.openLiveRoom'),
      detail: t('action.openLiveRoomDetail'),
      to: '/sessions',
      meta: t('action.liveRoom'),
    },
  ];

  for (const item of steady) {
    if (actions.length >= MAX_ACTIONS) break;
    if (actions.some((action) => action.id === item.id || action.to === item.to)) continue;
    actions.push(item);
  }

  return actions.slice(0, MAX_ACTIONS);
}
