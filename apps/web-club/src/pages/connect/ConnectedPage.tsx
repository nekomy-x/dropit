import { useEffect, useState } from 'react';
import { useLocation, useNavigate } from 'react-router-dom';
import WizardPage from '../../components/connect/WizardPage';
import Icon, { type IconName } from '../../components/connect/Icons';
import { useConnectDraft } from '../../connect/useConnectDraft';
import { useLocale } from '../../connect/locale';

interface ConnectedState {
  applicationId?: string;
  offline?: boolean;
}

const NEXT_STEPS: { icon: IconName; title: 'wizardModeration' | 'wizardQrActivation' | 'wizardFirstSession'; desc: 'wizardModerationDesc' | 'wizardQrDesc' | 'wizardSessionDesc'; status: 'wizardNow' | 'wizardAfterApproval' | 'wizardScheduled' }[] = [
  {
    icon: 'shield',
    title: 'wizardModeration', desc: 'wizardModerationDesc', status: 'wizardNow',
  },
  {
    icon: 'qr',
    title: 'wizardQrActivation', desc: 'wizardQrDesc', status: 'wizardAfterApproval',
  },
  {
    icon: 'music',
    title: 'wizardFirstSession', desc: 'wizardSessionDesc', status: 'wizardScheduled',
  },
];

