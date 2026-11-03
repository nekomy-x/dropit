/**
 * DJ console views — room control and session details.
 *
 * The request workspace lives in `components/dj-console/RequestsWorkspace.tsx`:
 * it is the screen the DJ looks at all night, and it owns its own live strip,
 * catalog preview and keyboard flow instead of borrowing the monitoring kit.
 *
 * Every surface here is built from the shared kit in `styles/panel.css`
 * (`panel-card`, `stat-card`, `data-table`, `list-row`, `chip`, `btn-*`,
 * `field-*`, `seg`) and reads colour from the token layer, so the console is
 * correct in both themes. Page-specific rhythm lives in `pages/dj-console.css`.
 *
 * Copy lives in `messages/dj-console.ts` (the `dj.*` namespace) and every
 * formatter takes `locale`, so money, clocks and ratios follow the chosen
 * language. Track titles, artist names and genre names come from the API and
 * are never translated.
 *
 * Honesty rules (docs/ui/PANEL-DESIGN.md §Data honesty):
 *   - live mode renders only what `getDjShift`/`getDjQueue`/`getDjStats`
 *     returned; a value the API does not expose is shown as `unavailable`.
 *   - demo mode is labelled as a local preview everywhere it can be mistaken
 *     for server state.
 *   - room rules and pricing are marked local: the backend has no endpoint that
 *     stores them, and the UI never claims a save that did not happen.
 */
import { useState } from 'react';
import { QRCodeSVG } from 'qrcode.react';
import { Link } from 'react-router-dom';
import { useAutoAnimate } from '@formkit/auto-animate/react';
import type { ReactNode } from 'react';
import Icon from '../components/Icon';
import { useLocale, type Translate } from '../locale';
import type {
  DjConnectionStatus,
  DjConsoleMockApi,
  DjRequestStatus,
  DjSessionStatus,
  DjTelemetry,
} from '../dj-console/types';

export type DjConsoleTab = 'requests' | 'room' | 'session';

type ViewProps = {
  dj: DjConsoleMockApi;
  loading?: boolean;
  onNotice: (message: string) => void;
  live?: boolean;
  sort?: 'new' | 'hot';
  onSortChange?: (sort: 'new' | 'hot') => void;
  roomMode?: 'Warm-up' | 'Peak time' | 'Closing soon';
  onRoomModeChange?: (mode: 'Warm-up' | 'Peak time' | 'Closing soon') => void;
};

function money(
  minor: number | null | undefined,
  locale: string,
  unavailable: string,
): string {
  if (typeof minor !== 'number' || !Number.isFinite(minor)) return unavailable;
  return new Intl.NumberFormat(locale, { style: 'currency', currency: 'EUR', minimumFractionDigits: 2, maximumFractionDigits: 2 }).format(minor / 100);
}

function clock(
  iso: string | null | undefined,
  locale: string,
  unavailable: string,
): string {
  if (!iso) return unavailable;
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return unavailable;
  return new Intl.DateTimeFormat(locale, { hour: '2-digit', minute: '2-digit' }).format(date);
}

/** Ratios are numbers too: 0.5 reads as `0,50` in ru, not `0.50`. */
function ratio(value: number, locale: string): string {
  return new Intl.NumberFormat(locale, { minimumFractionDigits: 2, maximumFractionDigits: 2 }).format(value);
}

const STATUS_KEY: Record<DjRequestStatus, string> = {
  pending: 'dj.statusPending',
  accepted: 'dj.statusAccepted',
  played: 'dj.statusPlayed',
  rejected: 'dj.statusRejected',
};

const CONNECTION_KEY: Record<DjConnectionStatus, string> = {
  connected: 'dj.connectionConnected',
  degraded: 'dj.connectionDegraded',
  offline: 'dj.connectionOffline',
};

const SESSION_STATUS_KEY: Record<DjSessionStatus, string> = {
  created: 'dj.sessionStatusCreated',
  active: 'dj.sessionStatusActive',
  paused_acceptance: 'dj.sessionStatusPaused',
  closed: 'dj.sessionStatusClosed',
};

