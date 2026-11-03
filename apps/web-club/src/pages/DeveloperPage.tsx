/**
 * Developers — the club's public API v1 surface: scoped keys, the reference
 * generated from the deployment's own OpenAPI document, and the long-form
 * guides served by the same binary.
 *
 * Why one page and not three routes: a key is created next to the documentation
 * that explains it (ADR-010 §8). The tabs are views of one subject, so the head
 * stays the same and the URL does not change — nothing here is a wizard.
 *
 * Data honesty (docs/ui/PANEL-DESIGN.md §Data honesty):
 *   - every list is a server fact with a loading state, an error with its code
 *     and a retry, and an empty state that says what the absence means;
 *   - no secret is ever kept after its one reveal, and no value is invented for
 *     a key the API did not return;
 *   - the reference and the guides are read from the deployment, so an empty
 *     document is reported as a contract problem rather than as "nothing here".
 */
import { Fragment, useCallback, useEffect, useMemo, useState, type FormEvent } from 'react';
import {
  ApiRequestError,
  NotImplementedStubError,
} from '../api/client';
import {
  createApiKey,
  deleteApiKey,
  getApiKeyUsage,
  getScopeCatalogue,
  listApiKeys,
  rotateApiKey,
  type ApiKeyEnvironment,
  type ApiKeyView,
  type CreateApiKeyInput,
  type CreatedApiKey,
  type KeyUsage,
  type RotatedApiKey,
  type ScopeCatalogue,
  type ScopeInfo,
  type ScopePreset,
} from '../api/developer';
import { useAsyncData } from '../hooks/useAsyncData';
import { useLocale, type Translate } from '../locale';
import PageHeading from '../components/PageHeading';
import Icon from '../components/Icon';
import { Modal } from '../modal';
import Select from '../components/ui/Select';
import Chip, { type ChipTone } from '../components/club/Chip';
import {
  EmptyState,
  ErrorState,
  Panel,
  Skeleton,
  ToastRegion,
  useToasts,
} from '../components/club';
import ReferenceTab from '../components/developers/ReferenceTab';
import GuidesTab from '../components/developers/GuidesTab';
import './developers.css';

const LABEL_CLASS = 'field-label';
const HINT_CLASS = 'mt-1 text-[length:var(--fs-11)] leading-[1.45] text-[color:var(--c-text-muted-sm)]';
const ERROR_CLASS = 'text-[length:var(--fs-13)] leading-[1.5] text-[color:var(--c-error)]';

/** The API's own bounds (ADR-010 §5). The server validates; the UI states them. */
const MIN_RATE_LIMIT = 30;
const MAX_RATE_LIMIT = 600;
const MAX_EXPIRY_DAYS = 365;
/** Rotation grace values offered by the contract: 0 / 24 / 48 / 72 h. */
const GRACE_HOURS = [0, 24, 48, 72] as const;

type Tab = 'keys' | 'reference' | 'guides';

const TAB_ORDER: Tab[] = ['keys', 'reference', 'guides'];

type SecretReveal =
  | { mode: 'created'; key: string; view: ApiKeyView }
  | { mode: 'rotated'; key: string; view: RotatedApiKey };

interface CreateDraft {
  name: string;
  presetId: string;
  scopes: string[];
  environment: ApiKeyEnvironment;
  expiresInDays: string;
  rateLimit: string;
  cidrs: string;
}

const EMPTY_DRAFT: CreateDraft = {
  name: '',
  presetId: '',
  scopes: [],
  environment: 'live',
  expiresInDays: '',
  rateLimit: '',
  cidrs: '',
};

/* --- formatting ------------------------------------------------------------ */

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
    return t('developers.failHttp', { action, status, message });
  }
  if (err instanceof NotImplementedStubError) {
    return t('developers.failNotConnected', { action });
  }
  return t('developers.failOffline', { action });
}

/** `active` / `expired` / `revoked` — the word the row prints, not a second enum. */
function keyStatusWord(key: ApiKeyView, t: Translate): { label: string; tone: ChipTone } {
  if (key.status === 'revoked') return { label: t('developers.statusRevoked'), tone: 'error' };
  if (key.expired) return { label: t('developers.statusExpired'), tone: 'warn' };
  return { label: t('developers.statusActive'), tone: 'live' };
}

/* --- copy ------------------------------------------------------------------ */

/**
 * One clipboard helper for the page. Returns the failure message instead of
 * throwing so each control can print its own aria-live feedback: a panel that
 * silently fails to copy a secret is worse than one that says it could not.
 */
async function copyText(text: string, t: Translate): Promise<string | null> {
  if (!navigator.clipboard) return t('developers.copyBlocked');
  try {
    await navigator.clipboard.writeText(text);
    return null;
  } catch {
    return t('developers.copyFailed');
  }
}

/* --- page ------------------------------------------------------------------ */

