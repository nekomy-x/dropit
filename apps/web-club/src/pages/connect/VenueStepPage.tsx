import { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import WizardPage from '../../components/connect/WizardPage';
import { Field, FormCard } from '../../components/connect/WizardKit';
import Icon from '../../components/connect/Icons';
import { useConnectDraft } from '../../connect/useConnectDraft';
import {
  VENUE_TYPES,
  hasErrors,
  validateVenue,
  type FieldErrors,
  type VenueStepData,
} from '../../connect/types';
import { translateValidationMessage, useLocale } from '../../connect/locale';

/** Step 02 — the physical venue: name, type, address, city/district, capacity. */
export default function VenueStepPage() {
  const navigate = useNavigate();
  const { draft, patchVenue, savedLabel } = useConnectDraft();
  const { t } = useLocale();
  const venue = draft.venue;
  const [errors, setErrors] = useState<FieldErrors<VenueStepData>>({});

  const set = (patch: Partial<VenueStepData>) => {
    patchVenue(patch);
    setErrors((prev) => {
      const next = { ...prev };
      for (const key of Object.keys(patch) as (keyof VenueStepData)[]) delete next[key];
      return next;
    });
  };

  const handleNext = () => {
    const nextErrors = Object.fromEntries(Object.entries(validateVenue(venue)).map(([key, message]) => [key, translateValidationMessage(message, t)])) as FieldErrors<VenueStepData>;
    setErrors(nextErrors);
    if (!hasErrors(nextErrors)) navigate('/connect/qr');
  };

  const capacity = Number(venue.capacity) || 0;
  const seats = Number(venue.seats) || 0;
  const area = Number(venue.area) || 0;

  return (
    <WizardPage
      step={2}
      title={t('wizardVenueTitle')}
      subtitle={t('wizardVenueSubtitle')}
      backTo="/connect/club"
      backLabel={`${t('wizardBack')}: ${t('wizardClub')}`}
      hint={savedLabel ? `${t('wizardSaved')} ${savedLabel}` : t('wizardAuto')}
      nextLabel={t('wizardNextQr')}
      onNext={handleNext}
    >
      <FormCard
        eyebrow={t('wizardVenueEyebrow')}
        title={t('wizardVenueFormTitle')}
        number="02"
        icon="pin"
      >
        <div className="grid gap-5 sm:grid-cols-[minmax(0,2fr)_minmax(0,1fr)]">
          <Field
            label={t('wizardVenueName')}
            value={venue.venueName}
            onChange={(venueName) => set({ venueName })}
            placeholder="Kater Blau"
            error={errors.venueName}
          />
          <Field
            label={t('wizardVenueType')}
            value={venue.venueType}
            onChange={(value) => set({ venueType: value as VenueStepData['venueType'] })}
            options={VENUE_TYPES}
          />
        </div>

        <Field
          label={t('wizardAddress')}
          value={venue.address}
          onChange={(address) => set({ address })}
          placeholder="Köpenicker Str. 16–17, 10179 Berlin"
          error={errors.address}
          icon="pin"
          autoComplete="street-address"
        />

        <div className="grid gap-5 sm:grid-cols-2">
          <Field
            label={t('wizardCity')}
            value={venue.city}
            onChange={(city) => set({ city })}
            placeholder="Berlin"
            error={errors.city}
            icon="globe"
            autoComplete="address-level2"
          />
          <Field
            label={t('wizardDistrict')}
            value={venue.district}
            onChange={(district) => set({ district })}
            placeholder="Kreuzberg"
            autoComplete="address-level3"
          />
        </div>

        <div className="grid gap-5 border-t border-drop-line pt-5 sm:grid-cols-3">
          <Field
            label={t('wizardCapacity')}
            value={venue.capacity}
            onChange={(capacityValue) => set({ capacity: capacityValue.replace(/[^\d]/g, '') })}
            placeholder="450"
            error={errors.capacity}
            inputMode="numeric"
            hint={t('wizardGuestsNight')}
            icon="users"
          />
          <Field
            label={t('wizardSeats')}
            value={venue.seats}
            onChange={(seatsValue) => set({ seats: seatsValue.replace(/[^\d]/g, '') })}
            placeholder="120"
            error={errors.seats}
            inputMode="numeric"
            hint={t('wizardOptional')}
          />
          <Field
            label={t('wizardArea')}
            value={venue.area}
            onChange={(areaValue) => set({ area: areaValue.replace(/[^\d]/g, '') })}
            placeholder="620"
            error={errors.area}
            inputMode="numeric"
            hint={t('wizardAreaOptional')}
          />
        </div>
      </FormCard>

      {/* Live summary of what the guests will see in the room list. */}
      <aside className="w-full shrink-0 rounded-card border border-drop-line bg-drop-surface p-6 lg:w-[380px]">
        <header className="flex items-center gap-2.5">
          <span className="text-drop-accent">
            <Icon name="pin" size={16} />
          </span>
          <h2 className="connect-mono-label flex-1 text-drop-muted">{t('wizardVenueSummary')}</h2>
          <span className="rounded-full border border-drop-line px-2 py-0.5 font-mono text-[8px] uppercase tracking-[1px] text-drop-dim">
            {venue.venueName.trim() ? t('wizardComplete') : t('wizardDraft')}
          </span>
        </header>

        <div className="mt-5">
          <p className="text-[18px] font-semibold text-drop-text">
            {venue.venueName.trim() || t('wizardVenueNamePlaceholder')}
          </p>
          <p className="mt-1 text-[12px] leading-relaxed text-drop-muted">
            {[venue.district, venue.city].filter(Boolean).join(' · ') || t('wizardDistrictCity')}
            {venue.address.trim() && (
              <>
                <br />
                {venue.address}
              </>
            )}
          </p>
        </div>

        <div className="mt-5 border-t border-drop-line pt-4">
          {[
            { label: t('wizardCapacity'), value: capacity ? `${capacity} ${t('wizardGuestsNight')}` : '—' },
            { label: t('wizardSeats'), value: seats ? String(seats) : '—' },
            { label: t('wizardArea'), value: area ? `${area} m²` : '—' },
            { label: t('wizardVenueType'), value: venue.venueType },
          ].map((row) => (
            <div key={row.label} className="flex items-center justify-between py-1.5">
              <span className="font-mono text-[10px] uppercase tracking-[1px] text-drop-dim">
                {row.label}
              </span>
              <span className="text-[12px] text-drop-text tabular">{row.value}</span>
            </div>
          ))}
        </div>

        <p className="mt-4 flex items-start gap-2.5 border-t border-drop-line pt-4 text-[11px] leading-relaxed text-drop-muted">
          <span className="mt-0.5 shrink-0 text-drop-dim">
            <Icon name="info" size={14} />
          </span>
          {t('wizardZonesLater')}
        </p>
      </aside>
    </WizardPage>
  );
}
