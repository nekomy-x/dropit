import { useState, type FormEvent } from 'react';
import { useNavigate } from 'react-router-dom';
import {
  login,
  login2fa,
  ApiRequestError,
  type LoginRequest,
  type TotpChallenge,
} from '../api/client';

/**
 * POST /v1/auth/club/login (email + password) → TokenPair | TotpChallenge;
 * on `mfa_required` the second step POSTs /v1/auth/club/2fa {email, totp_code}.
 * EN copy per Q9 (MVP languages: EN + ES — i18n wiring lands with content).
 */
export default function LoginPage() {
  const navigate = useNavigate();
  const [form, setForm] = useState<LoginRequest>({ email: '', password: '' });
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
        // Step 1 passed — show the TOTP field (F1 contract, TotpChallenge).
        setChallenge(result);
        return;
      }
      navigate('/dashboard', { replace: true });
    } catch (err) {
      if (err instanceof ApiRequestError) {
        setError(err.message);
        setCode(err.code);
      } else if (err instanceof Error) {
        setError(err.message);
      } else {
        setError('Login failed');
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
      navigate('/dashboard', { replace: true });
    } catch (err) {
      if (err instanceof ApiRequestError) {
        setError(err.message);
        setCode(err.code);
      } else if (err instanceof Error) {
        setError(err.message);
      } else {
        setError('Verification failed');
      }
    } finally {
      setSubmitting(false);
    }
  };

  const errorMessage = () => (
    <p className="text-sm text-red-300" role="alert">
      {error}
      {code ? (
        <span className="ml-1 font-mono text-xs text-red-400/60">({code})</span>
      ) : null}
    </p>
  );

  return (
    <div className="flex min-h-screen items-center justify-center px-4">
      <div className="w-full max-w-sm">
        <div className="mb-8 text-center">
          <p className="text-3xl font-black tracking-tight text-white">
            DROP IT
          </p>
          <p className="mt-2 text-sm text-neutral-400">Club panel</p>
        </div>

        {challenge ? (
          // Step 2 — TOTP code (mfa_required: true from the login response).
          <form onSubmit={onSubmitTotp} className="card space-y-4" noValidate>
            <div>
              <label htmlFor="totp-code" className="field-label">
                Two-factor code
              </label>
              <input
                id="totp-code"
                type="text"
                inputMode="numeric"
                autoComplete="one-time-code"
                pattern="\d{6}"
                maxLength={6}
                required
                placeholder="000000"
                className="field-input font-mono tracking-widest"
                value={totpCode}
                onChange={(e) => setTotpCode(e.target.value.replace(/\D/g, ''))}
              />
              <p className="mt-1 text-xs text-neutral-500">
                Enter the 6-digit code from your authenticator app.
              </p>
            </div>

            {error ? errorMessage() : null}

            <button
              type="submit"
              className="btn-primary w-full"
              disabled={submitting || totpCode.length !== 6}
            >
              {submitting ? 'Verifying…' : 'Verify code'}
            </button>

            <button
              type="button"
              className="w-full text-xs text-neutral-500 transition-colors hover:text-neutral-300"
              onClick={() => {
                setChallenge(null);
                setTotpCode('');
                setError(null);
                setCode(null);
              }}
            >
              Back to email and password
            </button>
          </form>
        ) : (
          // Step 1 — email + password.
          <form onSubmit={onSubmitPassword} className="card space-y-4" noValidate>
            <div>
              <label htmlFor="email" className="field-label">
                Email
              </label>
              <input
                id="email"
                type="email"
                autoComplete="username"
                required
                className="field-input"
                value={form.email}
                onChange={(e) => setForm({ ...form, email: e.target.value })}
              />
            </div>

            <div>
              <label htmlFor="password" className="field-label">
                Password
              </label>
              <input
                id="password"
                type="password"
                autoComplete="current-password"
                required
                className="field-input"
                value={form.password}
                onChange={(e) => setForm({ ...form, password: e.target.value })}
              />
            </div>

            {error ? errorMessage() : null}

            <button
              type="submit"
              className="btn-primary w-full"
              disabled={submitting || form.email === '' || form.password === ''}
            >
              {submitting ? 'Signing in…' : 'Sign in'}
            </button>

            <p className="text-center text-xs text-neutral-500">
              Club access is granted after verification by the DROP IT team.
            </p>
          </form>
        )}
      </div>
    </div>
  );
}
