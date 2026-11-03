import { useCallback } from 'react';
import { getAuditLog, type AuditLogEntry } from '../api/client';
import { useAsyncData } from '../hooks/useAsyncData';
import ErrorState from '../components/ErrorState';
import Loading from '../components/Loading';
import PageHeading from '../components/PageHeading';
import Icon from '../components/Icon';
import { useLocale } from '../locale';

/**
 * Audit log from the live append-only panel endpoint.
 */
export default function AuditLogPage() {
  const { t, locale } = useLocale();
  const count = (value: number) => new Intl.NumberFormat(locale).format(value);
  const fetchLog = useCallback(() => getAuditLog(), []);
  const { data, loading, error, reload } = useAsyncData<{ items: AuditLogEntry[] }>(
    fetchLog,
  );

  return (
    <div>
      <PageHeading
        title={t('auditLog.title')}
        action={(
          <button type="button" className="btn btn-secondary" onClick={reload} disabled={loading}>
            <Icon name="lucide:refresh-cw" size={15} />
            {loading ? t('common.refreshing') : t('common.refresh')}
          </button>
        )}
      />

      {error && !error.notImplemented ? <ErrorState message={error.message} code={error.code} onRetry={reload} /> : null}
      {error?.notImplemented ? (
        <div className="panel-empty" role="status">
          <strong>{t('auditLog.pendingTitle')}</strong>
          <span>{t('auditLog.pendingBody')}</span>
        </div>
      ) : null}
      {loading && !data ? <Loading label={t('auditLog.loading')} /> : null}

      {data ? (
        data.items.length > 0 ? (
          <section className="panel-card" aria-labelledby="audit-table-heading">
            <header className="panel-card-head">
              <div>
                <h2 id="audit-table-heading" className="panel-card-title">{t('auditLog.recordedChanges')}</h2>
                <p className="panel-card-sub">{t('auditLog.cardSubtitle')}</p>
              </div>
              <div className="panel-card-tools">
                <span className="chip chip-neutral">{t('auditLog.count', { count: count(data.items.length) })}</span>
              </div>
            </header>

            <div className="table-wrap">
              <table className="data-table data-table--stack" style={{ minWidth: 720 }}>
                <thead>
                  <tr>
                    <th scope="col">{t('auditLog.colWhen')}</th>
                    <th scope="col">{t('auditLog.colActor')}</th>
                    <th scope="col">{t('auditLog.colAction')}</th>
                    <th scope="col">{t('auditLog.colEntity')}</th>
                  </tr>
                </thead>
                <tbody>
                  {data.items.map((entry) => (
                    <tr key={entry.id}>
                      <td data-label="" className="cell-primary is-mono">{new Date(entry.at).toLocaleString(locale)}</td>
                      <td data-label={t('auditLog.colActor')} className="is-mono" style={{ color: 'var(--c-text-muted)' }}>
                        {entry.actorType}:{entry.actorId.slice(0, 8)}
                      </td>
                      <td data-label={t('auditLog.colAction')} className="is-strong">{entry.action}</td>
                      <td data-label={t('auditLog.colEntity')} className="is-mono" style={{ color: 'var(--c-text-muted)' }}>{entry.entity}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </section>
        ) : (
          <div className="panel-empty" role="status">
            <strong>{t('auditLog.emptyTitle')}</strong>
            <span>{t('auditLog.emptyBody')}</span>
          </div>
        )
      ) : null}
    </div>
  );
}
