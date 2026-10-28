import { useState, type FormEvent } from 'react';
import { Link, useLocation, useNavigate } from 'react-router-dom';
import {
  login,
  login2fa,
  masterLogin,
  ApiRequestError,
  type LoginRequest,
  type TotpChallenge,
} from '../api/client';
import { AuthAlert, AuthScreen, OtpInput, PasswordField } from '../components/auth/AuthKit';
import { useLocale } from '../locale';

/**
 * POST /v1/auth/club/login (email + password) → TokenPair | TotpChallenge;
 * on `mfa_required` the second step POSTs /v1/auth/club/2fa {email, totp_code}.
 *
 * Skin A (see components/auth/AuthKit.tsx): one card, two fields, one action.
 * The other ways into this account — registration and the one-time club access
 * token — are links under the form, not competing tabs above it.
 */
export default function LoginPage() {
  const { t } = useLocale();
  const navigate = useNavigate();
  const location = useLocation();
  const requested = (location.state as { from?: unknown } | null)?.from;
  const returnTo = typeof requested === 'string' && /^\/(dashboard|sessions|pricing|djs|earnings)(?:[/?#]|$)/.test(requested)
    ? requested
    : '/dashboard';

  const [form, setForm] = useState<LoginRequest>({ email: '', password: '' });
  const [error, setError] = useState<string | null>(null);
  const [code, setCode] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const [mode, setMode] = useState<'credentials' | 'master'>('credentials');
  const [masterToken, setMasterToken] = useState('');
  const [challenge, setChallenge] = useState<TotpChallenge | null>(null);
  const [totpCode, setTotpCode] = useState('');

  const authError = (err: unknown, fallback: string) => {
    if (err instanceof ApiRequestError) {
      setError(err.message);
      setCode(err.code);
    } else if (err instanceof Error) {
      setError(err.message);
    } else {
      setError(fallback);
    }
  };

  const onSubmitPassword = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setError(null);
    setCode(null);
    setSubmitting(true);
    try {
      const result = await login(form);
      if ('mfa_required' in result) {
        // Step 1 passed — a TOTP code is required (F1 contract, TotpChallenge).
        setChallenge(result);
        return;
      }
      navigate(returnTo, { replace: true });
    } catch (err) {
      authError(err, t('auth.loginFailed'));
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
      authError(err, t('auth.verifyFailed'));
    } finally {
      setSubmitting(false);
    }
  };

  const onSubmitMaster = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setError(null);
    setCode(null);
    setSubmitting(true);
    try {
      await masterLogin(masterToken.trim());
      navigate(returnTo, { replace: true });
    } catch (err) {
      authError(err, t('auth.tokenLoginFailed'));
    } finally {
      setSubmitting(false);
    }
  };

  const errorBox = error ? <AuthAlert tone="error" title={error} code={code} /> : null;

  return (
    <AuthScreen
      role={t('auth.clubPanel')}
      title={challenge ? t('auth.enterCode') : t('auth.signIn')}
      subtitle={
        challenge
          ? t('auth.twoFactorFor', { email: form.email })
          : t('auth.clubSignInSubtitle')
      }
      links={
        <>
          <Link to="/reset-password" className="auth-link">
            {t('auth.forgotPassword')}
          </Link>
          <Link to="/register" className="auth-link auth-link-muted">
            {t('auth.registerLink')}
          </Link>
          <Link to="/dj/login" className="auth-link auth-link-muted">
            {t('auth.djConsole')}
          </Link>
        </>
      }
      footer={
        mode === 'credentials'
          ? t('auth.sessionMemoryNote')
          : t('auth.tokenNote')
      }
    >
      {challenge ? (
        <form className="auth-form" onSubmit={onSubmitTotp} noValidate>
          <div className="auth-field">
            <span className="field-label">{t('auth.authenticationCode')}</span>
            <OtpInput value={totpCode} onChange={setTotpCode} disabled={submitting} />
          </div>
          {errorBox}
          <button type="submit" className="btn-primary" disabled={submitting || totpCode.length !== 6}>
            {submitting ? t('auth.verifying') : t('auth.verifyAndSignIn')}
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
            {t('auth.useAnotherAccount')}
          </button>
        </form>
      ) : mode === 'master' ? (
        <form className="auth-form" onSubmit={onSubmitMaster} noValidate>
          <div className="auth-field">
            <label htmlFor="master-token" className="field-label">{t('auth.clubAccessToken')}</label>
            <input
              id="master-token"
              type="text"
              autoComplete="off"
              className="field-input auth-input font-mono"
              value={masterToken}
              onChange={(event) => setMasterToken(event.target.value)}
            />
            <span className="auth-hint">{t('auth.clubAccessTokenHint')}</span>
          </div>
          {errorBox}
          <button type="submit" className="btn-primary" disabled={submitting || masterToken.trim() === ''}>
            {submitting ? t('auth.signingIn') : t('auth.signInWithToken')}
          </button>
          <button type="button" className="btn-quiet" onClick={() => { setMode('credentials'); setError(null); setCode(null); }}>
            {t('auth.useEmailPassword')}
          </button>
        </form>
      ) : (
        <form className="auth-form" onSubmit={onSubmitPassword} noValidate>
          <div className="auth-field">
            <label htmlFor="email" className="field-label">{t('auth.workEmail')}</label>
            <input
              id="email"
              type="email"
              autoComplete="username"
              className="field-input auth-input"
              value={form.email}
              onChange={(event) => setForm({ ...form, email: event.target.value })}
            />
          </div>
          <PasswordField
            id="password"
            label={t('auth.password')}
            autoComplete="current-password"
            value={form.password}
            onChange={(value) => setForm({ ...form, password: value })}
          />
          {errorBox}
          <button type="submit" className="btn-primary" disabled={submitting || form.email === '' || form.password === ''}>
            {submitting ? t('auth.signingIn') : t('auth.signIn')}
          </button>
          <div className="auth-links">
            <button
              type="button"
              className="auth-link auth-link-muted"
              onClick={() => { setMode('master'); setError(null); setCode(null); }}
            >
              {t('auth.signInWithClubToken')}
            </button>
          </div>
        </form>
      )}
    </AuthScreen>
  );
}