/** Step 04 — the application is in, the club is waiting for moderation. */
export default function ConnectedPage() {
  const navigate = useNavigate();
  const { state } = useLocation() as { state: ConnectedState | null };
  const { draft, reset } = useConnectDraft();
  const { t, language } = useLocale();

  // Snapshot the submitted values on the first render — the draft is cleared
  // right after, so reading `draft` later would show empty fields.
  const [submitted] = useState(() => ({
    clubName: draft.club.name.trim(),
    contact: draft.club.contact.trim(),
    priceMinor: draft.qr.priceMinor,
    zones: draft.qr.zones.length,
  }));

  // The draft has been submitted — drop the local copy so a later visit to
  // /connect/club starts from a clean form.
  useEffect(() => {
    reset();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const clubName = submitted.clubName || t('wizardYourClub');
  const applicationId = state?.applicationId ?? '—';
  const offline = state?.offline ?? true;
  const contact = submitted.contact || t('wizardYourEmail');
  const money = (minor: number) => new Intl.NumberFormat(language, {
    style: 'currency', currency: 'EUR', minimumFractionDigits: 2,
  }).format(minor / 100);
  const template = (key: 'wizardSavedOffline' | 'wizardAccepted') => t(key)
    .replace('{id}', applicationId)
    .replace('{contact}', contact);

  return (
    <WizardPage
      step={4}
      title={t('wizardConnected')}
      subtitle={t('wizardConnectedSubtitle')}
      backTo="/connect"
      backLabel={t('wizardHome')}
      hint={offline ? t('wizardSavedDevice') : t('wizardSubmitted')}
      nextLabel={t('wizardHome')}
      onNext={() => navigate('/connect')}
    >
      <section className="flex-1 rounded-card border border-drop-line bg-drop-surface p-7">
        <header className="flex items-center gap-3.5">
          <span className="flex h-5 w-5 items-center justify-center text-drop-accent">
            <Icon name="check" size={18} strokeWidth={2.4} />
          </span>
          <div className="min-w-0 flex-1">
            <p className="connect-mono-label">{t('wizardConnection')}</p>
            <p className="mt-1 text-[13px] text-drop-text sm:truncate">
              {t('wizardDataSaved')}
            </p>
          </div>
          <span className="font-mono text-[22px] leading-none text-drop-accent">04</span>
        </header>

        {/* Success row with the animated check */}
        <div className="wizard-inset mt-6 flex flex-col gap-4 rounded-card border border-drop-accent/25 bg-drop-accent/[0.05] p-5 sm:flex-row sm:items-center">
          <span className="relative flex h-11 w-11 shrink-0 items-center justify-center rounded-full bg-drop-accent text-drop-ink">
            <span className="absolute inset-0 animate-ring-out rounded-full border border-drop-accent" />
            <svg viewBox="0 0 24 24" width={18} height={18} fill="none" aria-hidden="true">
              <path
                d="M4 12.5l5 5L20 6.5"
                stroke="currentColor"
                strokeWidth={2.6}
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeDasharray="32"
                strokeDashoffset="32"
                className="animate-check-draw"
              />
            </svg>
          </span>

          <div className="min-w-0 flex-1">
            <p className="text-[16px] font-semibold text-drop-text">{clubName} · {t('wizardDone')}</p>
            <p className="mt-1 text-[12px] leading-relaxed text-drop-muted">
              {offline
                ? template('wizardSavedOffline')
                : template('wizardAccepted')}
            </p>
          </div>

          <span className="shrink-0 rounded-full bg-drop-accent px-2.5 py-1 font-mono text-[8px] uppercase tracking-[1px] text-drop-ink">
            {t('wizardDone')}
          </span>
        </div>

        {offline && (
          <p className="mt-3 flex items-start gap-2 rounded-field border border-drop-line bg-drop-bg px-3 py-2.5 text-[11px] leading-relaxed text-drop-muted">
            <span className="mt-0.5 shrink-0 text-drop-dim">
              <Icon name="info" size={13} />
            </span>
            {t('wizardOfflineNotice')}
          </p>
        )}

        <p className="connect-mono-label mt-7">{t('wizardNextSteps')}</p>
        <div className="mt-3 flex flex-col">
          {NEXT_STEPS.map((step, index) => (
            <div
              key={step.title}
              className={`flex items-center gap-3 py-3.5 ${
                index < NEXT_STEPS.length - 1 ? 'border-b border-drop-line/60' : ''
              }`}
            >
              <span className="flex h-7 w-7 shrink-0 items-center justify-center rounded-lg border border-drop-line text-drop-accent">
                <Icon name={step.icon} size={13} />
              </span>
              <div className="min-w-0 flex-1">
                <p className="text-[13px] font-semibold text-drop-text">{t(step.title)}</p>
                <p className="mt-0.5 text-[11px] text-drop-dim">{t(step.desc)}</p>
              </div>
              <span className="shrink-0 font-mono text-[8px] uppercase tracking-[1px] text-drop-muted">
                {t(step.status)}
              </span>
            </div>
          ))}
        </div>

        <p className="mt-4 flex items-start gap-2 border-t border-drop-line pt-4 text-[11px] leading-relaxed text-drop-dim">
          <Icon name="mail" size={13} className="mt-0.5 shrink-0" />
          {t('wizardMailNote')}
        </p>
      </section>

      <aside className="w-full shrink-0 rounded-card border border-drop-line bg-drop-surface p-6 lg:w-[380px]">
        <header className="flex items-center gap-2.5">
          <span className="text-drop-accent">
            <Icon name="users" size={16} />
          </span>
          <h2 className="connect-mono-label flex-1 text-drop-muted">{t('wizardAccessSupport')}</h2>
        </header>

        <div className="mt-4 flex flex-col">
          {[
            {
              icon: 'users' as IconName,
              title: t('wizardOwner'),
              meta: contact,
            },
            {
              icon: 'wallet' as IconName,
              title: t('wizardPriceSummary'),
              meta: `${money(submitted.priceMinor)} · ${submitted.zones} ${t('wizardZonesCount').toLowerCase()}`,
            },
            {
              icon: 'phone' as IconName,
              title: t('wizardManager'),
              meta: `clubs@dropit.music · ${t('wizardReplyDay')}`,
            },
          ].map((row, index, all) => (
            <div
              key={row.title}
              className={`flex items-start gap-3 py-3.5 ${
                index < all.length - 1 ? 'border-b border-drop-line/60' : ''
              }`}
            >
              <span className="mt-0.5 flex h-6 w-6 shrink-0 items-center justify-center rounded-lg border border-drop-line text-drop-muted">
                <Icon name={row.icon} size={12} />
              </span>
              <div className="min-w-0">
                <p className="text-[13px] font-semibold text-drop-text">{row.title}</p>
                <p className="mt-0.5 break-words text-[11px] text-drop-dim">{row.meta}</p>
              </div>
            </div>
          ))}
        </div>

        <p className="mt-4 border-t border-drop-line pt-4 text-[11px] leading-relaxed text-drop-muted">
          {t('wizardSupportNote')}
        </p>

        {/* Before approval there is no account to open a panel with: the
            e-mail invitation is the way in. The success screen therefore sends
            the visitor back to the public site instead of a login form they
            cannot pass yet. */}
        <button
          type="button"
          onClick={() => navigate('/connect')}
          className="connect-btn-primary mt-5 w-full"
        >
          <Icon name="arrowRight" size={14} strokeWidth={2} />
          {t('wizardHome')}
        </button>
      </aside>
    </WizardPage>
  );
}
