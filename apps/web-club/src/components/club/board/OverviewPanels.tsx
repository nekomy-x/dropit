/**
 * Overview panels — the club board in the panel design language.
 *
 * Data honesty is unchanged from the previous board (docs/ui/club-redesign):
 * every figure comes from `useClubBoard()` and carries a source chip, the zone
 * model is the only preview block, and a failed request produces `unavailable`
 * with no value instead of a stale number.
 *
 * Composition is the reference layout: KPI row → wide floor panel + narrow
 * access panel → roster table + next actions.
 */
import { Link } from 'react-router-dom';
import { useLocale } from '../../../locale';
import { QRCodeSVG } from 'qrcode.react';
import { guestJoinUrl } from '../../../guest/links';
import Icon from '../../Icon';
import { CapacityBars, seriesColor, type DonutSegment } from '../../ui/charts';
import SourceChip, { type MetricSource } from '../SourceChip';
import { formatClock, formatCount, formatMoneyMinor, formatPercent, shortId } from './format';
import type { NextAction } from './nextActions';
import type { BoardDj, BoardZone } from '../../../dashboard/useClubBoard';

/** The zone preview model is defined next to the data hook. */
type FloorZone = BoardZone;

export function FloorLoadPanel({
  zones,
  zonesSource,
  inside,
  capacity,
}: {
  zones: ReadonlyArray<FloorZone>;
  zonesSource: MetricSource;
  inside: number;
  capacity: number;
}) {
  const { t, locale } = useLocale();
  const ratio = capacity > 0 ? Math.round((inside / capacity) * 100) : 0;

  return (
    <section className="panel-card" data-surface="panel" aria-labelledby="floor-load-heading">
      <header className="panel-card-head">
        <div>
          <h2 id="floor-load-heading" className="panel-card-title">{t('floor.title')}</h2>
          <p className="panel-card-sub">{t('floor.sub')}</p>
        </div>
        <div className="panel-card-tools"><SourceChip source={zonesSource} /></div>
      </header>

      <div className="floor-total">
        <span className="floor-total-value">{formatCount(inside, locale)}</span>
        <span className="floor-total-unit">{t('floor.guestsInside', { count: formatCount(capacity, locale) ?? '' })}</span>
        <span className="floor-total-share">{t('dashboard.percentOfCapacity', { percent: ratio })}</span>
      </div>

      {zones.length === 0 ? (
        <div className="panel-empty"><strong>{t('floor.noneTitle')}</strong><span>{t('floor.noneBody')}</span></div>
      ) : (
        <CapacityBars
          ariaLabel={t('floor.aria', {
            zones: zones.map((zone) => `${zone.name} ${formatCount(zone.inside, locale) ?? '0'} / ${formatCount(zone.capacity, locale) ?? '0'}`).join(', '),
          })}
          items={zones.map((zone) => ({
            label: zone.name,
            value: zone.inside,
            max: zone.capacity,
            valueLabel: `${formatCount(zone.inside, locale)} / ${formatCount(zone.capacity, locale)} · ${formatPercent(zone.capacity > 0 ? zone.inside / zone.capacity : 0)}`,
            note: zone.qrActive ? t('floor.qrOn') : t('floor.qrOff'),
          }))}
        />
      )}

      <p className="panel-card-note">{t('floor.note')}</p>
    </section>
  );
}

