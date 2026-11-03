import { useEffect, useState, type FormEvent } from 'react';
import { Link, useNavigate, useParams } from 'react-router-dom';
import {
  acceptClubInvite,
  ApiRequestError,
  getClubInvite,
  type ClubInvitePreview,
} from '../api/client';
import {
  AuthAlert,
  AuthScreen,
  AuthSummary,
  PasswordField,
} from '../components/auth/AuthKit';
import { useLocale } from '../locale';

/**
 * Club invitation activation — `/club/invite/:token` (docs/onboarding-contract.md).
 *
 * The link is the only proof that the platform approved this application, and
 * it is single-use. The screen therefore shows the facts of the application
 * read-only (club, city, e-mail) and asks for the one thing the platform does
 * not have: the password. A successful accept returns a TokenPair and signs the
 * club in, so the next screen is the panel, not the login form.
 *
 * `404 invite_not_found` is one answer for unknown, used and expired links: the
 * page must not try to explain which of the three it was. The legacy
 * `/club/invite?token=` form is redirected to this path by `App.tsx`.
 */

interface InviteError {
  message: string;
  code?: string;
  hint?: string;
}

type InviteState =
  | { status: 'loading' }
  | { status: 'ready'; preview: ClubInvitePreview }
  | { status: 'error'; notFound: boolean; code?: string };

const MIN_PASSWORD = 8;

export default function ClubInvitePage() {
  const { t } = useLocale();
  const navigate = useNavigate();
  const { token = '' } = useParams();
  const [state, setState] = useState<InviteState>({ status: 'loading' });
  const [password, setPassword] = useState('');
  const [confirmation, setConfirmation] = useState('');
  const [error, setError] = useState<InviteError | null>(null);
  const [submitting, setSubmitting] = useState(false);

  useEffect(() => {
    let cancelled = false;
    const trimmed = token.trim();
    if (trimmed === '') {
      setState({ status: 'error', notFound: true });
      return () => { cancelled = true; };
    }
    setState({ status: 'loading' });
    getClubInvite(trimmed)
      .then((preview) => {
        if (!cancelled) setState({ status: 'ready', preview });
      })
      .catch((err: unknown) => {
        if (cancelled) return;
        const apiError = err instanceof ApiRequestError ? err : null;
        setState({
          status: 'error',
          notFound: apiError
            ? apiError.status === 404 || apiError.code === 'invite_not_found'
            : false,
          code: apiError?.code,
        });
      });
    return () => {
      cancelled = true;
    };
  }, [token]);

  const submit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setError(null);
    if (password.length < MIN_PASSWORD) {
      setError({ message: t('auth.passwordTooShort') });
      return;
    }
    if (password !== confirmation) {
      setError({ message: t('auth.passwordMismatch') });
      return;
    }
    setSubmitting(true);
    try {
      await acceptClubInvite(token.trim(), password);
      navigate('/dashboard', { replace: true });
    } catch (err) {
      if (err instanceof ApiRequestError) {
        if (err.status === 404 || err.code === 'invite_not_found') {
          setState({ status: 'error', notFound: true, code: err.code });
          return;
        }
        if (err.code === 'validation_error' || err.code === 'validation_failed') {
          setError({
            message: err.details?.field === 'password'
              ? t('auth.passwordTooShort')
              : t('auth.validationFailed'),
            code: err.code,
          });
          return;
        }
        setError({ message: t('auth.inviteFailed'), code: err.code });
        return;
      }
      setError({
        message: err instanceof Error ? t('auth.serverNoAnswer') : t('auth.inviteFailed'),
      });
    } finally {
      setSubmitting(false);
    }
  };

  const links = (
    <Link to="/login" className="auth-link auth-link-muted">
      {t('auth.backToSignIn')}
    </Link>
  );

  if (state.status === 'loading') {
    return (
      <AuthScreen role={t('auth.clubPanel')} title={t('auth.inviteTitle')} links={links}>
        <div className="auth-form">
          <p className="auth-hint">{t('auth.inviteLoading')}</p>
        </div>
      </AuthScreen>
    );
  }

  if (state.status === 'error') {
    return (
      <AuthScreen role={t('auth.clubPanel')} title={t('auth.inviteTitle')} links={links}>
        <div className="auth-form">
          <AuthAlert
            tone="error"
            title={state.notFound ? t('auth.inviteNotFoundTitle') : t('auth.inviteLoadFailed')}
            code={state.code}
          >
            {state.notFound ? t('auth.inviteNotFoundHint') : null}
          </AuthAlert>
        </div>
      </AuthScreen>
    );
  }

  const { preview } = state;
  return (
    <AuthScreen role={t('auth.clubPanel')} title={t('auth.inviteTitle')} links={links}>
      <AuthSummary
        rows={[
          { label: t('auth.inviteSummaryClub'), value: preview.club_name },
          { label: t('auth.cityLabel'), value: preview.city },
          { label: t('auth.email'), value: preview.email || preview.contact || '—' },
        ]}
      />
      <form className="auth-form" onSubmit={submit} noValidate>
        <PasswordField
          id="club-invite-password"
          label={t('auth.password')}
          autoComplete="new-password"
          value={password}
          onChange={(value) => {
            setPassword(value);
            setError(null);
          }}
        />
        <PasswordField
          id="club-invite-confirm"
          label={t('auth.confirmPassword')}
          autoComplete="new-password"
          value={confirmation}
          onChange={(value) => {
            setConfirmation(value);
            setError(null);
          }}
        />

        {error ? (
          <AuthAlert tone="error" title={error.message} code={error.code}>
            {error.hint}
          </AuthAlert>
        ) : null}

        <button type="submit" className="btn-primary" disabled={submitting}>
          {submitting ? t('auth.activating') : t('auth.activate')}
        </button>
      </form>
    </AuthScreen>
  );
}