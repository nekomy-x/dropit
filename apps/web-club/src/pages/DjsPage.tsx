import { Fragment, useCallback, useEffect, useRef, useState, type FormEvent } from 'react';
import {
  ApiRequestError,
  getDjs,
  inviteDj,
  NotImplementedStubError,
  patchClubDj,
  removeClubDj,
  restoreClubDj,
  type Dj,
  type DjInviteRequest,
  type DjInviteResponse,
} from '../api/client';
import { useAsyncData } from '../hooks/useAsyncData';
import { useLocale, type Translate } from '../locale';
import PageHeading from '../components/PageHeading';
import StatusBadge from '../components/StatusBadge';
import TrashAction from '../components/ui/TrashAction';
import { useBoardClock } from '../components/club/board/useBoardClock';
import {
  EmptyState,
  ErrorState,
  Panel,
  Skeleton,
  Tile,
  ToastRegion,
  useToasts,
} from '../components/club';
import './djs.css';

const LABEL_CLASS = 'mb-1 block text-[length:var(--fs-12)] font-medium text-[color:var(--c-text-2)]';
const NOTE_CLASS = 'text-[length:var(--fs-12)] leading-[1.5] text-[color:var(--c-text-2)]';
const HINT_CLASS = 'text-[length:var(--fs-11)] leading-[1.45] text-[color:var(--c-text-muted-sm)]';
const ERROR_CLASS = 'text-[length:var(--fs-13)] leading-[1.5] text-[color:var(--c-error)]';

/** How long the copy confirmation stays before the button resets. */
const COPIED_RESET_MS = 4000;
/**
 * Presence polling. The DJ console writes a heartbeat every 30 s and the server
 * treats a heartbeat younger than 90 s as online, so a 20 s read keeps the
 * roster within one heartbeat of a change without hammering the API.
 */
const PRESENCE_POLL_MS = 20_000;
/** A club note is a working reminder, not a document. */
const NOTE_MAX_LENGTH = 500;
/** A note cell shows one line; the full text is in the `title` attribute. */
const NOTE_PREVIEW_LENGTH = 64;

type AccessTarget = 'active' | 'suspended';

/** The row that is expanded into its detail form — one at a time. */
type RowPanel =
  | { kind: 'note'; djId: string; draft: string }
  | { kind: 'access'; djId: string; next: AccessTarget }
  | { kind: 'remove'; djId: string };

function formatExpiry(value: string, locale: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return new Intl.DateTimeFormat(locale, { dateStyle: 'medium', timeStyle: 'short' }).format(date);
}

/**
 * "3 min" from a server-computed age in seconds. Never invents a timestamp.
 *
 * The unit is a bare quantity: the sentence around it owns the preposition, so
 * "seen 3 min ago" / "visto hace 3 min" / "vor 3 Minuten" all read correctly.
 * The singular/plural pair is chosen here because most of the six languages
 * inflect the unit after the number.
 */
function formatAge(seconds: number, t: Translate): string {
  const safe = Math.max(0, seconds);
  if (safe < 60) {
    const value = Math.max(1, Math.round(safe));
    return t(value === 1 ? 'djs.ageSecond' : 'djs.ageSeconds', { count: value });
  }
  const minutes = Math.round(safe / 60);
  if (minutes < 60) return t(minutes === 1 ? 'djs.ageMinute' : 'djs.ageMinutes', { count: minutes });
  const hours = Math.round(minutes / 60);
  if (hours < 24) return t(hours === 1 ? 'djs.ageHour' : 'djs.ageHours', { count: hours });
  const days = Math.round(hours / 24);
  return t(days === 1 ? 'djs.ageDay' : 'djs.ageDays', { count: days });
}

function describeFailure(err: unknown, action: string, t: Translate): string {
  if (err instanceof ApiRequestError) {
    const status = `HTTP ${err.status}${err.code ? ` · ${err.code}` : ''}`;
    const message = err.message.replace(/\.$/, '');
    return t('djs.failHttp', { action, status, message });
  }
  if (err instanceof NotImplementedStubError) {
    return t('djs.failNotConnected', { action });
  }
  return t('djs.failOffline', { action });
}

