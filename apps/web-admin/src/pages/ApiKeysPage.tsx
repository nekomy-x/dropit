/**
 * API keys — platform-wide oversight of the developer keys (ADR-010 §5).
 *
 * Read-only except revocation: an operator can see every club's keys and stop
 * one immediately, but cannot create, rename or widen anything, and no admin
 * endpoint returns a secret. That asymmetry is the screen's shape:
 *
 *   - the table is the platform's whole key list, with the owning club first,
 *     because "whose key is this?" is the question this screen exists for;
 *   - filters are local (name, prefix, club, status) — the API returns the list
 *     in one page and a filter that re-queried per keystroke would hide rows
 *     while claiming to search them;
 *   - revocation is the one mutation, it is irreversible, and the dialog names
 *     the club, the key and the prefix before confirming.
 *
 * Data honesty: a failed read shows one `panel-error` with its code and a retry
 * and no rows; the count is only printed when the list was actually read.
 */
import { useCallback, useMemo, useState } from 'react';
import {
  getAdminApiKeyUsage,
  listAdminApiKeys,
  revokeAdminApiKey,
  type AdminApiKeyView,
  type KeyUsage,
} from '../api/developer';
import { ApiRequestError, NotImplementedStubError } from '../api/client';
import { useAsyncData } from '../hooks/useAsyncData';
import { useLocale, type Translate } from '../locale';
import PageHeading from '../components/PageHeading';
import ErrorState from '../components/ErrorState';
import Loading from '../components/Loading';
import { Modal } from '../modal';
import Select from '../components/ui/Select';
import Icon from '../components/Icon';
import './api-keys.css';

const DATE_CACHE = new Map<string, Intl.DateTimeFormat>();

function formatDate(value: string | null, locale: string): string | null {
  if (!value) return null;
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return null;
  let formatter = DATE_CACHE.get(locale);
  if (!formatter) {
    formatter = new Intl.DateTimeFormat(locale, { dateStyle: 'medium', timeStyle: 'short' });
    DATE_CACHE.set(locale, formatter);
  }
  return formatter.format(date);
}

function describeFailure(err: unknown, action: string, t: Translate): string {
  if (err instanceof ApiRequestError) {
    const status = `HTTP ${err.status}${err.code ? ` · ${err.code}` : ''}`;
    const message = err.message.replace(/\.$/, '');
    return t('apiKeys.failHttp', { action, status, message });
  }
  if (err instanceof NotImplementedStubError) {
    return t('apiKeys.failOffline', { action });
  }
  return t('apiKeys.failOffline', { action });
}

type StatusFilter = '' | 'active' | 'expired' | 'revoked';

/** The three words the table prints. `expired` is derived on the server. */
function statusKey(key: AdminApiKeyView): Exclude<StatusFilter, ''> {
  if (key.status === 'revoked') return 'revoked';
  if (key.expired) return 'expired';
  return 'active';
}

function statusTone(status: Exclude<StatusFilter, ''>): string {
  if (status === 'revoked') return 'chip-error';
  if (status === 'expired') return 'chip-warn';
  return 'chip-accent';
}

