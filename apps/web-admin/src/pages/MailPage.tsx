import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  ApiRequestError,
  MAIL_STATUSES,
  getMailMessage,
  getMailStats,
  listMail,
  listMailSuppressions,
  removeMailSuppression,
  resendMail,
  type MailDetail,
  type MailItem,
  type MailStats,
  type MailSuppression,
} from '../api/client';
import { useAsyncData } from '../hooks/useAsyncData';
import { useLocale } from '../locale';
import { STATUS_MESSAGE_KEY } from '../messages/status';
import ErrorState from '../components/ErrorState';
import Loading, { SkeletonRows, SkeletonTiles } from '../components/Loading';
import StatusBadge from '../components/StatusBadge';
import PageHeading from '../components/PageHeading';
import Icon from '../components/Icon';
import Select from '../components/ui/Select';
import StatCard from '../components/ui/StatCard';
import TrashAction from '../components/ui/TrashAction';
import { IS_DEMO } from '../app-mode';

/**
 * Mail — transactional delivery through Resend (docs/mail-infra-contract.md §7–8).
 *
 *   GET    /v1/admin/mail?status=&q=&limit=&cursor=   → {items, next_cursor}
 *   GET    /v1/admin/mail/stats                       → 24 h counters
 *   GET    /v1/admin/mail/{id}                        → row + html/text/vars/events
 *   POST   /v1/admin/mail/{id}/resend                 → {id, status}
 *   GET    /v1/admin/mail/suppressions                → {items}
 *   DELETE /v1/admin/mail/suppressions/{email}        → {removed}
 *
 * The screen is read-mostly: the only writes are "queue a copy" and "lift a
 * suppression". Every value is printed as the backend stores it (snake_case
 * statuses, raw ids, raw error text); `status` is the only thing that is
 * interpreted, and only as a chip tone. No figure is derived or seeded — in a
 * demo build with no backend the screen shows the error state, not numbers.
 */

const PAGE_SIZE = 25;

/** Short absolute stamp: "27 Oct, 18:00". Raw ISO is kept in the drawer. */
function shortStamp(iso: string | null | undefined, locale: string): string {
  if (!iso) return '—';
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return iso;
  return date.toLocaleString(locale, {
    day: '2-digit',
    month: 'short',
    hour: '2-digit',
    minute: '2-digit',
    hour12: false,
  });
}

/**
 * One compact KPI cell. The value is printed only when the stats request
 * answered; otherwise the cell carries the word "unavailable" and no number
 * (docs/ui/PANEL-DESIGN.md §Data honesty).
 */
function MailKpi({
  label,
  icon,
  value,
  available,
}: {
  label: string;
  icon: `lucide:${string}`;
  value: number | undefined;
  available: boolean;
}) {
  const { t, locale } = useLocale();
  const hasValue = available && typeof value === 'number';
  const printed = hasValue ? new Intl.NumberFormat(locale).format(value) : t('common.unavailable');
  return (
    <StatCard
      label={label}
      icon={icon}
      source={hasValue ? 'live' : 'unavailable'}
      value={printed}
      caption={t('mail.last24h')}
    >
      <p
        className={`stat-value${hasValue ? '' : ' is-muted'}`}
        style={hasValue ? { fontSize: 'var(--fs-20)' } : undefined}
      >
        {printed}
      </p>
    </StatCard>
  );
}

const FOCUSABLE =
  'button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), a[href], [tabindex]:not([tabindex="-1"])';

/**
 * Message detail drawer.
 *
 * Presentation is the shared drawer (`admin-drawer*`, the same layer the
 * console entity drawer uses); behaviour is a modal dialog: focus moves into
 * it, Tab is trapped, Escape closes, and focus returns to the row control.
 *
 * The stored body is rendered in a sandboxed iframe — `sandbox=""` (no
 * scripts, no same-origin, no forms, no navigation) and `srcdoc`, never
 * `dangerouslySetInnerHTML`.
 */