export default function DeveloperPage() {
  const { t } = useLocale();
  const { toasts, push, dismiss } = useToasts();
  const [tab, setTab] = useState<Tab>('keys');
  const [guideTarget, setGuideTarget] = useState<string | undefined>(undefined);

  const openTab = (next: Tab, section?: string) => {
    setTab(next);
    if (next === 'guides') setGuideTarget(section);
  };

  return (
    <div className="club-workspace-content">
      <PageHeading title={t('developers.heading')} subtitle={t('developers.subtitle')} />

      <div className="seg dev-tabs" role="tablist" aria-label={t('developers.tabsLabel')}>
        {TAB_ORDER.map((value) => (
          <button
            key={value}
            type="button"
            role="tab"
            id={`dev-tab-${value}`}
            aria-selected={tab === value}
            aria-controls={`dev-panel-${value}`}
            tabIndex={tab === value ? 0 : -1}
            className={`seg-item ${tab === value ? 'is-active' : ''}`.trim()}
            onClick={() => setTab(value)}
            onKeyDown={(event) => {
              const index = TAB_ORDER.indexOf(value);
              if (event.key === 'ArrowRight' || event.key === 'ArrowDown') {
                event.preventDefault();
                const next = TAB_ORDER[(index + 1) % TAB_ORDER.length] ?? 'keys';
                setTab(next);
                document.getElementById(`dev-tab-${next}`)?.focus();
              } else if (event.key === 'ArrowLeft' || event.key === 'ArrowUp') {
                event.preventDefault();
                const next = TAB_ORDER[(index - 1 + TAB_ORDER.length) % TAB_ORDER.length] ?? 'keys';
                setTab(next);
                document.getElementById(`dev-tab-${next}`)?.focus();
              } else if (event.key === 'Home') {
                event.preventDefault();
                setTab('keys');
                document.getElementById('dev-tab-keys')?.focus();
              } else if (event.key === 'End') {
                event.preventDefault();
                setTab('guides');
                document.getElementById('dev-tab-guides')?.focus();
              }
            }}
          >
            {value === 'keys'
              ? t('developers.tabKeys')
              : value === 'reference'
                ? t('developers.tabReference')
                : t('developers.tabGuides')}
          </button>
        ))}
      </div>

      <div id={`dev-panel-${tab}`} role="tabpanel" aria-labelledby={`dev-tab-${tab}`}>
        {tab === 'keys' ? <KeysTab onOpenGuides={(section) => openTab('guides', section)} push={push} /> : null}
        {tab === 'reference' ? <ReferenceTab /> : null}
        {tab === 'guides' ? <GuidesTab target={guideTarget} /> : null}
      </div>

      <ToastRegion toasts={toasts} onDismiss={dismiss} />
    </div>
  );
}

/* --- keys tab -------------------------------------------------------------- */

type PushToast = ReturnType<typeof useToasts>['push'];