export default function ApiKeysPage() {
  const { t, locale } = useLocale();
  const fetchKeys = useCallback(() => listAdminApiKeys(), []);
  const { data, loading, error, reload } = useAsyncData(fetchKeys);

  const [query, setQuery] = useState('');
  const [status, setStatus] = useState<StatusFilter>('');
  const [revokeTarget, setRevokeTarget] = useState<AdminApiKeyView | null>(null);
  const [usageTarget, setUsageTarget] = useState<AdminApiKeyView | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const items = data?.items ?? [];
  const filtered = useMemo(() => {
    const needle = query.trim().toLowerCase();
    return items.filter((key) => {
      if (status !== '' && statusKey(key) !== status) return false;
      if (needle === '') return true;
      return `${key.name} ${key.prefix} ${key.club_name}`.toLowerCase().includes(needle);
    });
  }, [items, query, status]);

  const isFiltered = query.trim() !== '' || status !== '';

  const clearFilters = () => {
    setQuery('');
    setStatus('');
  };

  // The dialog owns the failure: it stays open, prints why, and the operator
  // can retry or cancel. A failed revoke that silently closed would leave the
  // key alive while the screen looked like it had worked.
  const revoke = async (target: AdminApiKeyView) => {
    try {
      await revokeAdminApiKey(target.id);
    } catch (err) {
      throw new Error(describeFailure(err, t('apiKeys.actionRevoke', { name: target.name }), t));
    }
    setRevokeTarget(null);
    setNotice(t('apiKeys.revokedMessage', { name: target.name, club: target.club_name }));
    reload();
  };

  return (
    <div>
      <PageHeading
        title={t('apiKeys.title')}
        subtitle={t('apiKeys.subtitle')}
        action={(
          <button type="button" className="btn btn-secondary" onClick={reload} disabled={loading}>
            <Icon name="lucide:refresh-cw" size={15} />
            {loading ? t('common.refreshing') : t('common.refresh')}
          </button>
        )}
      />

      {error ? (
        <div className="ak-error">
          <ErrorState
            message={`${t('apiKeys.errorTitle')}. ${error.message}. ${t('apiKeys.errorTail')}`}
            code={error.code}
            onRetry={reload}
          />
        </div>
      ) : null}
      {loading && !data ? <Loading label={t('apiKeys.loading')} /> : null}

      {data ? (
        <section className="panel-card panel-section" aria-labelledby="api-keys-heading">
          <header className="panel-card-head">
            <div>
              <h2 id="api-keys-heading" className="panel-card-title">{t('apiKeys.panelTitle')}</h2>
              <p className="panel-card-sub">{t('apiKeys.panelDetail')}</p>
            </div>
            <div className="panel-card-tools">
              <span className="chip chip-neutral">
                {t(isFiltered ? 'apiKeys.countMatches' : 'apiKeys.countKeys', { count: filtered.length })}
              </span>
            </div>
          </header>

          <div className="ak-filters">
            <div className="ak-filter-select">
              <Select
                id="api-keys-status"
                label={t('apiKeys.statusLabel')}
                value={status}
                onChange={(value) => setStatus(value as StatusFilter)}
                options={[
                  { value: '', label: t('apiKeys.allStatuses') },
                  { value: 'active', label: t('apiKeys.statusActive') },
                  { value: 'expired', label: t('apiKeys.statusExpired') },
                  { value: 'revoked', label: t('apiKeys.statusRevoked') },
                ]}
              />
            </div>
            <div className="ak-filter-search">
              <label htmlFor="api-keys-search" className="field-label">{t('apiKeys.searchLabel')}</label>
              <input
                id="api-keys-search"
                type="search"
                className="field-input"
                value={query}
                onChange={(event) => setQuery(event.target.value)}
                placeholder={t('apiKeys.searchPlaceholder')}
                aria-label={t('apiKeys.searchAria')}
              />
            </div>
            {isFiltered ? (
              <button type="button" className="btn btn-quiet" onClick={clearFilters}>
                {t('apiKeys.clearFilters')}
              </button>
            ) : null}
          </div>

          {notice ? (
            <p className="ak-notice" role="status">
              <strong>{t('apiKeys.revokedTitle')}</strong> {notice}
            </p>
          ) : null}

          {items.length === 0 ? (
            <div className="panel-empty" role="status">
              <strong>{t('apiKeys.emptyTitle')}</strong>
              <span>{t('apiKeys.emptyDetail')}</span>
            </div>
          ) : null}

          {items.length > 0 && filtered.length === 0 ? (
            <div className="panel-empty" role="status">
              <strong>{t('apiKeys.emptyFilteredTitle')}</strong>
              <span>{t('apiKeys.emptyFilteredDetail')}</span>
              <button type="button" className="btn btn-secondary btn-sm" style={{ marginTop: 10 }} onClick={clearFilters}>
                {t('apiKeys.clearFilters')}
              </button>
            </div>
          ) : null}

          {filtered.length > 0 ? (
            <div className="table-wrap">
              <table
                className="data-table data-table--stack ak-table"
                aria-label={t('apiKeys.tableCaption')}
              >
                <thead>
                  <tr>
                    <th scope="col">{t('apiKeys.colClub')}</th>
                    <th scope="col">{t('apiKeys.colName')}</th>
                    <th scope="col">{t('apiKeys.colPrefix')}</th>
                    <th scope="col">{t('apiKeys.colEnv')}</th>
                    <th scope="col">{t('apiKeys.colScopes')}</th>
                    <th scope="col">{t('apiKeys.colStatus')}</th>
                    <th scope="col">{t('apiKeys.colLastUsed')}</th>
                    <th scope="col">{t('apiKeys.colCreated')}</th>
                    <th scope="col"><span className="sr-only">{t('apiKeys.colActions')}</span></th>
                  </tr>
                </thead>
                <tbody>
                  {filtered.map((key) => {
                    const word = statusKey(key);
                    const created = formatDate(key.created_at, locale);
                    const lastUsed = formatDate(key.last_used_at, locale);
                    const expiry = formatDate(key.expires_at, locale);
                    return (
                      <tr key={key.id}>
                        <td data-label="" className="cell-primary">{key.club_name || key.club_id}</td>
                        <td data-label={t('apiKeys.colName')}>{key.name}</td>
                        <td className="is-mono" data-label={t('apiKeys.colPrefix')}>{key.prefix}</td>
                        <td data-label={t('apiKeys.colEnv')}>
                          <span className={`chip ${key.environment === 'test' ? 'chip-warn' : 'chip-neutral'}`}>
                            {key.environment === 'test' ? t('apiKeys.envTest') : t('apiKeys.envLive')}
                          </span>
                        </td>
                        <td
                          data-label={t('apiKeys.colScopes')}
                          title={t('apiKeys.scopesLabel', { scopes: key.scopes.join(', ') || '—' })}
                        >
                          {t('apiKeys.scopesCount', { count: key.scopes.length })}
                        </td>
                        <td data-label={t('apiKeys.colStatus')}>
                          <span className={`chip ${statusTone(word)}`}>
                            {word === 'revoked'
                              ? t('apiKeys.statusRevoked')
                              : word === 'expired'
                                ? t('apiKeys.statusExpired')
                                : t('apiKeys.statusActive')}
                          </span>
                          <span className="ak-cell-sub">
                            {expiry ? t('apiKeys.expiresAt', { date: expiry }) : t('apiKeys.noExpiry')}
                          </span>
                        </td>
                        <td className="is-muted" data-label={t('apiKeys.colLastUsed')}>
                          {lastUsed ?? t('apiKeys.neverUsed')}
                        </td>
                        <td className="is-muted" data-label={t('apiKeys.colCreated')}>
                          {created ?? t('common.unavailable')}
                        </td>
                        <td data-label="" className="cell-actions">
                          <div className="ak-row-actions">
                            <button
                              type="button"
                              className="btn btn-quiet btn-sm"
                              aria-label={t('apiKeys.openUsage', { name: key.name })}
                              onClick={() => setUsageTarget(key)}
                            >
                              {t('apiKeys.usage')}
                            </button>
                            <button
                              type="button"
                              className="btn btn-danger btn-sm"
                              disabled={key.status === 'revoked'}
                              onClick={() => setRevokeTarget(key)}
                            >
                              {t('apiKeys.revoke')}
                            </button>
                          </div>
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>
          ) : null}
        </section>
      ) : null}

      {revokeTarget ? (
        <RevokeDialog
          apiKey={revokeTarget}
          onConfirm={revoke}
          onClose={() => setRevokeTarget(null)}
        />
      ) : null}

      {usageTarget ? (
        <UsageDialog apiKey={usageTarget} onClose={() => setUsageTarget(null)} />
      ) : null}
    </div>
  );
}

function RevokeDialog({
  apiKey,
  onConfirm,
  onClose,
}: {
  apiKey: AdminApiKeyView;
  onConfirm: (target: AdminApiKeyView) => Promise<void>;
  onClose: () => void;
}) {
  const { t } = useLocale();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const confirm = async () => {
    if (busy) return;
    setError(null);
    setBusy(true);
    try {
      await onConfirm(apiKey);
    } catch (err) {
      const detail = err instanceof Error ? err.message : '';
      setError(`${t('apiKeys.revokeFailed')} ${detail}`.trim());
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal
      title={t('apiKeys.revokeTitle', { club: apiKey.club_name })}
      description={t('apiKeys.revokeDescription', { name: apiKey.name, prefix: apiKey.prefix })}
      closeLabel={t('apiKeys.close')}
      onClose={onClose}
      footer={(
        <>
          <button type="button" className="btn btn-secondary" onClick={onClose} disabled={busy}>
            {t('apiKeys.cancel')}
          </button>
          <span className="panel-modal-foot-spacer" />
          <button type="button" className="btn btn-danger" onClick={() => { void confirm(); }} disabled={busy}>
            {busy ? t('apiKeys.revoking') : t('apiKeys.revokeAction')}
          </button>
        </>
      )}
    >
      <p className="ak-dialog-body">{apiKey.prefix}</p>
      {error ? <p className="panel-error" role="alert">{error}</p> : null}
    </Modal>
  );
}

function UsageDialog({ apiKey, onClose }: { apiKey: AdminApiKeyView; onClose: () => void }) {
  const { t, locale } = useLocale();
  const fetchUsage = useCallback(() => getAdminApiKeyUsage(apiKey.id), [apiKey.id]);
  const { data, loading, error, reload } = useAsyncData<KeyUsage>(fetchUsage);
  const lastCall = data ? formatDate(data.summary.last_call_at, locale) : null;

  return (
    <Modal
      title={t('apiKeys.usageTitle', { name: apiKey.name })}
      description={t('apiKeys.usageDetail')}
      closeLabel={t('apiKeys.close')}
      onClose={onClose}
      size="wide"
      footer={(
        <>
          <button type="button" className="btn btn-secondary" onClick={reload} disabled={loading}>
            {t('common.refresh')}
          </button>
          <span className="panel-modal-foot-spacer" />
          <button type="button" className="btn btn-secondary" onClick={onClose}>
            {t('apiKeys.close')}
          </button>
        </>
      )}
    >
      {loading && !data ? (
        <div className="ak-skeleton" role="status" aria-label={t('apiKeys.usageLoading')}>
          <span className="skeleton" style={{ height: 36 }} />
          <span className="skeleton" style={{ height: 36, width: '70%' }} />
        </div>
      ) : null}

      {error ? (
        <ErrorState
          message={`${t('apiKeys.usageFailed')} ${error.message}`}
          code={error.code}
          onRetry={reload}
        />
      ) : null}

      {data ? (
        <>
          <dl className="ak-summary">
            <div>
              <dt>{t('apiKeys.usageCalls24h')}</dt>
              <dd>{data.summary.calls_24h}</dd>
            </div>
            <div>
              <dt>{t('apiKeys.usageErrors24h')}</dt>
              <dd>{data.summary.errors_24h}</dd>
            </div>
            <div>
              <dt>{t('apiKeys.usageCalls30d')}</dt>
              <dd>{data.summary.calls_30d}</dd>
            </div>
            <div>
              <dt>{t('apiKeys.usageLastCall')}</dt>
              <dd className="ak-summary-date">{lastCall ?? t('apiKeys.neverUsed')}</dd>
            </div>
          </dl>

          {data.items.length === 0 ? (
            <div className="panel-empty" role="status">
              <strong>{t('apiKeys.usageEmpty')}</strong>
              <span>{t('apiKeys.usageDetail')}</span>
            </div>
          ) : (
            <div className="table-wrap">
              <table
                className="data-table data-table--stack"
                aria-label={t('apiKeys.usageTitle', { name: apiKey.name })}
              >
                <thead>
                  <tr>
                    <th scope="col">{t('apiKeys.usageColAt')}</th>
                    <th scope="col">{t('apiKeys.usageColMethod')}</th>
                    <th scope="col">{t('apiKeys.usageColPath')}</th>
                    <th scope="col">{t('apiKeys.usageColStatus')}</th>
                  </tr>
                </thead>
                <tbody>
                  {data.items.map((entry, index) => (
                    <tr key={`${entry.at}-${index}`}>
                      <td data-label="" className="cell-primary is-muted">{formatDate(entry.at, locale) ?? entry.at}</td>
                      <td data-label={t('apiKeys.usageColMethod')} className="is-mono">{entry.method}</td>
                      <td data-label={t('apiKeys.usageColPath')} className="is-mono ak-path">{entry.path}</td>
                      <td data-label={t('apiKeys.usageColStatus')} className="is-num">{entry.status}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </>
      ) : null}
    </Modal>
  );
}