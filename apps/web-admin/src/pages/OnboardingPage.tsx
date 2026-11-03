import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type FormEvent,
} from 'react';
import {
  ApiRequestError,
  approveOnboardingApplication,
  getOnboardingApplication,
  listOnboardingApplications,
  rejectOnboardingApplication,
  reissueOnboardingInvite,
  type OnboardingApplicationDetail,
  type OnboardingApplicationListItem,
  type OnboardingApplicationStatus,
  type OnboardingInvite,
  type OnboardingInviteState,
} from '../api/client';
import { useAsyncData } from '../hooks/useAsyncData';
import { useLocale } from '../locale';
import ErrorState from '../components/ErrorState';
import Loading, { SkeletonRows } from '../components/Loading';
import PageHeading from '../components/PageHeading';
import Icon from '../components/Icon';
import Select from '../components/ui/Select';
import { IS_DEMO } from '../app-mode';
import './onboarding.css';

/**
 * Onboarding queue — the club application review (docs/onboarding-contract.md).
 *
 *   GET  /v1/admin/onboarding-applications?status=&limit=&cursor=
 *   GET  /v1/admin/onboarding-applications/:id
 *   POST /v1/admin/onboarding-applications/:id/approve  {email?, slug?}
 *   POST /v1/admin/onboarding-applications/:id/reject   {reason}
 *   POST /v1/admin/onboarding-applications/:id/invite
 *
 * The decision creates the club and a one-time invite link; the owner sets the
 * password on the activation page. No screen here sends or shows a password.
 *
 * Every row is rendered from what the API answered. `club_id` is null and
 * `invite_state` is `none` before approval, so both are read defensively: a
 * pending application has no club, no invite, and the screen still draws.
 */

const PAGE_SIZE = 25;
const REJECT_REASON_MAX = 500;

const APPLICATION_STATUS_TONE: Record<OnboardingApplicationStatus, string> = {
  pending_moderation: 'chip-warn',
  approved: 'chip-accent',
  rejected: 'chip-neutral',
};

const APPLICATION_STATUS_KEY: Record<OnboardingApplicationStatus, string> = {
  pending_moderation: 'onboarding.statusPending',
  approved: 'onboarding.statusApproved',
  rejected: 'onboarding.statusRejected',
};

const INVITE_STATE_TONE: Record<OnboardingInviteState, string> = {
  none: 'chip-neutral',
  pending: 'chip-warn',
  accepted: 'chip-accent',
  expired: 'chip-error',
};

const INVITE_STATE_KEY: Record<OnboardingInviteState, string> = {
  none: 'onboarding.inviteNone',
  pending: 'onboarding.invitePending',
  accepted: 'onboarding.inviteAccepted',
  expired: 'onboarding.inviteExpired',
};

const FOCUSABLE =
  'button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), a[href], [tabindex]:not([tabindex="-1"])';

/** Short absolute stamp: "27 Oct, 18:00". */
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

/** A field of a submitted payload block: empty values render nothing. */
function payloadText(payload: Record<string, unknown> | undefined, key: string): string | null {
  if (!payload) return null;
  const value = payload[key];
  if (typeof value === 'string' && value.trim() !== '') return value;
  if (typeof value === 'number' && Number.isFinite(value)) return String(value);
  return null;
}

function payloadNumber(payload: Record<string, unknown> | undefined, key: string): number | null {
  if (!payload) return null;
  const value = payload[key];
  return typeof value === 'number' && Number.isFinite(value) ? value : null;
}

function payloadList(payload: Record<string, unknown> | undefined, key: string): string[] {
  if (!payload) return [];
  const value = payload[key];
  if (!Array.isArray(value)) return [];
  return value.filter((entry): entry is string => typeof entry === 'string' && entry.trim() !== '');
}

function ApplicationStatusChip({ status }: { status: OnboardingApplicationStatus }) {
  const { t } = useLocale();
  return (
    <span className={`chip ${APPLICATION_STATUS_TONE[status]}`}>
      {t(APPLICATION_STATUS_KEY[status])}
    </span>
  );
}

