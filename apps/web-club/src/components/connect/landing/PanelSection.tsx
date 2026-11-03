/**
 * The owner panel, shown on the landing as the product it actually is.
 *
 * The preview is built from the panel's own classes — `panel-shell`, `panel-rail`,
 * `stat-card`, `panel-card`, `bar-*`, `access-*`, `chip` — so what a visitor
 * sees is the first screen of the Overview board: the page head with the shift
 * state, the four KPIs with their source chips, floor load and guest access. It
 * is not a second, hand-built approximation of the panel that drifts away from
 * the real one, and it is exactly what a club owner sees on that page before
 * scrolling.
 *
 * The public page is dark in both themes, so `panel-mock.css` paints the panel
 * token names with the dark values inside this section only: a visitor who
 * switched the panel to light gets a black landing page, not white cards in it.
 *
 * Motion: the window lifts into place once its upper portion is visible, then
 * the navigation, KPIs and report sections settle in a short sequence. The text
 * stays sharp throughout; reduced motion shows the finished preview immediately.
 *
 * Copy comes from the panel catalogs through the app-level provider, so the
 * preview shares its sentences with the real panel instead of carrying a second
 * set. The figures are sample content, not a live board: every KPI carries the
 * panel's own `preview` word, and the note under the window spells out the
 * sample night.
 */
import { QRCodeSVG } from 'qrcode.react';
import Icon, { type IconName } from '../Icons';
import SectionHead from './SectionHead';
import { useLocale } from '../../../i18n';
import { usePanelEntrance } from '../../../connect/usePanelEntrance';
import {
  DEFAULT_PRICE_MINOR,
  PANEL_QUEUE_NOW,
  PANEL_REQUESTS_TODAY,
  PANEL_SCANS_HOUR,
  SERVICE_FEE_MINOR,
  clubShareMinor,
} from './scenario';
// The board's own stylesheet: the same classes the Overview page draws, not a
// second copy of them. It reads `--c-*`, which this section supplies.
import '../../club/board/overview.css';
// The source chip is the panel's own honesty marker; the preview wears the same
// one so the word "preview" on this page means what it means inside the panel.
import '../../club/club-primitives.css';
import './panel-mock.css';

type NavGroup = 'nav.workspace' | 'nav.manage' | 'nav.tools';

const NAV_GROUPS: ReadonlyArray<NavGroup> = ['nav.workspace', 'nav.manage', 'nav.tools'];

const NAV: ReadonlyArray<{
  group: NavGroup;
  icon: IconName;
  key: 'nav.overview' | 'nav.liveRoom' | 'nav.djs' | 'nav.pricing' | 'nav.earnings' | 'nav.djConsole';
  active?: boolean;
}> = [
  // The preview shows the board, so Overview is the active rail item — the
  // state a venue owner is in while looking at this exact screen.
  { group: 'nav.workspace', icon: 'chart', key: 'nav.overview', active: true },
  { group: 'nav.workspace', icon: 'live', key: 'nav.liveRoom' },
  { group: 'nav.manage', icon: 'users', key: 'nav.djs' },
  { group: 'nav.manage', icon: 'wallet', key: 'nav.pricing' },
  { group: 'nav.manage', icon: 'coins', key: 'nav.earnings' },
  { group: 'nav.tools', icon: 'music2', key: 'nav.djConsole' },
];

/** The floor model the board reads in place of a zone endpoint (preview only). */
const ZONES = [
  { key: 'panelMainHall' as const, inside: 316, capacity: 420 },
  { key: 'panelBar' as const, inside: 74, capacity: 160 },
  { key: 'panelTerrace' as const, inside: 61, capacity: 90 },
];