function MailDetailDrawer({
  item,
  onClose,
  onResent,
}: {
  item: MailItem;
  onClose: () => void;
  onResent: () => void;
}) {
  const fetchDetail = useCallback(() => getMailMessage(item.id), [item.id]);
  const { data, loading, error, reload } = useAsyncData<MailDetail>(fetchDetail);
  const { t, locale } = useLocale();

  const [resending, setResending] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);

  const drawerRef = useRef<HTMLDivElement | null>(null);
  const closeRef = useRef<HTMLButtonElement | null>(null);
  const previousFocusRef = useRef<HTMLElement | null>(null);
  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;

  useEffect(() => {
    previousFocusRef.current = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const frame = requestAnimationFrame(() => closeRef.current?.focus({ preventScroll: true }));
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.preventDefault();
        onCloseRef.current();
        return;
      }
      if (event.key !== 'Tab' || !drawerRef.current) return;
      const focusable = Array.from(drawerRef.current.querySelectorAll<HTMLElement>(FOCUSABLE));
      if (focusable.length === 0) return;
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      const active = document.activeElement;
      if (!drawerRef.current.contains(active)) {
        event.preventDefault();
        (event.shiftKey ? last : first)?.focus();
      } else if (event.shiftKey && active === first) {
        event.preventDefault();
        last?.focus();
      } else if (!event.shiftKey && active === last) {
        event.preventDefault();
        first?.focus();
      }
    };
    document.addEventListener('keydown', onKeyDown);
    return () => {
      cancelAnimationFrame(frame);
      document.removeEventListener('keydown', onKeyDown);
      if (previousFocusRef.current?.isConnected) previousFocusRef.current.focus({ preventScroll: true });
    };
  }, []);

  const onResend = async () => {
    setActionError(null);
    setNotice(null);
    setResending(true);
    try {
      const result = await resendMail(item.id);
      const statusKey = STATUS_MESSAGE_KEY[result.status];
      setNotice(
        t('mail.resendNotice', {
          id: result.id,
          status: statusKey ? t(statusKey) : result.status,
        }),
      );
      onResent();
      reload();
    } catch (err) {
      setActionError(err instanceof ApiRequestError ? err.message : t('mail.resendFailed'));
    } finally {
      setResending(false);
    }
  };

  const view: MailItem = data ?? item;
  const events = useMemo(
    () => [...(data?.events ?? [])].sort((a, b) => a.received_at.localeCompare(b.received_at)),
    [data],
  );

  return (
    <div
      className="admin-drawer-backdrop"
      role="presentation"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <div
        ref={drawerRef}
        className="admin-drawer"
        role="dialog"
        aria-modal="true"
        aria-labelledby="mail-detail-heading"
      >
        <header className="admin-drawer-head">
          <div style={{ minWidth: 0 }}>
            <p className="admin-eyebrow">{t('mail.drawerEyebrow')}</p>
            <h2
              id="mail-detail-heading"
              className="panel-card-title"
              style={{ fontSize: 'var(--fs-20)', overflowWrap: 'anywhere' }}
            >
              {view.subject || t('mail.noSubject')}
            </h2>
          </div>
          <button
            ref={closeRef}
            type="button"
            className="panel-icon-button"
            aria-label={t('mail.closeDetail')}
            onClick={onClose}
          >
            <Icon name="lucide:x" size={16} />
          </button>
        </header>

        <div className="admin-drawer-body">
          {actionError ? (
            <p className="panel-error" role="alert" style={{ marginBottom: 12 }}>
              {actionError}
            </p>
          ) : null}
          {notice ? (
            <p className="admin-toast" role="status" style={{ margin: '0 0 12px' }}>
              {notice}
            </p>
          ) : null}

          <dl className="admin-meta" style={{ marginTop: 0 }}>
            <div>
              <dt>{t('mail.fieldRecipient')}</dt>
              <dd>
                {view.to}
                {view.to_name ? <span style={{ color: 'var(--c-text-muted)' }}> · {view.to_name}</span> : null}
              </dd>
            </div>
            <div>
              <dt>{t('mail.template')}</dt>
              <dd className="is-mono">{view.template}</dd>
            </div>
            <div>
              <dt>{t('mail.statusLabel')}</dt>
              <dd><StatusBadge status={view.status} /></dd>
            </div>
            <div>
              <dt>{t('mail.attempts')}</dt>
              <dd>
                {t('mail.attemptsOf', {
                  attempts: new Intl.NumberFormat(locale).format(view.attempts),
                  maxAttempts: new Intl.NumberFormat(locale).format(view.max_attempts),
                })}
              </dd>
            </div>
            <div>
              <dt>{t('mail.created')}</dt>
              <dd className="is-mono">{view.created_at}</dd>
            </div>
            <div>
              <dt>{t('mail.sent')}</dt>
              <dd className="is-mono">{view.sent_at ?? t('mail.notSent')}</dd>
            </div>
            <div>
              <dt>{t('mail.fieldDelivered')}</dt>
              <dd className="is-mono">{view.delivered_at ?? t('mail.notDelivered')}</dd>
            </div>
            {view.provider_id ? (
              <div>
                <dt>{t('mail.fieldProviderId')}</dt>
                <dd className="is-mono">{view.provider_id}</dd>
              </div>
            ) : null}
            {view.aggregate_type ? (
              <div>
                <dt>{t('mail.fieldAggregate')}</dt>
                <dd className="is-mono">
                  {view.aggregate_type} / {view.aggregate_id ?? '—'}
                </dd>
              </div>
            ) : null}
            <div>
              <dt>{t('mail.fieldMessageId')}</dt>
              <dd className="is-mono">{view.id}</dd>
            </div>
          </dl>

          {view.last_error ? (
            <div className="panel-error" style={{ display: 'block', marginTop: 16 }}>
              <strong>{t('mail.lastError')}</strong>
              <p
                style={{
                  margin: '2px 0 0',
                  color: 'var(--c-text-2)',
                  fontFamily: 'var(--font-mono)',
                  fontSize: 'var(--fs-11)',
                  overflowWrap: 'anywhere',
                }}
              >
                {view.last_error}
              </p>
            </div>
          ) : null}

          <section aria-labelledby="mail-events-heading" style={{ marginTop: 18 }}>
            <h3 id="mail-events-heading" className="panel-card-title" style={{ fontSize: 'var(--fs-13)' }}>
              {t('mail.eventsTitle')}
            </h3>
            {error ? (
              <div style={{ marginTop: 8 }}>
                <ErrorState message={error.message} code={error.code} onRetry={reload} />
              </div>
            ) : null}
            {loading && !data ? <Loading label={t('mail.loadingMessage')} /> : null}
            {data && events.length === 0 ? (
              <p className="admin-table-hint" style={{ marginTop: 6 }}>
                {t('mail.eventsEmpty')}
              </p>
            ) : null}
            {events.length > 0 ? (
              <ul className="list" style={{ margin: 0, padding: 0, listStyle: 'none' }}>
                {events.map((event) => (
                  <li key={event.id} className="list-row" style={{ alignItems: 'flex-start' }}>
                    <div style={{ minWidth: 0, flex: 1 }}>
                      <span
                        className="list-title"
                        style={{ fontFamily: 'var(--font-mono)', fontSize: 'var(--fs-12)', overflowWrap: 'anywhere' }}
                      >
                        {event.type}
                      </span>
                      <span className="list-sub">{shortStamp(event.received_at, locale)}</span>
                    </div>
                  </li>
                ))}
              </ul>
            ) : null}
          </section>

          <section aria-labelledby="mail-body-heading" style={{ marginTop: 18 }}>
            <h3 id="mail-body-heading" className="panel-card-title" style={{ fontSize: 'var(--fs-13)' }}>
              {t('mail.renderedTitle')}
            </h3>
            {data ? (
              data.html ? (
                <iframe
                  title={t('mail.renderedFrameTitle', { subject: view.subject || t('mail.noSubject') })}
                  sandbox=""
                  srcDoc={data.html}
                  style={{
                    width: '100%',
                    height: 320,
                    marginTop: 8,
                    border: 0,
                    borderRadius: 'var(--r-tile)',
                    background: 'var(--c-inset)',
                  }}
                />
              ) : (
                <p className="admin-table-hint" style={{ marginTop: 6 }}>
                  {t('mail.noHtml')}
                </p>
              )
            ) : null}
            {data && data.text ? (
              <details style={{ marginTop: 10 }}>
                <summary style={{ cursor: 'pointer', color: 'var(--c-text-2)', fontSize: 'var(--fs-12)' }}>
                  {t('mail.plainText')}
                </summary>
                <pre
                  style={{
                    margin: '8px 0 0',
                    padding: 12,
                    borderRadius: 'var(--r-tile)',
                    background: 'var(--c-inset)',
                    color: 'var(--c-text-2)',
                    fontFamily: 'var(--font-mono)',
                    fontSize: 'var(--fs-11)',
                    whiteSpace: 'pre-wrap',
                    overflowWrap: 'anywhere',
                  }}
                >
                  {data.text}
                </pre>
              </details>
            ) : null}
          </section>
        </div>

        <footer className="admin-drawer-foot">
          <button type="button" className="btn btn-secondary" onClick={onClose}>
            {t('common.close')}
          </button>
          <button
            type="button"
            className="btn btn-primary"
            onClick={onResend}
            disabled={resending || loading}
          >
            <Icon name="lucide:rotate-ccw" size={15} />
            {resending ? t('mail.queueing') : t('mail.resend')}
          </button>
        </footer>
      </div>
    </div>
  );
}

