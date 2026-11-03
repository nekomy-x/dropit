import type { ActiveSession, SessionStatus } from './types';
import { useLocale } from '../../locale';

/** Session state → catalog key; the stored state word never changes. */
const STATUS_KEY: Record<SessionStatus, string> = {
  live: 'status.live',
  starting: 'status.starting',
  paused: 'status.paused',
};

export interface ActiveSessionMonitorProps {
  sessions: ReadonlyArray<ActiveSession>;
  title?: string;
  className?: string;
  onSessionSelect?: (session: ActiveSession) => void;
  /** Where the rows come from; the caller must keep this honest. */
  caption?: string;
}

const STATUS_CHIP: Record<SessionStatus, { tone: string }> = {
  live: { tone: 'chip-accent' },
  starting: { tone: 'chip-warn' },
  paused: { tone: 'chip-neutral' },
};

/**
 * Rooms that are currently sending activity.
 *
 * A `data-table` surface in a `panel-card`, so it reads as the same product as
 * the club roster. The status is a chip with a word — never a bare coloured dot.
 */
export default function ActiveSessionMonitor({
  sessions,
  title,
  className = '',
  onSessionSelect,
  caption,
}: ActiveSessionMonitorProps) {
  const { t } = useLocale();
  return (
    <section className={`panel-card ${className}`.trim()} aria-labelledby="analytics-sessions-heading">
      <header className="panel-card-head">
        <div>
          <h2 id="analytics-sessions-heading" className="panel-card-title">{title ?? t('analytics.activeSessions')}</h2>
          <p className="panel-card-sub">{caption ?? t('analytics.roomsSending')}</p>
        </div>
        <div className="panel-card-tools">
          <span className="chip chip-neutral">{t('analytics.roomsCount', { count: sessions.length })}</span>
        </div>
      </header>

      {sessions.length === 0 ? (
        <div className="panel-empty">
          <strong>{t('analytics.noActiveSessions')}</strong>
          <span>{t('analytics.noActiveSessionsBody')}</span>
        </div>
      ) : (
        <div className="table-wrap">
          <table className="data-table data-table--stack" style={{ minWidth: 560 }}>
            <thead>
              <tr>
                <th scope="col">{t('analytics.colVenue')}</th>
                <th scope="col">{t('analytics.colDj')}</th>
                <th scope="col" className="is-num">{t('analytics.colRequests')}</th>
                <th scope="col" className="is-num">{t('analytics.colGuests')}</th>
                <th scope="col" className="is-num">{t('analytics.colState')}</th>
              </tr>
            </thead>
            <tbody>
              {sessions.map((session) => {
                const status = STATUS_CHIP[session.status];
                return (
                  <tr key={session.id}>
                    {/* `cell-primary` is the stacked-row identity: on a phone it
                        is the row's title line and carries no column label. */}
                    <td className="is-strong cell-primary">
                      {onSessionSelect ? (
                        <button
                          type="button"
                          onClick={() => onSessionSelect(session)}
                          aria-label={t('analytics.openSession', { venue: session.venue })}
                          style={{ display: 'block', minWidth: 0, textAlign: 'left', background: 'none', border: 0, padding: 0, color: 'var(--c-text)', cursor: 'pointer' }}
                        >
                          <span style={{ display: 'block', maxWidth: 180, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{session.venue}</span>
                          <span className="list-sub" style={{ fontFamily: 'var(--font-mono)' }}>
                            {session.city ?? session.id} · {session.startedAt}
                          </span>
                        </button>
                      ) : (
                        <>
                          <span style={{ display: 'block', maxWidth: 180, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{session.venue}</span>
                          <span className="list-sub" style={{ fontFamily: 'var(--font-mono)' }}>
                            {session.city ?? session.id} · {session.startedAt}
                          </span>
                        </>
                      )}
                    </td>
                    <td data-label={t('analytics.colDj')}>
                      <span style={{ display: 'block', maxWidth: 160, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{session.dj}</span>
                    </td>
                    <td className="is-num" data-label={t('analytics.colRequests')}>{session.requests}</td>
                    <td className="is-num" data-label={t('analytics.colGuests')}>{session.guests}</td>
                    <td className="is-num" data-label={t('analytics.colState')}>
                      <span className={`chip ${status.tone}`}>
                        {t(STATUS_KEY[session.status])}
                      </span>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}
    </section>
  );
}