export default function PanelSection() {
  const { t, locale } = useLocale();
  const entrance = usePanelEntrance<HTMLDivElement>();

  const money = (minor: number) =>
    new Intl.NumberFormat(locale, {
      style: 'currency',
      currency: 'EUR',
      minimumFractionDigits: 2,
      maximumFractionDigits: 2,
    }).format(minor / 100);
  const count = (value: number) => new Intl.NumberFormat(locale).format(value);

  const clubShare = clubShareMinor(DEFAULT_PRICE_MINOR);
  const shareLabel = money(clubShare);
  const feeLabel = money(SERVICE_FEE_MINOR);
  const requestsLabel = count(PANEL_REQUESTS_TODAY);

  const inside = ZONES.reduce((total, zone) => total + zone.inside, 0);
  const capacity = ZONES.reduce((total, zone) => total + zone.capacity, 0);
  const share = capacity > 0 ? Math.round((inside / capacity) * 100) : 0;

  /**
   * KPI cards in the panel's own anatomy — label, icon tile, value, source
   * chip, caption — carrying the figures this sample night can actually back.
   * The panel's own board tile "Net settled" is a lifetime total; a preview
   * cannot know a lifetime, so the revenue tile states what it is: requests
   * today × the club share, the same arithmetic the calculator and the stats
   * card on this page use.
   */
  const KPI: ReadonlyArray<{ label: string; value: string; caption: string; icon: IconName }> = [
    {
      label: t('panelKpiRevenue'),
      value: money(PANEL_REQUESTS_TODAY * clubShare),
      caption: t('panelKpiRevenueNote', { requests: requestsLabel, share: shareLabel }),
      icon: 'wallet',
    },
    {
      label: t('panelKpiRequests'),
      value: requestsLabel,
      caption: t('dashboard.requests24hCaption'),
      icon: 'listMusic',
    },
    {
      label: t('panelKpiQueue'),
      value: count(PANEL_QUEUE_NOW),
      caption: t('panelKpiQueueCaption'),
      icon: 'live',
    },
    {
      label: t('panelKpiScans'),
      value: count(PANEL_SCANS_HOUR),
      caption: t('panelKpiScansCaption'),
      icon: 'qr',
    },
  ];

  return (
    <section id="panel" className="connect-panel-section">
      <div className="connect-panel-section__inner">
        <SectionHead eyebrow={t('panelEyebrow')} headline={t('panelTitle')} note={t('panelNote')} />

        <div ref={entrance} className="connect-panel-stage">
          <div className="connect-panel-window">
            {/* The preview is dressed like a screen — window chrome, URL — but
                everything inside is the panel's own shell. */}
            <div className="connect-panel-chrome">
              <span className="connect-panel-dots" aria-hidden="true">
                <span className="connect-panel-dot" />
                <span className="connect-panel-dot" />
                <span className="connect-panel-dot" />
              </span>
              {/* A phone cannot hold the whole address at a legible size, so
                  the bar keeps the two parts that identify the screen — the
                  product and the club — and drops the date there instead of
                  cutting the line with an ellipsis mid-word. */}
              <p className="connect-panel-url">
                <span className="connect-panel-url-wide">drop.it/panel — KRAFT Room · {t('panelWindowTitle')}</span>
                <span className="connect-panel-url-short">drop.it/panel — KRAFT Room</span>
              </p>
            </div>

            <div className="panel-shell">
                <aside className="panel-rail">
                  <div className="panel-rail-brand">
                    <span className="connect-panel-brand" aria-hidden="true">DROP IT</span>
                  </div>

                  {/* No workspace badge: the preview mirrors the real panel,
                      which names the club in the page identity instead. */}
                  <nav className="panel-rail-nav" aria-hidden="true">
                    {NAV_GROUPS.map((group) => (
                      <div key={group} className="panel-nav-group">
                        <p className="panel-nav-label">{t(group)}</p>
                        <ul className="panel-nav-list">
                          {NAV.filter((item) => item.group === group).map((item) => (
                            <li key={item.key}>
                              <span className={`panel-nav-link ${item.active ? 'is-active' : ''}`.trim()}>
                                <span className="panel-nav-icon"><Icon name={item.icon} size={15} /></span>
                                {t(item.key)}
                              </span>
                            </li>
                          ))}
                        </ul>
                      </div>
                    ))}
                  </nav>

                  <div className="panel-rail-foot">
                    <span className="panel-account">
                      <span className="panel-workspace-avatar" aria-hidden="true">KO</span>
                      <span className="panel-workspace-copy">
                        <span className="panel-account-name">{t('panelAccountOwner')}</span>
                        <span className="panel-account-meta">KRAFT Room</span>
                      </span>
                    </span>
                  </div>
                </aside>

                <div className="panel-main">
                  <header className="panel-topbar">
                    <span className="panel-search">
                      <Icon name="search" size={14} />
                      <span>{t('shell.jumpTo')}</span>
                      <kbd aria-hidden="true">⌘K</kbd>
                    </span>
                    <span className="panel-icon-button panel-search-compact" aria-hidden="true">
                      <Icon name="search" size={15} />
                    </span>
                    <div className="panel-topbar-actions">
                      <span className="panel-icon-button" aria-hidden="true">
                        <Icon name="moon" size={14} />
                      </span>
                      {/* The panel moves the account here below the rail
                          breakpoint, where the rail foot is gone; above it
                          this element is hidden by the panel's own rule. */}
                      <span className="panel-topbar-account">
                        <span className="panel-workspace-avatar" aria-hidden="true">KO</span>
                      </span>
                    </div>
                  </header>

                  <div className="panel-content">
                    <header className="panel-page-head">
                      <div>
                        <p className="page-identity">KRAFT Room</p>
                        <h3 className="panel-page-title">{t('board.headlineActive')}</h3>
                        <p className="panel-page-sub">{t('board.detailActive')}</p>
                      </div>
                      <div className="panel-page-actions">
                        <span className="freshness">
                          <span className="connect-panel-live-dot" aria-hidden="true" />
                          {t('common.updated')}
                        </span>
                      </div>
                    </header>

                    <div className="stat-grid" role="group" aria-label={t('dashboard.kpiRow')}>
                      {KPI.map((entry) => (
                        <article key={entry.label} className="stat-card">
                          <div className="stat-head">
                            <p className="stat-label">{entry.label}</p>
                            <span className="stat-icon" aria-hidden="true">
                              <Icon name={entry.icon} size={16} />
                            </span>
                          </div>
                          <p className="stat-value">{entry.value}</p>
                          <div className="stat-row">
                            {/* The panel's own source chip, carrying the word
                                that is true here: a preview of the product,
                                not a reading from a club API. */}
                            <span className="club-source-chip club-source-preview">
                              {t('source.preview')}
                            </span>
                          </div>
                          <p className="stat-foot">{entry.caption}</p>
                        </article>
                      ))}
                    </div>

                    <div className="panel-section panel-grid-hero">
                      <div className="panel-card">
                        <header className="panel-card-head">
                          <div>
                            <h4 className="panel-card-title">{t('floor.title')}</h4>
                            <p className="panel-card-sub">{t('floor.sub')}</p>
                          </div>
                          <div className="panel-card-tools">
                            <span className="club-source-chip club-source-preview">
                              {t('source.preview')}
                            </span>
                          </div>
                        </header>

                        <div className="floor-total">
                          <span className="floor-total-value">{count(inside)}</span>
                          <span className="floor-total-unit">
                            {t('floor.guestsInside', { count: count(capacity) })}
                          </span>
                          <span className="floor-total-share">
                            {t('dashboard.percentOfCapacity', { percent: share })}
                          </span>
                        </div>

                        <div
                          className="bar-list"
                          role="img"
                          aria-label={t('floor.aria', {
                            zones: ZONES.map(
                              (zone) => `${t(zone.key)} ${count(zone.inside)} / ${count(zone.capacity)}`,
                            ).join(', '),
                          })}
                        >
                          {ZONES.map((zone) => {
                            const percent = Math.round((zone.inside / zone.capacity) * 100);
                            return (
                              <div key={zone.key} className="bar-row">
                                <div className="bar-row-head">
                                  <span className="bar-row-label">{t(zone.key)}</span>
                                  <span className="bar-row-value">
                                    {`${count(zone.inside)} / ${count(zone.capacity)} · ${percent}%`}
                                  </span>
                                </div>
                                <div className="bar-track">
                                  <div className="bar-fill" style={{ width: `${percent}%` }} />
                                </div>
                                <p className="bar-row-note">{t('floor.qrOn')}</p>
                              </div>
                            );
                          })}
                        </div>
                      </div>

                      <div className="panel-card">
                        <header className="panel-card-head">
                          <div>
                            <h4 className="panel-card-title">{t('access.title')}</h4>
                            <p className="panel-card-sub">{t('access.sub')}</p>
                          </div>
                        </header>

                        <div className="access-qr">
                          <div className="access-qr-frame">
                            <QRCodeSVG
                              value="drop.it/join/kraft-room"
                              size={92}
                              marginSize={2}
                              bgColor="#FFFFFF"
                              fgColor="#0A0A0A"
                              level="M"
                            />
                          </div>
                          <dl className="access-meta">
                            <div>
                              <dt>{t('access.token')}</dt>
                              <dd className="is-mono">7f3c·91ad</dd>
                            </div>
                            <div>
                              <dt>{t('access.expires')}</dt>
                              <dd>06:00</dd>
                            </div>
                            <div>
                              <dt>{t('access.guests')}</dt>
                              <dd>{t('access.guestsValue')}</dd>
                            </div>
                          </dl>
                        </div>
                      </div>
                    </div>

                  </div>
                </div>
              </div>
          </div>
        </div>

        <p className="connect-panel-sample">
          {t('panelSampleNote', {
            requests: requestsLabel,
            share: shareLabel,
            fee: feeLabel,
          })}
        </p>
      </div>
    </section>
  );
}
