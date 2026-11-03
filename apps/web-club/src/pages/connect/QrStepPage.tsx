import { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import WizardPage from '../../components/connect/WizardPage';
import { FormCard } from '../../components/connect/WizardKit';
import Icon from '../../components/connect/Icons';
import QrPreview from '../../components/connect/QrPreview';
import { useConnectDraft } from '../../connect/useConnectDraft';
import {
  formatEuro,
  hasErrors,
  validateQr,
  type FieldErrors,
  type QrStepData,
} from '../../connect/types';
import { offlineApplicationId, submitConnectApplication } from '../../api/connect';
import { NotImplementedStubError } from '../../api/client';
import { translateValidationMessage, useLocale } from '../../connect/locale';

const PRICE_PRESETS = [100, 200, 300, 500];
const SERVICE_FEE_MINOR = 30;

/** Step 03 — guest price, zones and the QR kit, then submit the application. */
export default function QrStepPage() {
  const navigate = useNavigate();
  const { draft, patchQr, savedLabel } = useConnectDraft();
  const { t } = useLocale();
  const qr = draft.qr;
  const [errors, setErrors] = useState<FieldErrors<QrStepData>>({});
  const [zoneDraft, setZoneDraft] = useState('');
  const [submitting, setSubmitting] = useState(false);
  const [submitError, setSubmitError] = useState<string | null>(null);

  const set = (patch: Partial<QrStepData>) => {
    patchQr(patch);
    setErrors((prev) => {
      const next = { ...prev };
      for (const key of Object.keys(patch) as (keyof QrStepData)[]) delete next[key];
      return next;
    });
  };

  const addZone = () => {
    const name = zoneDraft.trim();
    if (!name || qr.zones.includes(name)) return;
    set({ zones: [...qr.zones, name] });
    setZoneDraft('');
  };

  const clubShare = Math.max(0, qr.priceMinor - SERVICE_FEE_MINOR);

  const handleSubmit = async () => {
    const nextErrors = Object.fromEntries(Object.entries(validateQr(qr)).map(([key, message]) => [key, translateValidationMessage(message, t)])) as FieldErrors<QrStepData>;
    setErrors(nextErrors);
    if (hasErrors(nextErrors)) return;

    setSubmitting(true);
    setSubmitError(null);
    try {
      const result = await submitConnectApplication(draft);
      navigate('/connect/connected', {
        state: { applicationId: result.application_id, offline: false },
      });
    } catch (error) {
      if (error instanceof NotImplementedStubError) {
        // The onboarding endpoint is not on the backend yet — the draft is
        // already stored locally, so the flow still completes honestly.
        navigate('/connect/connected', {
          state: { applicationId: offlineApplicationId(), offline: true },
        });
      } else {
        setSubmitError(error instanceof Error ? error.message : t('wizardSubmitError'));
      }
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <WizardPage
      step={3}
      title={t('wizardQrTitle')}
      subtitle={t('wizardQrSubtitle')}
      backTo="/connect/venue"
      backLabel={t('wizardBackVenue')}
      hint={savedLabel ? `${t('wizardSaved')} ${savedLabel}` : t('wizardAuto')}
      nextLabel={t('wizardSubmit')}
      onNext={handleSubmit}
      loading={submitting}
    >
      <FormCard
        eyebrow={t('wizardQrEyebrow')}
        title={t('wizardQrFormTitle')}
        number="03"
        icon="qr"
      >
        {/* Price */}
        <div className="flex flex-col gap-3">
          <span className="connect-mono-label">{t('wizardGuestPrice')}</span>
          <div className="flex flex-wrap items-center gap-3">
            <div className="flex items-center gap-2">
              <button
                type="button"
                onClick={() => set({ priceMinor: Math.max(100, qr.priceMinor - 50) })}
                disabled={qr.priceMinor <= 100}
                aria-label={t('wizardDecrease')}
                className="flex h-9 w-9 items-center justify-center rounded-field border border-drop-line text-drop-muted transition hover:border-drop-accent/50 hover:text-drop-accent active:scale-95 disabled:opacity-35"
              >
                <Icon name="minus" size={14} strokeWidth={2.2} />
              </button>
              <span className="min-w-[92px] rounded-field bg-drop-surface2 px-4 py-2 text-center text-[15px] font-semibold text-drop-text tabular">
                {formatEuro(qr.priceMinor)}
              </span>
              <button
                type="button"
                onClick={() => set({ priceMinor: Math.min(5000, qr.priceMinor + 50) })}
                disabled={qr.priceMinor >= 5000}
                aria-label={t('wizardIncrease')}
                className="flex h-9 w-9 items-center justify-center rounded-field border border-drop-line text-drop-muted transition hover:border-drop-accent/50 hover:text-drop-accent active:scale-95 disabled:opacity-35"
              >
                <Icon name="plus" size={14} strokeWidth={2.2} />
              </button>
            </div>

            <div className="flex flex-wrap gap-2">
              {PRICE_PRESETS.map((preset) => (
                <button
                  key={preset}
                  type="button"
                  onClick={() => set({ priceMinor: preset })}
                  aria-pressed={qr.priceMinor === preset}
                  className={`rounded-full px-3 py-1.5 text-[11px] transition ${
                    qr.priceMinor === preset
                      ? 'bg-drop-accent/10 text-drop-accent ring-1 ring-drop-accent/60'
                      : 'bg-drop-surface2 text-drop-muted hover:text-drop-text'
                  }`}
                >
                  {formatEuro(preset, 0)}
                </button>
              ))}
            </div>

            <span className="text-[11px] text-drop-dim">
              {t('wizardClubGets')} {formatEuro(clubShare)}
            </span>
          </div>
          {errors.priceMinor && (
            <p className="text-[11px] text-red-400" role="alert">
              {errors.priceMinor}
            </p>
          )}
        </div>

        {/* Zones */}
        <div className="flex flex-col gap-3 border-t border-drop-line pt-5">
          <span className="connect-mono-label">{t('wizardZones')}</span>
          <div className="wizard-zone-entry flex gap-2">
            <input
              value={zoneDraft}
              onChange={(event) => setZoneDraft(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === 'Enter') {
                  event.preventDefault();
                  addZone();
                }
              }}
              placeholder={t('wizardZonePlaceholder')}
              className="connect-field flex-1"
            />
            <button
              type="button"
              onClick={addZone}
              disabled={!zoneDraft.trim()}
              className="connect-btn-ghost h-12 px-4"
            >
              <Icon name="plus" size={14} strokeWidth={2.2} />
              {t('wizardAdd')}
            </button>
          </div>

          <div className="flex flex-wrap gap-2">
            {qr.zones.map((zone) => (
              <span
                key={zone}
                className="inline-flex items-center gap-2 rounded-full border border-drop-accent/40 bg-drop-accent/[0.08] px-3 py-1.5 text-[11px] text-drop-accent"
              >
                <Icon name="qr" size={11} />
                {zone}
                <button
                  type="button"
                  aria-label={`${t('wizardRemoveZone')}: ${zone}`}
                  onClick={() => set({ zones: qr.zones.filter((z) => z !== zone) })}
                  className="text-drop-accent/60 transition-colors hover:text-drop-accent"
                >
                  <Icon name="x" size={11} strokeWidth={2.4} />
                </button>
              </span>
            ))}
          </div>
          {errors.zones && (
            <p className="text-[11px] text-red-400" role="alert">
              {errors.zones}
            </p>
          )}
        </div>

        {/* QR kit preview */}
        <div className="flex flex-col gap-4 border-t border-drop-line pt-5 sm:flex-row sm:items-center">
          <QrPreview seed={`${draft.club.name}-${qr.zones.length}`} />
          <div className="flex-1">
            <p className="text-[13px] text-drop-text">{t('wizardQrKit')}</p>
            <p className="mt-1.5 text-[11px] leading-relaxed text-drop-muted">
              {t('wizardQrKitCopy')}
            </p>
          </div>
        </div>

        {/* Terms */}
        <label className="flex cursor-pointer items-start gap-3 border-t border-drop-line pt-5">
          <span
            className={`mt-0.5 flex h-4 w-4 shrink-0 items-center justify-center rounded-[var(--r-control)] border transition-all duration-200 ${
              qr.accepted
                ? 'border-drop-accent bg-drop-accent text-drop-ink'
                : 'border-drop-line text-transparent'
            }`}
          >
            <Icon name="check" size={10} strokeWidth={3} />
          </span>
          <input
            type="checkbox"
            className="sr-only"
            checked={qr.accepted}
            onChange={(event) => set({ accepted: event.target.checked })}
          />
          <span className="text-[12px] leading-relaxed text-drop-muted">
            {t('wizardAccept')}
          </span>
        </label>
        {errors.accepted && (
          <p className="text-[11px] text-red-400" role="alert">
            {errors.accepted}
          </p>
        )}

        {submitError && (
          <p className="rounded-field border border-red-500/30 bg-red-500/10 px-3 py-2 text-[11px] text-red-300">
            {submitError}
          </p>
        )}
      </FormCard>

      {/* Tariff summary */}
      <aside className="w-full shrink-0 rounded-card border border-drop-line bg-drop-surface p-6 lg:w-[380px]">
        <header className="flex items-center gap-2.5">
          <span className="text-drop-accent">
            <Icon name="wallet" size={16} />
          </span>
          <h2 className="connect-mono-label flex-1 text-drop-muted">{t('wizardPriceSummary')}</h2>
        </header>

        <div className="mt-5 flex items-end justify-between border-b border-drop-line pb-4">
          <div>
            <p className="text-[11px] text-drop-dim">{t('wizardClubReceives')}</p>
            <p className="mt-1 font-display text-[30px] leading-none text-drop-accent tabular">
              {formatEuro(clubShare)}
            </p>
          </div>
          <p className="text-right text-[11px] text-drop-dim">
            {t('wizardGuestPays')}
            <br />
            <span className="text-drop-text">{formatEuro(qr.priceMinor)}</span>
          </p>
        </div>

        <div className="mt-4 flex flex-col gap-2.5">
          {[
            { label: t('wizardSetup'), value: '0 €' },
            { label: t('wizardService'), value: '0.30 €' },
            { label: t('wizardSubscription'), value: t('wizardNone') },
            { label: t('wizardPayouts'), value: t('wizardWeekly') },
            { label: t('wizardZonesCount'), value: String(qr.zones.length) },
          ].map((row) => (
            <div key={row.label} className="flex items-center justify-between">
              <span className="font-mono text-[10px] uppercase tracking-[1px] text-drop-dim">
                {row.label}
              </span>
              <span className="text-[12px] text-drop-text">{row.value}</span>
            </div>
          ))}
        </div>

        <p className="mt-5 flex items-start gap-2.5 border-t border-drop-line pt-4 text-[11px] leading-relaxed text-drop-muted">
          <span className="mt-0.5 shrink-0 text-drop-dim">
            <Icon name="shield" size={14} />
          </span>
          {t('wizardApplicationReview')}
        </p>
      </aside>
    </WizardPage>
  );
}
