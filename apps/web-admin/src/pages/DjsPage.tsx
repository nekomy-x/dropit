import { useCallback } from 'react';
import { listDjs, type AdminDj } from '../api/client';
import { useAsyncData } from '../hooks/useAsyncData';
import ErrorState from '../components/ErrorState';
import Loading from '../components/Loading';
import StatusBadge from '../components/StatusBadge';
import PageHeading from '../components/PageHeading';
import Icon from '../components/Icon';
import { useLocale } from '../locale';

/**
 * DJs from the live admin panel endpoint.
 */
export default function DjsPage() {
  const { t, locale } = useLocale();
  const count = (value: number) => new Intl.NumberFormat(locale).format(value);
  const fetchDjs = useCallback(() => listDjs(), []);
  const { data, loading, error, reload } = useAsyncData<{ items: AdminDj[] }>(fetchDjs);

  return (
    <div>
      <PageHeading
        title={t('djs.title')}
        action={(
          <button type="button" className="btn btn-secondary" onClick={reload} disabled={loading}>
            <Icon name="lucide:refresh-cw" size={15} />
            {loading ? t('common.refreshing') : t('common.refresh')}
          </button>
        )}
      />

      {error?.notImplemented ? (
        <div className="panel-empty" role="status">
          <strong>{t('djs.pendingTitle')}</strong>
          <span>{t('djs.pendingBody')}</span>
        </div>
      ) : null}
      {error && !error.notImplemented ? <ErrorState message={error.message} code={error.code} onRetry={reload} /> : null}
      {loading && !data ? <Loading label={t('djs.loading')} /> : null}

      {data && data.items.length === 0 && !error ? (
        <div className="panel-empty" role="status">
          <strong>{t('djs.emptyTitle')}</strong>
          <span>{t('djs.emptyBody')}</span>
        </div>
      ) : null}

      {data && data.items.length > 0 ? (
        <section className="panel-card" aria-label={t('djs.cardLabel')}>
          <header className="panel-card-head">
            <div>
              <h2 className="panel-card-title">{t('djs.allDjs')}</h2>
              <p className="panel-card-sub">{t('djs.cardSubtitle')}</p>
            </div>
            <div className="panel-card-tools">
              <span className="chip chip-neutral">{t('djs.count', { count: count(data.items.length) })}</span>
            </div>
          </header>

          <div className="table-wrap">
            <table className="data-table" style={{ minWidth: 720 }}>
              <thead>
                <tr>
                  <th scope="col">{t('djs.colDj')}</th>
                  <th scope="col">{t('djs.colClub')}</th>
                  <th scope="col">{t('djs.colStripe')}</th>
                  <th scope="col">{t('djs.colStatus')}</th>
                </tr>
              </thead>
              <tbody>
                {data.items.map((dj) => (
                  <tr key={dj.id}>
                    <td className="is-strong">
                      <span style={{ display: 'block' }}>{dj.displayName}</span>
                      <span style={{ color: 'var(--c-text-muted-sm)', fontSize: 'var(--fs-11)' }}>{dj.email}</span>
                    </td>
                    <td className="is-mono" style={{ color: 'var(--c-text-muted)' }}>
                      {dj.clubId ?? t('djs.unassigned')}
                    </td>
                    <td><StatusBadge status={dj.stripeOnboardingStatus} /></td>
                    <td><StatusBadge status={dj.status} /></td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </section>
      ) : null}
    </div>
  );
}
