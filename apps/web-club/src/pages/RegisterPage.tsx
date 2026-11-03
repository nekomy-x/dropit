import { useEffect, useState, type FormEvent } from 'react';
import { Link, useNavigate } from 'react-router-dom';
import {
  ApiRequestError,
  applyClubOnboarding,
  isAuthenticated,
} from '../api/client';
import { AuthAlert, AuthScreen } from '../components/auth/AuthKit';
import BrandLogo from '../components/BrandLogo';
import { useLocale, type Translate } from '../locale';

/**
 * Club application — docs/onboarding-contract.md.
 *
 * Registration is closed: this screen submits `POST /v1/club/onboarding/apply`
 * and the club gets a one-time invitation link from the platform after the
 * application is approved. Nothing is created here and no password is asked
 * for — the owner sets it on the invitation page, where the application facts
 * are shown read-only.
 *
 * Four fields, one action, one column: name, city, e-mail, capacity. The guest
 * price is deliberately not asked for here — the club sets it in the panel
 * (`/pricing`) after activation, so the owner never names a price before the
 * invitation arrives. The application body therefore carries an empty `qr`
 * block; the endpoint still accepts a price from richer flows such as
 * `/connect`.
 *
 * The success state names the address the invitation will go to and says
 * nothing else; the DJ branch is one line pointing at the club invitation,
 * because a DJ cannot apply on their own.
 */

interface ApplicationForm {
  name: string;
  city: string;
  email: string;
  capacity: string;
}

interface ApplicationError {
  message: string;
  code?: string;
  hint?: string;
}

const EMPTY_FORM: ApplicationForm = {
  name: '',
  city: '',
  email: '',
  capacity: '',
};

const EMAIL_RE = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;
const MIN_CAPACITY = 1;
const MAX_CAPACITY = 100_000;

/** Field name from `details.field` → the message that names the same field. */
function messageForField(field: unknown, t: Translate): string | undefined {
  switch (field) {
    case 'club.name':
    case 'venue.name':
      return t('auth.clubNameRequired');
    case 'club.city':
    case 'venue.city':
      return t('auth.cityRequired');
    case 'club.contact':
      return t('auth.emailInvalid');
    case 'venue.capacity':
      return t('auth.capacityInvalid');
    default:
      return undefined;
  }
}

function describeApplicationError(err: unknown, t: Translate): ApplicationError {
  if (err instanceof ApiRequestError) {
    if (err.code === 'validation_error' || err.code === 'validation_failed') {
      const fieldMessage = messageForField(err.details?.field, t);
      return { message: fieldMessage ?? t('auth.validationFailed'), code: err.code };
    }
    if (err.code === 'rate_limited') {
      return { message: t('auth.rateLimited'), code: err.code, hint: t('auth.rateLimitedHint') };
    }
    return { message: t('auth.applicationFailed'), code: err.code };
  }
  if (err instanceof Error) return { message: t('auth.serverNoAnswer'), hint: err.message };
  return { message: t('auth.applicationFailed') };
}

