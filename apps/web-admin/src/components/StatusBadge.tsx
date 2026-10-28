import { useLocale } from '../locale';
import { STATUS_MESSAGE_KEY } from '../messages/status';

type ChipTone = 'accent' | 'warn' | 'error' | 'neutral';

/**
 * Status tones — same mapping the panel used before the redesign, now expressed
 * as the shared `chip-*` classes. The stored word is always printed, so status
 * never depends on colour alone (docs/ui/PANEL-DESIGN.md).
 */
const STATUS_TONE: Record<string, ChipTone> = {
  active: 'accent',
  paid: 'accent',
  captured: 'accent',
  verified: 'accent',
  in_flight: 'warn',
  pending: 'warn',
  pending_verification: 'warn',
  scheduled: 'neutral',
  on_hold: 'neutral',
  invited: 'neutral',
  suspended: 'warn',
  blocked: 'error',
  failed: 'error',
  outage: 'error',
  critical: 'error',
  refunded: 'neutral',
  partially_refunded: 'warn',
  cancelled: 'neutral',
  cancelling: 'warn',
  // audit severities (admin-console/AuditLogStream)
  warning: 'warn',
  info: 'neutral',
  // mail_message.status vocabulary (docs/mail-infra-contract.md §2). Only the
  // tone is mapped here — the stored word is what gets printed.
  queued: 'neutral',
  sending: 'warn',
  sent: 'accent',
  delivered: 'accent',
  delivery_delayed: 'warn',
  bounced: 'error',
  complained: 'error',
  suppressed: 'neutral',
};

export default function StatusBadge({ status }: { status: string }) {
  const { t } = useLocale();
  const tone = STATUS_TONE[status] ?? 'neutral';
  const key = STATUS_MESSAGE_KEY[status];
  return (
    <span className={`chip chip-${tone}`}>
      {key ? t(key) : status.replaceAll('_', ' ')}
    </span>
  );
}