export function GuestAccessPanel({
  hasSession,
  sessionSource,
  qr,
  qrSource,
}: {
  hasSession: boolean;
  sessionSource: MetricSource;
  qr: { token: string; expiresAt: string } | null;
  qrSource: MetricSource;
}) {
  const { t, locale } = useLocale();
  const expires = qr ? formatClock(qr.expiresAt, locale) : null;
  const shiftRead = sessionSource !== 'unavailable';

  return (
    <section className="panel-card" data-surface="panel" aria-labelledby="guest-access-heading">
      <header className="panel-card-head">
        <div>
          <h2 id="guest-access-heading" className="panel-card-title">{t('access.title')}</h2>
          <p className="panel-card-sub">{t('access.sub')}</p>
        </div>
        {qr ? <div className="panel-card-tools"><SourceChip source={qrSource} /></div> : null}
      </header>

      {qr ? (
        <div className="access-qr">
          <div className="access-qr-frame">
            <QRCodeSVG
              value={guestJoinUrl(qr.token)}
              size={116}
              marginSize={2}
              bgColor="#FFFFFF"
              fgColor="#0A0A0A"
              level="M"
              title={t('access.qrTitle')}
            />
          </div>
          <dl className="access-meta">
            <div><dt>{t('access.token')}</dt><dd className="is-mono">{shortId(qr.token)}</dd></div>
            {expires ? <div><dt>{t('access.expires')}</dt><dd>{expires}</dd></div> : null}
            <div><dt>{t('access.guests')}</dt><dd>{t('access.guestsValue')}</dd></div>
          </dl>
        </div>
      ) : (
        <div className="panel-empty">
          <strong>{hasSession ? t('access.noCodeTitle') : shiftRead ? t('access.noShiftTitle') : t('access.notReadTitle')}</strong>
          <span>
            {hasSession
              ? t('access.noCodeBody')
              : shiftRead
                ? t('access.noShiftBody')
                : t('access.notReadBody')}
          </span>
          <Link to="/sessions" className="btn btn-secondary btn-sm" style={{ marginTop: 10 }}>
            {t('board.actionOpenLiveRoom')} <Icon name="lucide:arrow-right" size={14} />
          </Link>
        </div>
      )}
    </section>
  );
}