const EVENT_KEY: Record<NonNullable<DjTelemetry['lastEvent']>, string> = {
  request_received: 'dj.eventRequestReceived',
  request_updated: 'dj.eventRequestUpdated',
  tip_received: 'dj.eventTipReceived',
  player_changed: 'dj.eventPlayerChanged',
};

const RULE_ROWS = [
  { key: 'requests', title: 'dj.ruleRequests', detail: 'dj.ruleRequestsDetail' },
  { key: 'explicit', title: 'dj.ruleExplicit', detail: 'dj.ruleExplicitDetail' },
  { key: 'duplicates', title: 'dj.ruleDuplicates', detail: 'dj.ruleDuplicatesDetail' },
] as const;

const PLURAL_KEYS: Record<string, string> = {
  one: 'dj.requestsForTrackOne',
  few: 'dj.requestsForTrackFew',
  many: 'dj.requestsForTrackMany',
  other: 'dj.requestsForTrackOther',
};

/** Russian needs one/few/many: `3 запроса`, not `3 запросов`.
 *  Exported: the requests workspace labels the same counter on every row. */
export function trackRequestLabel(count: number, locale: string, t: Translate): string {
  const category = new Intl.PluralRules(locale).select(count);
  return t(PLURAL_KEYS[category] ?? 'dj.requestsForTrackOther', { count });
}

const METRIC_KEYS = {
  overview: 'dj.metricOverview',
  analytics: 'dj.metricAnalytics',
  payouts: 'dj.metricPayouts',
} as const;

function Section({ title, detail, tools, children, id }: { title: string; detail?: string; tools?: ReactNode; children: ReactNode; id: string }) {
  return (
    <section className="panel-card" data-surface="panel" aria-labelledby={id}>
      <header className="panel-card-head">
        <div>
          <h2 id={id} className="panel-card-title">{title}</h2>
          {detail ? <p className="panel-card-sub">{detail}</p> : null}
        </div>
        {tools ? <div className="panel-card-tools">{tools}</div> : null}
      </header>
      {children}
    </section>
  );
}

/* ---------------------------------------------------------------------------
   Monitoring cards
   ------------------------------------------------------------------------ */

/**
 * Tips waiting to be acknowledged.
 *
 * These two cards used to sit beside the queue, which meant the DJ's work list
 * shared a screen with the numbers about the work. They live with the session
 * now: the requests surface is for processing requests, and money that arrived
 * is a session fact, not a decision to make about a row.
 */
export function DjTipsSection({ dj, onNotice, live = false }: ViewProps) {
  const { t, locale } = useLocale();
  const formatMoney = (minor: number | null | undefined) => money(minor, locale, t('common.unavailable'));
  const formatClock = (iso: string | null | undefined) => clock(iso, locale, t('common.unavailable'));
  const receivedTips = dj.tips.filter((tip) => tip.status === 'received');
  // Live mode has no tips to show, and it is not "no tips waiting": the club
  // API has no tips endpoint yet (docs/audit/web-club-gaps.md §6.6), so the
  // card says so instead of claiming an empty, all-clear evening.
  const tipsUnavailable = live;
  return (
    <Section
      id="dj-tips-heading"
      title={t('dj.tips')}
      detail={tipsUnavailable ? t('common.unavailable') : receivedTips.length ? t('dj.tipsWaiting', { count: receivedTips.length }) : t('dj.allClear')}
    >
      {tipsUnavailable ? (
        <div className="panel-empty">
          <strong>{t('dj.tipsUnavailableTitle')}</strong>
          <span>{t('dj.tipsUnavailableBody')}</span>
        </div>
      ) : receivedTips.length ? (
        <div className="list">
          {receivedTips.map((tip) => (
            <button
              type="button"
              key={tip.id}
              className="dj-tip-row"
              onClick={() => { dj.acknowledgeTip(tip.id); onNotice(t('dj.noticeTipAcknowledged', { name: tip.guestName ?? t('dj.guest') })); }}
            >
              <span>
                <span className="dj-tip-name">{tip.guestName ?? t('dj.guest')}</span>
                <span className="dj-tip-hint">{t('dj.tipReceivedHint', { time: formatClock(tip.receivedAt) })}</span>
              </span>
              <span className="dj-tip-amount">{formatMoney(tip.amountMinor)}</span>
            </button>
          ))}
        </div>
      ) : (
        <div className="panel-empty"><strong>{t('dj.tipsEmptyTitle')}</strong><span>{t('dj.tipsEmptyBody')}</span></div>
      )}
    </Section>
  );
}

