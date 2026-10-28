import { useEffect, useMemo, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import WizardPage from '../../components/connect/WizardPage';
import { ChipGroup, Field, FormCard } from '../../components/connect/WizardKit';
import ChecklistCard from '../../components/connect/ChecklistCard';
import { useConnectDraft } from '../../connect/useConnectDraft';
import { GENRES, hasErrors, validateClub, type FieldErrors, type ClubStepData } from '../../connect/types';
import { prefillFromPanel } from '../../api/connect';
import { translateValidationMessage, useLocale } from '../../connect/locale';

/** Step 01 — club profile (name, city, address, links, party genres). */
export default function ClubStepPage() {
  const navigate = useNavigate();
  const { draft, patchClub, savedLabel } = useConnectDraft();
  const { t } = useLocale();
  const club = draft.club;
  const [errors, setErrors] = useState<FieldErrors<ClubStepData>>({});

  // A logged-in owner gets the form prefilled from /v1/club/profile.
  useEffect(() => {
    let cancelled = false;
    prefillFromPanel().then((profile) => {
      if (cancelled || !profile) return;
      patchClub({
        name: club.name || profile.name,
        city: club.city || profile.city,
        address: club.address || profile.address,
        contact: club.contact || profile.email,
      });
    });
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const set = (patch: Partial<ClubStepData>) => {
    patchClub(patch);
    setErrors((prev) => {
      const next = { ...prev };
      for (const key of Object.keys(patch) as (keyof ClubStepData)[]) delete next[key];
      return next;
    });
  };

  const checklist = useMemo(
    () => [
      {
          label: t('wizardName'),
          meta: club.name && club.city ? `${club.name} · ${club.city}` : t('wizardGuestVisible'),
        done: Boolean(club.name.trim() && club.city.trim()),
      },
      {
          label: t('wizardAddress'),
          meta: club.address || t('wizardStreetHint'),
        done: club.address.trim().length > 4,
      },
      {
          label: t('wizardWebsite'),
          meta: club.website || t('wizardProfileLink'),
        done: club.website.trim().length > 3,
      },
      {
          label: t('wizardContact'),
          meta: club.contact || t('wizardOwnerContactHint'),
        done: club.contact.trim().length > 4,
      },
      {
          label: t('wizardGenres'),
          meta: club.genres.length ? club.genres.join(' · ') : t('wizardGenreHint'),
        done: club.genres.length > 0,
      },
    ],
    [club],
  );

  const handleNext = () => {
    const nextErrors = Object.fromEntries(Object.entries(validateClub(club)).map(([key, message]) => [key, translateValidationMessage(message, t)])) as FieldErrors<ClubStepData>;
    setErrors(nextErrors);
    if (!hasErrors(nextErrors)) navigate('/connect/venue');
  };

  return (
    <WizardPage
      step={1}
      title={t('wizardClub')}
      subtitle={t('wizardClubSubtitle')}
      backTo="/connect"
      backLabel={`${t('wizardBack')}: ${t('wizardOverview')}`}
      hint={savedLabel ? `${t('wizardSaved')} ${savedLabel}` : t('wizardAuto')}
      nextLabel={t('wizardNext')}
      onNext={handleNext}
    >
      <FormCard
        eyebrow={t('wizardClubEyebrow')}
        title={t('wizardClubTitle')}
        number="01"
        icon="store"
      >
        <div className="grid gap-5 sm:grid-cols-2">
          <Field
            label={t('wizardName')}
            value={club.name}
            onChange={(name) => set({ name })}
            placeholder="Kater Blau"
            error={errors.name}
            autoComplete="organization"
          />
          <Field
            label={t('wizardCity')}
            value={club.city}
            onChange={(city) => set({ city })}
            placeholder="Berlin"
            error={errors.city}
            autoComplete="address-level2"
          />
        </div>

        <Field
          label={t('wizardAddress')}
          value={club.address}
          onChange={(address) => set({ address })}
          placeholder="Revaler Str. 99, 10245 Berlin"
          error={errors.address}
          icon="pin"
          autoComplete="street-address"
        />

        <div className="grid gap-5 sm:grid-cols-2">
          <Field
          label={t('wizardWebsite')}
            value={club.website}
            onChange={(website) => set({ website })}
            placeholder="katerblau.de"
            error={errors.website}
            icon="globe"
            autoComplete="url"
          />
          <Field
          label={t('wizardContact')}
            value={club.contact}
            onChange={(contact) => set({ contact })}
            placeholder="owner@katerblau.de"
            error={errors.contact}
            icon="mail"
            autoComplete="email"
          />
        </div>

        <div className="flex flex-col gap-2">
          <span className="connect-mono-label">{t('wizardGenres')}</span>
          <ChipGroup
            options={GENRES}
            selected={club.genres}
            error={errors.genres}
            onToggle={(genre) => {
              const next = club.genres.includes(genre as never)
                ? club.genres.filter((g) => g !== genre)
                : [...club.genres, genre as never];
              set({ genres: next });
            }}
          />
        </div>
      </FormCard>

      <ChecklistCard
        title={t('wizardChecklist')}
        items={checklist}
        note={t('wizardReview')}
      />
    </WizardPage>
  );
}
