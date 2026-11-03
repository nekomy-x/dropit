import { useState, type FormEvent } from 'react';
import { Link, useNavigate, useParams } from 'react-router-dom';
import { ApiRequestError, resetPassword } from '../api/client';
import { AuthAlert, AuthScreen, PasswordField } from '../components/auth/AuthKit';
import { useLocale } from '../locale';

/**
 * Password reset — choose a new password.
 *
 * POST /v1/auth/password/reset consumes the token and revokes every refresh
 * token of the account: a reset is also a sign-out everywhere. The token is
 * single-use, so a double submit is reported as an invalid link rather than
 * silently succeeding twice.
 */

export default function ResetPasswordPage() {
  const { t } = useLocale();
  const { token = '' } = useParams();
  const navigate = useNavigate();
  const [password, setPassword] = useState('');
  const [confirmation, setConfirmation] = useState('');
  const [submitting, setSubmitting] = useState(false);
  const [done, setDone] = useState(false);
  const [error, setError] = useState<{ message: string; code?: string; hint?: string } | null>(null);

  const submit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setError(null);
    if (password.length < 8) {
      setError({ message: t('auth.passwordTooShort'), hint: t('auth.passwordTooShortHint') });
      return;
    }
    if (password !== confirmation) {
      setError({ message: t('auth.passwordsNoMatch') });
      return;
    }
    setSubmitting(true);
    try {
      await resetPassword({ token, password });
      setDone(true);
    } catch (err) {
      if (err instanceof ApiRequestError) {
        if (err.status === 401 || err.code === 'invalid_reset_token') {
          setError({
            message: t('auth.resetLinkInvalid'),
            code: err.code,
            hint: t('auth.resetLinkInvalidHint'),
          });
        } else if (err.status === 503) {
          setError({ message: t('auth.resetNotEnabled'), code: err.code });
        } else if (err.status === 400) {
          setError({ message: err.message, code: err.code, hint: t('auth.passwordTooShortHint') });
        } else {
          setError({ message: err.message, code: err.code });
        }
      } else if (err instanceof Error) {
        setError({ message: t('auth.requestNotDelivered'), hint: err.message });
      } else {
        setError({ message: t('auth.resetFailed') });
      }
    } finally {
      setSubmitting(false);
    }
  };

  if (done) {
    return (
      <AuthScreen
        role={t('auth.recovery')}
        title={t('auth.passwordChangedTitle')}
        subtitle={t('auth.passwordChangedSubtitle')}
        footer={<Link to="/login" className="auth-link">{t('auth.clubSignIn')}</Link>}
      >
        <div className="auth-form">
          <AuthAlert tone="ok" title={t('auth.passwordActive')}>
            {t('auth.passwordActiveBody')}
          </AuthAlert>
          <button type="button" className="btn-primary" onClick={() => navigate('/login', { replace: true })}>
            {t('auth.goToSignIn')}
          </button>
        </div>
      </AuthScreen>
    );
  }

  return (
    <AuthScreen
      role={t('auth.recovery')}
      title={t('auth.resetChooseTitle')}
      subtitle={t('auth.resetChooseSubtitle')}
      footer={
        <>
          <Link to="/reset-password" className="auth-link">{t('auth.requestNewLink')}</Link>
          <span className="auth-foot-sep" aria-hidden="true">·</span>
          <Link to="/login" className="auth-link auth-link-muted">{t('auth.backToSignIn')}</Link>
        </>
      }
    >
      <form className="auth-form" onSubmit={submit} noValidate>
        <PasswordField
          id="reset-new-password"
          label={t('auth.newPassword')}
          autoComplete="new-password"
          hint={t('auth.passwordMin8')}
          value={password}
          onChange={setPassword}
        />
        <div className="auth-field">
          <label htmlFor="reset-confirm-password" className="field-label">{t('auth.repeatPassword')}</label>
          <input
            id="reset-confirm-password"
            type="password"
            autoComplete="new-password"
            className="field-input auth-input"
            value={confirmation}
            onChange={(event) => setConfirmation(event.target.value)}
          />
        </div>

        {error ? (
          <AuthAlert tone="error" title={error.message} code={error.code}>
            {error.hint}
          </AuthAlert>
        ) : null}

        <button type="submit" className="btn-primary" disabled={submitting}>
          {submitting ? t('auth.saving') : t('auth.setNewPassword')}
        </button>
      </form>
    </AuthScreen>
  );
}
