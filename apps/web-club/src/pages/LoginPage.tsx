import { useState, type FormEvent } from 'react';
import { useNavigate } from 'react-router-dom';
import { login, ApiRequestError, type LoginRequest } from '../api/client';

/**
 * POST /v1/auth/club/login (email + password; TOTP step to follow in F1).
 * EN copy per Q9 (MVP languages: EN + ES — i18n wiring lands with content).
 */
export default function LoginPage() {
  const navigate = useNavigate();
  const [form, setForm] = useState<LoginRequest>({ email: '', password: '' });
  const [error, setError] = useState<string | null>(null);
  const [code, setCode] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);

  const onSubmit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setError(null);
    setCode(null);
    setSubmitting(true);
    try {
      const result = await login(form);
      if (result.otpRequired) {
        // F1 will add the TOTP screen; skeleton notes it explicitly.
        setError('Two-factor code required — TOTP screen ships with F1.');
        setCode('otp_required');
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

  return (
    <div className="flex min-h-screen items-center justify-center px-4">
      <div className="w-full max-w-sm">
        <div className="mb-8 text-center">
          <p className="text-3xl font-black tracking-tight text-white">
            DROP IT
          </p>
          <p className="mt-2 text-sm text-neutral-400">Club panel</p>
        </div>

        <form onSubmit={onSubmit} className="card space-y-4" noValidate>
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

          {error ? (
            <p className="text-sm text-red-300" role="alert">
              {error}
              {code ? (
                <span className="ml-1 font-mono text-xs text-red-400/60">
                  ({code})
                </span>
              ) : null}
            </p>
          ) : null}

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
      </div>
    </div>
  );
}