function InviteStateChip({ state }: { state: OnboardingInviteState }) {
  const { t } = useLocale();
  return (
    <span className={`chip ${INVITE_STATE_TONE[state]}`}>{t(INVITE_STATE_KEY[state])}</span>
  );
}

/** One label/value line; a missing value is not a line at all. */
function MetaRow({
  label,
  value,
  mono = false,
}: {
  label: string;
  value: string | null | undefined;
  mono?: boolean;
}) {
  if (value === null || value === undefined || value === '') return null;
  return (
    <div>
      <dt>{label}</dt>
      <dd className={mono ? 'is-mono' : undefined}>{value}</dd>
    </div>
  );
}

interface ActionError {
  message: string;
  code?: string;
  /** `details.field` of a `422 validation_error`, when the backend named one. */
  field?: string;
}

function actionErrorFrom(err: unknown, fallback: string): ActionError {
  if (err instanceof ApiRequestError) {
    const field = err.details?.field;
    return {
      message: err.message || fallback,
      code: err.code,
      field: typeof field === 'string' ? field : undefined,
    };
  }
  if (err instanceof Error && err.message) return { message: err.message };
  return { message: fallback };
}

/**
 * Application detail drawer.
 *
 * Same layer and behaviour as the mail drawer: a modal dialog with a focus
 * trap, Escape to close, and focus returned to the row control. The decision
 * forms live in the body because both need an input before the request.
 */