/** Queue load and connection facts — the "why does the list look like this" card. */
export function DjQueueHealthSection({ dj, live = false }: { dj: DjConsoleMockApi; live?: boolean }) {
  const { t, locale } = useLocale();
  const formatClock = (iso: string | null | undefined) => clock(iso, locale, t('common.unavailable'));
  return (
    <Section id="dj-queue-health-heading" title={t('dj.queueLoad')} detail={t('dj.queueLoadDetail')}>
      <div className="dj-bar-row">
        <div className="bar-row-head">
          <span className="bar-row-label">{t('dj.requestsReady')}</span>
          <span className="bar-row-value">{dj.queue.length}</span>
        </div>
        <div className="bar-track">
          <div className="bar-fill" style={{ width: `${Math.min(100, dj.queue.length * 14)}%` }} />
        </div>
      </div>
      <dl className="dj-kv">
        <div className="dj-kv-row"><dt>{t('dj.awaitingDecision')}</dt><dd>{dj.queue.filter((item) => item.status === 'pending').length}</dd></div>
        <div className="dj-kv-row"><dt>{t('dj.connection')}</dt><dd>{live ? t(CONNECTION_KEY[dj.telemetry.connection]) : t('common.demo')}</dd></div>
        <div className="dj-kv-row"><dt>{t('dj.lastSync')}</dt><dd>{live ? formatClock(dj.telemetry.lastSyncedAt) : t('common.unavailable')}</dd></div>
      </dl>
    </Section>
  );
}

/* ---------------------------------------------------------------------------
   Room control
   ------------------------------------------------------------------------ */
