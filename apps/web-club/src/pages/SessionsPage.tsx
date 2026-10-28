import { useCallback, useEffect, useRef, useState } from 'react';
import { Link } from 'react-router-dom';
import { QRCodeSVG } from 'qrcode.react';
import {
  ApiRequestError,
  closeSession,
  createSession,
  generateSessionQr,
  getClubDashboard,
  getDjs,
  type Dj,
  type QrGenerateResponse,
  type SessionStatus,
} from '../api/client';
import { useAsyncData } from '../hooks/useAsyncData';
import { useLocale, type Translate } from '../locale';
import PageHeading from '../components/PageHeading';
import StatusBadge from '../components/StatusBadge';
import Select from '../components/ui/Select';
import {
  EmptyState,
  ErrorState,
  Panel,
  Skeleton,
  Tile,
  ToastRegion,
  useToasts,
} from '../components/club';

const DEFAULT_TIMEZONE = 'Europe/Madrid';

/** What the page knows about the shift. `timezone`/`djId` are absent after a reload restore. */
interface Shift {
  id: string;
  status: SessionStatus;
  timezone: string | null;
  djId: string | null;
}

/** Shape returned by the dashboard endpoint for the open shift. */
interface CurrentShift {
  id: string;
  status: SessionStatus;
}

const LABEL_CLASS = 'text-[length:var(--fs-12)] font-medium text-[color:var(--c-text-muted)]';
const BODY_CLASS = 'text-[length:var(--fs-13)] leading-[1.5] text-[color:var(--c-text-2)]';
const NOTE_CLASS = 'text-[length:var(--fs-12)] leading-[1.5] text-[color:var(--c-text-2)]';

/** Cached per locale: the expiry is formatted on every QR render and in toasts. */
const EXPIRY_CACHE = new Map<string, Intl.DateTimeFormat>();

/**
 * `null` for a missing or unparsable timestamp so the caller renders the honest
 * `unavailable` word: an expiry we cannot read must not echo `undefined` back
 * into the sentence that tells the guest how long the code is valid.
 */
function formatExpiry(value: string | null | undefined, locale: string): string | null {
  if (typeof value !== 'string' || value.trim() === '') return null;
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return null;
  let formatter = EXPIRY_CACHE.get(locale);
  if (!formatter) {
    formatter = new Intl.DateTimeFormat(locale, { dateStyle: 'medium', timeStyle: 'short' });
    EXPIRY_CACHE.set(locale, formatter);
  }
  return formatter.format(date);
}

/** One honest sentence: what failed, what the server said, what to do next. */
function describeFailure(err: unknown, action: string, t: Translate): string {
  if (err instanceof ApiRequestError) {
    const status = `HTTP ${err.status}${err.code ? ` · ${err.code}` : ''}`;
    const message = err.message.replace(/\.$/, '');
    return t('sessions.failureHttp', { action, status, message });
  }
  return t('sessions.failureNetwork', { action });
}

/**
 * Live room — the only place where a shift is opened or closed.
 *
 * Functional contract:
 *  - F13a the open shift survives a reload: it is read back from the dashboard
 *    endpoint (`getClubDashboard().session`), because there is no club
 *    GET-session route;
 *  - F13b closing is irreversible on the backend (`closed` has no outgoing
 *    transition), so it takes an explicit second step that says so;
 *  - F13c the QR comes only from `generateSessionQr()` and keeps the one-time
 *    token semantics; rotating it invalidates the previous code, so it confirms.
 *
 * No invented data: when the club API is unreachable (demo mode has no backend)
 * the page says so and blocks the actions that would be unsafe to guess.
 */
