import { useState, type FormEvent } from 'react';
import { Link, useSearchParams } from 'react-router-dom';
import { ApiRequestError, requestPasswordReset, type ResetRole } from '../api/client';
import {
  AuthAlert,
  AuthFacts,
  AuthWizard,
  type WizardStep,
} from '../components/auth/AuthKit';
import { useLocale, type Translate } from '../locale';

/**
 * Password reset — request a link (skin B, step 1 of 2).
 *
 * POST /v1/auth/password/reset-request always answers 202 for a well-formed
 * request, whether or not the address exists (no user enumeration), so the
 * success state must not claim "account found". Two honest outcomes:
 *
 *   - production: "if that address has an account, a link is on its way";
 *   - this stand has no mailer, so when the server has `DROPIT_RESET_REVEAL_TOKEN`
 *     on it hands the link back in the 202 body and the page shows it instead of
 *     telling the person to check an inbox that will stay empty.
 *
 * A 503 means the endpoint is not deployed on this server. That is a state of
 * the stand, not a typo by the person, and it is reported as such.
 */
const STEPS = (t: Translate): ReadonlyArray<WizardStep> => [
  { title: t('auth.resetEmailStep'), note: t('auth.resetEmailStepNote') },
  { title: t('auth.resetLinkStep'), note: t('auth.resetLinkStepNote') },
];

const ROLE_LABELS: ReadonlyArray<{ value: ResetRole; labelKey: string }> = [
  { value: 'club', labelKey: 'auth.resetRoleClub' },
  { value: 'dj', labelKey: 'auth.resetRoleDj' },
  { value: 'admin', labelKey: 'auth.resetRoleAdmin' },
];

export default function ForgotPasswordPage() {
  const { t } = useLocale();
  const [searchParams] = useSearchParams();
  const requestedRole = searchParams.get('role');
  const [role, setRole] = useState<ResetRole>(
    requestedRole === 'dj' || requestedRole === 'admin' ? requestedRole : 'club',
  );
  const [email, setEmail] = useState('');
  const [submitting, setSubmitting] = useState(false);
  const [sent, setSent] = useState<{ resetUrl?: string; expiresAt?: string } | null>(null);
  const [error, setError] = useState<{ message: string; code?: string; hint?: string } | null>(null);

  const submit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setError(null);
    const address = email.trim();
    if (!/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(address)) {
      setError({ message: t('auth.mailInvalid'), hint: t('auth.mailInvalidHint') });
      return;
    }
    setSubmitting(true);
    try {
      const result = await requestPasswordReset({ email: address, role });
      setSent({ resetUrl: result.reset_url, expiresAt: result.expires_at });
    } catch (err) {
      if (err instanceof ApiRequestError) {
        if (err.status === 503 || err.code === 'external_service_unavailable') {
          setError({ message: t('auth.resetNotEnabled'), code: err.code, hint: t('auth.resetNotEnabledHint') });
        } else if (err.status === 429) {
          setError({ message: t('auth.tooManyRequests'), code: err.code, hint: t('auth.tooManyRequestsHint') });
        } else if (err.status === 400) {
          setError({ message: t('auth.mailNotAccepted'), code: err.code, hint: t('auth.mailNotAcceptedHint') });
        } else {
          setError({ message: err.message, code: err.code });
        }
      } else if (err instanceof Error) {
        setError({ message: t('auth.requestNotDelivered'), hint: err.message });
      } else {
        setError({ message: t('auth.resetRequestFailed') });
      }
    } finally {
      setSubmitting(false);
    }
  };

  const signInLink = (
    <Link to={role === 'dj' ? '/dj/login' : '/login'} className="auth-link">
      {t('auth.backToSignIn')}
    </Link>
  );

  if (sent) {
    return (
      <AuthWizard
        role={t('auth.recovery')}
        steps={STEPS(t)}
        step={2}
        title={t('auth.resetLinkTitle')}
        subtitle={sent.resetUrl ? t('auth.resetLinkShown') : t('auth.resetLinkSent', { email: email.trim() })}
        asideFooter={
          <AuthFacts
            facts={[
              { label: t('auth.resetLinkLivesLabel'), value: t('auth.resetLinkLivesValue') },
              { label: t('auth.resetNextLabel'), value: t('auth.resetNextOpen') },
            ]}
          />
        }
        footer={signInLink}
      >
        <div className="auth-form">
          {sent.resetUrl ? (
            <AuthAlert tone="note" title={t('auth.resetLinkTitle')}>
              <a className="auth-link font-mono" href={sent.resetUrl}>{sent.resetUrl}</a>
            </AuthAlert>
          ) : null}
          <button type="button" className="btn-quiet" onClick={() => setSent(null)}>
            {t('auth.useAnotherAddress')}
          </button>
        </div>
      </AuthWizard>
    );
  }

  return (
    <AuthWizard
      role={t('auth.recovery')}
      steps={STEPS(t)}
      step={1}
      title={t('auth.forgotTitle')}
      subtitle={t('auth.forgotSubtitle')}
      asideFooter={
        <AuthFacts
          facts={[
            { label: t('auth.resetLinkLivesLabel'), value: t('auth.resetLinkLivesValue') },
            { label: t('auth.resetLimitLabel'), value: t('auth.resetLimitValue') },
            { label: t('auth.resetAfterLabel'), value: t('auth.resetAfterValue') },
          ]}
        />
      }
      footer={signInLink}
    >
      <form className="auth-form" onSubmit={submit} noValidate>
        <div className="auth-field">
          <span className="field-label">{t('auth.resetAccountType')}</span>
          <div className="auth-switch" role="group" aria-label={t('auth.resetAccountType')}>
            {ROLE_LABELS.map((option) => (
              <button
                key={option.value}
                type="button"
                className="auth-switch-item"
                aria-pressed={role === option.value}
                onClick={() => { setRole(option.value); setError(null); }}
              >
                {t(option.labelKey)}
              </button>
            ))}
          </div>
        </div>

        <div className="auth-field">
          <label htmlFor="reset-email" className="field-label">{t('auth.workEmail')}</label>
          <input
            id="reset-email"
            type="email"
            autoComplete="username"
            className="field-input auth-input"
            value={email}
            onChange={(event) => setEmail(event.target.value)}
          />
        </div>

        {error ? (
          <AuthAlert tone="error" title={error.message} code={error.code}>
            {error.hint}
          </AuthAlert>
        ) : null}

        <button type="submit" className="btn-primary" disabled={submitting}>
          {submitting ? t('auth.requesting') : t('auth.sendResetLink')}
        </button>
      </form>
    </AuthWizard>
  );
}
