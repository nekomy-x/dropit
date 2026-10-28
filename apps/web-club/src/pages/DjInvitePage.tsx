import { useState, type FormEvent } from 'react';
import { Link, useNavigate, useParams } from 'react-router-dom';
import { acceptDjInvite, ApiRequestError } from '../api/client';
import BrandLogo from '../components/BrandLogo';
import { useLocale } from '../locale';

export default function DjInvitePage() {
  const { t } = useLocale();
  const { token = '' } = useParams();
  const navigate = useNavigate();
  const [password, setPassword] = useState('');
  const [confirmation, setConfirmation] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const submit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault(); setError(null);
    if (password.length < 10) { setError(t('auth.passwordMin10')); return; }
    if (password !== confirmation) { setError(t('auth.passwordMismatch')); return; }
    setSubmitting(true);
    try { await acceptDjInvite(token, password); navigate('/dj/login', { replace: true }); }
    catch (err) { setError(err instanceof ApiRequestError ? err.message : t('auth.inviteFailed')); }
    finally { setSubmitting(false); }
  };
  return (
    <div className="app-shell flex min-h-screen items-center justify-center px-4">
      <div className="w-full max-w-md">
        <div className="flex items-center gap-3">
          <BrandLogo size="large" />
          <span className="text-[10px] uppercase tracking-[.18em] text-neutral-500">{t('auth.djEyebrow')}</span>
        </div>
        <p className="mt-8 font-mono text-[10px] uppercase tracking-[.18em] text-brand-400">{t('auth.inviteEyebrow')}</p>
        <h1 className="mt-2 text-3xl font-semibold tracking-tight text-white">{t('auth.inviteTitle')}</h1>
        <p className="mt-2 text-sm text-neutral-500">{t('auth.inviteDetail')}</p>
        <form onSubmit={submit} className="card mt-7 space-y-4" noValidate>
          <div>
            <label htmlFor="new-password" className="field-label">{t('auth.password')}</label>
            <input id="new-password" type="password" autoComplete="new-password" className="field-input" value={password} onChange={(event) => setPassword(event.target.value)} />
          </div>
          <div>
            <label htmlFor="confirm-password" className="field-label">{t('auth.confirmPassword')}</label>
            <input id="confirm-password" type="password" autoComplete="new-password" className="field-input" value={confirmation} onChange={(event) => setConfirmation(event.target.value)} />
          </div>
          {error ? <p className="text-sm text-red-300" role="alert">{error}</p> : null}
          <button type="submit" disabled={submitting || !password || !confirmation} className="btn-primary w-full">
            {submitting ? t('auth.activating') : t('auth.activateAccount')}
          </button>
          <p className="text-xs text-neutral-500">{t('auth.inviteHint')}</p>
        </form>
        <Link to="/dj/login" className="mt-5 inline-block text-sm text-neutral-500 hover:text-neutral-200">
          {t('auth.alreadyActivated')}
        </Link>
      </div>
    </div>
  );
}