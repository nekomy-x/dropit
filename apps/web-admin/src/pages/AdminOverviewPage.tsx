/**
 * Platform overview — the operator dashboard.
 *
 * Layout follows the panel reference: page head → four KPI cards → a wide trend
 * panel beside a share panel → a live-session table beside the attention list.
 *
 * Data honesty (docs/ui/PANEL-DESIGN.md §Data honesty):
 *   - club / DJ / transaction counts and captured volume come from the local
 *     admin store (`useAdminData`) and are labelled `live store`;
 *   - the session monitor and both trend series are simulated and say so on the
 *     surface, next to the figure — not in fine print;
 *   - a card with no reading renders `unavailable`, never a zero.
 */
import { useEffect, useMemo, useState } from 'react';
import { Link, useNavigate, useSearchParams } from 'react-router-dom';
import Icon from '../components/Icon';
import StatCard from '../components/ui/StatCard';
import { DonutChart, LegendRow, TrendChart, type ChartPoint, type SeriesIndex } from '../components/ui/charts';
import QuickActionConsole from '../components/admin-console/QuickActionConsole';
import { useAdminData } from '../hooks/useAdminData';
import { analyticsMockRevenueSeries, analyticsMockSeries } from '../data/analyticsMock';
import { useLocale } from '../locale';
import { STATUS_MESSAGE_KEY } from '../messages/status';

type Period = '24h' | '7d' | '30d';
const PERIODS: readonly Period[] = ['24h', '7d', '30d'];

function isPeriod(value: string | null): value is Period {
  return value !== null && PERIODS.includes(value as Period);
}

function euro(minor: number, locale: string): string {
  return new Intl.NumberFormat(locale, { style: 'currency', currency: 'EUR', maximumFractionDigits: 0 }).format(minor / 100);
}

/** The mock revenue series is already denominated in euro, unlike the store's minor units. */
function euroFromSeries(value: number, locale: string): string {
  return new Intl.NumberFormat(locale, { style: 'currency', currency: 'EUR', maximumFractionDigits: 0 }).format(value);
}

/** The mock series carries English axis labels; they are display copy, not data. */
const AXIS_KEY: Record<string, string> = {
  Now: 'overview.axisNow',
  Mon: 'overview.axisMon',
  Tue: 'overview.axisTue',
  Wed: 'overview.axisWed',
  Thu: 'overview.axisThu',
  Fri: 'overview.axisFri',
  Sat: 'overview.axisSat',
  Sun: 'overview.axisSun',
};

