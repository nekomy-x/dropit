import { useEffect, useState, type FormEvent } from 'react';
import { Link, useNavigate, useParams } from 'react-router-dom';
import {
  acceptDjInvite,
  ApiRequestError,
  getDjInvite,
  type DjInvitePreview,
} from '../api/client';
import {
  AuthAlert,
  AuthScreen,
  AuthSummary,
  PasswordField,
} from '../components/auth/AuthKit';
import { useLocale } from '../locale';

/**
 * DJ invitation activation — `/dj/invite/:token` (docs/onboarding-contract.md).
 *
 * A club invites a DJ; the link names the DJ and the club and the DJ sets a
 * password. The invite endpoints take no TokenPair, so activation ends on the
 * DJ sign-in screen rather than inside the console.
 *
 * The index is the legacy `?token=` form (`App.tsx` redirects it to the path)
 * because mail already delivered to inboxes uses it. The route uses the same
 * card as every other auth screen instead of the old standalone page: one
 * visual language, tokens only.
 */

interface InviteError {
  message: string;
  code?: string;
  hint?: string;
}

type InviteState =
  | { status: 'loading' }
  | { status: 'ready'; preview: DjInvitePreview }
  | { status: 'error'; notFound: boolean; code?: string };

const MIN_PASSWORD = 10;

export default function DjInvitePage() {
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
    getDjInvite(trimmed)
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
      setError({ message: t('auth.passwordMin10') });
      return;
    }
    if (password !== confirmation) {
      setError({ message: t('auth.passwordMismatch') });
      return;
    }
    setSubmitting(true);
    try {
      await acceptDjInvite(token.trim(), password);
      navigate('/dj/login', { replace: true });
    } catch (err) {
      if (err instanceof ApiRequestError) {
        // The accept endpoint answers 401 unauthorized for an unknown, used or
        // expired token and the preview answers 404; both are one dead link.
        if (err.status === 404 || err.status === 401 || err.code === 'invite_not_found') {
          setState({ status: 'error', notFound: true, code: err.code });
          return;
        }
        if (err.code === 'validation_error' || err.code === 'validation_failed') {
          setError({ message: t('auth.passwordMin10'), code: err.code });
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
    <Link to="/dj/login" className="auth-link auth-link-muted">
      {t('auth.backToSignIn')}
    </Link>
  );

  if (state.status === 'loading') {
    return (
      <AuthScreen role={t('auth.djConsole')} title={t('auth.inviteTitle')} links={links}>
        <div className="auth-form">
          <p className="auth-hint">{t('auth.inviteLoading')}</p>
        </div>
      </AuthScreen>
    );
  }

  if (state.status === 'error') {
    return (
      <AuthScreen role={t('auth.djConsole')} title={t('auth.inviteTitle')} links={links}>
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
    <AuthScreen role={t('auth.djConsole')} title={t('auth.inviteTitle')} links={links}>
      <AuthSummary
        rows={[
          { label: t('auth.inviteSummaryDj'), value: preview.display_name },
          { label: t('auth.inviteSummaryClub'), value: preview.club_name },
        ]}
      />
      <form className="auth-form" onSubmit={submit} noValidate>
        <PasswordField
          id="dj-invite-password"
          label={t('auth.password')}
          autoComplete="new-password"
          value={password}
          onChange={(value) => {
            setPassword(value);
            setError(null);
          }}
        />
        <PasswordField
          id="dj-invite-confirm"
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