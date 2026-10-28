import { useState, type FormEvent } from 'react';
import { useLocation, useNavigate } from 'react-router-dom';
import {
  AdminAuthAlert,
  AdminAuthScreen,
  AdminOtpInput,
  AdminPasswordField,
} from '../components/auth/AdminAuth';
import { useLocale } from '../locale';
import {
  login,
  login2fa,
  ApiRequestError,
  type AdminLoginRequest,
  type TotpChallenge,
} from '../api/client';

/**
 * POST /v1/auth/admin/login (email + password) — admin 2FA is MANDATORY
 * (Part I §3), so the first step always answers with TotpChallenge;
 * step 2 POSTs /v1/auth/admin/2fa {email, totp_code}.
 */
export default function LoginPage() {
  const { t } = useLocale();
  const navigate = useNavigate();
  const location = useLocation();
  const requestedPath = (location.state as { from?: unknown } | null)?.from;
  const returnTo = typeof requestedPath === 'string' && /^\/(dashboard|clubs|djs|guests|payments|payouts|incidents|audit-log|platform-settings|admin)(?:[/?#]|$)/.test(requestedPath) ? requestedPath : '/payouts';
  const [form, setForm] = useState<AdminLoginRequest>({
    email: '',
    password: '',
  });
  const [error, setError] = useState<string | null>(null);
  const [code, setCode] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);

  // Step 2 state: password passed, 6-digit TOTP code required.
  const [challenge, setChallenge] = useState<TotpChallenge | null>(null);
  const [totpCode, setTotpCode] = useState('');

  const onSubmitPassword = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setError(null);
    setCode(null);
    setSubmitting(true);
    try {
      const result = await login(form);
      if ('mfa_required' in result) {
        setChallenge(result);
        return;
      }
      // Defensive: the backend mandates 2FA, but the contract allows a
      // direct TokenPair when no secret is configured.
      navigate(returnTo, { replace: true });
    } catch (err) {
      if (err instanceof ApiRequestError) {
        setError(err.message);
        setCode(err.code);
      } else if (err instanceof Error) {
        setError(err.message);
      } else {
        setError(t('login.failed'));
      }
    } finally {
      setSubmitting(false);
    }
  };

  const onSubmitTotp = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setError(null);
    setCode(null);
    setSubmitting(true);
    try {
      await login2fa({ email: form.email, totp_code: totpCode });
      navigate(returnTo, { replace: true });
    } catch (err) {
      if (err instanceof ApiRequestError) {
        setError(err.message);
        setCode(err.code);
      } else if (err instanceof Error) {
        setError(err.message);
      } else {
        setError(t('login.verificationFailed'));
      }
    } finally {
      setSubmitting(false);
    }
  };

  const errorBox = error ? (
    <AdminAuthAlert tone="error" title={error} code={code} />
  ) : null;

  return (
    <AdminAuthScreen
      title={challenge ? t('login.twoFactor') : t('login.signIn')}
      subtitle={
        challenge
          ? t('login.twoFactorHint')
          : t('login.note')
      }
      footer={
        <>
          <a className="auth-link auth-link-muted" href="/login">Club panel sign-in</a>
          <span className="auth-foot-sep" aria-hidden="true">·</span>
          <a className="auth-link auth-link-muted" href="mailto:ops@dropit.music">Request access</a>
        </>
      }
    >
      {challenge ? (
        <form className="auth-form" onSubmit={onSubmitTotp} noValidate>
          <div className="auth-field">
            <span className="field-label">{t('login.twoFactor')}</span>
            <AdminOtpInput value={totpCode} onChange={setTotpCode} disabled={submitting} />
          </div>
          {errorBox}
          <button
            type="submit"
            className="btn-primary"
            disabled={submitting || totpCode.length !== 6}
          >
            {submitting ? t('login.verifying') : t('login.verifyCode')}
          </button>
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
            {t('login.backToPassword')}
          </button>
        </form>
      ) : (
        <form className="auth-form" onSubmit={onSubmitPassword} noValidate>
          <div className="auth-field">
            <label htmlFor="email" className="field-label">{t('login.email')}</label>
            <input
              id="email"
              type="email"
              autoComplete="username"
              className="field-input auth-input"
              value={form.email}
              onChange={(event) => setForm({ ...form, email: event.target.value })}
            />
          </div>
          <AdminPasswordField
            id="password"
            label={t('login.password')}
            autoComplete="current-password"
            value={form.password}
            onChange={(value) => setForm({ ...form, password: value })}
          />
          {errorBox}
          <button
            type="submit"
            className="btn-primary"
            disabled={submitting || form.email === '' || form.password === ''}
          >
            {submitting ? t('login.signingIn') : t('login.signIn')}
          </button>
        </form>
      )}
    </AdminAuthScreen>
  );
}
