import { useState, type FormEvent } from 'react';
import { Link, useLocation, useNavigate } from 'react-router-dom';
import { ApiRequestError, djLogin, djLogin2fa, type LoginRequest, type TotpChallenge } from '../api/client';
import { AuthAlert, AuthScreen, OtpInput, PasswordField } from '../components/auth/AuthKit';
import { useLocale } from '../locale';

/**
 * DJ sign-in — the same skin as the club screen (AuthKit, skin A) with the DJ
 * role label and the DJ-only paths under the form. A DJ arrives mid-night with
 * a phone in one hand, so this screen has nothing on it except the way in.
 */
export default function DjLoginPage() {
  const { t } = useLocale();
  const navigate = useNavigate();
  const location = useLocation();
  const requested = (location.state as { from?: unknown } | null)?.from;
  const returnTo = typeof requested === 'string'
    && (requested === '/dj' || requested.startsWith('/dj/') || requested.startsWith('/dj?') || requested.startsWith('/dj#'))
    ? requested
    : '/dj';

  const [form, setForm] = useState<LoginRequest>({ email: '', password: '' });
  const [challenge, setChallenge] = useState<TotpChallenge | null>(null);
  const [totpCode, setTotpCode] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [code, setCode] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);

  const submit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setError(null);
    setCode(null);
    setSubmitting(true);
    try {
      const result = challenge
        ? await djLogin2fa({ email: form.email, totp_code: totpCode })
        : await djLogin(form);
      if (!challenge && 'mfa_required' in result) {
        setChallenge(result);
        return;
      }
      navigate(returnTo, { replace: true });
    } catch (err) {
      setError(err instanceof ApiRequestError ? err.message : t('auth.signInFailed'));
      setCode(err instanceof ApiRequestError ? err.code : null);
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <AuthScreen
      role={t('auth.djConsole')}
      title={challenge ? t('auth.enterCode') : t('auth.signIn')}
      subtitle={
        challenge
          ? t('auth.twoFactorFor', { email: form.email })
          : t('auth.djSignInSubtitle')
      }
      links={
        <>
          <Link to="/reset-password?role=dj" className="auth-link">
            {t('auth.forgotPassword')}
          </Link>
          <Link to="/register?role=dj" className="auth-link auth-link-muted">
            {t('auth.djSignUp')}
          </Link>
          <Link to="/login" className="auth-link auth-link-muted">
            {t('auth.clubPanel')}
          </Link>
        </>
      }
      footer={t('auth.djInviteFooter')}
    >
      <form className="auth-form" onSubmit={submit} noValidate>
        {challenge ? (
          <div className="auth-field">
            {/* The six cells carry `auth-otp` (the accent rule); the two-field
                step below uses the same 16px rhythm, so the step change only
                swaps the controls and never the spacing between them. */}
            <span className="field-label">{t('auth.authenticationCode')}</span>
            <OtpInput value={totpCode} onChange={setTotpCode} disabled={submitting} />
          </div>
        ) : (
          <>
            <div className="auth-field">
              <label htmlFor="dj-email" className="field-label">{t('auth.email')}</label>
              <input
                id="dj-email"
                type="email"
                autoComplete="username"
                className="field-input auth-input"
                value={form.email}
                onChange={(event) => setForm({ ...form, email: event.target.value })}
              />
            </div>
            <PasswordField
              id="dj-password"
              label={t('auth.password')}
              autoComplete="current-password"
              value={form.password}
              onChange={(value) => setForm({ ...form, password: value })}
            />
          </>
        )}

        {error ? <AuthAlert tone="error" title={error} code={code} /> : null}

        <button
          type="submit"
          className="btn-primary"
          disabled={submitting || (challenge ? totpCode.length !== 6 : !form.email || !form.password)}
        >
          {submitting
            ? t('auth.signingIn')
            : challenge
              ? t('auth.verifyAndOpenConsole')
              : t('auth.openConsole')}
        </button>
        {challenge ? (
          <button
            type="button"
            className="btn-quiet"
            onClick={() => {
              setChallenge(null);
              setTotpCode('');
              setError(null);
              setCode(null);
            }}
          >
            {t('auth.useAnotherAccount')}
          </button>
        ) : null}
      </form>
    </AuthScreen>
  );
}