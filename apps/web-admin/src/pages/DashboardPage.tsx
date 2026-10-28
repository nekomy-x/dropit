import { useCallback } from 'react';
import { Link } from 'react-router-dom';
import { getDashboardNow, type DashboardNow } from '../api/client';
import { useAsyncData } from '../hooks/useAsyncData';
import Loading from '../components/Loading';
import ErrorState from '../components/ErrorState';
import PageHeading from '../components/PageHeading';
import Icon from '../components/Icon';
import { useLocale } from '../locale';

/**
 * Live platform metrics (`GET /v1/admin/dashboard/now`).
 *
 * Composition follows the reference layout: page head, a KPI row of four
 * `stat-card`s, then one panel of operator links. Every figure is the API
 * response and nothing is derived — the card captions name the window the
 * endpoint reports. If the endpoint is not implemented the page says so and
 * shows no numbers at all.
 */
export default function DashboardPage() {
  const { t, locale } = useLocale();
  const money = (minor: number, currency: string): string =>
    new Intl.NumberFormat(locale, { style: 'currency', currency, maximumFractionDigits: 2 }).format(minor / 100);
  const count = (value: number) => new Intl.NumberFormat(locale).format(value);
  const fetchNow = useCallback(() => getDashboardNow(), []);
  const { data, loading, error, reload } = useAsyncData<DashboardNow>(fetchNow);

  const shortcuts: ReadonlyArray<{ to: string; label: string; hint: string }> = [
    { to: '/clubs', label: t('dashboard.reviewClubs'), hint: t('dashboard.reviewClubsHint') },
    { to: '/payouts', label: t('dashboard.runPayouts'), hint: t('dashboard.runPayoutsHint') },
    {
      to: '/incidents',
      label: t('dashboard.checkIncidents'),
      hint: data ? t('dashboard.openIncidents', { count: count(data.openIncidents) }) : t('dashboard.incidentsUnavailable'),
    },
  ];

  const tiles = data
    ? [
        { label: t('dashboard.activeSessions'), value: count(data.activeSessions), caption: t('dashboard.sessionsNotClosed'), icon: 'lucide:radio-tower' as const },
        { label: t('dashboard.requestsToday'), value: count(data.requestsToday), caption: t('dashboard.last24Hours'), icon: 'lucide:music-2' as const },
        { label: t('dashboard.requestsPerMin'), value: count(data.requestsPerMinute), caption: t('dashboard.last60Seconds'), icon: 'lucide:activity' as const },
        { label: t('dashboard.turnoverToday'), value: money(data.turnoverTodayMinor, data.currency), caption: t('dashboard.capturedVolume'), icon: 'lucide:banknote' as const },
      ]
    : [];

  return (
    <div>
      <PageHeading
        title={t('dashboard.title')}
        action={(
          <button type="button" className="btn btn-secondary" onClick={reload} disabled={loading}>
            <Icon name="lucide:refresh-cw" size={15} />
            {loading ? t('common.refreshing') : t('common.refresh')}
          </button>
        )}
      />

      {loading && !data ? <Loading label={t('dashboard.loading')} /> : null}

      {error?.notImplemented ? (
        <section className="panel-card" role="status">
          <header className="panel-card-head">
            <div>
              <h2 className="panel-card-title">{t('dashboard.pendingTitle')}</h2>
              <p className="panel-card-sub">{t('dashboard.pendingBody')}</p>
            </div>
          </header>
          <p style={{ margin: 0, color: 'var(--c-text-2)', fontSize: 'var(--fs-13)' }}>
            {t('dashboard.liveActionsLead')}{' '}
            <Link to="/payouts" style={{ color: 'var(--c-accent-quiet)', fontWeight: 500 }}>{t('payouts.title')}</Link> {t('dashboard.and')}{' '}
            <Link to="/incidents" style={{ color: 'var(--c-accent-quiet)', fontWeight: 500 }}>{t('incidents.title')}</Link>.
          </p>
        </section>
      ) : null}

      {error && !error.notImplemented ? <ErrorState message={error.message} code={error.code} onRetry={reload} /> : null}

      {data ? (
        <>
          <section className="stat-grid" aria-label={t('dashboard.metricsLabel')}>
            {tiles.map((tile) => (
              <article key={tile.label} className="stat-card">
                <div className="stat-head">
                  <p className="stat-label">{tile.label}</p>
                  <span className="stat-icon"><Icon name={tile.icon} size={17} /></span>
                </div>
                <p className="stat-value" style={{ fontSize: 'var(--fs-20)' }}>{tile.value}</p>
                <p className="stat-foot" style={{ marginTop: 6 }}>{tile.caption}</p>
              </article>
            ))}
          </section>

          <section className="panel-card panel-section" aria-labelledby="operator-shortcuts-heading">
            <header className="panel-card-head">
              <div>
                <h2 id="operator-shortcuts-heading" className="panel-card-title">{t('dashboard.shortcuts')}</h2>
                <p className="panel-card-sub">{t('dashboard.shortcutsSubtitle')}</p>
              </div>
            </header>
            <ul className="list" style={{ margin: 0, padding: 0, listStyle: 'none' }}>
              {shortcuts.map((shortcut) => (
                <li key={shortcut.to}>
                  <Link to={shortcut.to} className="list-row">
                    <span className="list-main">
                      <span className="list-title">{shortcut.label}</span>
                      <span className="list-sub">{shortcut.hint}</span>
                    </span>
                    <Icon name="lucide:arrow-up-right" size={15} />
                  </Link>
                </li>
              ))}
            </ul>
          </section>

          <p className="admin-page-note" style={{ marginTop: 18 }}>
            {t('dashboard.noteLead')} <span className="admin-code">GET /v1/admin/dashboard/now</span> {t('dashboard.noteTail')}
          </p>
        </>
      ) : null}
    </div>
  );
}