function KeysTab({ onOpenGuides, push }: { onOpenGuides: (section?: string) => void; push: PushToast }) {
  const { t, locale } = useLocale();

  const fetchKeys = useCallback(() => listApiKeys(), []);
  const { data, loading, error, reload, failedAt, staleSince } = useAsyncData(fetchKeys);
  const fetchCatalogue = useCallback(() => getScopeCatalogue(), []);
  const catalogue = useAsyncData<ScopeCatalogue>(fetchCatalogue);

  const keys = data?.items ?? [];
  const scopes = catalogue.data?.scopes ?? [];
  const presets = catalogue.data?.presets ?? [];

  const [createOpen, setCreateOpen] = useState(false);
  const [draft, setDraft] = useState<CreateDraft>(EMPTY_DRAFT);
  const [creating, setCreating] = useState(false);
  const [createError, setCreateError] = useState<string | null>(null);
  const [reveal, setReveal] = useState<SecretReveal | null>(null);
  const [rotateTarget, setRotateTarget] = useState<ApiKeyView | null>(null);
  const [revokeTarget, setRevokeTarget] = useState<ApiKeyView | null>(null);
  const [usageId, setUsageId] = useState<string | null>(null);
  const [expandedId, setExpandedId] = useState<string | null>(null);
  const [copied, setCopied] = useState<string | null>(null);
  const [copyError, setCopyError] = useState<string | null>(null);

  // "Copied" is feedback for one control, not a page state.
  useEffect(() => {
    if (!copied) return undefined;
    const timer = window.setTimeout(() => setCopied(null), 4000);
    return () => window.clearTimeout(timer);
  }, [copied]);

  const copy = async (id: string, text: string) => {
    const problem = await copyText(text, t);
    setCopyError(problem);
    setCopied(problem ? null : id);
  };

  const openCreate = () => {
    setDraft(EMPTY_DRAFT);
    setCreateError(null);
    setCreateOpen(true);
  };

  const toggleScope = (id: string) => {
    setDraft((current) => {
      const next = current.scopes.includes(id)
        ? current.scopes.filter((scope) => scope !== id)
        : [...current.scopes, id];
      // Any manual change makes the preset no longer describe the selection.
      return { ...current, scopes: next, presetId: 'custom' };
    });
  };

  const applyPreset = (presetId: string) => {
    if (presetId === 'custom') {
      setDraft((current) => ({ ...current, presetId }));
      return;
    }
    const preset = presets.find((entry) => entry.id === presetId);
    setDraft((current) => ({ ...current, presetId, scopes: preset ? [...preset.scopes] : current.scopes }));
  };

  const submitCreate = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (creating) return;
    setCreateError(null);

    const name = draft.name.trim();
    if (!name) {
      setCreateError(t('developers.nameRequired'));
      return;
    }
    if (draft.scopes.length === 0) {
      setCreateError(t('developers.scopesRequired'));
      return;
    }
    const expiry = draft.expiresInDays.trim();
    if (expiry !== '' && (!Number.isInteger(Number(expiry)) || Number(expiry) < 1 || Number(expiry) > MAX_EXPIRY_DAYS)) {
      setCreateError(t('developers.expiryInvalid'));
      return;
    }
    const rate = draft.rateLimit.trim();
    if (rate !== '' && (!Number.isInteger(Number(rate)) || Number(rate) < MIN_RATE_LIMIT || Number(rate) > MAX_RATE_LIMIT)) {
      setCreateError(t('developers.rateInvalid'));
      return;
    }
    const cidrs = draft.cidrs
      .split('\n')
      .map((line) => line.trim())
      .filter(Boolean);
    // Conservative on purpose: only obviously malformed rules are caught here.
    // The server owns validation of an address; blocking a valid one here would
    // be worse than letting the API answer 422.
    const suspect = cidrs.find((rule) => /\s/.test(rule) || rule.split('/').length > 2);
    if (suspect) {
      setCreateError(t('developers.cidrInvalid', { value: suspect }));
      return;
    }

    const input: CreateApiKeyInput = {
      name,
      scopes: draft.scopes,
      environment: draft.environment,
      allowed_cidrs: cidrs,
    };
    if (expiry !== '') input.expires_in_days = Number(expiry);
    if (rate !== '') input.rate_limit_per_min = Number(rate);

    setCreating(true);
    try {
      const created: CreatedApiKey = await createApiKey(input);
      setCreateOpen(false);
      setReveal({ mode: 'created', key: created.key, view: created });
      push({
        tone: 'live',
        title: t('developers.createKey'),
        message: created.prefix,
      });
      reload();
    } catch (err) {
      setCreateError(`${t('developers.createFailed')} ${describeFailure(err, t('developers.actionCreate'), t)}`);
    } finally {
      setCreating(false);
    }
  };

  // Both dialogs keep their own failure: a rotate that failed must not close,
  // because the operator would then believe a new secret exists.
  const submitRotate = async (target: ApiKeyView, graceHours: number): Promise<string | null> => {
    try {
      const rotated = await rotateApiKey(target.id, graceHours);
      setRotateTarget(null);
      setReveal({ mode: 'rotated', key: rotated.key, view: rotated });
      push({ tone: 'live', title: t('developers.rotate'), message: target.name });
      reload();
      return null;
    } catch (err) {
      return `${t('developers.rotateFailed')} ${describeFailure(err, t('developers.actionRotate', { name: target.name }), t)}`;
    }
  };

  const confirmRevoke = async (target: ApiKeyView): Promise<string | null> => {
    try {
      await deleteApiKey(target.id);
      setRevokeTarget(null);
      if (usageId === target.id) setUsageId(null);
      push({ tone: 'live', title: t('developers.revoke'), message: target.name });
      reload();
      return null;
    } catch (err) {
      return `${t('developers.revokeFailed')} ${describeFailure(err, t('developers.actionRevoke', { name: target.name }), t)}`;
    }
  };

  const firstLoad = loading && data === null;
  const pollFailed = failedAt !== null && data !== null;
  const usageKey = keys.find((key) => key.id === usageId) ?? null;

  return (
    <>
      {error && !error.notImplemented ? (
        <ErrorState
          title={t('developers.errorTitle')}
          message={`${error.message}. ${t('developers.errorTail')}`}
          code={error.code}
          onRetry={reload}
        />
      ) : null}

      {error?.notImplemented ? (
        <EmptyState
          title={t('developers.errorTitle')}
          detail={t('developers.failNotConnected', { action: t('developers.actionCreate') })}
        />
      ) : null}

      {!error ? (
        <Panel
          className="dev-panel"
          title={t('developers.keysPanel')}
          detail={t('developers.keysPanelDetail')}
          action={(
            <div className="dev-toolbar">
              <button type="button" className="btn btn-secondary" onClick={reload} disabled={loading}>
                {loading && data !== null ? t('common.refreshing') : t('common.refresh')}
              </button>
              <button
                type="button"
                className="btn btn-primary"
                onClick={openCreate}
                disabled={catalogue.loading && catalogue.data === null}
              >
                {t('developers.createKey')}
              </button>
            </div>
          )}
        >
          {pollFailed ? (
            <p className={`mb-3 ${ERROR_CLASS}`} role="alert">
              {staleSince !== null
                ? t('developers.failOffline', { action: t('developers.actionLoadUsage', { name: t('developers.keysPanel') }) })
                : t('developers.errorTail')}
            </p>
          ) : null}

          {firstLoad ? (
            <div className="flex flex-col gap-3" role="status" aria-label={t('developers.loading')}>
              <Skeleton height={44} />
              <Skeleton height={44} />
              <Skeleton height={44} width="80%" />
            </div>
          ) : null}

          {!firstLoad && keys.length === 0 ? (
            <EmptyState
              title={t('developers.emptyTitle')}
              detail={t('developers.emptyDetail')}
              action={(
                <button type="button" className="btn btn-primary" onClick={openCreate}>
                  {t('developers.createKey')}
                </button>
              )}
            />
          ) : null}

          {!firstLoad && keys.length > 0 ? (
            <div className="table-wrap">
              <table className="data-table dev-keys-table data-table--stack">
                <caption className="sr-only">{t('developers.tableCaption')}</caption>
                <thead>
                  <tr>
                    <th scope="col">{t('developers.colName')}</th>
                    <th scope="col">{t('developers.colPrefix')}</th>
                    <th scope="col">{t('developers.colEnv')}</th>
                    <th scope="col">{t('developers.colScopes')}</th>
                    <th scope="col">{t('developers.colStatus')}</th>
                    <th scope="col">{t('developers.colLastUsed')}</th>
                    <th scope="col"><span className="sr-only">{t('developers.colActions')}</span></th>
                  </tr>
                </thead>
                <tbody>
                  {keys.map((key) => {
                    const status = keyStatusWord(key, t);
                    const expanded = expandedId === key.id;
                    const lastUsed = formatDate(key.last_used_at, locale);
                    const expiry = formatDate(key.expires_at, locale);
                    return (
                      <Fragment key={key.id}>
                      <tr className={expanded ? 'is-open' : undefined}>
                        <td className="is-strong cell-primary">
                          <span className="dev-key-name" title={key.name}>{key.name}</span>
                          <span className="dev-cell-sub">
                            {expiry
                              ? t('developers.expiresAt', { date: expiry })
                              : t('developers.neverExpires')}
                          </span>
                        </td>
                        <td data-label={t('developers.colPrefix')}>
                          <span className="dev-prefix-row">
                            <span className="dev-prefix">{key.prefix}</span>
                            <button
                              type="button"
                              className="panel-icon-button dev-copy-prefix"
                              aria-label={t('developers.copyPrefix', { name: key.name })}
                              title={t('developers.copyPrefix', { name: key.name })}
                              onClick={() => { void copy(key.id, key.prefix); }}
                            >
                              <Icon name={copied === key.id ? 'lucide:check' : 'lucide:copy'} size={14} />
                            </button>
                          </span>
                        </td>
                        <td data-label={t('developers.colEnv')}>
                          <Chip tone={key.environment === 'test' ? 'warn' : 'neutral'}>
                            {key.environment === 'test' ? t('developers.envTest') : t('developers.envLive')}
                          </Chip>
                        </td>
                        <td data-label={t('developers.colScopes')}>
                          <button
                            type="button"
                            className="btn btn-quiet btn-sm dev-scopes-toggle"
                            aria-expanded={expanded}
                            aria-label={expanded ? t('developers.hideScopes') : t('developers.showScopes')}
                            onClick={() => setExpandedId(expanded ? null : key.id)}
                          >
                            <span>{t('developers.scopesCount', { count: key.scopes.length })}</span>
                            <Icon name={expanded ? 'lucide:chevron-up' : 'lucide:chevron-down'} size={13} />
                          </button>
                        </td>
                        <td data-label={t('developers.colStatus')}>
                          <Chip tone={status.tone}>{status.label}</Chip>
                        </td>
                        <td data-label={t('developers.colLastUsed')} className="is-muted">
                          {lastUsed ?? t('developers.neverUsed')}
                        </td>
                        <td data-label="" className="cell-actions">
                          {/* Actions are icon buttons with a full accessible name:
                              the column is the last thing a narrow table has room
                              for, and three word labels there pushed the row past
                              the viewport and clipped the last one. */}
                          <div className="dev-row-actions">
                            <button
                              type="button"
                              className="panel-icon-button"
                              aria-expanded={usageId === key.id}
                              aria-label={t('developers.usageOf', { name: key.name })}
                              title={t('developers.usage')}
                              onClick={() => setUsageId(usageId === key.id ? null : key.id)}
                            >
                              <Icon name="lucide:chart-column" size={15} />
                            </button>
                            <button
                              type="button"
                              className="panel-icon-button"
                              disabled={key.status !== 'active'}
                              aria-label={t('developers.rotateOf', { name: key.name })}
                              title={t('developers.rotate')}
                              onClick={() => setRotateTarget(key)}
                            >
                              <Icon name="lucide:refresh-cw" size={15} />
                            </button>
                            <button
                              type="button"
                              className="panel-icon-button dev-icon-danger"
                              disabled={key.status === 'revoked'}
                              aria-label={t('developers.revokeOf', { name: key.name })}
                              title={t('developers.revoke')}
                              onClick={() => setRevokeTarget(key)}
                            >
                              <Icon name="lucide:power" size={15} />
                            </button>
                          </div>
                        </td>
                      </tr>
                      {expanded ? (
                        /* The chips live in their own row: the scopes column is
                           one narrow token wide, and six scope ids inside it
                           would stack into a column of pills taller than the
                           row it belongs to. */
                        <tr className="dev-keys-scope-row">
                          <td colSpan={7} data-label="">
                            <span className="dev-scope-chips">
                              {key.scopes.map((scope) => (
                                <span key={scope} className="chip chip-neutral dev-scope-chip">{scope}</span>
                              ))}
                            </span>
                          </td>
                        </tr>
                      ) : null}
                      </Fragment>
                    );
                  })}
                </tbody>
              </table>
            </div>
          ) : null}

          {copyError ? (
            <p className={`mt-3 ${ERROR_CLASS}`} role="alert">{copyError}</p>
          ) : null}
        </Panel>
      ) : null}

      {usageKey ? (
        <div className="mt-4">
          <UsagePanel
            key={usageKey.id}
            apiKey={usageKey}
            onClose={() => setUsageId(null)}
          />
        </div>
      ) : (
        !error && keys.length > 0 ? <p className="dev-hint">{t('developers.usageSelect')}</p> : null
      )}

      {createOpen ? (
        <CreateKeyDialog
          draft={draft}
          setDraft={setDraft}
          scopes={scopes}
          presets={presets}
          catalogueError={catalogue.error}
          onRetryCatalogue={catalogue.reload}
          onToggleScope={toggleScope}
          onApplyPreset={applyPreset}
          onCreate={submitCreate}
          onClose={() => setCreateOpen(false)}
          creating={creating}
          error={createError}
        />
      ) : null}

      {reveal ? (
        <SecretDialog
          reveal={reveal}
          onClose={() => setReveal(null)}
          onOpenDocs={() => { setReveal(null); onOpenGuides('quickstart'); }}
        />
      ) : null}

      {rotateTarget ? (
        <RotateDialog
          apiKey={rotateTarget}
          onRotate={submitRotate}
          onClose={() => setRotateTarget(null)}
        />
      ) : null}

      {revokeTarget ? (
        <RevokeDialog
          apiKey={revokeTarget}
          onConfirm={confirmRevoke}
          onClose={() => setRevokeTarget(null)}
        />
      ) : null}
    </>
  );
}