/**
 * DJs — the club's roster as a management table.
 *
 * Columns follow what staff scan under time pressure: who → access → presence →
 * shift → note → actions. Presence and shift are server facts
 * (`GET /v1/club/djs`): `online` is a DJ-console heartbeat inside the server's
 * TTL, `on_shift` is an unclosed session assigned to that DJ. The list
 * re-reads every 20 s, so presence changes without a manual refresh; a failed
 * poll keeps the rows but says how old they are.
 *
 * The three actions mean different things and stay separate:
 *   * access — suspend/restore; the DJ stays on the roster;
 *   * note — the club's own working reminder, invisible to the DJ and guests;
 *   * remove — off the roster; shift and payout history is kept, and the DJ
 *     can be restored from "Show removed".
 */
export default function DjsPage() {
  const { t, locale } = useLocale();
  const { toasts, push, dismiss } = useToasts();
  const now = useBoardClock();

  const [includeRemoved, setIncludeRemoved] = useState(false);
  const fetchDjs = useCallback(() => getDjs(includeRemoved), [includeRemoved]);
  const { data, loading, error, reload, failedAt, staleSince } =
    useAsyncData(fetchDjs, PRESENCE_POLL_MS);

  const [invite, setInvite] = useState<DjInviteRequest>({ email: '', displayName: '' });
  const [inviting, setInviting] = useState(false);
  const [inviteError, setInviteError] = useState<string | null>(null);
  const [inviteLink, setInviteLink] = useState<DjInviteResponse | null>(null);
  const [copied, setCopied] = useState(false);
  const [copyError, setCopyError] = useState<string | null>(null);

  const [rowPanel, setRowPanel] = useState<RowPanel | null>(null);
  const [busyDjId, setBusyDjId] = useState<string | null>(null);
  const [rowError, setRowError] = useState<string | null>(null);

  // Focus contract for the expanded row: the form or its confirm button takes
  // focus, Escape closes it, and closing returns focus to the row trigger.
  const triggerRef = useRef<HTMLElement | null>(null);
  const primaryRef = useRef<HTMLTextAreaElement | HTMLButtonElement | null>(null);
  const wasOpen = useRef(false);

  useEffect(() => {
    if (rowPanel) {
      wasOpen.current = true;
      primaryRef.current?.focus();
    } else if (wasOpen.current) {
      wasOpen.current = false;
      triggerRef.current?.focus();
    }
  }, [rowPanel]);

  useEffect(() => {
    if (!rowPanel) return undefined;
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') setRowPanel(null);
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [rowPanel]);

  // "Copied" is feedback for one action, not a permanent state.
  useEffect(() => {
    if (!copied) return undefined;
    const timer = window.setTimeout(() => setCopied(false), COPIED_RESET_MS);
    return () => window.clearTimeout(timer);
  }, [copied]);

  const togglePanel = (panel: RowPanel, trigger: HTMLElement) => {
    setRowError(null);
    triggerRef.current = trigger;
    setRowPanel((current) => (current?.kind === panel.kind && current.djId === panel.djId ? null : panel));
  };

  const changeAccess = async (dj: Dj, next: AccessTarget) => {
    if (busyDjId !== null) return;
    setRowError(null);
    setBusyDjId(dj.id);
    try {
      await patchClubDj(dj.id, { status: next });
      setRowPanel(null);
      push({
        tone: 'live',
        title: next === 'suspended'
          ? t('djs.toastSuspendedTitle', { name: dj.displayName })
          : t('djs.toastRestoredTitle', { name: dj.displayName }),
        message:
          next === 'suspended'
            ? t('djs.toastSuspendedMessage')
            : t('djs.confirmRestoreBody'),
      });
      reload();
    } catch (err) {
      const action = next === 'suspended'
        ? t('djs.actionSuspend', { name: dj.displayName })
        : t('djs.actionRestoreAccess', { name: dj.displayName });
      const message = describeFailure(err, action, t);
      setRowError(message);
      push({ tone: 'error', title: t('djs.toastAccessError'), message });
    } finally {
      setBusyDjId(null);
    }
  };

  const saveNote = async (dj: Dj, draft: string) => {
    if (busyDjId !== null) return;
    const trimmed = draft.trim();
    if (trimmed.length > NOTE_MAX_LENGTH) {
      setRowError(t('djs.noteTooLong', { count: trimmed.length, max: NOTE_MAX_LENGTH }));
      return;
    }
    if (trimmed === (dj.note ?? '')) {
      setRowPanel(null);
      return;
    }
    setRowError(null);
    setBusyDjId(dj.id);
    try {
      await patchClubDj(dj.id, { note: trimmed === '' ? null : trimmed });
      setRowPanel(null);
      push({
        tone: 'live',
        title: trimmed === ''
          ? t('djs.toastNoteCleared', { name: dj.displayName })
          : t('djs.toastNoteSaved', { name: dj.displayName }),
        message: trimmed === ''
          ? t('djs.toastNoteClearedMessage')
          : t('djs.toastNoteSavedMessage'),
      });
      reload();
    } catch (err) {
      const message = describeFailure(err, t('djs.actionSaveNote', { name: dj.displayName }), t);
      setRowError(message);
      push({ tone: 'error', title: t('djs.toastNoteError'), message });
    } finally {
      setBusyDjId(null);
    }
  };

  const removeDj = async (dj: Dj) => {
    if (busyDjId !== null) return;
    setRowError(null);
    setBusyDjId(dj.id);
    try {
      const result = await removeClubDj(dj.id);
      setRowPanel(null);
      push({
        tone: 'live',
        title: t('djs.toastRemovedTitle', { name: dj.displayName }),
        message:
          result.deleted === 'hard'
            ? t('djs.toastRemovedHard')
            : t('djs.toastRemovedSoft'),
      });
      reload();
    } catch (err) {
      const message = describeFailure(err, t('djs.actionRemove', { name: dj.displayName }), t);
      setRowError(message);
      push({ tone: 'error', title: t('djs.toastRemoveError'), message });
    } finally {
      setBusyDjId(null);
    }
  };

  const restoreDj = async (dj: Dj) => {
    if (busyDjId !== null) return;
    setRowError(null);
    setBusyDjId(dj.id);
    try {
      await restoreClubDj(dj.id);
      push({
        tone: 'live',
        title: t('djs.toastBackTitle', { name: dj.displayName }),
        message: t('djs.toastBackMessage'),
      });
      reload();
    } catch (err) {
      const message = describeFailure(err, t('djs.actionRestore', { name: dj.displayName }), t);
      setRowError(message);
      push({ tone: 'error', title: t('djs.toastRestoreError'), message });
    } finally {
      setBusyDjId(null);
    }
  };

  const copyInviteLink = async () => {
    if (!inviteLink) return;
    const link = `${window.location.origin}/dj/invite/${inviteLink.inviteToken}`;
    if (!navigator.clipboard) {
      setCopied(false);
      setCopyError(t('djs.copyBlocked'));
      return;
    }
    try {
      await navigator.clipboard.writeText(link);
      setCopyError(null);
      setCopied(true);
    } catch {
      setCopied(false);
      setCopyError(t('djs.copyFailed'));
    }
  };

  const onInvite = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (inviting) return;
    setInviteError(null);
    setInviting(true);
    try {
      const result = await inviteDj(invite);
      setInvite({ email: '', displayName: '' });
      setInviteLink(result);
      setCopied(false);
      setCopyError(null);
      push({
        tone: 'live',
        title: t('djs.toastInvitedTitle'),
        message: t('djs.toastInvitedMessage', {
          name: invite.displayName,
          date: formatExpiry(result.inviteExpiresAt, locale),
        }),
      });
      reload();
    } catch (err) {
      const message = describeFailure(err, t('djs.actionInvite'), t);
      setInviteError(message);
      push({ tone: 'error', title: t('djs.toastInviteError'), message });
    } finally {
      setInviting(false);
    }
  };

  const inviteUrl = inviteLink
    ? `${window.location.origin}/dj/invite/${inviteLink.inviteToken}`
    : null;

  const items = data?.items ?? [];
  const roster = items.filter((dj) => !dj.removed);
  const removedDjs = items.filter((dj) => dj.removed);
  const onlineCount = roster.filter((dj) => dj.online).length;
  const onShiftCount = roster.filter((dj) => dj.onShift).length;
  const firstLoad = loading && data === null;
  // The first failed poll is not the same as "the roster could not load": the
  // rows are still real, they are just older than the poll interval.
  const pollFailed = failedAt !== null && data !== null;

  return (
    <div className="club-workspace-content">
      <PageHeading
        title={t('djs.heading')}
        action={(
          <>
            <button type="button" className="btn btn-secondary" onClick={reload} disabled={loading}>
              {loading && data !== null ? t('common.refreshing') : t('common.refresh')}
            </button>
            <button
              type="button"
              className="btn btn-secondary"
              aria-pressed={includeRemoved}
              onClick={() => setIncludeRemoved((current) => !current)}
            >
              {includeRemoved ? t('djs.hideRemoved') : t('djs.showRemoved')}
            </button>
          </>
        )}
      />

      {/* A hard failure replaces the roster: rows would otherwise present old
          access and presence as current. A soft failure (a failed poll) keeps
          them and is reported inside the panel instead. */}
      {error && !error.notImplemented ? (
        <Panel title={t('djs.panelYourDjs')}>
          <ErrorState
            title={t('djs.errorTitle')}
            message={`${error.message}. ${t('djs.errorTail')}`}
            code={error.code}
            onRetry={reload}
          />
        </Panel>
      ) : null}

      {error?.notImplemented ? (
        <Panel title={t('djs.panelYourDjs')} detail={t('djs.panelYourDjsDetail')}>
          <EmptyState
            title={t('djs.stubTitle')}
            detail={t('djs.stubDetail')}
          />
        </Panel>
      ) : null}

      {!error ? (
        <Panel
          title={t('djs.rosterTitle')}
          detail={t('djs.rosterDetail')}
          action={(
            <span className={HINT_CLASS} aria-live="polite">
              {onShiftCount > 0 ? `${t('djs.onShiftCount', { count: onShiftCount })} · ` : ''}
              {t('djs.presenceSummary', { online: onlineCount, total: roster.length })}
            </span>
          )}
        >
          {pollFailed ? (
            <p className={`mb-3 ${ERROR_CLASS}`} role="alert">
              {staleSince !== null
                ? t('djs.pollFailedStale', { age: formatAge((now - staleSince) / 1000, t) })
                : t('djs.pollFailedEarlier')}
            </p>
          ) : null}

          {firstLoad ? (
            <div className="flex flex-col gap-3" role="status" aria-label={t('djs.loadingDjs')}>
              <Skeleton height={44} />
              <Skeleton height={44} />
              <Skeleton height={44} width="80%" />
            </div>
          ) : null}

          {rowError ? (
            <p className={`mb-3 ${ERROR_CLASS}`} role="alert">
              {rowError}
            </p>
          ) : null}

          {!firstLoad && roster.length === 0 ? (
            <EmptyState
              title={t('djs.emptyTitle')}
              detail={t('djs.emptyDetail')}
            />
          ) : null}

          {!firstLoad && roster.length > 0 ? (
            <div className="table-wrap">
              <table className="data-table dj-table">
                <caption className="sr-only">
                  {t('djs.tableCaption')}
                </caption>
                <thead>
                  <tr>
                    <th scope="col">{t('djs.colDj')}</th>
                    <th scope="col">{t('djs.colAccess')}</th>
                    <th scope="col">{t('djs.colPresence')}</th>
                    <th scope="col">{t('djs.colShift')}</th>
                    <th scope="col">{t('djs.colNote')}</th>
                    <th scope="col"><span className="sr-only">{t('djs.colActions')}</span></th>
                  </tr>
                </thead>
                <tbody>
                  {roster.map((dj) => {
                    const panel = rowPanel?.djId === dj.id ? rowPanel : null;
                    const busy = busyDjId === dj.id;
                    const canToggleAccess = dj.status === 'active' || dj.status === 'suspended';
                    const presence = dj.online
                      ? {
                          label: t('djs.presenceOnline'),
                          detail: dj.presenceAgeSeconds !== null
                            ? t('djs.presenceSeen', { age: formatAge(dj.presenceAgeSeconds, t) })
                            : t('djs.presenceLive'),
                        }
                      : dj.presenceAgeSeconds !== null
                        ? {
                            label: t('djs.presenceOffline'),
                            detail: t('djs.presenceLastSeen', { age: formatAge(dj.presenceAgeSeconds, t) }),
                          }
                        : { label: t('djs.presenceUnknown'), detail: t('djs.presenceNever') };
                    return (
                      <Fragment key={dj.id}>
                        <tr className={panel ? 'is-open' : undefined}>
                          <td className="is-strong" data-label={t('djs.colDj')}>
                            <span className="dj-cell">
                              <span className="list-thumb" aria-hidden="true">
                                {dj.displayName.slice(0, 1).toUpperCase()}
                              </span>
                              <span className="dj-cell-copy">
                                {dj.displayName}
                                <span className="dj-cell-sub">{dj.email}</span>
                              </span>
                            </span>
                          </td>
                          <td data-label={t('djs.colAccess')}>
                            <StatusBadge status={dj.status} />
                            {dj.status === 'invited' ? (
                              <span className="dj-cell-sub">
                                {dj.inviteExpiresAt
                                  ? t('djs.inviteExpires', { date: formatExpiry(dj.inviteExpiresAt, locale) })
                                  : t('djs.inviteNotAccepted')}
                              </span>
                            ) : null}
                          </td>
                          <td data-label={t('djs.colPresence')}>
                            <span className={`chip ${dj.online ? 'chip-accent' : 'chip-neutral'}`}>
                              {presence.label}
                            </span>
                            <span className="dj-cell-sub">{presence.detail}</span>
                          </td>
                          <td data-label={t('djs.colShift')}>
                            {dj.onShift ? t('djs.onShift') : t('djs.offShift')}
                            {dj.onShift ? <span className="dj-cell-sub">{t('djs.openSession')}</span> : null}
                          </td>
                          <td data-label={t('djs.colNote')}>
                            {dj.note ? (
                              <span className="dj-note" title={dj.note}>
                                {dj.note.length > NOTE_PREVIEW_LENGTH
                                  ? `${dj.note.slice(0, NOTE_PREVIEW_LENGTH).trimEnd()}…`
                                  : dj.note}
                              </span>
                            ) : (
                              <span className="dj-note is-empty">{t('djs.noNote')}</span>
                            )}
                          </td>
                          <td data-label={t('djs.colActions')}>
                            <div className="dj-row-actions">
                              <button
                                type="button"
                                className="btn btn-quiet btn-sm"
                                aria-expanded={panel?.kind === 'note'}
                                onClick={(event) => togglePanel(
                                  { kind: 'note', djId: dj.id, draft: dj.note ?? '' },
                                  event.currentTarget,
                                )}
                              >
                                {dj.note ? t('djs.editNote') : t('djs.addNote')}
                              </button>
                              {canToggleAccess ? (
                                <button
                                  type="button"
                                  className="btn btn-quiet btn-sm"
                                  aria-expanded={panel?.kind === 'access'}
                                  disabled={busyDjId !== null}
                                  onClick={(event) => togglePanel(
                                    { kind: 'access', djId: dj.id, next: dj.status === 'active' ? 'suspended' : 'active' },
                                    event.currentTarget,
                                  )}
                                >
                                  {dj.status === 'active' ? t('djs.suspendAction') : t('djs.restoreAccess')}
                                </button>
                              ) : null}
                              <TrashAction
                                label={t('djs.remove')}
                                detail={dj.displayName}
                                expanded={panel?.kind === 'remove'}
                                disabled={busyDjId !== null || dj.onShift}
                                disabledReason={dj.onShift ? t('djs.removeBlockedTitle') : undefined}
                                onClick={(event) => togglePanel({ kind: 'remove', djId: dj.id }, event.currentTarget)}
                              />
                            </div>
                          </td>
                        </tr>

                        {panel ? (
                          <tr className="dj-row-panel">
                            <td colSpan={6}>
                              {panel.kind === 'note' ? (
                                <form
                                  onSubmit={(event) => {
                                    event.preventDefault();
                                    void saveNote(dj, panel.draft);
                                  }}
                                  noValidate
                                >
                                  <label htmlFor={`dj-note-${dj.id}`} className={LABEL_CLASS}>
                                    {t('djs.noteLabel', { name: dj.displayName })}
                                  </label>
                                  <textarea
                                    id={`dj-note-${dj.id}`}
                                    ref={(node) => { primaryRef.current = node; }}
                                    className="field-input"
                                    rows={3}
                                    maxLength={NOTE_MAX_LENGTH}
                                    value={panel.draft}
                                    onChange={(event) => setRowPanel({ ...panel, draft: event.target.value })}
                                    placeholder={t('djs.notePlaceholder')}
                                  />
                                  <div className="dj-panel-foot">
                                    <span className={HINT_CLASS}>
                                      {t('djs.noteCounter', { count: panel.draft.trim().length, max: NOTE_MAX_LENGTH })}
                                    </span>
                                    <div className="flex flex-wrap gap-2">
                                      <button type="submit" className="btn btn-primary btn-sm" disabled={busy}>
                                        {busy ? t('common.saving') : t('djs.saveNote')}
                                      </button>
                                      <button
                                        type="button"
                                        className="btn btn-quiet btn-sm"
                                        onClick={() => setRowPanel(null)}
                                        disabled={busy}
                                      >
                                        {t('common.cancel')}
                                      </button>
                                    </div>
                                  </div>
                                </form>
                              ) : null}

                              {panel.kind === 'access' ? (
                                <Tile tone="inset">
                                  <p className="dj-panel-title">
                                    {panel.next === 'suspended'
                                      ? t('djs.confirmSuspendTitle', { name: dj.displayName })
                                      : t('djs.confirmRestoreTitle', { name: dj.displayName })}
                                  </p>
                                  <p className={`mt-1 ${NOTE_CLASS}`}>
                                    {panel.next === 'suspended'
                                      ? t('djs.confirmSuspendBody')
                                      : t('djs.confirmRestoreBody')}
                                  </p>
                                  <div className="mt-3 flex flex-wrap gap-2">
                                    <button
                                      ref={(node) => { primaryRef.current = node; }}
                                      type="button"
                                      className={panel.next === 'suspended' ? 'btn btn-danger btn-sm' : 'btn btn-primary btn-sm'}
                                      onClick={() => { void changeAccess(dj, panel.next); }}
                                      disabled={busyDjId !== null}
                                    >
                                      {busy
                                        ? t('common.saving')
                                        : panel.next === 'suspended'
                                          ? t('djs.confirmSuspendAction')
                                          : t('djs.confirmRestoreAction')}
                                    </button>
                                    <button
                                      type="button"
                                      className="btn btn-quiet btn-sm"
                                      onClick={() => setRowPanel(null)}
                                      disabled={busyDjId !== null}
                                    >
                                      {t('common.cancel')}
                                    </button>
                                  </div>
                                </Tile>
                              ) : null}

                              {panel.kind === 'remove' ? (
                                <Tile tone="inset">
                                  <p className="dj-panel-title">
                                    {t('djs.confirmRemoveTitle', { name: dj.displayName })}
                                  </p>
                                  <p className={`mt-1 ${NOTE_CLASS}`}>
                                    {dj.hasPassword
                                      ? t('djs.confirmRemoveBodyKeep')
                                      : t('djs.confirmRemoveBodyHard')}
                                  </p>
                                  <div className="mt-3 flex flex-wrap gap-2">
                                    <button
                                      ref={(node) => { primaryRef.current = node; }}
                                      type="button"
                                      className="btn btn-danger btn-sm"
                                      onClick={() => { void removeDj(dj); }}
                                      disabled={busyDjId !== null}
                                    >
                                      {busy ? t('djs.removing') : t('djs.confirmRemoveAction')}
                                    </button>
                                    <button
                                      type="button"
                                      className="btn btn-quiet btn-sm"
                                      onClick={() => setRowPanel(null)}
                                      disabled={busyDjId !== null}
                                    >
                                      {t('common.cancel')}
                                    </button>
                                  </div>
                                </Tile>
                              ) : null}
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

          {includeRemoved && removedDjs.length > 0 ? (
            <div className="dj-removed">
              <h3 className="dj-removed-title">{t('djs.removedTitle')}</h3>
              <p className={HINT_CLASS}>
                {t('djs.removedDetail')}
              </p>
              <ul className="club-list">
                {removedDjs.map((dj) => (
                  <li key={dj.id} className="club-list-row">
                    <span className="club-list-row-leading" aria-hidden="true">
                      {dj.displayName.slice(0, 1).toUpperCase()}
                    </span>
                    <span className="club-list-row-main">
                      <span className="club-list-row-title">{dj.displayName}</span>
                      <span className="club-list-row-detail">
                        {dj.email}
                        {dj.removedAt ? ` · ${t('djs.removedAt', { date: formatExpiry(dj.removedAt, locale) })}` : ''}
                      </span>
                    </span>
                    <span className="club-list-row-meta">
                      <button
                        type="button"
                        className="btn btn-secondary btn-sm"
                        onClick={() => { void restoreDj(dj); }}
                        disabled={busyDjId !== null}
                      >
                        {busyDjId === dj.id ? t('djs.restoring') : t('djs.restore')}
                      </button>
                    </span>
                  </li>
                ))}
              </ul>
            </div>
          ) : null}

          {includeRemoved && removedDjs.length === 0 && !firstLoad ? (
            <p className={`mt-4 ${HINT_CLASS}`}>{t('djs.removedEmpty')}</p>
          ) : null}
        </Panel>
      ) : null}

      <div className="mt-4">
        <Panel title={t('djs.invitePanelTitle')} detail={t('djs.invitePanelDetail')}>
          <form
            onSubmit={(event) => { void onInvite(event); }}
            className="dj-invite-form"
            noValidate
          >
            <div>
              <label htmlFor="dj-name" className={LABEL_CLASS}>
                {t('djs.fieldDisplayName')}
              </label>
              <input
                id="dj-name"
                type="text"
                required
                autoComplete="off"
                className="field-input"
                value={invite.displayName}
                onChange={(event) => setInvite({ ...invite, displayName: event.target.value })}
              />
            </div>
            <div>
              <label htmlFor="dj-email" className={LABEL_CLASS}>
                {t('djs.fieldEmail')}
              </label>
              <input
                id="dj-email"
                type="email"
                required
                autoComplete="off"
                className="field-input"
                value={invite.email}
                onChange={(event) => setInvite({ ...invite, email: event.target.value })}
              />
            </div>

            {inviteError ? (
              <p className={`dj-invite-wide ${ERROR_CLASS}`} role="alert">
                {inviteError}
              </p>
            ) : null}

            {inviteLink && inviteUrl ? (
              <Tile tone="inset" className="dj-invite-wide">
                <p className="dj-panel-title">{t('djs.inviteReady')}</p>
                <p className={`mt-1 ${NOTE_CLASS}`}>
                  {t('djs.inviteReadyDetail')}
                </p>
                <code className="dj-invite-link">{inviteUrl}</code>
                <p className={`mt-2 ${HINT_CLASS}`}>
                  {t('djs.inviteExpiresOnce', { date: formatExpiry(inviteLink.inviteExpiresAt, locale) })}
                </p>
                <button
                  type="button"
                  className="btn btn-secondary mt-3"
                  onClick={() => { void copyInviteLink(); }}
                >
                  {copied ? t('djs.copied') : t('djs.copyLink')}
                </button>
                {copyError ? (
                  <p className={`mt-2 ${ERROR_CLASS}`} role="alert">
                    {copyError}
                  </p>
                ) : null}
              </Tile>
            ) : null}

            <div className="dj-invite-wide">
              <button
                type="submit"
                className="btn btn-primary"
                disabled={inviting || invite.email === '' || invite.displayName === ''}
              >
                {inviting ? t('djs.inviting') : t('djs.sendInvitation')}
              </button>
              <p className={`mt-2 ${HINT_CLASS}`}>
                {t('djs.inviteHintSingleUse')}
              </p>
            </div>
          </form>
        </Panel>
      </div>

      <ToastRegion toasts={toasts} onDismiss={dismiss} />
    </div>
  );
}