export function DjRoomControlView({ dj, onNotice, live = false }: ViewProps) {
  const { t, locale } = useLocale();
  const paused = dj.state.session?.status === 'paused_acceptance';
  const session = dj.state.session;

  const [minimumPrice, setMinimumPrice] = useState('2');
  const [roomMessage, setRoomMessage] = useState(() => t('dj.defaultRoomMessage'));
  const [showQr, setShowQr] = useState(false);
  const [rules, setRules] = useState({ explicit: true, duplicates: false, requests: true });
  const [genres, setGenres] = useState<Record<string, boolean>>({ Techno: true, House: true, 'Hip-Hop': false, Pop: true });
  const [saved, setSaved] = useState(false);
  const toggleRule = (key: keyof typeof rules) => setRules((previous) => ({ ...previous, [key]: !previous[key] }));
  const toggleGenre = (genre: string) => setGenres((previous) => ({ ...previous, [genre]: !previous[genre] }));
  const formatMoney = (minor: number | null | undefined) => money(minor, locale, t('common.unavailable'));

  return (
    <>
      <section className="dj-intake-bar" aria-label={t('dj.requestIntake')}>
        <div className="dj-intake-copy">
          <span className="dj-intake-text">
            <strong>{!session ? t('dj.noActiveShiftAssigned') : paused ? t('dj.requestsPaused') : t('dj.acceptingGuestRequests')}</strong>
            <span>{session ? t('dj.sessionId', { id: session.id }) : live ? t('dj.assignYourself') : t('dj.localDemoState')}</span>
          </span>
        </div>
        <div className="dj-intake-actions">
          {session ? (
            <button type="button" className={paused ? 'btn btn-primary' : 'btn btn-secondary'} onClick={() => dj.toggleAcceptance()}>
              {paused ? t('dj.resumeRequestIntake') : t('dj.pauseRequestIntake')}
            </button>
          ) : null}
        </div>
      </section>

      <div className="panel-section dj-room-grid">
        <div className="dj-room-stack">
          <Section id="dj-room-rules-heading" title={t('dj.roomRules')} detail={t('dj.roomRulesDetail')}>
            <div>
              {RULE_ROWS.map((row) => (
                <label key={row.key} className="dj-rule-row">
                  <span className="dj-rule-copy">
                    <strong>{t(row.title)}</strong>
                    <span>{t(row.detail)}</span>
                  </span>
                  <input type="checkbox" checked={rules[row.key]} onChange={() => toggleRule(row.key)} aria-label={t(row.title)} />
                </label>
              ))}
            </div>
            <div className="dj-form-actions">
              <button type="button" className="btn btn-primary" onClick={() => { dj.setAcceptance(!rules.requests); setSaved(true); onNotice(t('dj.noticeRulesApplied')); }}>
                {t('dj.applyInPreview')}
              </button>
              <button type="button" className="btn btn-secondary" onClick={() => setRules({ explicit: true, duplicates: false, requests: true })}>{t('dj.reset')}</button>
              <span className="dj-counter">{saved ? t('dj.appliedLocally') : t('dj.unsaved')}</span>
            </div>

            <div className="dj-field-row">
              <span className="field-label" style={{ margin: 0 }}>{t('dj.genreFilter')}</span>
            </div>
            <div className="dj-chip-row">
              {Object.entries(genres).map(([genre, enabled]) => (
                <button
                  type="button"
                  key={genre}
                  aria-pressed={enabled}
                  onClick={() => toggleGenre(genre)}
                  className={`chip dj-chip-toggle ${enabled ? 'chip-accent' : 'chip-neutral'}`}
                >
                  {enabled ? t('dj.genreAllow', { genre }) : t('dj.genreBlock', { genre })}
                </button>
              ))}
            </div>

            {live ? (
              <p className="panel-card-note">{t('dj.rulesLiveNote')}</p>
            ) : null}
          </Section>

          <Section id="dj-room-message-heading" title={t('dj.guestMessage')} detail={saved ? t('dj.savedInPreview') : t('dj.unsavedChanges')}>
            <label className="field">
              <span className="field-label">{t('dj.guestMessageLabel')}</span>
              <textarea
                className="field-input"
                value={roomMessage}
                maxLength={140}
                onChange={(event) => { setRoomMessage(event.target.value); setSaved(false); }}
                rows={3}
                aria-label={t('dj.guestMessageAria')}
              />
            </label>
            <div className="dj-form-actions">
              <span className="dj-counter">{t('dj.charactersCount', { count: roomMessage.length, max: 140 })}</span>
              <button type="button" className="btn btn-primary" onClick={() => { setSaved(true); onNotice(t('dj.noticeMessageSaved')); }}>{t('dj.saveMessage')}</button>
            </div>
          </Section>
        </div>

        <div className="dj-side-stack">
          <Section id="dj-room-price-heading" title={t('dj.requestPrice')} detail={t('dj.requestPriceDetail')}>
            <label className="field">
              <span className="field-label">{t('dj.minimumRequestPrice')}</span>
              <input
                className="field-input"
                type="number"
                min="0"
                step="0.01"
                value={minimumPrice}
                onChange={(event) => setMinimumPrice(event.target.value)}
                inputMode="decimal"
                aria-label={t('dj.minimumRequestPrice')}
              />
            </label>
            <dl className="dj-kv" style={{ marginTop: 16 }}>
              <div className="dj-kv-row"><dt>{t('dj.guestPrice')}</dt><dd>{formatMoney(Number(minimumPrice || 0) * 100)}</dd></div>
              <div className="dj-kv-row"><dt>{t('dj.share')}</dt><dd>{t('dj.setByClub')}</dd></div>
            </dl>
            <p className="panel-card-note">{t('dj.priceNote')}</p>
          </Section>

          <Section id="dj-room-link-heading" title={t('dj.roomLink')} detail={t('dj.roomLinkDetail')}>
            <div className="dj-kv">
              <div className="dj-kv-row"><dt>{t('dj.roomLabel')}</dt><dd>{session ? `room/${session.id}` : t('dj.noActiveShiftShort')}</dd></div>
              <div className="dj-kv-row"><dt>{t('dj.intake')}</dt><dd>{rules.requests ? t('dj.intakeOpen') : t('dj.intakePaused')}</dd></div>
            </div>
            <div className="dj-form-actions">
              <button type="button" className="btn btn-primary" onClick={() => setShowQr(true)}>
                <Icon name="lucide:qr-code" size={15} />{t('dj.showQr')}
              </button>
            </div>
          </Section>
        </div>
      </div>

      {showQr ? (
        <div className="dj-qr-scrim" onClick={() => setShowQr(false)}>
          <div className="dj-qr-dialog" role="dialog" aria-modal="true" aria-labelledby="dj-qr-title" onClick={(event) => event.stopPropagation()}>
            <p className="field-label" id="dj-qr-title">{t('dj.scanToRequest')}</p>
            <div className="dj-qr-frame">
              <QRCodeSVG value={`${window.location.origin}/room/${session?.id ?? 'session_mock_01'}`} size={184} title={t('dj.qrTitle')} />
            </div>
            <span className="dj-qr-value">{window.location.origin}/room/{session?.id ?? 'session_mock_01'}</span>
            <p className="panel-card-note">{t('dj.qrIntakeNote', { state: rules.requests ? t('dj.intakeOpen') : t('dj.intakePaused') })}</p>
            <div className="dj-form-actions" style={{ justifyContent: 'center' }}>
              {/* The console shows the live link; a printed sign is designed in
                  the studio, which opens with this same room already in it. */}
              <Link className="btn btn-quiet" to="/qr" data-control="button">{t('dj.qrStudioLink')}</Link>
              <button type="button" className="btn btn-secondary" onClick={() => setShowQr(false)}>{t('common.close')}</button>
            </div>
          </div>
        </div>
      ) : null}
    </>
  );
}