/* --- create dialog --------------------------------------------------------- */

function CreateKeyDialog({
  draft,
  setDraft,
  scopes,
  presets,
  catalogueError,
  onRetryCatalogue,
  onToggleScope,
  onApplyPreset,
  onCreate,
  onClose,
  creating,
  error,
}: {
  draft: CreateDraft;
  setDraft: (next: CreateDraft) => void;
  scopes: ScopeInfo[];
  presets: ScopePreset[];
  catalogueError: { message: string; code?: string } | null;
  onRetryCatalogue: () => void;
  onToggleScope: (id: string) => void;
  onApplyPreset: (id: string) => void;
  onCreate: (event: FormEvent<HTMLFormElement>) => void;
  onClose: () => void;
  creating: boolean;
  error: string | null;
}) {
  const { t } = useLocale();

  // Checkbox groups keep the catalogue's own order and group names — the panel
  // never invents a scope or a group the server did not describe.
  const groups = useMemo(() => {
    const map = new Map<string, ScopeInfo[]>();
    for (const scope of scopes) {
      const list = map.get(scope.group) ?? [];
      list.push(scope);
      map.set(scope.group, list);
    }
    return [...map.entries()];
  }, [scopes]);

  const riskLabel = (risk: string) => {
    if (risk === 'write') return t('developers.riskWrite');
    if (risk === 'sensitive') return t('developers.riskSensitive');
    return t('developers.riskRead');
  };

  return (
    <Modal
      title={t('developers.createTitle')}
      description={t('developers.createDescription')}
      closeLabel={t('common.close')}
      onClose={onClose}
      size="wide"
      footer={(
        <>
          <button type="button" className="btn btn-secondary" onClick={onClose} disabled={creating}>
            {t('common.cancel')}
          </button>
          <span className="panel-modal-foot-spacer" />
          <button type="submit" form="dev-create-form" className="btn btn-primary" disabled={creating}>
            {creating ? t('developers.creating') : t('developers.createAction')}
          </button>
        </>
      )}
    >
      <form id="dev-create-form" onSubmit={onCreate} noValidate className="dev-form">
        <div>
          <label htmlFor="dev-key-name" className={LABEL_CLASS}>{t('developers.nameLabel')}</label>
          <input
            id="dev-key-name"
            type="text"
            className="field-input"
            autoComplete="off"
            maxLength={64}
            placeholder={t('developers.namePlaceholder')}
            value={draft.name}
            onChange={(event) => setDraft({ ...draft, name: event.target.value })}
            aria-describedby="dev-key-name-hint"
          />
          <p id="dev-key-name-hint" className={HINT_CLASS}>{t('developers.nameHint')}</p>
        </div>

        <div>
          <Select
            id="dev-key-preset"
            label={t('developers.presetLabel')}
            value={draft.presetId}
            onChange={onApplyPreset}
            placeholder={t('developers.presetCustom')}
            options={[
              { value: '', label: t('developers.presetCustom') },
              ...presets.map((preset) => ({
                value: preset.id,
                label: preset.title,
                description: preset.description,
              })),
            ]}
          />
          <p className={HINT_CLASS}>{t('developers.presetHint')}</p>
        </div>

        <fieldset className="dev-scopes">
          <legend className="field-label">{t('developers.scopesLabel')}</legend>
          <p className={HINT_CLASS}>{t('developers.scopesHint')}</p>
          {/* The checklist vocabulary is the API contract's own English
              (ids like `requests:read` are protocol, not copy). Saying so once
              keeps the panel from reading as an unfinished translation. */}
          {groups.length > 0 ? (
            <p className={`dev-scope-origin ${HINT_CLASS}`}>{t('developers.scopesFromApi')}</p>
          ) : null}

          {catalogueError ? (
            <div className="mt-2">
              <ErrorState
                message={catalogueError.message}
                code={catalogueError.code}
                onRetry={onRetryCatalogue}
              />
            </div>
          ) : null}

          {!catalogueError && groups.length === 0 ? (
            <p className={`mt-2 ${HINT_CLASS}`}>{t('developers.scopesHint')}</p>
          ) : null}

          {groups.map(([group, entries]) => (
            <div key={group} className="dev-scope-group">
              <p className="dev-scope-group-title">{group}</p>
              <div className="dev-scope-options">
                {entries.map((scope) => {
                  const checked = draft.scopes.includes(scope.id);
                  const inputId = `dev-scope-${scope.id.replace(/[^a-z0-9]+/gi, '-')}`;
                  return (
                    <label key={scope.id} className={`dev-scope-option ${checked ? 'is-checked' : ''}`.trim()} htmlFor={inputId}>
                      <input
                        id={inputId}
                        type="checkbox"
                        checked={checked}
                        onChange={() => onToggleScope(scope.id)}
                      />
                      <span className="dev-scope-copy">
                        <span className="dev-scope-title">{scope.title}</span>
                        <span className="dev-scope-id">{scope.id}</span>
                        <span className="dev-scope-desc">{scope.description}</span>
                      </span>
                      <span className={`chip dev-scope-risk ${scope.risk === 'sensitive' ? 'chip-warn' : 'chip-neutral'}`}>
                        {riskLabel(scope.risk)}
                      </span>
                    </label>
                  );
                })}
              </div>
            </div>
          ))}
        </fieldset>

        <details className="dev-advanced">
          <summary className="dev-advanced-summary">{t('developers.advancedTitle')}</summary>
          <div className="dev-advanced-body">
            <div>
              <Select
                id="dev-key-env"
                label={t('developers.environmentLabel')}
                value={draft.environment}
                onChange={(value) => setDraft({ ...draft, environment: value === 'test' ? 'test' : 'live' })}
                options={[
                  { value: 'live', label: t('developers.envLiveOption') },
                  { value: 'test', label: t('developers.envTestOption') },
                ]}
              />
              <p className={HINT_CLASS}>{t('developers.envHint')}</p>
            </div>

            <div>
              <label htmlFor="dev-key-expiry" className={LABEL_CLASS}>{t('developers.expiryLabel')}</label>
              <input
                id="dev-key-expiry"
                type="number"
                inputMode="numeric"
                min={1}
                max={MAX_EXPIRY_DAYS}
                step={1}
                className="field-input"
                placeholder={t('developers.expiryPlaceholder')}
                value={draft.expiresInDays}
                onChange={(event) => setDraft({ ...draft, expiresInDays: event.target.value })}
                aria-describedby="dev-key-expiry-hint"
              />
              <p id="dev-key-expiry-hint" className={HINT_CLASS}>{t('developers.expiryHint')}</p>
            </div>

            <div>
              <label htmlFor="dev-key-rate" className={LABEL_CLASS}>{t('developers.rateLimitLabel')}</label>
              <input
                id="dev-key-rate"
                type="number"
                inputMode="numeric"
                min={MIN_RATE_LIMIT}
                max={MAX_RATE_LIMIT}
                step={1}
                className="field-input"
                value={draft.rateLimit}
                onChange={(event) => setDraft({ ...draft, rateLimit: event.target.value })}
                aria-describedby="dev-key-rate-hint"
              />
              <p id="dev-key-rate-hint" className={HINT_CLASS}>{t('developers.rateLimitHint')}</p>
            </div>

            <div className="dev-advanced-wide">
              <label htmlFor="dev-key-cidrs" className={LABEL_CLASS}>{t('developers.cidrsLabel')}</label>
              <textarea
                id="dev-key-cidrs"
                className="field-input"
                rows={3}
                spellCheck={false}
                placeholder={t('developers.cidrsPlaceholder')}
                value={draft.cidrs}
                onChange={(event) => setDraft({ ...draft, cidrs: event.target.value })}
                aria-describedby="dev-key-cidrs-hint"
              />
              <p id="dev-key-cidrs-hint" className={HINT_CLASS}>{t('developers.cidrsHint')}</p>
            </div>
          </div>
        </details>

        {error ? <p className={ERROR_CLASS} role="alert">{error}</p> : null}
      </form>
    </Modal>
  );
}

