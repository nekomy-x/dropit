import Chip, { type ChipTone } from './club/Chip';
import { useLocale } from '../locale';

/**
 * Status of a club object, rendered with the `Chip` primitive.
 *
 * Contract (§4.5/§4.9): pill only for status chips, tint background, no border,
 * and the word is always visible — colour is a second signal, never the only
 * one. The old `.status-badge` classes were a second, bordered status system;
 * this file now has exactly one rendering path.
 */
const STATUS_TONE: Record<string, ChipTone> = {
  active: 'live',
  open: 'live',
  live: 'live',
  verified: 'live',
  accepted: 'live',
  played: 'neutral',
  pending: 'warn',
  pending_verification: 'warn',
  invited: 'neutral',
  paused: 'warn',
  paused_acceptance: 'warn',
  suspended: 'warn',
  closed: 'neutral',
  blocked: 'error',
  failed: 'error',
  rejected: 'error',
  offline: 'neutral',
};

/**
 * The word is the status; the tint is the second signal. Server enum values are
 * translated through `status.*`; an enum the catalog does not know yet falls
 * back to the raw value with underscores turned into spaces, which is still
 * readable rather than blank.
 */
export default function StatusBadge({ status }: { status: string }) {
  const { t, has } = useLocale();
  const key = `status.${status}`;
  const label = has(key) ? t(key) : status.replaceAll('_', ' ');
  return <Chip tone={STATUS_TONE[status] ?? 'neutral'}>{label}</Chip>;
}