/* ---------------------------------------------------------------------------
   Session details
   ------------------------------------------------------------------------ */
export function DjSessionDetailsView({ dj, onNotice, live = false }: ViewProps) {
  const { t, locale } = useLocale();
  const unavailable = t('common.unavailable');
  const formatMoney = (minor: number | null | undefined) => money(minor, locale, unavailable);
  const formatClock = (iso: string | null | undefined) => clock(iso, locale, unavailable);
  const [metric, setMetric] = useState<'overview' | 'analytics' | 'payouts'>('overview');
  const [exported, setExported] = useState(false);
  const [historyListRef] = useAutoAnimate({ duration: 180 });
  // Live reads: `liveData` is present only on the live adapter. A read that
  // failed renders the word `unavailable` — never the last good value, never 0.
  const liveData = dj.liveData;
  const statsAvailable = !live || liveData?.stats === 'ok';
  const historyStatus = live ? (liveData?.history ?? 'loading') : 'ready';
  // The club API exposes no tips to the console (audit §6.6), so live mode has
  // no tip figure at all; the demo fixtures still show theirs.
  const tipsAvailable = !live;
  const receivedTips = tipsAvailable ? dj.tips.filter((tip) => tip.status === 'received') : [];
  const totalTips = tipsAvailable ? receivedTips.reduce((sum, tip) => sum + tip.amountMinor, 0) : undefined;
  const stats = dj.state.stats;
  const session = dj.state.session;
  const playedLabel = statsAvailable ? String(stats.played) : unavailable;

  const exportSession = () => {
    // Real rows from GET /v1/dj/history: the time is the server's play time,
    // and a row without one leaves the cell empty instead of inventing a clock.
    const rows = [['track', 'artist', 'status', 'playedAt'], ...dj.history.map((entry) => [entry.track.title, entry.track.artist, entry.status, entry.occurredAt])];
    const csv = rows.map((row) => row.map((value) => `"${value.replace(/"/g, '""')}"`).join(',')).join('\n');
    const url = URL.createObjectURL(new Blob([csv], { type: 'text/csv;charset=utf-8' }));
    const link = document.createElement('a');
    link.href = url;
    link.download = 'dropit-session-history.csv';
    link.click();
    window.setTimeout(() => URL.revokeObjectURL(url), 1000);
    setExported(true);
    onNotice(t('dj.noticeExported'));
  };

  const copySummary = async () => {
    const summary = t('dj.summaryShare', {
      id: session?.id ?? t('dj.unassigned'),
      played: statsAvailable ? stats.played : unavailable,
      tips: formatMoney(totalTips),
    });
    try {
      await navigator.clipboard.writeText(summary);
      onNotice(t('dj.noticeCopied'));
    } catch {
      onNotice(t('dj.noticeClipboardDenied'));
    }
  };

  const acceptance = stats.requests ? Math.round((stats.accepted / stats.requests) * 100) : null;
  // Export is a claim ("your shift, on disk"), so it is offered only when the
  // log was actually read and there is at least one row in it.
  const canExport = historyStatus === 'ready' && dj.history.length > 0;

  return (
    <>
      <div className="dj-session-grid">
        <Section
          id="dj-session-summary-heading"
          title={t('dj.sessionSummary')}
          detail={live ? t('dj.summaryLive') : t('dj.summaryDemo')}
          tools={(
            <>
              <button type="button" className="btn btn-secondary btn-sm" onClick={() => { void copySummary(); }}>
                <Icon name="lucide:copy" size={14} />{t('dj.copy')}
              </button>
              <button type="button" className="btn btn-secondary btn-sm" onClick={exportSession} disabled={!canExport}>
                <Icon name="lucide:download" size={14} />{exported ? t('dj.exported') : t('dj.exportCsv')}
              </button>
            </>
          )}
        >
          <div className="seg dj-metric-tabs" role="tablist" aria-label={t('dj.sessionMetricGroup')}>
            {(['overview', 'analytics', 'payouts'] as const).map((value) => (
              <button
                type="button"
                key={value}
                role="tab"
                aria-selected={metric === value}
                className={`seg-item ${metric === value ? 'is-active' : ''}`}
                onClick={() => setMetric(value)}
              >
                {t(METRIC_KEYS[value])}
              </button>
            ))}
          </div>

          {metric === 'overview' ? (
            <div className="stat-grid dj-session-stats">
              <MiniStat label={t('dj.requestsReceived')} value={statsAvailable ? String(stats.requests) : unavailable} muted={!statsAvailable} />
              <MiniStat label={t('dj.accepted')} value={statsAvailable ? String(stats.accepted) : unavailable} muted={!statsAvailable} />
              {/* Played comes from the shift counters, not from the rows on this
                  page: the log is paginated, so counting it would undercount
                  every shift longer than one page. */}
              <MiniStat label={t('dj.played')} value={playedLabel} muted={!statsAvailable} />
              <MiniStat label={t('dj.tipsReceived')} value={formatMoney(totalTips)} muted={!tipsAvailable} />
            </div>
          ) : metric === 'analytics' ? (
            <>
              <div className="stat-grid dj-session-stats">
                <MiniStat label={t('dj.acceptanceRate')} value={acceptance === null || !statsAvailable ? unavailable : `${acceptance}%`} muted={acceptance === null || !statsAvailable} />
                <MiniStat label={t('dj.requestVelocity')} value={dj.telemetry.requestsPerMinute ? t('dj.perMinute', { value: dj.telemetry.requestsPerMinute }) : unavailable} muted={!dj.telemetry.requestsPerMinute} />
                <MiniStat label={t('dj.eventsSynced')} value={live ? String(dj.telemetry.eventsReceived) : unavailable} muted={!live} />
                <MiniStat label={t('dj.playsPerRequest')} value={stats.requests && statsAvailable ? ratio(stats.played / stats.requests, locale) : unavailable} muted={!stats.requests || !statsAvailable} />
              </div>
              {acceptance !== null ? (
                <div className="dj-bar-row" style={{ marginTop: 16 }}>
                  <div className="bar-row-head"><span className="bar-row-label">{t('dj.acceptedVsReceived')}</span><span className="bar-row-value">{acceptance}%</span></div>
                  <div className="bar-track"><div className="bar-fill" style={{ width: `${acceptance}%` }} /></div>
                </div>
              ) : null}
            </>
          ) : (
            <>
              <div className="stat-grid dj-session-stats">
                {/* The DJ share is the shift's money (split rows of this
                    session), not the DJ's lifetime total. */}
                <MiniStat label={t('dj.share')} value={statsAvailable ? formatMoney(stats.earnedMinor) : unavailable} muted={!statsAvailable} />
                <MiniStat label={t('dj.tipsShort')} value={formatMoney(totalTips)} muted={!tipsAvailable} />
                {/* A total that silently drops tips is not a total. */}
                <MiniStat label={t('dj.sessionTotal')} value={tipsAvailable && statsAvailable ? formatMoney(stats.earnedMinor + (totalTips ?? 0)) : unavailable} muted={!tipsAvailable || !statsAvailable} />
                <MiniStat label={t('dj.payoutStatus')} value={t('dj.payoutPending')} />
              </div>
              <p className="panel-card-note">{tipsAvailable ? t('dj.payoutNote') : `${t('dj.payoutNote')} ${t('dj.tipsUnavailableNote')}`}</p>
            </>
          )}
        </Section>

        <Section id="dj-session-log-heading" title={t('dj.sessionLog')} detail={t('dj.operationalDetails')}>
          <dl className="dj-kv">
            <div className="dj-kv-row"><dt>{t('dj.sessionLabel')}</dt><dd>{session?.id ?? unavailable}</dd></div>
            <div className="dj-kv-row"><dt>{t('dj.statusLabel')}</dt><dd>{session ? t(SESSION_STATUS_KEY[session.status]) : unavailable}</dd></div>
            <div className="dj-kv-row"><dt>{t('dj.started')}</dt><dd>{formatClock(dj.state.nowPlaying.startedAt)}</dd></div>
            <div className="dj-kv-row"><dt>{t('dj.timezone')}</dt><dd>{session?.timezone ?? unavailable}</dd></div>
            <div className="dj-kv-row"><dt>{t('dj.connection')}</dt><dd>{live ? t(CONNECTION_KEY[dj.telemetry.connection]) : t('common.demo')}</dd></div>
            <div className="dj-kv-row"><dt>{t('dj.lastEvent')}</dt><dd>{dj.telemetry.lastEvent ? t(EVENT_KEY[dj.telemetry.lastEvent]) : t('dj.noEvents')}</dd></div>
          </dl>
        </Section>
      </div>

      <div className="panel-section dj-console-split">
        <DjTipsSection dj={dj} onNotice={onNotice} live={live} />
        <DjQueueHealthSection dj={dj} live={live} />
      </div>

      <div className="panel-section">
        <Section
          id="dj-session-history-heading"
          title={t('dj.playedHistory')}
          detail={historyStatus === 'unavailable' ? unavailable : t('dj.loggedCount', { count: dj.history.length })}
          tools={live && liveData?.hasMoreHistory ? (
            <button type="button" className="btn btn-secondary btn-sm" onClick={liveData.loadMoreHistory} disabled={liveData.historyLoading}>
              <Icon name="lucide:chevron-down" size={14} />{t('dj.loadMoreHistory')}
            </button>
          ) : undefined}
        >
          {historyStatus === 'unavailable' ? (
            <div className="panel-empty">
              <strong>{t('dj.historyUnavailableTitle')}</strong>
              <span>{t('dj.historyUnavailableBody')}</span>
            </div>
          ) : dj.history.length ? (
            <div className="table-wrap">
              <table className="data-table data-table--stack">
                <thead>
                  <tr><th scope="col">{t('dj.colTrack')}</th><th scope="col">{t('dj.colArtist')}</th><th scope="col">{t('dj.colTime')}</th><th scope="col" className="is-num">{t('dj.statusLabel')}</th></tr>
                </thead>
                <tbody ref={historyListRef}>
                  {dj.history.map((entry) => (
                    <tr key={entry.id}>
                      <td className="is-strong cell-primary">{entry.track.title}</td>
                      <td className="is-muted" data-label={t('dj.colArtist')}>{entry.track.artist}</td>
                      <td className="is-mono" data-label={t('dj.colTime')}>{formatClock(entry.occurredAt)}</td>
                      <td className="is-num" data-label={t('dj.statusLabel')}>
                        <span className={`chip ${entry.status === 'played' ? 'chip-accent' : 'chip-error'}`}>
                          {t(STATUS_KEY[entry.status])}
                        </span>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          ) : (
            <div className="panel-empty"><strong>{t('dj.noTracksTitle')}</strong><span>{t('dj.noTracksBody')}</span></div>
          )}
        </Section>
      </div>
    </>
  );
}

function MiniStat({ label, value, muted = false }: { label: string; value: string; muted?: boolean }) {
  return (
    <div className="dj-mini-stat">
      <span className="stat-label">{label}</span>
      <span className={`dj-mini-value ${muted ? 'is-muted' : ''}`.trim()}>{value}</span>
    </div>
  );
}