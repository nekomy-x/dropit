import { useMemo, useState } from 'react';
import StatusBadge from '../StatusBadge';
import Select from '../ui/Select';
import { useLocale } from '../../locale';
import { STATUS_MESSAGE_KEY } from '../../messages/status';

export type AuditSeverity = 'critical' | 'warning' | 'info';
export interface ConsoleAuditEvent {
  id: string;
  at: string;
  severity: AuditSeverity;
  actor: string;
  action: string;
  target: string;
  payload: Record<string, string | number | boolean>;
}
export interface AuditLogStreamProps {
  events?: ConsoleAuditEvent[];
  onRollback?: (event: ConsoleAuditEvent) => void;
  onBan?: (event: ConsoleAuditEvent) => void;
}

const EVENTS: ConsoleAuditEvent[] = [
  { id: 'evt_01J8M2', at: '23:14:08', severity: 'critical', actor: 'ops.alex', action: 'club.suspended', target: 'club_velvet_room', payload: { reason: 'payment_dispute', previousStatus: 'active', initiatedBy: 'ops.alex', approved: true } },
  { id: 'evt_01J8LQ', at: '23:09:41', severity: 'warning', actor: 'mchen', action: 'user.flagged', target: 'usr_84f2a19c', payload: { reason: 'chargeback_review', riskScore: 72, automated: false } },
  { id: 'evt_01J8KX', at: '22:58:16', severity: 'info', actor: 'system', action: 'payout.processed', target: 'pay_8830', payload: { amount: 248.5, currency: 'EUR', destination: 'club_account', attempt: 1 } },
  { id: 'evt_01J8JN', at: '22:47:03', severity: 'warning', actor: 'ops.jules', action: 'dj.access_restored', target: 'dj_0192', payload: { previousStatus: 'suspended', newStatus: 'active', ticket: 'SUP-1842' } },
];
const SEVERITIES: Array<'all' | AuditSeverity> = ['all', 'critical', 'warning', 'info'];

/**
 * Audit stream: a list of events on the left, the selected event's payload on
 * the right.
 *
 * Presentation is a `panel-card` with hairlines between rows; severity is a
 * status chip with a word, so a row never relies on colour alone. The JSON block
 * reads `--c-inset` so it stays legible on both themes.
 */
export default function AuditLogStream({ events = EVENTS, onRollback, onBan }: AuditLogStreamProps) {
  const { t } = useLocale();
  const [severity, setSeverity] = useState<'all' | AuditSeverity>('all');
  const [selected, setSelected] = useState<string | null>(events[0]?.id ?? null);
  const [jsonExpanded, setJsonExpanded] = useState(true);
  const [actionNotice, setActionNotice] = useState('');
  const filtered = useMemo(() => events.filter((event) => severity === 'all' || event.severity === severity), [events, severity]);
  const active = filtered.find((event) => event.id === selected) ?? filtered[0];

  const rollback = () => {
    if (!active || !onRollback) return;
    onRollback(active);
    setActionNotice(t('auditStream.rollbackRequested', { id: active.id }));
  };
  const ban = () => {
    if (!active || !onBan) return;
    onBan(active);
    setActionNotice(t('auditStream.banRequested', { target: active.target }));
  };

  return (
    <section aria-labelledby="audit-stream-heading" className="panel-card">
      <header className="panel-card-head">
        <div>
          <h2 id="audit-stream-heading" className="panel-card-title">{t('auditStream.title')}</h2>
          <p className="panel-card-sub">{t('auditStream.subtitle')}</p>
        </div>
        <div className="panel-card-tools">
          <Select
            size="sm"
            className="admin-audit-severity"
            ariaLabel={t('auditStream.severity')}
            value={severity}
            onChange={(next) => setSeverity(next as 'all' | AuditSeverity)}
            options={SEVERITIES.map((item) => {
              const key = STATUS_MESSAGE_KEY[item];
              return { value: item, label: item === 'all' ? t('auditStream.allLevels') : key ? t(key) : item };
            })}
          />
        </div>
      </header>

      <div className="admin-audit-grid">
        <ol aria-label={t('auditStream.eventsLabel')} className="admin-audit-list">
          {filtered.map((event) => (
            <li key={event.id}>
              <button
                type="button"
                aria-pressed={active?.id === event.id}
                onClick={() => setSelected(event.id)}
                className="admin-audit-row"
              >
                <span className="admin-audit-time">{event.at}</span>
                <span style={{ minWidth: 0 }}>
                  <span style={{ marginRight: 8 }}><StatusBadge status={event.severity} /></span>
                  <span className="admin-audit-action">{event.action}</span>
                  <span className="admin-audit-target">{event.actor} → {event.target}</span>
                </span>
              </button>
            </li>
          ))}
          {filtered.length === 0 && (
            <li className="panel-empty" style={{ padding: '18px 8px' }}>
              <strong>{t('auditStream.emptyTitle')}</strong>
              <span>{t('auditStream.emptyBody')}</span>
            </li>
          )}
        </ol>

        <div style={{ minWidth: 0 }}>
          <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 12 }}>
            <h3 style={{ margin: 0, color: 'var(--c-text-2)', fontSize: 'var(--fs-12)', fontWeight: 500 }}>{t('auditStream.payload')}</h3>
            {active && (
              <button
                type="button"
                className="btn btn-secondary btn-sm"
                aria-expanded={jsonExpanded}
                onClick={() => setJsonExpanded((expanded) => !expanded)}
              >
                {jsonExpanded ? t('auditStream.collapseJson') : t('auditStream.expandJson')}
              </button>
            )}
          </div>

          {active ? (
            <>
              <p style={{ margin: '4px 0 0', color: 'var(--c-text-muted-sm)', fontFamily: 'var(--font-mono)', fontSize: 'var(--fs-11)', overflowWrap: 'anywhere' }}>
                {active.id} · {active.at}
              </p>
              {jsonExpanded && (
                <pre aria-label={t('auditStream.selectedJson')} className="admin-json">
                  <code>{JSON.stringify(active, null, 2)}</code>
                </pre>
              )}
              <div style={{ display: 'flex', flexWrap: 'wrap', gap: 8, marginTop: 12 }}>
                <button type="button" className="btn btn-secondary btn-sm" disabled={!onRollback} onClick={rollback}>{t('auditStream.rollback')}</button>
                <button type="button" className="btn btn-secondary btn-sm" disabled={!onBan} onClick={ban}>{t('auditStream.banTarget')}</button>
              </div>
              <p role="status" style={{ minHeight: 20, margin: '8px 0 0', color: 'var(--c-accent-quiet)', fontSize: 'var(--fs-11)' }}>{actionNotice}</p>
            </>
          ) : (
            <p className="panel-empty">{t('auditStream.selectEvent')}</p>
          )}
        </div>
      </div>
    </section>
  );
}