/* --- secret dialog --------------------------------------------------------- */

function SecretDialog({
  reveal,
  onClose,
  onOpenDocs,
}: {
  reveal: SecretReveal;
  onClose: () => void;
  onOpenDocs: () => void;
}) {
  const { t } = useLocale();
  const [copied, setCopied] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const copy = async () => {
    const problem = await copyText(reveal.key, t);
    setError(problem);
    setCopied(problem === null);
  };

  return (
    <Modal
      title={t('developers.secretTitle')}
      description={t('developers.secretDescription')}
      closeLabel={t('common.close')}
      onClose={onClose}
      footer={(
        <>
          <button type="button" className="btn btn-secondary" onClick={onOpenDocs}>
            {t('developers.secretDocs')}
          </button>
          <span className="panel-modal-foot-spacer" />
          <button type="button" className="btn btn-primary" onClick={onClose}>
            {t('developers.secretDone')}
          </button>
        </>
      )}
    >
      <div>
        <p className={LABEL_CLASS}>{t('developers.secretLabel')}</p>
        <div className="dev-secret">
          <code className="dev-secret-value">{reveal.key}</code>
          <button type="button" className="btn btn-secondary btn-sm" onClick={() => { void copy(); }}>
            {copied ? t('developers.copied') : t('developers.copy')}
          </button>
        </div>
        <p className="panel-note mt-3" role="note">{t('developers.secretWarning')}</p>
        <p className={HINT_CLASS}>{t('developers.secretPrefixNote', { prefix: reveal.view.prefix })}</p>
        {error ? <p className={`mt-2 ${ERROR_CLASS}`} role="alert">{error}</p> : null}
      </div>
    </Modal>
  );
}