function OnboardingDrawer({
  item,
  onClose,
  onDecided,
  onRejected,
}: {
  item: OnboardingApplicationListItem;
  onClose: () => void;
  onDecided: () => void;
  onRejected: () => void;
}) {
  const { t, locale } = useLocale();
  const fetchDetail = useCallback(() => getOnboardingApplication(item.id), [item.id]);
  const detail = useAsyncData<OnboardingApplicationDetail>(fetchDetail);
  const view: OnboardingApplicationListItem = detail.data ?? item;

  // Decision state. `emailDraft === null` means "not edited yet", so the field
  // shows the application e-mail as soon as the detail arrives.
  const [emailDraft, setEmailDraft] = useState<string | null>(null);
  const [slug, setSlug] = useState('');
  const [reason, setReason] = useState('');
  const [decidedStatus, setDecidedStatus] = useState<OnboardingApplicationStatus | null>(null);
  const [approving, setApproving] = useState(false);
  const [rejecting, setRejecting] = useState(false);
  const [reissuing, setReissuing] = useState(false);
  const [confirmReissue, setConfirmReissue] = useState(false);
  const [invite, setInvite] = useState<OnboardingInvite | null>(null);
  const [copied, setCopied] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [approveError, setApproveError] = useState<ActionError | null>(null);
  const [rejectError, setRejectError] = useState<ActionError | null>(null);
  const [inviteError, setInviteError] = useState<ActionError | null>(null);

  const email = emailDraft ?? detail.data?.email ?? '';

  // The decision is reflected as soon as the API answers; the refetch only
  // confirms it. Without this the approve form stayed visible and clickable
  // while the new status was still in flight.
  const status = decidedStatus ?? view.status;
  const isPending = status === 'pending_moderation';
  const isApproved = status === 'approved';
  // Approve and reissue both end with a fresh token, i.e. an active link.
  const inviteState: OnboardingInviteState = invite ? 'pending' : view.invite_state;
  // A reissue is offered for every approved application whose club has not set
  // a password yet — including the legacy row that has a club but no link.
  const canReissue = isApproved && inviteState !== 'accepted';
  // Only an existing link is worth a confirmation: issuing the first one
  // invalidates nothing.
  const hasIssuedLink = inviteState === 'pending' || inviteState === 'expired';

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

  // A missing price is printed as a dash, never as €0.00: the compact
  // application names no price at all (null), and a zero would be a fabricated
  // figure. The club sets its tariff in the panel after activation.
  const money = (minor: number | null) =>
    minor !== null && Number.isFinite(minor) && minor > 0
      ? new Intl.NumberFormat(locale, {
          style: 'currency',
          currency: 'EUR',
          minimumFractionDigits: 2,
          maximumFractionDigits: 2,
        }).format(minor / 100)
      : '—';

  const onApprove = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setApproveError(null);
    setNotice(null);
    const address = email.trim();
    if (!address) {
      setApproveError({ message: t('onboarding.emailRequired') });
      return;
    }
    setApproving(true);
    try {
      const result = await approveOnboardingApplication(item.id, {
        email: address,
        slug: slug.trim() || undefined,
      });
      setInvite(result);
      setDecidedStatus('approved');
      setNotice(
        t('onboarding.approvedNotice', {
          club: view.club_name || t('onboarding.detailFallbackTitle'),
        }),
      );
      detail.reload();
      onDecided();
    } catch (err) {
      setApproveError(actionErrorFrom(err, t('onboarding.approveFailed')));
    } finally {
      setApproving(false);
    }
  };

  const onReject = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setRejectError(null);
    setNotice(null);
    const trimmed = reason.trim();
    if (trimmed.length < 1 || trimmed.length > REJECT_REASON_MAX) {
      setRejectError({ message: t('onboarding.rejectReasonRequired') });
      return;
    }
    setRejecting(true);
    try {
      await rejectOnboardingApplication(item.id, trimmed);
      detail.reload();
      onDecided();
      onRejected();
      onClose();
    } catch (err) {
      setRejectError(actionErrorFrom(err, t('onboarding.rejectFailed')));
    } finally {
      setRejecting(false);
    }
  };

  const onReissue = async () => {
    setInviteError(null);
    setNotice(null);
    if (hasIssuedLink && !confirmReissue) {
      setConfirmReissue(true);
      return;
    }
    setReissuing(true);
    try {
      const result = await reissueOnboardingInvite(item.id);
      setInvite(result);
      setConfirmReissue(false);
      setNotice(t('onboarding.reissuedNotice'));
      detail.reload();
      onDecided();
    } catch (err) {
      setInviteError(actionErrorFrom(err, t('onboarding.reissueFailed')));
    } finally {
      setReissuing(false);
    }
  };

  const onCopy = async () => {
    setInviteError(null);
    const url = invite?.invite_url;
    if (!url) return;
    if (!navigator.clipboard) {
      setInviteError({ message: t('onboarding.clipboardUnavailable') });
      return;
    }
    try {
      await navigator.clipboard.writeText(url);
      setCopied(true);
    } catch {
      setInviteError({ message: t('onboarding.copyFailed') });
    }
  };

  const club = detail.data?.club;
  const venue = detail.data?.venue;
  const qr = detail.data?.qr;
  const qrPrice = payloadNumber(qr, 'price_minor') ?? view.price_minor;

  // The state note explains a link the operator cannot see. Right after
  // approve/reissue the link IS on screen, so the "already sent, not shown
  // again" note would contradict the field below it: the chip carries the
  // state and `inviteLinkHint` explains what to do with the visible link.
  const inviteVisible = Boolean(invite?.invite_url);
  const inviteNote =
    inviteVisible && inviteState === 'pending'
      ? null
      : inviteState === 'accepted'
        ? t('onboarding.inviteAcceptedNote')
        : inviteState === 'pending'
          ? t('onboarding.invitePendingNote')
          : inviteState === 'expired'
            ? t('onboarding.inviteExpiredNote')
            : t('onboarding.inviteNoneNote');

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
        aria-labelledby="onboarding-detail-heading"
      >
        <header className="admin-drawer-head">
          <div style={{ minWidth: 0 }}>
            <p className="admin-eyebrow">{t('onboarding.detailEyebrow')}</p>
            <h2
              id="onboarding-detail-heading"
              className="panel-card-title"
              style={{ fontSize: 'var(--fs-20)', overflowWrap: 'anywhere' }}
            >
              {view.club_name || t('onboarding.detailFallbackTitle')}
            </h2>
            <p className="onboarding-head-meta">
              <ApplicationStatusChip status={status} />
              {view.club_id || invite ? <InviteStateChip state={inviteState} /> : null}
            </p>
          </div>
          <button
            ref={closeRef}
            type="button"
            className="panel-icon-button"
            aria-label={t('onboarding.closeDetail')}
            onClick={onClose}
          >
            <Icon name="lucide:x" size={16} />
          </button>
        </header>

        <div className="admin-drawer-body">
          {notice ? (
            <p className="admin-toast" role="status" style={{ margin: '0 0 12px' }}>
              {notice}
            </p>
          ) : null}

          {isApproved ? (
            <section className="onboarding-block" aria-labelledby="onboarding-invite-heading">
              <h3 id="onboarding-invite-heading" className="onboarding-block-title">
                {t('onboarding.sectionInvite')}
              </h3>
              <p className="onboarding-note">
                <InviteStateChip state={inviteState} />
                {inviteNote ? <span>{inviteNote}</span> : null}
              </p>

              {invite?.invite_url ? (
                <>
                  <p className="onboarding-note">{t('onboarding.inviteLinkHint')}</p>
                  <code className="onboarding-link">{invite.invite_url}</code>
                  <p className="onboarding-meta-line">
                    {invite.invite_expires_at
                      ? t('onboarding.inviteExpires', {
                          date: shortStamp(invite.invite_expires_at, locale),
                        })
                      : null}
                    {invite.email ? <span className="onboarding-mono"> · {invite.email}</span> : null}
                  </p>
                  <p className="onboarding-meta-line">
                    {invite.mail_queued
                      ? t('onboarding.mailQueued')
                      : t('onboarding.mailNotQueued')}
                  </p>
                  <div className="onboarding-actions">
                    <button type="button" className="btn btn-secondary" onClick={() => { void onCopy(); }}>
                      <Icon name="lucide:copy" size={15} />
                      {copied ? t('onboarding.copied') : t('onboarding.copyLink')}
                    </button>
                  </div>
                </>
              ) : null}

              {inviteError ? (
                <p className="panel-error" role="alert" style={{ marginTop: 12 }}>
                  <span>
                    {inviteError.message}
                    {inviteError.code ? <span className="admin-code">{inviteError.code}</span> : null}
                  </span>
                </p>
              ) : null}

              {canReissue ? (
                <div className="onboarding-actions" style={{ marginTop: 12 }}>
                  {confirmReissue ? (
                    <>
                      <span className="onboarding-note">{t('onboarding.reissueConfirm')}</span>
                      <button
                        type="button"
                        className="btn btn-secondary"
                        onClick={() => setConfirmReissue(false)}
                        disabled={reissuing}
                      >
                        {t('common.cancel')}
                      </button>
                    </>
                  ) : null}
                  <button
                    type="button"
                    className={confirmReissue ? 'btn btn-primary' : 'btn btn-secondary'}
                    onClick={() => { void onReissue(); }}
                    disabled={reissuing}
                  >
                    <Icon name="lucide:rotate-ccw" size={15} />
                    {reissuing
                      ? t('onboarding.reissuing')
                      : confirmReissue
                        ? t('onboarding.reissueConfirmAction')
                        : t('onboarding.reissue')}
                  </button>
                </div>
              ) : null}
            </section>
          ) : null}

          {isPending ? (
            <section className="onboarding-block" aria-labelledby="onboarding-decision-heading">
              <h3 id="onboarding-decision-heading" className="onboarding-block-title">
                {t('onboarding.sectionDecision')}
              </h3>

              <form className="onboarding-form" onSubmit={onApprove} noValidate>
                <p className="onboarding-form-title">{t('onboarding.approveTitle')}</p>
                <p className="onboarding-note">{t('onboarding.approveHint')}</p>
                <div className="onboarding-fields">
                  <div>
                    <label htmlFor="onboarding-email" className="field-label">
                      {t('onboarding.emailLabel')}
                    </label>
                    <input
                      id="onboarding-email"
                      type="email"
                      autoComplete="email"
                      inputMode="email"
                      className="field-input"
                      required
                      aria-invalid={approveError?.field === 'email' || undefined}
                      value={email}
                      onChange={(event) => setEmailDraft(event.target.value)}
                      placeholder={t('onboarding.emailPlaceholder')}
                    />
                  </div>
                  <div>
                    <label htmlFor="onboarding-slug" className="field-label">
                      {t('onboarding.slugLabel')}
                    </label>
                    <input
                      id="onboarding-slug"
                      type="text"
                      className="field-input"
                      aria-invalid={approveError?.field === 'slug' || undefined}
                      value={slug}
                      onChange={(event) => setSlug(event.target.value)}
                      placeholder={t('onboarding.slugPlaceholder')}
                    />
                  </div>
                </div>
                {approveError ? (
                  <p className="panel-error" role="alert">
                    <span>
                      {approveError.message}
                      {approveError.code ? <span className="admin-code">{approveError.code}</span> : null}
                    </span>
                  </p>
                ) : null}
                <div className="onboarding-actions">
                  <button type="submit" className="btn btn-primary" disabled={approving}>
                    <Icon name="lucide:check" size={15} />
                    {approving ? t('onboarding.approving') : t('onboarding.approve')}
                  </button>
                </div>
              </form>

              <form className="onboarding-form" onSubmit={onReject} noValidate>
                <p className="onboarding-form-title">{t('onboarding.rejectTitle')}</p>
                <div>
                  <label htmlFor="onboarding-reason" className="field-label">
                    {t('onboarding.rejectReasonLabel')}
                  </label>
                  <textarea
                    id="onboarding-reason"
                    className="field-input"
                    rows={3}
                    required
                    maxLength={REJECT_REASON_MAX}
                    value={reason}
                    onChange={(event) => setReason(event.target.value)}
                    placeholder={t('onboarding.rejectReasonPlaceholder')}
                    aria-describedby="onboarding-reason-hint"
                  />
                  <p id="onboarding-reason-hint" className="field-hint">
                    {t('onboarding.rejectReasonHint', { count: reason.length })}
                  </p>
                </div>
                {rejectError ? (
                  <p className="panel-error" role="alert">
                    <span>
                      {rejectError.message}
                      {rejectError.code ? <span className="admin-code">{rejectError.code}</span> : null}
                    </span>
                  </p>
                ) : null}
                <div className="onboarding-actions">
                  <button type="submit" className="btn btn-danger" disabled={rejecting}>
                    {rejecting ? t('onboarding.rejecting') : t('onboarding.reject')}
                  </button>
                </div>
              </form>
            </section>
          ) : null}

          {detail.loading && !detail.data ? (
            <Loading label={t('onboarding.loading')} />
          ) : null}

          {detail.error ? (
            <div style={{ marginTop: 16 }}>
              <ErrorState
                message={detail.error.message}
                code={detail.error.code}
                onRetry={detail.reload}
              />
            </div>
          ) : null}

          <section className="onboarding-block" aria-labelledby="onboarding-application-heading">
            <h3 id="onboarding-application-heading" className="onboarding-block-title">
              {t('onboarding.detailFallbackTitle')}
            </h3>
            <dl className="admin-meta onboarding-meta">
              <MetaRow label={t('onboarding.fieldApplicationId')} value={view.id} mono />
              <MetaRow
                label={t('onboarding.fieldSubmitted')}
                value={shortStamp(view.created_at, locale)}
              />
              <MetaRow label={t('onboarding.fieldPrice')} value={money(view.price_minor)} />
              <MetaRow
                label={t('onboarding.fieldReviewedAt')}
                value={view.reviewed_at ? shortStamp(view.reviewed_at, locale) : null}
              />
              <MetaRow label={t('onboarding.fieldReviewedBy')} value={detail.data?.reviewed_by ?? null} mono />
              <MetaRow
                label={t('onboarding.fieldRejectReason')}
                value={detail.data?.reject_reason ?? null}
              />
              <MetaRow label={t('onboarding.fieldSourceIp')} value={detail.data?.source_ip ?? null} mono />
            </dl>
          </section>

          {view.contact || detail.data?.email ? (
            <section className="onboarding-block" aria-labelledby="onboarding-contact-heading">
              <h3 id="onboarding-contact-heading" className="onboarding-block-title">
                {t('onboarding.sectionContact')}
              </h3>
              <dl className="admin-meta onboarding-meta">
                <MetaRow label={t('onboarding.fieldContact')} value={view.contact} />
                <MetaRow label={t('onboarding.fieldEmail')} value={detail.data?.email ?? null} />
              </dl>
            </section>
          ) : null}

          {detail.data ? (
            <>
              <section className="onboarding-block" aria-labelledby="onboarding-club-heading">
                <h3 id="onboarding-club-heading" className="onboarding-block-title">
                  {t('onboarding.sectionClub')}
                </h3>
                <dl className="admin-meta onboarding-meta">
                  <MetaRow
                    label={t('onboarding.fieldClubName')}
                    value={payloadText(club, 'name') ?? view.club_name}
                  />
                  <MetaRow label={t('onboarding.fieldClubCity')} value={payloadText(club, 'city') ?? view.city} />
                  <MetaRow label={t('onboarding.fieldClubAddress')} value={payloadText(club, 'address')} />
                  <MetaRow label={t('onboarding.fieldClubWebsite')} value={payloadText(club, 'website')} />
                  <MetaRow
                    label={t('onboarding.fieldClubGenres')}
                    value={payloadList(club, 'genres').join(', ') || null}
                  />
                </dl>
              </section>

              <section className="onboarding-block" aria-labelledby="onboarding-venue-heading">
                <h3 id="onboarding-venue-heading" className="onboarding-block-title">
                  {t('onboarding.sectionVenue')}
                </h3>
                <dl className="admin-meta onboarding-meta">
                  <MetaRow label={t('onboarding.fieldVenueName')} value={payloadText(venue, 'name')} />
                  <MetaRow label={t('onboarding.fieldVenueType')} value={payloadText(venue, 'type')} />
                  <MetaRow label={t('onboarding.fieldVenueAddress')} value={payloadText(venue, 'address')} />
                  <MetaRow
                    label={t('onboarding.fieldVenueCity')}
                    value={payloadText(venue, 'city') ?? view.city}
                  />
                  <MetaRow label={t('onboarding.fieldVenueDistrict')} value={payloadText(venue, 'district')} />
                  <MetaRow
                    label={t('onboarding.fieldVenueCapacity')}
                    value={payloadText(venue, 'capacity')}
                  />
                  <MetaRow label={t('onboarding.fieldVenueSeats')} value={payloadText(venue, 'seats')} />
                  <MetaRow label={t('onboarding.fieldVenueArea')} value={payloadText(venue, 'area_m2')} />
                </dl>
              </section>

              <section className="onboarding-block" aria-labelledby="onboarding-qr-heading">
                <h3 id="onboarding-qr-heading" className="onboarding-block-title">
                  {t('onboarding.sectionQr')}
                </h3>
                <dl className="admin-meta onboarding-meta">
                  <MetaRow label={t('onboarding.fieldQrPrice')} value={money(qrPrice)} />
                  <MetaRow
                    label={t('onboarding.fieldQrZones')}
                    value={payloadList(qr, 'zones').join(', ') || null}
                  />
                </dl>
              </section>
            </>
          ) : null}
        </div>
      </div>
    </div>
  );
}