export function RosterPanel({ djs, djsSource }: { djs: ReadonlyArray<BoardDj>; djsSource: MetricSource }) {
  const { t } = useLocale();
  return (
    <section className="panel-card" data-surface="panel" aria-labelledby="roster-heading">
      <header className="panel-card-head">
        <div>
          <h2 id="roster-heading" className="panel-card-title">{t('roster.title')}</h2>
          <p className="panel-card-sub">{t('roster.sub')}</p>
        </div>
        <div className="panel-card-tools"><SourceChip source={djsSource} /></div>
      </header>

      {djs.length === 0 ? (
        <div className="panel-empty">
          <strong>{djsSource === 'unavailable' ? t('roster.failedTitle') : t('roster.emptyTitle')}</strong>
          <span>{djsSource === 'unavailable' ? t('roster.failedBody') : t('roster.emptyBody')}</span>
        </div>
      ) : (
        <div className="table-wrap">
          <table className="data-table data-table--stack">
            <thead>
              <tr>
                <th scope="col">{t('roster.colDj')}</th>
                <th scope="col">{t('roster.colEmail')}</th>
                <th scope="col">{t('roster.colStatus')}</th>
                <th scope="col">{t('roster.colShift')}</th>
              </tr>
            </thead>
            <tbody>
              {djs.map((dj) => (
                <tr key={dj.id}>
                  <td className="is-strong cell-primary">
                    <span className="dj-cell">
                      <span className="list-thumb" aria-hidden="true">{dj.displayName.slice(0, 1).toUpperCase()}</span>
                      {dj.displayName}
                    </span>
                  </td>
                  <td className="is-muted" data-label={t('roster.colEmail')}>{dj.email}</td>
                  <td data-label={t('roster.colStatus')}>
                    <span className={`chip ${dj.status === 'active' ? 'chip-accent' : dj.status === 'invited' ? 'chip-warn' : 'chip-error'}`}>
                      {dj.status}
                    </span>
                  </td>
                  <td data-label={t('roster.colShift')}>{dj.isOnShift ? t('roster.onShift') : t('roster.offShift')}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </section>
  );
}

export function NextActionsPanel({
  actions,
  djs,
  djsSource,
}: {
  actions: ReadonlyArray<NextAction>;
  djs: ReadonlyArray<BoardDj>;
  djsSource: MetricSource;
}) {
  const { t } = useLocale();
  return (
    <section className="panel-card" data-surface="panel" aria-labelledby="next-actions-heading">
      <header className="panel-card-head">
        <div>
          <h2 id="next-actions-heading" className="panel-card-title">{t('next.title')}</h2>
          <p className="panel-card-sub">{t('next.sub')}</p>
        </div>
      </header>

      {actions.length === 0 ? (
        <div className="panel-empty"><strong>{t('next.emptyTitle')}</strong><span>{t('next.emptyBody')}</span></div>
      ) : (
        <ul className="list">
          {actions.map((action) => (
            <li key={action.id}>
              <Link to={action.to} className="list-row action-row">
                <span className="list-thumb action-thumb"><Icon name="lucide:arrow-right" size={15} /></span>
                <span className="list-main">
                  <span className="list-title">{action.title}</span>
                  <span className="list-sub">{action.detail}</span>
                </span>
                <span className="action-meta">{action.meta}</span>
              </Link>
            </li>
          ))}
        </ul>
      )}

      {/* The lineup split closes the card: it is the context the actions above
          are derived from, and it costs a strip rather than a new panel. */}
      <div className="roster-mix-block">
        <p className="roster-mix-title">{t('mix.title')}</p>
        <RosterMix djs={djs} djsSource={djsSource} />
      </div>
    </section>
  );
}

export function MoneyStat({ minor, source }: { minor: number | null; source: MetricSource }) {
  const { t, locale } = useLocale();
  return <>{source === 'unavailable' || minor === null ? t('common.unavailable') : formatMoneyMinor(minor, locale)}</>;
}

/**
 * Roster mix — the club's DJ lineup as a compact share strip.
 *
 * Every count comes from `GET /v1/club/djs`, the same response the roster table
 * renders: no series is generated and no slice is invented. A failed read shows
 * the honest empty state instead of a strip built from zeroes, which would draw
 * "0 active" as a fact the API never stated.
 *
 * A strip rather than a ring: the board already sits at its height budget, and
 * a 210px donut under a 275px card would push the page past it. One row of
 * proportional segments plus the counts reads the same split in ~40px, and the
 * exact numbers stay in the text so the colour is never the only carrier.
 */
export function RosterMix({ djs, djsSource }: { djs: ReadonlyArray<BoardDj>; djsSource: MetricSource }) {
  const { t } = useLocale();
  const segments: DonutSegment[] = ([
    { label: t('mix.active'), value: djs.filter((dj) => dj.status === 'active').length, series: 1 as const },
    { label: t('mix.invited'), value: djs.filter((dj) => dj.status === 'invited').length, series: 2 as const },
    { label: t('mix.suspended'), value: djs.filter((dj) => dj.status === 'suspended').length, series: 3 as const },
    { label: t('mix.blocked'), value: djs.filter((dj) => dj.status === 'blocked').length, series: 4 as const },
  ]).filter((segment) => segment.value > 0);
  const total = djs.length;

  if (djsSource === 'unavailable' || total === 0) {
    return (
      <p className="roster-mix-empty">
        {djsSource === 'unavailable' ? t('mix.failedBody') : t('mix.emptyBody')}
      </p>
    );
  }

  return (
    <div className="roster-mix">
      <div
        className="roster-mix-strip"
        role="img"
        aria-label={t('mix.aria', {
          values: segments.map((segment) => `${segment.label} ${segment.value}`).join(', '),
        })}
      >
        {segments.map((segment) => (
          <span
            key={segment.label}
            className="roster-mix-seg"
            style={{ flexGrow: segment.value, background: seriesColor(segment.series) }}
          />
        ))}
      </div>
      <ul className="roster-mix-legend">
        {segments.map((segment) => (
          <li key={segment.label}>
            <span className="roster-mix-dot" style={{ background: seriesColor(segment.series) }} aria-hidden="true" />
            {segment.label}
            <span className="roster-mix-count">{segment.value}</span>
          </li>
        ))}
      </ul>
    </div>
  );
}