export default function AdminOverviewPage() {
  const navigate = useNavigate();
  const { t, locale } = useLocale();
  const count = (value: number) => new Intl.NumberFormat(locale).format(value);
  const axisLabel = (label: string): string => {
    const key = AXIS_KEY[label];
    if (key) return t(key);
    const week = /^W(\d+)$/.exec(label);
    return week ? t('overview.axisWeek', { index: week[1] ?? '' }) : label;
  };
  const [searchParams, setSearchParams] = useSearchParams();
  const queryPeriod = searchParams.get('period');
  const period: Period = isPeriod(queryPeriod) ? queryPeriod : '24h';
  const [simulationPaused, setSimulationPaused] = useState(false);
  const [simulationTick, setSimulationTick] = useState(0);

  const clubs = useAdminData('clubs');
  const djs = useAdminData('djs');
  const transactions = useAdminData('transactions');

  useEffect(() => {
    if (simulationPaused) return undefined;
    const timer = window.setInterval(() => setSimulationTick((tick) => tick + 1), 8000);
    return () => window.clearInterval(timer);
  }, [simulationPaused]);

  const capturedMinor = transactions.data
    .filter((transaction) => transaction.status === 'captured' || transaction.status === 'partially_refunded')
    .reduce((sum, transaction) => sum + transaction.amountMinor - transaction.refundedMinor, 0);
  const pendingClubs = clubs.data.filter((club) => club.status === 'pending_verification').length;
  const invitedDjs = djs.data.filter((dj) => dj.status === 'invited').length;
  const failedPayments = transactions.data.filter((transaction) => transaction.status === 'failed').length;
  const activeClubs = clubs.data.filter((club) => club.status === 'active').length;
  const activeDjs = djs.data.filter((dj) => dj.status === 'active').length;
  const capturedCount = transactions.data.filter((transaction) => transaction.status === 'captured').length;

  const trend: ChartPoint[] = useMemo(
    () => analyticsMockSeries[period].map((point, index) => ({ label: point.label ? axisLabel(point.label) : String(index), value: point.value })),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [period, locale],
  );
  const revenue: ChartPoint[] = useMemo(
    () => analyticsMockRevenueSeries[period].map((point, index) => ({ label: point.label ? axisLabel(point.label) : String(index), value: point.value })),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [period, locale],
  );

  const revenueTotal = revenue.reduce((sum, point) => sum + point.value, 0);
  const statusSegments = useMemo(() => {
    const counts = new Map<string, number>();
    for (const club of clubs.data) counts.set(club.status, (counts.get(club.status) ?? 0) + 1);
    const order = ['active', 'pending_verification', 'suspended'];
    const palette: SeriesIndex[] = [1, 2, 3];
    return order
      .filter((status) => counts.has(status))
      .map((status, index) => ({ label: t(STATUS_MESSAGE_KEY[status] ?? 'status.unknown'), value: counts.get(status) ?? 0, series: palette[index] ?? 5 }));
  }, [clubs.data, t]);
  const clubTotal = statusSegments.reduce((sum, segment) => sum + segment.value, 0);

  const setPeriod = (next: Period) => {
    const params = new URLSearchParams(searchParams);
    params.set('period', next);
    setSearchParams(params);
  };

  const monitoredSessions = useMemo(() => {
    const liveClubs = clubs.data.slice(0, 3);
    return liveClubs.map((club, index) => ({
      id: club.id,
      venue: club.name,
      city: club.city ?? '—',
      requests: 148 + index * 19 + ((simulationTick + index) % 4),
      status: (index === 0 ? 'live' : index === 1 ? 'starting' : 'paused') as 'live' | 'starting' | 'paused',
    }));
  }, [clubs.data, simulationTick]);

  return (
    <div>
      <header className="panel-page-head">
        <div style={{ minWidth: 0 }}>
          <h1 className="panel-page-title">{t('overview.title')}</h1>
        </div>
        <div className="panel-page-actions">
          <span className="chip chip-warn" title={t('overview.simulatedMonitorHint')}>
            {t('overview.simulatedMonitor')}
          </span>
          <div className="seg" role="group" aria-label={t('overview.periodGroup')}>
            {PERIODS.map((value) => (
              <button
                type="button"
                key={value}
                className={`seg-item ${period === value ? 'is-active' : ''}`}
                aria-pressed={period === value}
                onClick={() => setPeriod(value)}
              >
                {value}
              </button>
            ))}
          </div>
          <QuickActionConsole onAction={(action) => {
            const destinations: Record<string, string> = { review: '/manage/users', suspend: '/manage/clubs', audit: '/manage/activity', role: '/manage/access' };
            if (destinations[action]) navigate(destinations[action]);
          }} />
        </div>
      </header>

      <section className="stat-grid" aria-label={t('overview.kpiGroup')}>
        <StatCard
          label={t('overview.activeClubs')}
          source={'live'}
          icon="lucide:building-2"
          value={count(activeClubs)}
          meta={<span className="chip chip-accent">{t('overview.liveStore')}</span>}
          caption={t('overview.venuesInWorkspace', { count: count(clubs.data.length) })}
        />
        <StatCard
          label={t('overview.onlineDjs')}
          source={'live'}
          icon="lucide:disc-3"
          value={count(activeDjs)}
          meta={<span className="chip chip-accent">{t('overview.liveStore')}</span>}
          caption={t('overview.invitedWaiting', { count: count(invitedDjs) })}
        />
        <StatCard
          label={t('overview.capturedVolume')}
          source={'live'}
          icon="lucide:banknote"
          value={euro(capturedMinor, locale)}
          meta={<span className="chip chip-accent">{t('overview.liveStore')}</span>}
          caption={t('overview.capturedPayments', { count: count(capturedCount) })}
        />
        <StatCard
          label={t('overview.sessionsMonitored')}
          source={'preview'}
          icon="lucide:activity"
          value={count(monitoredSessions.length)}
          meta={<span className="chip chip-warn">{t('overview.simulated')}</span>}
          caption={simulationPaused ? t('overview.monitorPaused') : t('overview.refreshEvery8')}
        />
      </section>

      <div className="panel-section panel-grid-hero">
        <section className="panel-card" data-surface="panel" aria-labelledby="admin-trend-heading">
          <header className="panel-card-head">
            <div>
              <h2 id="admin-trend-heading" className="panel-card-title">{t('overview.requestVolume')}</h2>
              <p className="panel-card-sub">{t('overview.trendSub', { count: count(trend.length) })}</p>
            </div>
            <div className="panel-card-tools">
              <span className="chip chip-warn">{t('overview.simulated')}</span>
            </div>
          </header>
          <TrendChart
            points={trend}
            ariaLabel={t('overview.trendAria', {
              period,
              count: count(trend.length),
              values: trend.map((point) => `${point.label} ${count(Math.round(point.value))}`).join(', '),
            })}
            height={232}
            valueFormat={(value) => Math.round(value).toLocaleString(locale)}
            unit={t('overview.requestsUnit')}
          />
          <p className="panel-card-note">{t('overview.trendNote')}</p>
        </section>

        <section className="panel-card" data-surface="panel" aria-labelledby="admin-status-heading">
          <header className="panel-card-head">
            <div>
              <h2 id="admin-status-heading" className="panel-card-title">{t('overview.clubStatus')}</h2>
              <p className="panel-card-sub">{t('overview.clubStatusSub', { count: count(clubTotal) })}</p>
            </div>
          </header>
          {statusSegments.length ? (
            <div className="chart-donut-block">
              <DonutChart
                segments={statusSegments}
                ariaLabel={t('overview.clubStatusAria', { values: statusSegments.map((segment) => `${segment.label} ${count(segment.value)}`).join(', ') })}
                centerValue={count(clubTotal)}
                centerLabel={t('overview.venuesUnit')}
                size={158}
              />
              <ul className="chart-legend">
                {statusSegments.map((segment) => (
                  <LegendRow key={segment.label} segment={segment} total={clubTotal} />
                ))}
              </ul>
            </div>
          ) : (
            <div className="panel-empty"><strong>{t('overview.noClubsTitle')}</strong><span>{t('overview.noClubsBody')}</span></div>
          )}
        </section>
      </div>

      <div className="panel-section panel-grid-split">
        <section className="panel-card" data-surface="panel" aria-labelledby="admin-sessions-heading">
          <header className="panel-card-head">
            <div>
              <h2 id="admin-sessions-heading" className="panel-card-title">{t('overview.roomsHeading')}</h2>
              <p className="panel-card-sub">{t('overview.roomsSub')}</p>
            </div>
            <div className="panel-card-tools">
              <button type="button" className="panel-card-link" onClick={() => setSimulationPaused((paused) => !paused)}>
                <Icon name={simulationPaused ? 'lucide:play' : 'lucide:pause'} size={13} />
                {simulationPaused ? t('overview.resume') : t('overview.pause')}
              </button>
            </div>
          </header>

          {monitoredSessions.length ? (
            <div className="table-wrap">
              <table className="data-table">
                <thead>
                  <tr>
                    <th scope="col">{t('overview.colVenue')}</th>
                    <th scope="col">{t('overview.colCity')}</th>
                    <th scope="col">{t('overview.colState')}</th>
                    <th scope="col" className="is-num">{t('overview.colRequests')}</th>
                  </tr>
                </thead>
                <tbody>
                  {monitoredSessions.map((session) => (
                    <tr key={session.id}>
                      <td className="is-strong"><Link to="/manage/clubs" className="table-link">{session.venue}</Link></td>
                      <td className="is-muted">{session.city}</td>
                      <td>
                        <span className={`chip ${session.status === 'live' ? 'chip-accent' : session.status === 'paused' ? 'chip-warn' : 'chip-neutral'}`}>
                          {t(STATUS_MESSAGE_KEY[session.status] ?? 'status.unknown')}
                        </span>
                      </td>
                      <td className="is-num">{count(session.requests)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          ) : (
            <div className="panel-empty"><strong>{t('overview.noVenuesTitle')}</strong><span>{t('overview.noVenuesBody')}</span></div>
          )}

          <p className="panel-card-note">{t('overview.roomsNote')}</p>
        </section>

        <section className="panel-card" data-surface="panel" aria-labelledby="admin-attention-heading">
          <header className="panel-card-head">
            <div>
              <h2 id="admin-attention-heading" className="panel-card-title">{t('overview.attention')}</h2>
              <p className="panel-card-sub">{t('overview.attentionSub')}</p>
            </div>
          </header>
          <ul className="list">
            <li>
              <Link to="/manage/clubs?status=pending_verification" className="list-row attention-row">
                <span className="list-main"><span className="list-title">{t('overview.clubsPending')}</span><span className="list-sub">{t('overview.clubsPendingSub')}</span></span>
                <span className="attention-count is-warn">{count(pendingClubs)}</span>
              </Link>
            </li>
            <li>
              <Link to="/manage/djs?status=invited" className="list-row attention-row">
                <span className="list-main"><span className="list-title">{t('overview.djsAwaiting')}</span><span className="list-sub">{t('overview.djsAwaitingSub')}</span></span>
                <span className="attention-count is-warn">{count(invitedDjs)}</span>
              </Link>
            </li>
            <li>
              <Link to="/manage/transactions?status=failed" className="list-row attention-row">
                <span className="list-main"><span className="list-title">{t('overview.failedPayments')}</span><span className="list-sub">{t('overview.failedPaymentsSub')}</span></span>
                <span className={`attention-count ${failedPayments > 0 ? 'is-error' : ''}`.trim()}>{count(failedPayments)}</span>
              </Link>
            </li>
          </ul>

          <div className="attention-foot">
            <div className="bar-row-head">
              <span className="bar-row-label">{t('overview.simulatedRevenue')}</span>
              <span className="bar-row-value">{euroFromSeries(revenueTotal, locale)}</span>
            </div>
            <div className="bar-track"><div className="bar-fill is-preview" style={{ width: '62%' }} /></div>
            <p className="panel-card-note">{t('overview.simulatedRevenueNote')}</p>
          </div>
        </section>
      </div>

      <p className="board-footnote">{t('overview.footnote')}</p>
    </div>
  );
}
