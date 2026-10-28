import { useMemo, useState } from 'react';
import { useNavigate, useSearchParams } from 'react-router-dom';
import { useAdminData } from '../hooks/useAdminData';
import AuditLogStream, { type ConsoleAuditEvent } from '../components/admin-console/AuditLogStream';
import QuickActionConsole from '../components/admin-console/QuickActionConsole';
import StatusBadge from '../components/StatusBadge';
import PageHeading from '../components/PageHeading';
import Icon from '../components/Icon';
import { useLocale } from '../locale';

/** Severity is derived from the stored action name so the stream stays in sync with the mock audit trail. */
function severityFor(action: string): ConsoleAuditEvent['severity'] {
  if (/failed|deleted|blocked|rejected/i.test(action)) return 'critical';
  if (/suspend|updated|bulk|created|flagged/i.test(action)) return 'warning';
  return 'info';
}

const SEVERITY_KEY: Record<ConsoleAuditEvent['severity'], string> = {
  critical: 'status.critical', warning: 'status.warning', info: 'status.info',
};

function toPayload(diff: Record<string, unknown> | null): Record<string, string | number | boolean> {
  if (!diff) return {};
  return Object.fromEntries(Object.entries(diff).map(([key, value]) => [
    key,
    typeof value === 'string' || typeof value === 'number' || typeof value === 'boolean' ? value : JSON.stringify(value),
  ]));
}

export default function AdminActivityPage() {
  const navigate = useNavigate();
  const { t, locale } = useLocale();
  const fmtCount = (value: number) => new Intl.NumberFormat(locale).format(value);
  const audit = useAdminData('audit');
  const users = useAdminData('users');
  const clubs = useAdminData('clubs');
  const djs = useAdminData('djs');
  const transactions = useAdminData('transactions');
  const [searchParams, setSearchParams] = useSearchParams();
  const [toast, setToast] = useState('');
  const [query, setQuery] = useState(() => searchParams.get('q') ?? '');

  const events = useMemo<ConsoleAuditEvent[]>(() => audit.data.map((entry) => ({
    id: entry.id,
    at: new Intl.DateTimeFormat(locale, { hour: '2-digit', minute: '2-digit', second: '2-digit' }).format(new Date(entry.at)),
    severity: severityFor(entry.action),
    actor: `${entry.actorType}:${entry.actorId}`,
    action: entry.action,
    target: entry.entity,
    payload: toPayload(entry.diff),
  })), [audit.data, locale]);

  const visibleEvents = useMemo(() => {
    const needle = query.trim().toLowerCase();
    if (!needle) return events;
    return events.filter((event) => `${event.action} ${event.actor} ${event.target}`.toLowerCase().includes(needle));
  }, [events, query]);

  const counts = useMemo(() => ({
    critical: events.filter((event) => event.severity === 'critical').length,
    warning: events.filter((event) => event.severity === 'warning').length,
    info: events.filter((event) => event.severity === 'info').length,
  }), [events]);

  const rollback = (event: ConsoleAuditEvent) => {
    const target = event.target.split(',');
    let changed = 0;
    for (const id of target) {
      if (users.allData.some((row) => row.id === id)) { users.update(id, { status: 'active' }); changed += 1; }
      else if (djs.allData.some((row) => row.id === id)) { djs.update(id, { status: 'active' }); changed += 1; }
      else if (clubs.allData.some((row) => row.id === id)) { clubs.update(id, { status: 'active' }); changed += 1; }
      else if (transactions.allData.some((row) => row.id === id)) { transactions.update(id, { status: 'captured' }); changed += 1; }
    }
    setToast(t('activity.rollbackToast', { changed: fmtCount(changed), total: fmtCount(target.length), id: event.id }));
  };

  const ban = (event: ConsoleAuditEvent) => {
    const target = event.target.split(',');
    let changed = 0;
    for (const id of target) {
      if (users.allData.some((row) => row.id === id)) { users.update(id, { status: 'blocked' }); changed += 1; }
      else if (djs.allData.some((row) => row.id === id)) { djs.update(id, { status: 'blocked' }); changed += 1; }
      else if (clubs.allData.some((row) => row.id === id)) { clubs.update(id, { status: 'blocked' }); changed += 1; }
    }
    setToast(t('activity.banToast', { changed: fmtCount(changed), total: fmtCount(target.length), id: event.id }));
  };

  const applyFilter = (value: string) => {
    setQuery(value);
    const next = new URLSearchParams(searchParams);
    if (value) next.set('q', value); else next.delete('q');
    setSearchParams(next, { replace: true });
  };

  return (
    <div>
      <PageHeading
        title={t('activity.title')}
        action={(
          <>
            <QuickActionConsole onAction={() => navigate('/manage/access')} />
            <button type="button" className="btn btn-secondary" onClick={() => users.download('json')}>
              <Icon name="lucide:download" size={15} />
              {t('management.exportJson')}
            </button>
          </>
        )}
      />

      <p className="admin-page-note">
        <strong>{t('management.previewTitle')}</strong>{t('activity.noteBody')}
      </p>

      <div style={{ display: 'flex', flexWrap: 'wrap', alignItems: 'center', gap: 8, marginBottom: 16 }} role="status">
        <span className="chip chip-error">{t(SEVERITY_KEY.critical)} · {fmtCount(counts.critical)}</span>
        <span className="chip chip-warn">{t(SEVERITY_KEY.warning)} · {fmtCount(counts.warning)}</span>
        <span className="chip chip-neutral">{t(SEVERITY_KEY.info)} · {fmtCount(counts.info)}</span>
        <span className="chip chip-neutral">{t('activity.shown', { count: fmtCount(visibleEvents.length) })}</span>
        <span className="chip chip-neutral">{t('activity.totalEvents', { count: fmtCount(events.length) })}</span>
      </div>

      <label style={{ display: 'block', marginBottom: 16, maxWidth: 420 }}>
        <span className="sr-only">{t('activity.filterLabel')}</span>
        <input
          className="field-input"
          value={query}
          onChange={(event) => applyFilter(event.target.value)}
          placeholder={t('activity.filterPlaceholder')}
        />
      </label>

      {toast ? <p className="admin-toast" role="status" aria-live="polite" style={{ marginBottom: 16 }}>{toast}</p> : null}

      <AuditLogStream events={visibleEvents} onRollback={rollback} onBan={ban} />

      <section className="panel-card panel-section" aria-labelledby="severity-rules-heading">
        <header className="panel-card-head">
          <div>
            <h2 id="severity-rules-heading" className="panel-card-title">{t('activity.severityRules')}</h2>
            <p className="panel-card-sub">{t('activity.severityRulesSubtitle')}</p>
          </div>
        </header>
        <ul className="list" style={{ margin: 0, padding: 0, listStyle: 'none' }}>
          <li className="list-row">
            <span style={{ flex: '0 0 auto' }}><StatusBadge status="critical" /></span>
            <span className="list-main"><span className="list-sub" style={{ marginTop: 0 }}>{t('activity.ruleCritical')}</span></span>
          </li>
          <li className="list-row">
            <span style={{ flex: '0 0 auto' }}><StatusBadge status="warning" /></span>
            <span className="list-main"><span className="list-sub" style={{ marginTop: 0 }}>{t('activity.ruleWarning')}</span></span>
          </li>
          <li className="list-row">
            <span style={{ flex: '0 0 auto' }}><StatusBadge status="info" /></span>
            <span className="list-main"><span className="list-sub" style={{ marginTop: 0 }}>{t('activity.ruleInfo')}</span></span>
          </li>
        </ul>
      </section>
    </div>
  );
}