export default function RegisterPage() {
  const { t } = useLocale();
  const navigate = useNavigate();
  const [form, setForm] = useState<ApplicationForm>(EMPTY_FORM);
  const [error, setError] = useState<ApplicationError | null>(null);
  const [submitting, setSubmitting] = useState(false);
  /** Set to the contact e-mail on success: the confirmation names it. */
  const [sentTo, setSentTo] = useState<string | null>(null);

  // A live session does not need the application form.
  useEffect(() => {
    if (isAuthenticated('dj')) navigate('/dj', { replace: true });
    else if (isAuthenticated()) navigate('/dashboard', { replace: true });
  }, [navigate]);

  const set = (patch: Partial<ApplicationForm>) => {
    setForm((current) => ({ ...current, ...patch }));
    setError(null);
  };

  const onSubmit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setError(null);

    const name = form.name.trim();
    const city = form.city.trim();
    const email = form.email.trim();
    if (name === '') return setError({ message: t('auth.clubNameRequired') });
    if (city === '') return setError({ message: t('auth.cityRequired') });
    if (!EMAIL_RE.test(email)) return setError({ message: t('auth.emailInvalid') });

    const capacityText = form.capacity.trim();
    const capacity = Number(capacityText);
    if (
      !/^\d+$/.test(capacityText)
      || !Number.isFinite(capacity)
      || capacity < MIN_CAPACITY
      || capacity > MAX_CAPACITY
    ) {
      return setError({ message: t('auth.capacityInvalid') });
    }

    setSubmitting(true);
    try {
      await applyClubOnboarding({
        club: {
          name,
          city,
          address: '',
          website: '',
          contact: email,
          genres: [],
        },
        venue: {
          name,
          type: 'club',
          address: '',
          city,
          district: '',
          capacity,
          seats: null,
          area_m2: null,
        },
        // No price: the owner sets the guest price in the panel after the
        // account is activated, so the application must not name one.
        qr: { zones: [] },
      });
      setSentTo(email);
    } catch (err) {
      setError(describeApplicationError(err, t));
    } finally {
      setSubmitting(false);
    }
  };

  // Photograph column (desktop only, see styles/auth.css): the room the
  // application is for, and the three steps the product runs on. Built here
  // rather than in the kit because no other auth screen carries it.
  const aside = (
    <>
      <Link to="/connect" className="auth-brand">
        <BrandLogo size="small" />
        <span className="auth-brand-role">{t('auth.clubPanel')}</span>
      </Link>
      <div className="auth-media-copy">
        <h2 className="auth-media-title">{t('auth.applyAsideTitle')}</h2>
        <ol className="auth-media-steps">
          <li className="auth-media-step">
            <span className="auth-media-step-index" aria-hidden="true">01</span>
            <span>{t('auth.applyAsideStepScan')}</span>
          </li>
          <li className="auth-media-step">
            <span className="auth-media-step-index" aria-hidden="true">02</span>
            <span>{t('auth.applyAsideStepPick')}</span>
          </li>
          <li className="auth-media-step">
            <span className="auth-media-step-index" aria-hidden="true">03</span>
            <span>{t('auth.applyAsideStepQueue')}</span>
          </li>
        </ol>
      </div>
    </>
  );

  return (
    <AuthScreen
      role={t('auth.clubPanel')}
      title={t('auth.applyTitle')}
      subtitle={t('auth.applySubtitle')}
      media={aside}
      links={
        <span className="auth-links-lead">
          {t('auth.haveAccount')}{' '}
          <Link to="/login" className="auth-link">{t('auth.signIn')}</Link>
        </span>
      }
    >
      {sentTo ? (
        <div className="auth-form">
          <AuthAlert tone="ok" title={t('auth.applicationSentTitle')}>
            {t('auth.applicationSentBody', { email: sentTo })}
          </AuthAlert>
        </div>
      ) : (
        <form className="auth-form" onSubmit={onSubmit} noValidate>
          <div className="auth-field">
            <label htmlFor="apply-name" className="field-label">{t('auth.clubNameLabel')}</label>
            <input
              id="apply-name"
              type="text"
              autoComplete="organization"
              className="field-input auth-input"
              value={form.name}
              onChange={(event) => set({ name: event.target.value })}
            />
          </div>

          <div className="auth-field">
            <label htmlFor="apply-city" className="field-label">{t('auth.cityLabel')}</label>
            <input
              id="apply-city"
              type="text"
              autoComplete="address-level2"
              className="field-input auth-input"
              value={form.city}
              onChange={(event) => set({ city: event.target.value })}
            />
          </div>

          <div className="auth-field">
            <label htmlFor="apply-email" className="field-label">{t('auth.email')}</label>
            <input
              id="apply-email"
              type="email"
              autoComplete="email"
              className="field-input auth-input"
              value={form.email}
              onChange={(event) => set({ email: event.target.value })}
            />
          </div>

          <div className="auth-field">
            <label htmlFor="apply-capacity" className="field-label">{t('auth.capacityLabel')}</label>
            <input
              id="apply-capacity"
              type="number"
              inputMode="numeric"
              min={MIN_CAPACITY}
              max={MAX_CAPACITY}
              step={1}
              className="field-input auth-input"
              value={form.capacity}
              onChange={(event) => set({ capacity: event.target.value })}
            />
          </div>

          {error ? (
            <AuthAlert tone="error" title={error.message} code={error.code}>
              {error.hint}
            </AuthAlert>
          ) : null}

          <button type="submit" className="btn-primary" disabled={submitting}>
            {submitting ? t('auth.sending') : t('auth.sendApplication')}
          </button>

          <p className="auth-aside">
            {t('auth.djInviteOnly')}{' '}
            <Link to="/dj/login" className="auth-link">{t('auth.djSignIn')}</Link>
          </p>
        </form>
      )}
    </AuthScreen>
  );
}