export default function OnboardingPage() {
  const { t, locale } = useLocale();
  const [statusFilter, setStatusFilter] = useState<OnboardingApplicationStatus | 'all'>(
    'pending_moderation',
  );
  const [selected, setSelected] = useState<OnboardingApplicationListItem | null>(null);
  const [pageNotice, setPageNotice] = useState<string | null>(null);

  const fetchApplications = useCallback(
    () => listOnboardingApplications({ status: statusFilter, limit: PAGE_SIZE }),
    [statusFilter],
  );
  const applications = useAsyncData(fetchApplications);

  // Cursor pagination: the first page belongs to `applications`, the rest is
  // appended here and dropped whenever the first page or the filter changes.
  const [moreItems, setMoreItems] = useState<OnboardingApplicationListItem[]>([]);
  const [moreCursor, setMoreCursor] = useState<string | null>(null);
  const [moreLoading, setMoreLoading] = useState(false);
  const [moreError, setMoreError] = useState<string | null>(null);

  useEffect(() => {
    setMoreItems([]);
    setMoreCursor(null);
    setMoreError(null);
  }, [applications.data, statusFilter]);

  const items = useMemo(
    () => [...(applications.data?.items ?? []), ...moreItems],
    [applications.data, moreItems],
  );
  const nextCursor = moreItems.length > 0 ? moreCursor : applications.data?.next_cursor ?? null;

  const loadMore = async () => {
    if (!nextCursor || statusFilter === 'all') return;
    setMoreLoading(true);
    setMoreError(null);
    try {
      const page = await listOnboardingApplications({
        status: statusFilter,
        limit: PAGE_SIZE,
        cursor: nextCursor,
      });
      setMoreItems((previous) => [...previous, ...page.items]);
      setMoreCursor(page.next_cursor);
    } catch (err) {
      setMoreError(err instanceof Error ? err.message : t('onboarding.loadMoreFailed'));
    } finally {
      setMoreLoading(false);
    }
  };

  const refreshAll = () => {
    setPageNotice(null);
    applications.reload();
  };

  const filtered = statusFilter !== 'pending_moderation';
  const countLabel = applications.data
    ? t(filtered ? 'onboarding.countMatches' : 'onboarding.count', {
        count: new Intl.NumberFormat(locale).format(items.length),
      })
    : t('common.unavailable');

  return (
    <div>
      <PageHeading
        title={t('onboarding.title')}
        action={(
          <button
            type="button"
            className="btn btn-secondary"
            onClick={refreshAll}
            disabled={applications.loading}
          >
            <Icon name="lucide:refresh-cw" size={15} />
            {applications.loading ? t('common.refreshing') : t('common.refresh')}
          </button>
        )}
      />

      {IS_DEMO ? (
        <p className="admin-page-note">
          <strong>{t('onboarding.demoTitle')}</strong> {t('onboarding.demoBody')}
        </p>
      ) : null}

      {pageNotice ? (
        <p className="admin-toast" role="status">
          {pageNotice}
        </p>
      ) : null}

      {applications.error ? (
        <div style={{ marginBottom: 16 }}>
          <ErrorState
            message={applications.error.message}
            code={applications.error.code}
            onRetry={refreshAll}
          />
        </div>
      ) : null}

      {applications.loading && !applications.data ? (
        <div className="panel-section">
          <SkeletonRows rows={6} cols={5} />
        </div>
      ) : (
        <section className="panel-card panel-section" aria-labelledby="onboarding-queue-heading">
          <header className="panel-card-head">
            <div>
              <h2 id="onboarding-queue-heading" className="panel-card-title">
                {t('onboarding.queueTitle')}
              </h2>
              <p className="panel-card-sub">{t('onboarding.queueSubtitle')}</p>
            </div>
            <div className="panel-card-tools">
              <span className="chip chip-neutral">{countLabel}</span>
            </div>
          </header>

          <div className="onboarding-toolbar">
            <div className="onboarding-toolbar-field">
              <Select
                id="onboarding-status-filter"
                label={t('onboarding.statusFilter')}
                value={statusFilter}
                onChange={(value) => setStatusFilter(value as OnboardingApplicationStatus | 'all')}
                options={[
                  { value: 'pending_moderation', label: t('onboarding.statusPending') },
                  { value: 'approved', label: t('onboarding.statusApproved') },
                  { value: 'rejected', label: t('onboarding.statusRejected') },
                  { value: 'all', label: t('onboarding.statusAll') },
                ]}
              />
            </div>
          </div>

          {applications.error ? (
            <p className="admin-table-hint" style={{ margin: 0 }}>
              {t('onboarding.listUnavailable')}
            </p>
          ) : null}

          {!applications.error && applications.data && items.length === 0 ? (
            <div className="panel-empty" role="status">
              <strong>
                {t(filtered ? 'onboarding.emptyFilteredTitle' : 'onboarding.emptyTitle')}
              </strong>
              <span>
                {t(filtered ? 'onboarding.emptyFilteredBody' : 'onboarding.emptyBody')}
              </span>
              {filtered ? (
                <button
                  type="button"
                  className="btn btn-secondary btn-sm"
                  style={{ marginTop: 10 }}
                  onClick={() => setStatusFilter('pending_moderation')}
                >
                  {t('onboarding.showPending')}
                </button>
              ) : null}
            </div>
          ) : null}

          {items.length > 0 ? (
            <div className="table-wrap">
              <table
                className="data-table data-table--stack"
                aria-label={t('onboarding.tableLabel')}
                style={{ minWidth: 880 }}
              >
                <thead>
                  <tr>
                    <th scope="col">{t('onboarding.colClub')}</th>
                    <th scope="col">{t('onboarding.colCity')}</th>
                    <th scope="col">{t('onboarding.colContact')}</th>
                    <th scope="col">{t('onboarding.colCreated')}</th>
                    <th scope="col">{t('onboarding.colStatus')}</th>
                    <th scope="col">{t('onboarding.colInvite')}</th>
                    <th scope="col">{t('onboarding.colOpen')}</th>
                  </tr>
                </thead>
                <tbody>
                  {items.map((application) => (
                    <tr
                      key={application.id}
                      onClick={() => {
                        setPageNotice(null);
                        setSelected(application);
                      }}
                      style={{ cursor: 'pointer' }}
                    >
                      <td className="cell-primary" data-label="">
                        {application.club_name || t('onboarding.detailFallbackTitle')}
                      </td>
                      <td data-label={t('onboarding.colCity')}>{application.city || '—'}</td>
                      <td data-label={t('onboarding.colContact')} style={{ overflowWrap: 'anywhere' }}>
                        {application.contact || '—'}
                      </td>
                      <td data-label={t('onboarding.colCreated')} className="is-mono" style={{ whiteSpace: 'nowrap' }}>
                        {shortStamp(application.created_at, locale)}
                      </td>
                      <td data-label={t('onboarding.colStatus')}>
                        <ApplicationStatusChip status={application.status} />
                      </td>
                      <td data-label={t('onboarding.colInvite')}>
                        {/* Pending applications have no club yet: `club_id` is
                            null and there is nothing to invite, so the cell
                            stays empty instead of printing a fake state. */}
                        {application.club_id ? (
                          <InviteStateChip state={application.invite_state} />
                        ) : (
                          <span className="onboarding-muted">—</span>
                        )}
                      </td>
                      <td data-label="" className="cell-actions">
                        <button
                          type="button"
                          className="panel-icon-button"
                          aria-label={t('onboarding.openApplication', {
                            club: application.club_name || t('onboarding.detailFallbackTitle'),
                          })}
                          onClick={(event) => {
                            event.stopPropagation();
                            setPageNotice(null);
                            setSelected(application);
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
                onClick={() => { void loadMore(); }}
                disabled={moreLoading}
              >
                {moreLoading ? t('common.loading') : t('onboarding.loadMore')}
              </button>
            </div>
          ) : null}
        </section>
      )}

      {selected ? (
        <OnboardingDrawer
          key={selected.id}
          item={selected}
          onClose={() => setSelected(null)}
          onDecided={refreshAll}
          onRejected={() => setPageNotice(t('onboarding.rejectedNotice'))}
        />
      ) : null}
    </div>
  );
}