export default function MailPage() {
  const { t, locale } = useLocale();
  const [status, setStatus] = useState('');
  const [query, setQuery] = useState('');
  const [debouncedQuery, setDebouncedQuery] = useState('');
  const [selected, setSelected] = useState<MailItem | null>(null);

  // Search is debounced so a keystroke does not become a request.
  useEffect(() => {
    const timer = window.setTimeout(() => setDebouncedQuery(query.trim()), 300);
    return () => window.clearTimeout(timer);
  }, [query]);

  const fetchStats = useCallback(() => getMailStats(), []);
  const stats = useAsyncData<MailStats>(fetchStats);

  const fetchMessages = useCallback(
    () =>
      listMail({
        status: status || undefined,
        q: debouncedQuery || undefined,
        limit: PAGE_SIZE,
      }),
    [status, debouncedQuery],
  );
  const messages = useAsyncData(fetchMessages);

  const fetchSuppressions = useCallback(() => listMailSuppressions(), []);
  const suppressions = useAsyncData(fetchSuppressions);

  // Cursor pagination: the first page belongs to `messages`, the rest is
  // appended here and dropped whenever the first page changes.
  const [moreItems, setMoreItems] = useState<MailItem[]>([]);
  const [moreCursor, setMoreCursor] = useState<string | null>(null);
  const [moreLoading, setMoreLoading] = useState(false);
  const [moreError, setMoreError] = useState<string | null>(null);

  useEffect(() => {
    setMoreItems([]);
    setMoreCursor(null);
    setMoreError(null);
  }, [messages.data]);

  const items = useMemo(
    () => [...(messages.data?.items ?? []), ...moreItems],
    [messages.data, moreItems],
  );
  const nextCursor = moreItems.length > 0 ? moreCursor : messages.data?.next_cursor ?? null;

  const loadMore = async () => {
    if (!nextCursor) return;
    setMoreLoading(true);
    setMoreError(null);
    try {
      const page = await listMail({
        status: status || undefined,
        q: debouncedQuery || undefined,
        limit: PAGE_SIZE,
        cursor: nextCursor,
      });
      setMoreItems((previous) => [...previous, ...page.items]);
      setMoreCursor(page.next_cursor);
    } catch (err) {
      setMoreError(err instanceof Error ? err.message : t('mail.loadMoreFailed'));
    } finally {
      setMoreLoading(false);
    }
  };

  // Suppressions: an explicit two-step confirm, no browser modal.
  const [confirmEmail, setConfirmEmail] = useState<string | null>(null);
  const [removingEmail, setRemovingEmail] = useState<string | null>(null);
  const [suppressionError, setSuppressionError] = useState<string | null>(null);

  const onRemoveSuppression = async (email: string) => {
    setRemovingEmail(email);
    setSuppressionError(null);
    try {
      await removeMailSuppression(email);
      setConfirmEmail(null);
      suppressions.reload();
    } catch (err) {
      setSuppressionError(err instanceof ApiRequestError ? err.message : t('mail.removeFailed'));
    } finally {
      setRemovingEmail(null);
    }
  };

  const reloadAll = () => {
    stats.reload();
    messages.reload();
    suppressions.reload();
  };

  const filtered = Boolean(status || debouncedQuery);
  const clearFilters = () => {
    setStatus('');
    setQuery('');
    setDebouncedQuery('');
  };

  const pageError = messages.error ?? stats.error ?? suppressions.error;
  const statsAvailable = Boolean(stats.data);
  const suppressionItems: MailSuppression[] = suppressions.data?.items ?? [];

  return (
    <div>
      <PageHeading
        title={t('mail.title')}
        action={(
          <button
            type="button"
            className="btn btn-secondary"
            onClick={reloadAll}
            disabled={messages.loading || stats.loading}
          >
            <Icon name="lucide:refresh-cw" size={15} />
            {messages.loading ? t('common.refreshing') : t('common.refresh')}
          </button>
        )}
      />

      {IS_DEMO ? (
        <p className="admin-page-note">
          <strong>{t('mail.demoTitle')}</strong> {t('mail.demoBody')}
        </p>
      ) : null}

      {pageError ? (
        <div style={{ marginBottom: 16 }}>
          <ErrorState message={pageError.message} code={pageError.code} onRetry={reloadAll} />
        </div>
      ) : null}

      {stats.loading && !stats.data ? (
        <SkeletonTiles />
      ) : (
        <section className="stat-grid" aria-label={t('mail.statsLabel')}>
          <MailKpi
            label={t('mail.kpiQueued')}
            icon="lucide:hourglass"
            value={stats.data?.queued}
            available={statsAvailable}
          />
          <MailKpi
            label={t('mail.kpiDelivered')}
            icon="lucide:circle-check"
            value={stats.data?.delivered}
            available={statsAvailable}
          />
          <MailKpi
            label={t('mail.kpiBounced')}
            icon="lucide:triangle-alert"
            value={stats.data?.bounced}
            available={statsAvailable}
          />
          <MailKpi
            label={t('mail.kpiFailed')}
            icon="lucide:circle-alert"
            value={stats.data?.failed}
            available={statsAvailable}
          />
        </section>
      )}

      {messages.loading && !messages.data ? (
        <div className="panel-section">
          <SkeletonRows rows={6} cols={4} />
        </div>
      ) : (
        <section
          className="panel-card panel-section"
          aria-labelledby="mail-messages-heading"
        >
          <header className="panel-card-head">
            <div>
              <h2 id="mail-messages-heading" className="panel-card-title">
                {t('mail.messagesTitle')}
              </h2>
              <p className="panel-card-sub">
                {t('mail.messagesSubtitle')}
              </p>
            </div>
            <div className="panel-card-tools">
              {messages.data ? (
                <span className="chip chip-neutral">
                  {t(filtered ? 'mail.countMatches' : 'mail.countMessages', {
                    count: new Intl.NumberFormat(locale).format(items.length),
                  })}
                </span>
              ) : (
                <span className="chip chip-neutral">{t('common.unavailable')}</span>
              )}
            </div>
          </header>

          <div style={{ display: 'flex', flexWrap: 'wrap', alignItems: 'flex-end', gap: 12, marginBottom: 16 }}>
            <div style={{ flex: '0 1 200px', minWidth: 160 }}>
              <Select
                id="mail-status"
                label={t('mail.statusLabel')}
                value={status}
                onChange={setStatus}
                options={[
                  { value: '', label: t('mail.allStatuses') },
                  ...MAIL_STATUSES.map((value) => {
                    const key = STATUS_MESSAGE_KEY[value];
                    return { value, label: key ? t(key) : value.replaceAll('_', ' ') };
                  }),
                ]}
              />
            </div>
            <div style={{ flex: '1 1 260px', maxWidth: 380, minWidth: 0 }}>
              <label htmlFor="mail-search" className="field-label">
                {t('mail.searchLabel')}
              </label>
              <input
                id="mail-search"
                type="search"
                className="field-input"
                value={query}
                onChange={(event) => setQuery(event.target.value)}
                placeholder={t('mail.searchPlaceholder')}
                aria-label={t('mail.searchAria')}
              />
            </div>
            {filtered ? (
              <button type="button" className="btn btn-quiet" onClick={clearFilters}>
                {t('mail.clearFilters')}
              </button>
            ) : null}
          </div>

          {messages.error ? (
            <p className="admin-table-hint" style={{ margin: 0 }}>
              {t('mail.listUnavailable')}
            </p>
          ) : null}

          {!messages.error && messages.data && items.length === 0 ? (
            <div className="panel-empty" role="status">
              <strong>{t(filtered ? 'mail.emptyFilteredTitle' : 'mail.emptyTitle')}</strong>
              <span>
                {t(filtered ? 'mail.emptyFilteredBody' : 'mail.emptyBody')}
              </span>
              <button
                type="button"
                className="btn btn-secondary btn-sm"
                style={{ marginTop: 10 }}
                onClick={filtered ? clearFilters : reloadAll}
              >
                <Icon name="lucide:refresh-cw" size={14} />
                {filtered ? t('mail.clearFilters') : t('common.refresh')}
              </button>
            </div>
          ) : null}

          {items.length > 0 ? (
            <div className="table-wrap">
              <table className="data-table" aria-label={t('mail.tableLabel')} style={{ minWidth: 860 }}>
                <thead>
                  <tr>
                    <th scope="col">{t('mail.created')}</th>
                    <th scope="col">{t('mail.colTo')}</th>
                    <th scope="col">{t('mail.colSubject')}</th>
                    <th scope="col">{t('mail.template')}</th>
                    <th scope="col">{t('mail.statusLabel')}</th>
                    <th scope="col" className="is-num">{t('mail.attempts')}</th>
                    <th scope="col">{t('mail.colOpen')}</th>
                  </tr>
                </thead>
                <tbody>
                  {items.map((message) => (
                    <tr
                      key={message.id}
                      onClick={() => setSelected(message)}
                      style={{ cursor: 'pointer' }}
                    >
                      <td className="is-mono is-muted" style={{ whiteSpace: 'nowrap' }}>
                        {shortStamp(message.created_at, locale)}
                      </td>
                      <td style={{ overflowWrap: 'anywhere' }}>{message.to}</td>
                      <td className="is-strong" style={{ overflowWrap: 'anywhere' }}>
                        {message.subject || t('mail.noSubject')}
                      </td>
                      <td className="is-mono is-muted">{message.template}</td>
                      <td>
                        <StatusBadge status={message.status} />
                      </td>
                      <td className="is-num">
                        {message.attempts}/{message.max_attempts}
                      </td>
                      <td className="is-num">
                        <button
                          type="button"
                          className="panel-icon-button"
                          aria-label={t('mail.openMessage', { subject: message.subject || message.id })}
                          onClick={(event) => {
                            event.stopPropagation();
                            setSelected(message);
                          }}
                        >
                          <Icon name="lucide:chevron-right" size={16} />
                        </button>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          ) : null}

          {moreError ? (
            <p className="panel-error" role="alert" style={{ marginTop: 12 }}>
              {moreError}
            </p>
          ) : null}

          {nextCursor ? (
            <div style={{ display: 'flex', justifyContent: 'center', marginTop: 16 }}>
              <button
                type="button"
                className="btn btn-secondary"
                onClick={loadMore}
                disabled={moreLoading}
              >
                {moreLoading ? t('common.loading') : t('mail.loadMore')}
              </button>
            </div>
          ) : null}
        </section>
      )}

      <section
        className="panel-card panel-section"
        aria-labelledby="mail-suppressions-heading"
      >
        <header className="panel-card-head">
          <div>
            <h2 id="mail-suppressions-heading" className="panel-card-title">
              {t('mail.suppressionsTitle')}
            </h2>
            <p className="panel-card-sub">
              {t('mail.suppressionsSubtitle')}
            </p>
          </div>
          <div className="panel-card-tools">
            {suppressions.data ? (
              <span className="chip chip-neutral">
                {t('mail.suppressionCount', { count: new Intl.NumberFormat(locale).format(suppressionItems.length) })}
              </span>
            ) : (
              <span className="chip chip-neutral">{t('common.unavailable')}</span>
            )}
          </div>
        </header>

        {suppressionError ? (
          <p className="panel-error" role="alert" style={{ marginBottom: 12 }}>
            {suppressionError}
          </p>
        ) : null}

        {suppressions.loading && !suppressions.data ? (
          <p className="admin-table-hint" role="status" style={{ margin: 0 }}>
            {t('mail.loadingSuppressions')}
          </p>
        ) : null}

        {suppressions.error ? (
          <p className="admin-table-hint" style={{ margin: 0 }}>
            {t('mail.suppressionsUnavailable')}
          </p>
        ) : null}

        {suppressions.data && suppressionItems.length === 0 ? (
          <div className="panel-empty" role="status">
            <strong>{t('mail.suppressionsEmptyTitle')}</strong>
            <span>{t('mail.suppressionsEmptyBody')}</span>
          </div>
        ) : null}

        {suppressionItems.length > 0 ? (
          <ul className="list" style={{ margin: 0, padding: 0, listStyle: 'none' }}>
            {suppressionItems.map((entry) => (
              <li key={entry.email} className="list-row" style={{ alignItems: 'flex-start', flexWrap: 'wrap' }}>
                <div style={{ minWidth: 0, flex: 1 }}>
                  <span className="list-title" style={{ overflowWrap: 'anywhere' }}>
                    {entry.email}
                  </span>
                  <span className="list-sub">
                    {entry.reason}
                    {entry.detail ? ` · ${entry.detail}` : ''} · {shortStamp(entry.created_at, locale)}
                  </span>
                </div>

                <div style={{ display: 'flex', flexWrap: 'wrap', alignItems: 'center', justifyContent: 'flex-end', gap: 8 }}>
                  {confirmEmail === entry.email ? (
                    <>
                      <span style={{ color: 'var(--c-text-2)', fontSize: 'var(--fs-12)' }}>
                        {t('mail.confirmRemove')}
                      </span>
                      <button
                        type="button"
                        className="btn btn-secondary btn-sm"
                        onClick={() => setConfirmEmail(null)}
                        disabled={removingEmail === entry.email}
                      >
                        {t('common.cancel')}
                      </button>
                    </>
                  ) : null}
                  {/* Suppressing an address is a destructive row action, so it is
                      the same control as every other one in the panels: a shut
                      bin that opens on approach.

                      It is also the confirmation button, exactly as the roster
                      row does it: "Remove" asks, "Yes, remove" acts. The label
                      changes with the state, so an open confirmation never puts
                      two buttons saying the same word side by side. Keeping one
                      control also keeps `aria-expanded` meaningful — the panel
                      is tied to the trigger that opened it, and the trigger is
                      still there to close it again. */}
                  <TrashAction
                    label={confirmEmail === entry.email ? t('mail.confirmRemoveAction') : t('mail.remove')}
                    detail={entry.email}
                    expanded={confirmEmail === entry.email}
                    disabled={removingEmail === entry.email}
                    onClick={() => {
                      setSuppressionError(null);
                      if (confirmEmail === entry.email) {
                        void onRemoveSuppression(entry.email);
                        return;
                      }
                      setConfirmEmail(entry.email);
                    }}
                  />
                </div>
              </li>
            ))}
          </ul>
        ) : null}
      </section>

      {selected ? (
        <MailDetailDrawer
          item={selected}
          onClose={() => setSelected(null)}
          onResent={() => messages.reload()}
        />
      ) : null}
    </div>
  );
}