export default function SessionsPage() {
  const { t, locale } = useLocale();
  const { toasts, push, dismiss } = useToasts();

  const loadDjs = useCallback(() => getDjs(), []);
  const {
    data: djsData,
    loading: loadingDjs,
    error: djsError,
    reload: reloadDjs,
  } = useAsyncData<{ items: Dj[] }>(loadDjs);

  // F13a — restore the shift after a reload. Additive: the dashboard endpoint
  // already returns the latest open session; no new route is invented.
  const loadCurrentShift = useCallback(async (): Promise<CurrentShift | null> => {
    const dashboard = await getClubDashboard();
    return dashboard.session ? { id: dashboard.session.id, status: dashboard.session.status } : null;
  }, []);
  const {
    data: restoredShift,
    loading: restoring,
    error: restoreError,
    reload: reloadRestore,
  } = useAsyncData<CurrentShift | null>(loadCurrentShift);

  const [shift, setShift] = useState<Shift | null>(null);
  const [timezone, setTimezone] = useState(DEFAULT_TIMEZONE);
  const [djId, setDjId] = useState('');
  const [confirmClose, setConfirmClose] = useState(false);
  const [confirmRotate, setConfirmRotate] = useState(false);
  const [creating, setCreating] = useState(false);
  const [closing, setClosing] = useState(false);
  const [rotating, setRotating] = useState(false);
  const [qr, setQr] = useState<QrGenerateResponse | null>(null);
  const [shiftError, setShiftError] = useState<string | null>(null);
  const [qrError, setQrError] = useState<string | null>(null);

  // Focus management for the destructive confirmations (§5.3): opening one moves
  // focus into it, closing it (cancel or done) returns focus to the control that
  // opened it instead of dropping the caret on <body>.
  const closeTriggerRef = useRef<HTMLButtonElement | null>(null);
  const closeCancelRef = useRef<HTMLButtonElement | null>(null);
  const rotateTriggerRef = useRef<HTMLButtonElement | null>(null);
  const rotateCancelRef = useRef<HTMLButtonElement | null>(null);
  const nextShiftRef = useRef<HTMLButtonElement | null>(null);

  const closeWasOpen = useRef(false);
  const rotateWasOpen = useRef(false);

  useEffect(() => {
    if (confirmClose) {
      closeWasOpen.current = true;
      closeCancelRef.current?.focus();
    } else if (closeWasOpen.current) {
      closeWasOpen.current = false;
      closeTriggerRef.current?.focus();
    }
  }, [confirmClose]);

  useEffect(() => {
    if (confirmRotate) {
      rotateWasOpen.current = true;
      rotateCancelRef.current?.focus();
    } else if (rotateWasOpen.current) {
      rotateWasOpen.current = false;
      rotateTriggerRef.current?.focus();
    }
  }, [confirmRotate]);

  // Apply the restored shift once the check finishes. Local open/close results
  // do not touch `restoredShift`, so they are never overwritten by this effect.
  useEffect(() => {
    if (restoring || restoreError) return;
    setShift(
      restoredShift
        ? { id: restoredShift.id, status: restoredShift.status, timezone: null, djId: null }
        : null,
    );
  }, [restoredShift, restoring, restoreError]);

  // Escape dismisses whichever confirmation is open, without acting on it.
  useEffect(() => {
    if (!confirmClose && !confirmRotate) return undefined;
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== 'Escape') return;
      setConfirmClose(false);
      setConfirmRotate(false);
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [confirmClose, confirmRotate]);

  const busy = creating || closing || rotating;
  const shiftClosed = shift?.status === 'closed';

  const openShift = async () => {
    if (busy || shift) return;
    setShiftError(null);
    setCreating(true);
    try {
      const created = await createSession({
        timezone: timezone.trim() || DEFAULT_TIMEZONE,
        // dj_id is optional: a shift can run unassigned (F2 contract).
        dj_id: djId.trim() === '' ? undefined : djId.trim(),
      });
      setShift({
        id: created.id,
        status: created.status,
        timezone: created.timezone,
        djId: created.dj_id ?? null,
      });
      setQr(null);
      setConfirmClose(false);
      push({
        tone: 'live',
        title: t('sessions.toastOpenedTitle'),
        message: t('sessions.toastOpenedBody', { timezone: created.timezone }),
      });
    } catch (err) {
      const message = describeFailure(err, t('sessions.actionOpenShift'), t);
      setShiftError(message);
      push({ tone: 'error', title: t('sessions.toastOpenFailed'), message });
    } finally {
      setCreating(false);
    }
  };

  const closeShift = async () => {
    if (!shift || busy) return;
    setShiftError(null);
    setClosing(true);
    try {
      const closed = await closeSession(shift.id);
      setShift({
        id: closed.id,
        status: closed.status,
        timezone: closed.timezone,
        djId: closed.dj_id ?? null,
      });
      setQr(null);
      setConfirmClose(false);
      setConfirmRotate(false);
      push({
        tone: 'live',
        title: t('sessions.toastClosedTitle'),
        message: t('sessions.toastClosedBody'),
      });
    } catch (err) {
      const message = describeFailure(err, t('sessions.actionCloseShift'), t);
      setShiftError(message);
      push({ tone: 'error', title: t('sessions.toastCloseFailed'), message });
    } finally {
      setClosing(false);
    }
  };

  const generateQr = async () => {
    if (!shift || shiftClosed || busy) return;
    setQrError(null);
    setRotating(true);
    try {
      const issued = await generateSessionQr(shift.id);
      setQr(issued);
      setConfirmRotate(false);
      push({
        tone: 'live',
        title: t('sessions.toastQrTitle'),
        message: t('sessions.toastQrBody', {
          date: formatExpiry(issued.expires_at, locale) ?? t('common.unavailable'),
        }),
      });
    } catch (err) {
      const message = describeFailure(err, t('sessions.actionIssueQr'), t);
      setQrError(message);
      push({ tone: 'error', title: t('sessions.toastQrFailed'), message });
    } finally {
      setRotating(false);
    }
  };

  const activeDjs = (djsData?.items ?? []).filter((dj) => dj.status === 'active');

  return (
    <div className="club-workspace-content">
      <PageHeading
        title={t('sessions.title')}
      />

      {restoreError ? (
        <div className="mb-4">
          <ErrorState
            title={t('sessions.checkErrorTitle')}
            message={t('sessions.checkErrorBody')}
            code={restoreError.code}
            onRetry={reloadRestore}
            retrying={restoring}
          />
        </div>
      ) : null}

      <div className="grid items-start gap-4 lg:grid-cols-[minmax(0,1.15fr)_minmax(0,0.85fr)]">
        <Panel
          title={t('sessions.shiftPanel')}
          detail={t('sessions.shiftPanelDetail')}
          action={<StatusBadge status={shift ? shift.status : 'offline'} />}
        >
          {restoring ? (
            <div className="flex flex-col gap-3" role="status" aria-label={t('sessions.checkingShift')}>
              <Skeleton height={18} width="55%" />
              <Skeleton height={44} />
              <Skeleton height={36} width="45%" />
            </div>
          ) : shift ? (
            <>
              <Tile tone="inset">
                <dl className="grid gap-x-4 gap-y-2 sm:grid-cols-[104px_minmax(0,1fr)]">
                  <dt className={LABEL_CLASS}>{t('sessions.shiftId')}</dt>
                  <dd className="break-all font-mono text-[length:var(--fs-12)] leading-[1.5] text-[color:var(--c-text)]">
                    {shift.id}
                  </dd>
                  <dt className={LABEL_CLASS}>{t('sessions.timezone')}</dt>
                  <dd className={BODY_CLASS}>
                    {shift.timezone ?? t('sessions.timezoneUnknown')}
                  </dd>
                  <dt className={LABEL_CLASS}>{t('sessions.djOnShift')}</dt>
                  <dd className={BODY_CLASS}>{shift.djId ?? t('sessions.noDjAssigned')}</dd>
                </dl>
              </Tile>

              {shiftClosed ? (
                <Tile tone="inset" className="mt-4">
                  <p className="text-[length:var(--fs-13)] font-semibold text-[color:var(--c-text)]">
                    {t('sessions.shiftClosedTitle')}
                  </p>
                  <p className={`mt-1 ${NOTE_CLASS}`}>{t('sessions.shiftClosedBody')}</p>
                  <div className="mt-3">
                    <button
                      ref={nextShiftRef}
                      type="button"
                      className="club-button club-button-primary"
                      onClick={() => {
                        // Drop the closed shift locally so the setup form returns;
                        // nothing on the backend is touched here.
                        setShift(null);
                        setQr(null);
                        setConfirmClose(false);
                      }}
                    >
                      {t('sessions.startNextShift')}
                    </button>
                  </div>
                </Tile>
              ) : confirmClose ? (
                <Tile tone="inset" className="mt-4">
                  <p className="text-[length:var(--fs-13)] font-semibold text-[color:var(--c-text)]">
                    {t('sessions.closeQuestion')}
                  </p>
                  <p className={`mt-1 ${NOTE_CLASS}`}>{t('sessions.closeWarning')}</p>
                  <div className="mt-3 flex flex-wrap gap-2">
                    <button
                      type="button"
                      className="club-button"
                      style={{ color: 'var(--c-error)' }}
                      onClick={() => { void closeShift(); }}
                      disabled={busy}
                    >
                      {closing ? t('sessions.closing') : t('sessions.closeConfirm')}
                    </button>
                    <button
                      ref={closeCancelRef}
                      type="button"
                      className="club-button club-button-quiet"
                      onClick={() => setConfirmClose(false)}
                      disabled={busy}
                    >
                      {t('sessions.keepOpen')}
                    </button>
                  </div>
                </Tile>
              ) : (
                <button
                  ref={closeTriggerRef}
                  type="button"
                  className="club-button mt-4"
                  onClick={() => { setShiftError(null); setConfirmClose(true); }}
                  disabled={busy}
                >
                  {t('sessions.closeShift')}
                </button>
              )}

              {shiftError ? (
                <p className="mt-3 text-[length:var(--fs-13)] leading-[1.5] text-[color:var(--c-error)]" role="alert">
                  {shiftError}
                </p>
              ) : null}
            </>
          ) : (
            <>
              <p className={BODY_CLASS}>{t('sessions.noShiftBody')}</p>

              <div className="mt-4 grid gap-4 sm:grid-cols-2">
                <div>
                  <label htmlFor="session-timezone" className={`mb-1 block ${LABEL_CLASS}`}>
                    {t('sessions.timezoneLabel')}
                  </label>
                  <input
                    id="session-timezone"
                    type="text"
                    required
                    autoComplete="off"
                    spellCheck={false}
                    placeholder={DEFAULT_TIMEZONE}
                    className="field-input"
                    value={timezone}
                    onChange={(event) => setTimezone(event.target.value)}
                    aria-describedby="session-timezone-hint"
                  />
                  <p id="session-timezone-hint" className="mt-1 text-[length:var(--fs-11)] leading-[1.45] text-[color:var(--c-text-muted-sm)]">
                    {t('sessions.timezoneHint')}
                  </p>
                </div>
                <div>
                  <label id="session-dj-label" htmlFor="session-dj" className={`mb-1 block ${LABEL_CLASS}`}>
                    {t('sessions.djLabel')}
                  </label>
                  <Select
                    id="session-dj"
                    labelledBy="session-dj-label"
                    value={djId}
                    onChange={setDjId}
                    disabled={loadingDjs || Boolean(djsError)}
                    placeholder={loadingDjs ? t('sessions.loadingDjs') : t('sessions.noDjAssigned')}
                    options={[
                      {
                        value: '',
                        label: loadingDjs ? t('sessions.loadingDjs') : t('sessions.noDjAssigned'),
                      },
                      ...activeDjs.map((dj) => ({
                        value: dj.id,
                        label: dj.displayName,
                        description: dj.email,
                      })),
                    ]}
                  />
                  {djsError ? (
                    <div className="mt-2 flex flex-wrap items-center gap-2">
                      <p className="text-[length:var(--fs-11)] leading-[1.45] text-[color:var(--c-text-2)]">
                        {t('sessions.djListFailed')}
                      </p>
                      <button
                        type="button"
                        className="club-button club-button-quiet"
                        onClick={reloadDjs}
                        disabled={loadingDjs}
                      >
                        {loadingDjs ? t('sessions.retrying') : t('common.tryAgain')}
                      </button>
                    </div>
                  ) : null}
                  {!loadingDjs && !djsError && activeDjs.length === 0 ? (
                    <p className="mt-2 text-[length:var(--fs-11)] leading-[1.45] text-[color:var(--c-text-2)]">
                      {t('sessions.noActiveDjs')}
                    </p>
                  ) : null}
                </div>
              </div>

              {shiftError ? (
                <p className="mt-3 text-[length:var(--fs-13)] leading-[1.5] text-[color:var(--c-error)]" role="alert">
                  {shiftError}
                </p>
              ) : null}

              <button
                type="button"
                className="club-button club-button-primary mt-4"
                onClick={() => { void openShift(); }}
                disabled={busy || restoring || Boolean(restoreError) || timezone.trim() === ''}
              >
                {creating ? t('sessions.opening') : t('sessions.openShift')}
              </button>
              {restoreError ? (
                <p className={`mt-2 ${NOTE_CLASS}`}>{t('sessions.openBlocked')}</p>
              ) : null}
            </>
          )}
        </Panel>

        <Panel title={t('sessions.qrPanel')} detail={t('sessions.qrPanelDetail')}>
          {restoring ? (
            <div className="flex flex-col gap-3" role="status" aria-label={t('sessions.checkingShift')}>
              <Skeleton height={160} />
              <Skeleton height={18} width="60%" />
            </div>
          ) : !shift ? (
            <EmptyState
              title={t('sessions.qrEmptyTitle')}
              detail={t('sessions.qrEmptyBody')}
            />
          ) : shiftClosed ? (
            <EmptyState
              title={t('sessions.shiftClosedTitle')}
              detail={t('sessions.qrClosedBody')}
            />
          ) : qr ? (
            <>
              <Tile tone="inset">
                <div className="flex items-start gap-4 max-sm:flex-col">
                  <div
                    className="w-[200px] shrink-0 rounded-[var(--r-tile)] bg-white p-3"
                    role="img"
                    aria-label={t('sessions.qrImageLabel')}
                  >
                    <QRCodeSVG
                      value={qr.qr_token}
                      size={176}
                      bgColor="#FFFFFF"
                      fgColor="#06130C"
                      level="M"
                      marginSize={0}
                    />
                  </div>
                  <div className="min-w-0">
                    <p className="text-[length:var(--fs-13)] font-medium text-[color:var(--c-text)]">
                      {t('sessions.qrScan')}
                    </p>
                    <p className={`mt-2 ${NOTE_CLASS}`}>
                      {t('sessions.qrExpires', {
                        date: formatExpiry(qr.expires_at, locale) ?? t('common.unavailable'),
                      })}
                    </p>
                    <p className={`mt-1 ${NOTE_CLASS}`}>{t('sessions.qrOneTime')}</p>
                    {/* A live token is a screen artifact; the thing on the bar is a
                        printed sign. The studio opens with this token already in it,
                        so printing tonight's sign is one click from the shift. */}
                    <Link className="club-button club-button-quiet mt-3" to="/qr" data-control="button">
                      {t('sessions.qrStudioLink')}
                    </Link>
                  </div>
                </div>
              </Tile>

              {qrError ? (
                <p className="mt-3 text-[length:var(--fs-13)] leading-[1.5] text-[color:var(--c-error)]" role="alert">
                  {qrError}
                </p>
              ) : null}

              {confirmRotate ? (
                <Tile tone="inset" className="mt-3">
                  <p className="text-[length:var(--fs-13)] font-semibold text-[color:var(--c-text)]">
                    {t('sessions.rotateQuestion')}
                  </p>
                  <p className={`mt-1 ${NOTE_CLASS}`}>{t('sessions.rotateWarning')}</p>
                  <div className="mt-3 flex flex-wrap gap-2">
                    <button
                      type="button"
                      className="club-button"
                      style={{ color: 'var(--c-error)' }}
                      onClick={() => { void generateQr(); }}
                      disabled={busy}
                    >
                      {rotating ? t('sessions.rotating') : t('sessions.rotateConfirm')}
                    </button>
                    <button
                      ref={rotateCancelRef}
                      type="button"
                      className="club-button club-button-quiet"
                      onClick={() => setConfirmRotate(false)}
                      disabled={busy}
                    >
                      {t('sessions.keepQr')}
                    </button>
                  </div>
                </Tile>
              ) : (
                <button
                  ref={rotateTriggerRef}
                  type="button"
                  className="club-button mt-3 w-full"
                  onClick={() => { setQrError(null); setConfirmRotate(true); }}
                  disabled={busy}
                >
                  {t('sessions.rotateQr')}
                </button>
              )}
            </>
          ) : (
            <>
              <EmptyState
                title={t('sessions.qrMissingTitle')}
                detail={t('sessions.qrMissingBody')}
              />
              {qrError ? (
                <p className="mt-3 text-[length:var(--fs-13)] leading-[1.5] text-[color:var(--c-error)]" role="alert">
                  {qrError}
                </p>
              ) : null}
              <button
                type="button"
                className="club-button club-button-primary mt-3 w-full"
                onClick={() => { void generateQr(); }}
                disabled={busy}
              >
                {rotating ? t('sessions.generating') : t('sessions.generateQr')}
              </button>
            </>
          )}
        </Panel>
      </div>

      <ToastRegion toasts={toasts} onDismiss={dismiss} />
    </div>
  );
}
