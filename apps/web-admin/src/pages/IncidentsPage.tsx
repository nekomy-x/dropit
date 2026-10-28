import { useCallback, useState } from 'react';
import {
  ApiRequestError,
  listIncidents,
  resolveIncident,
  type IncidentDto,
} from '../api/client';
import { useAsyncData } from '../hooks/useAsyncData';
import ErrorState from '../components/ErrorState';
import Loading from '../components/Loading';
import StatusBadge from '../components/StatusBadge';
import Icon from '../components/Icon';
import PageHeading from '../components/PageHeading';
import { useLocale } from '../locale';

/**
 * Incidents page — a LIVE admin function (F6):
 *   GET /v1/admin/incidents            → {items: [{id, kind, message}]}
 *   POST /v1/admin/incidents/:id/resolve → {resolved: true}
 * The backend lists only OPEN incidents; resolving removes a row from
 * the next fetch.
 */
export default function IncidentsPage() {
  const { t, locale } = useLocale();
  const count = (value: number) => new Intl.NumberFormat(locale).format(value);
  const fetchIncidents = useCallback(() => listIncidents(), []);
  const { data, loading, error, reload } = useAsyncData(fetchIncidents);

  const [resolvingId, setResolvingId] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);

  const onResolve = async (incident: IncidentDto) => {
    setActionError(null);
    setResolvingId(incident.id);
    try {
      await resolveIncident(incident.id);
      reload();
    } catch (err) {
      setActionError(
        err instanceof ApiRequestError ? err.message : t('incidents.resolveFailed'),
      );
    } finally {
      setResolvingId(null);
    }
  };

  return (
    <div>
      <PageHeading
        title={t('incidents.title')}
        action={(
          <button type="button" className="btn btn-secondary" onClick={reload} disabled={loading}>
            <Icon name="lucide:refresh-cw" size={15} />
            {loading ? t('common.refreshing') : t('common.refresh')}
          </button>
        )}
      />

      {actionError ? <p className="panel-error" role="alert" style={{ marginBottom: 16 }}>{actionError}</p> : null}
      {error ? <ErrorState message={error.message} code={error.code} onRetry={reload} /> : null}
      {loading && !data ? <Loading label={t('incidents.loading')} /> : null}

      {data ? (
        data.items.length === 0 ? (
          <div className="panel-empty" role="status">
            <strong>{t('incidents.emptyTitle')}</strong>
            <span>{t('incidents.emptyBody')}</span>
          </div>
        ) : (
          <section className="panel-card" aria-labelledby="open-incidents-heading">
            <header className="panel-card-head">
              <div>
                <h2 id="open-incidents-heading" className="panel-card-title">{t('incidents.openIncidents')}</h2>
                <p className="panel-card-sub">{t('incidents.openSubtitle')}</p>
              </div>
              <div className="panel-card-tools">
                <span className="chip chip-warn">{t('incidents.openCount', { count: count(data.items.length) })}</span>
              </div>
            </header>

            <ul className="list" style={{ margin: 0, padding: 0, listStyle: 'none' }}>
              {data.items.map((incident) => (
                <li
                  key={incident.id}
                  className="list-row"
                  style={{ alignItems: 'flex-start', flexWrap: 'wrap', paddingBlock: 14 }}
                >
                  <div style={{ minWidth: 0, flex: 1 }}>
                    <div style={{ display: 'flex', flexWrap: 'wrap', alignItems: 'center', gap: 8 }}>
                      <StatusBadge status="failed" />
                      <span style={{ color: 'var(--c-text-muted)', fontFamily: 'var(--font-mono)', fontSize: 'var(--fs-11)' }}>{incident.kind}</span>
                    </div>
                    <p style={{ margin: '8px 0 0', color: 'var(--c-text)', fontSize: 'var(--fs-13)', lineHeight: 1.5 }}>{incident.message}</p>
                    <p style={{ margin: '4px 0 0', color: 'var(--c-text-muted-sm)', fontFamily: 'var(--font-mono)', fontSize: 'var(--fs-11)', overflowWrap: 'anywhere' }}>
                      {incident.id}
                    </p>
                  </div>
                  <button
                    type="button"
                    className="btn btn-primary"
                    style={{ flex: '0 0 auto' }}
                    onClick={() => onResolve(incident)}
                    disabled={resolvingId === incident.id}
                  >
                    <Icon name="lucide:check" size={15} />
                    {resolvingId === incident.id ? t('incidents.resolving') : t('incidents.resolve')}
                  </button>
                </li>
              ))}
            </ul>
          </section>
        )
      ) : null}
    </div>
  );
}
