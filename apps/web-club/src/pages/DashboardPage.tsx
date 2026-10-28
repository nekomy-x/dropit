/**
 * Overview — the club board.
 *
 * Composition follows the panel reference layout:
 *
 *   page head   club identity, shift headline, freshness, refresh
 *   KPI row     net settled · guest requests 24 h · request price · roster
 *   row 2       floor load (2fr) · guest access (1fr)
 *   row 3       DJ roster (1.45fr) · next actions (1fr)
 *
 * The page is a composition only: every figure comes from `useClubBoard()`.
 * No number is computed here except the zone sums, which are labelled preview.
 * A failed request produces `unavailable` with no value — never a stale or
 * invented number.
 */
import { Link } from 'react-router-dom';
import Icon from '../components/Icon';
import { useLocale } from '../locale';
import SourceChip from '../components/club/SourceChip';
import StatCard from '../components/ui/StatCard';
import { useClubBoard } from '../dashboard/useClubBoard';
import {
  FloorLoadPanel,
  GuestAccessPanel,
  NextActionsPanel,
  RosterPanel,
} from '../components/club/board/OverviewPanels';
import {
  deriveNextActions,
  describeSession,
  formatCount,
  formatFreshness,
  formatMoneyMinor,
  useBoardClock,
} from '../components/club/board';
import '../components/club/board/overview.css';

export default function DashboardPage() {
  const board = useClubBoard();
  const now = useBoardClock();
  const { t, locale } = useLocale();

  const session = describeSession(board.session, board.sessionSource, t);
  const inside = board.zones.reduce((total, zone) => total + zone.inside, 0);
  const capacity = board.zones.reduce((total, zone) => total + zone.capacity, 0);
  const actions = deriveNextActions({
    session: board.session,
    sessionSource: board.sessionSource,
    djs: board.djs,
    djsSource: board.djsSource,
    requestPriceSource: board.requestPriceMinor.source,
    requestPriceMinor: board.requestPriceMinor.value,
    requests24h: board.requests24h.value,
    sessionQrSource: board.sessionQrSource,
  }, t);

  const previewOn = board.zonesSource === 'preview' || board.previewCount > 0;
  // Initial load only: a refresh over existing data keeps the numbers on screen
  // instead of collapsing the board back to skeletons.
  const loading = board.refreshing && board.lastFetchedAt === null && !board.error;

  const netSettled = board.netSettledMinor;
  const requests = board.requests24h;
  const price = board.requestPriceMinor;
  const activeDjs = board.djs.filter((dj) => dj.status === 'active').length;

  return (
    <div>
      <header className="panel-page-head">
        <div style={{ minWidth: 0 }}>
          <h1 className="panel-page-title">{session.headline}</h1>
        </div>
        <div className="panel-page-actions">
          <span className="freshness">{formatFreshness(board.lastFetchedAt, now, t)}</span>
          <button
            type="button"
            className="btn btn-secondary"
            data-control="button"
            onClick={board.reload}
            disabled={board.refreshing}
          >
            <Icon name="lucide:refresh-cw" size={15} />
            {board.refreshing ? t('common.refreshing') : t('common.refresh')}
          </button>
        </div>
      </header>

      {board.error ? (
        <div className="panel-error" role="alert" style={{ marginBottom: 16 }}>
          <div>
            <strong>{t('dashboard.boardError')}</strong>
            <span className="error-copy">
              {board.error.message} {t('dashboard.boardErrorTail')}
            </span>
            {board.error.code ? <small>{board.error.code}</small> : null}
          </div>
          <button type="button" className="btn btn-secondary btn-sm" onClick={board.reload} disabled={board.refreshing}>
            {board.refreshing ? t('dashboard.retrying') : t('common.tryAgain')}
          </button>
        </div>
      ) : null}

      {previewOn ? (
        <div className="panel-note" data-preview-banner="" role="note" style={{ marginBottom: 16 }}>
          <Icon name="lucide:triangle-alert" size={15} />
          <span>{t('dashboard.previewBanner')}</span>
        </div>
      ) : null}

      {loading ? (
        <div className="stat-grid" aria-busy="true" aria-label={t('dashboard.loadingBoard')}>
          {[0, 1, 2, 3].map((index) => (
            <div key={index} className="stat-card">
              <span className="skeleton" style={{ height: 12, width: '55%' }} />
              <span className="skeleton" style={{ height: 28, width: '70%', marginTop: 14 }} />
              <span className="skeleton" style={{ height: 10, width: '45%', marginTop: 12 }} />
            </div>
          ))}
        </div>
      ) : (
        <>
          <section className="stat-grid" aria-label={t('dashboard.kpiRow')}>
            <StatCard
              label={t('dashboard.netSettled')}
              source={netSettled.source}
              icon="lucide:wallet-cards"
              value={netSettled.source === 'unavailable' ? t('common.unavailable') : formatMoneyMinor(netSettled.value, locale) ?? '—'}
              valueMuted={netSettled.source === 'unavailable'}
              meta={<SourceChip source={netSettled.source} />}
              caption={netSettled.caption}
            />
            <StatCard
              label={t('dashboard.requests24h')}
              source={requests.source}
              icon="lucide:list-music"
              value={requests.source === 'unavailable' ? t('common.unavailable') : formatCount(requests.value, locale) ?? '—'}
              valueMuted={requests.source === 'unavailable'}
              meta={<SourceChip source={requests.source} />}
              caption={requests.caption}
            />
            <StatCard
              label={t('dashboard.requestPrice')}
              source={price.source}
              icon="lucide:badge-euro"
              value={price.source === 'unavailable' ? t('common.unavailable') : formatMoneyMinor(price.value, locale) ?? '—'}
              valueMuted={price.source === 'unavailable'}
              meta={<SourceChip source={price.source} />}
              caption={price.caption}
            />
            <StatCard
              label={t('dashboard.roster')}
              source={board.djsSource}
              icon="lucide:users"
              value={board.djsSource === 'unavailable' ? t('common.unavailable') : `${activeDjs} / ${board.djs.length}`}
              valueMuted={board.djsSource === 'unavailable'}
              meta={<SourceChip source={board.djsSource} />}
              caption={board.djsSource === 'unavailable' ? t('dashboard.rosterFailed') : t('dashboard.rosterCaptionActive')}
            />
          </section>

          <div className="panel-section panel-grid-hero">
            <FloorLoadPanel
              zones={board.zones}
              zonesSource={board.zonesSource}
              inside={inside}
              capacity={capacity}
            />
            <GuestAccessPanel
              hasSession={board.session !== null}
              sessionSource={board.sessionSource}
              qr={board.sessionQr}
              qrSource={board.sessionQrSource}
            />
          </div>

          {/* The lineup split closes the next-actions card: same /v1/club/djs
              response as the roster table beside it, read once as a share strip
              instead of opening a new row the board has no height for. */}
          <div className="panel-section panel-grid-split">
            <RosterPanel djs={board.djs} djsSource={board.djsSource} />
            <NextActionsPanel actions={actions} djs={board.djs} djsSource={board.djsSource} />
          </div>

          <p className="board-footnote">
            {t('dashboard.footnote')}{' '}
            <Link to="/sessions" className="board-link">{t('dashboard.footnoteLink')}</Link> {t('dashboard.footnoteTail')}
          </p>
        </>
      )}
    </div>
  );
}