/* --- rotate dialog --------------------------------------------------------- */

function RotateDialog({
  apiKey,
  onRotate,
  onClose,
}: {
  apiKey: ApiKeyView;
  onRotate: (target: ApiKeyView, graceHours: number) => Promise<string | null>;
  onClose: () => void;
}) {
  const { t } = useLocale();
  const [grace, setGrace] = useState('24');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const graceOptions = GRACE_HOURS.map((hours) => ({
    value: String(hours),
    label: hours === 0
      ? t('developers.graceNone')
      : t('developers.graceOption', { hours }),
  }));

  const rotate = async () => {
    if (busy) return;
    setError(null);
    setBusy(true);
    try {
      const problem = await onRotate(apiKey, Number(grace));
      if (problem) setError(problem);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal
      title={t('developers.rotateTitle', { name: apiKey.name })}
      description={t('developers.rotateDescription')}
      closeLabel={t('common.close')}
      onClose={onClose}
      footer={(
        <>
          <button type="button" className="btn btn-secondary" onClick={onClose} disabled={busy}>
            {t('common.cancel')}
          </button>
          <span className="panel-modal-foot-spacer" />
          <button type="button" className="btn btn-primary" onClick={() => { void rotate(); }} disabled={busy}>
            {busy ? t('developers.rotating') : t('developers.rotateAction')}
          </button>
        </>
      )}
    >
      <div>
        <Select
          id="dev-rotate-grace"
          label={t('developers.graceLabel')}
          value={grace}
          onChange={setGrace}
          options={graceOptions}
        />
      </div>
      {error ? <p className={ERROR_CLASS} role="alert">{error}</p> : null}
    </Modal>
  );
}

/* --- revoke dialog --------------------------------------------------------- */

function RevokeDialog({
  apiKey,
  onConfirm,
  onClose,
}: {
  apiKey: ApiKeyView;
  onConfirm: (target: ApiKeyView) => Promise<string | null>;
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
      const problem = await onConfirm(apiKey);
      if (problem) setError(problem);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal
      title={t('developers.revokeTitle', { name: apiKey.name })}
      description={t('developers.revokeDescription')}
      closeLabel={t('common.close')}
      onClose={onClose}
      footer={(
        <>
          <button type="button" className="btn btn-secondary" onClick={onClose} disabled={busy}>
            {t('common.cancel')}
          </button>
          <span className="panel-modal-foot-spacer" />
          <button type="button" className="btn btn-danger" onClick={() => { void confirm(); }} disabled={busy}>
            {busy ? t('developers.revoking') : t('developers.revokeAction')}
          </button>
        </>
      )}
    >
      <p className="dev-dialog-body">{apiKey.prefix}</p>
      {error ? <p className={ERROR_CLASS} role="alert">{error}</p> : null}
    </Modal>
  );
}

/* --- usage panel ----------------------------------------------------------- */

function UsagePanel({ apiKey, onClose }: { apiKey: ApiKeyView; onClose: () => void }) {
  const { t, locale } = useLocale();
  const fetchUsage = useCallback(() => getApiKeyUsage(apiKey.id), [apiKey.id]);
  const { data, loading, error, reload } = useAsyncData<KeyUsage>(fetchUsage);

  const lastCall = data ? formatDate(data.summary.last_call_at, locale) : null;

  return (
    <Panel
      className="dev-panel"
      title={t('developers.usageTitle', { name: apiKey.name })}
      detail={t('developers.usageDetail')}
      action={(
        <div className="dev-toolbar">
          <button type="button" className="btn btn-secondary btn-sm" onClick={reload} disabled={loading}>
            {loading ? t('common.refreshing') : t('common.refresh')}
          </button>
          <button type="button" className="btn btn-quiet btn-sm" onClick={onClose}>
            {t('common.close')}
          </button>
        </div>
      )}
    >
      {loading && !data ? (
        <div className="flex flex-col gap-3" role="status" aria-label={t('developers.usageLoading')}>
          <Skeleton height={36} />
          <Skeleton height={36} width="70%" />
        </div>
      ) : null}

      {error ? (
        <ErrorState
          title={t('developers.usageFailed')}
          message={error.message}
          code={error.code}
          onRetry={reload}
        />
      ) : null}

      {data ? (
        <>
          <dl className="dev-summary">
            <div>
              <dt>{t('developers.usageCalls24h')}</dt>
              <dd>{data.summary.calls_24h}</dd>
            </div>
            <div>
              <dt>{t('developers.usageErrors24h')}</dt>
              <dd>{data.summary.errors_24h}</dd>
            </div>
            <div>
              <dt>{t('developers.usageCalls30d')}</dt>
              <dd>{data.summary.calls_30d}</dd>
            </div>
            <div>
              <dt>{t('developers.usageLastCall')}</dt>
              <dd className="dev-summary-date">{lastCall ?? t('developers.neverUsed')}</dd>
            </div>
          </dl>

          {data.items.length === 0 ? (
            <EmptyState title={t('developers.usageEmpty')} detail={t('developers.usageDetail')} />
          ) : (
            <div className="table-wrap">
              <table className="data-table dev-usage-table data-table--stack">
                <caption className="sr-only">{t('developers.usageTitle', { name: apiKey.name })}</caption>
                <thead>
                  <tr>
                    <th scope="col">{t('developers.usageColAt')}</th>
                    <th scope="col">{t('developers.usageColMethod')}</th>
                    <th scope="col">{t('developers.usageColPath')}</th>
                    <th scope="col" className="is-num">{t('developers.usageColStatus')}</th>
                  </tr>
                </thead>
                <tbody>
                  {data.items.map((entry, index) => (
                    <tr key={`${entry.at}-${index}`}>
                      <td className="is-muted" data-label={t('developers.usageColAt')}>
                        {formatDate(entry.at, locale) ?? entry.at}
                      </td>
                      <td data-label={t('developers.usageColMethod')}>
                        <span className="dev-method">{entry.method}</span>
                      </td>
                      <td className="is-mono" data-label={t('developers.usageColPath')}>{entry.path}</td>
                      <td className="is-num" data-label={t('developers.usageColStatus')}>{entry.status}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </>
      ) : null}
    </Panel>
  );
}