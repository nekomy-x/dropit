import { useEffect, useState, type FormEvent } from 'react';
import { Link, useNavigate, useSearchParams } from 'react-router-dom';
import {
  ApiRequestError,
  isAuthenticated,
  registerClub,
  registerDj,
} from '../api/client';
import { AuthAlert, AuthFacts, AuthWizard, PasswordField, type WizardStep } from '../components/auth/AuthKit';
import { useLocale, type Translate } from '../locale';

/**
 * Open registration (Test Mode stand) — docs/signup-contract.md.
 *
 * One path: the form posts to POST /v1/auth/club/register or
 * /v1/auth/dj/register, whose 201 body is a TokenPair, so the panel opens
 * without a second request. "Generate" fills the three fields with a demo
 * identity for someone who does not want to type one; it is a shortcut into
 * the same form, not a second submit path.
 *
 * Tokens stay in memory (project rule), so a page refresh sends the user back
 * to /login — the fields are what they must have written down.
 */

type Role = 'club' | 'dj';

interface GeneratedIdentity {
  name: string;
  email: string;
  password: string;
}

interface SignupError {
  scope: 'quick' | 'manual';
  message: string;
  hint?: string;
  code?: string;
}

const ROLE_OPTIONS: ReadonlyArray<{ value: Role; labelKey: string; hintKey: string }> = [
  { value: 'club', labelKey: 'auth.roleClub', hintKey: 'auth.roleClubHint' },
  { value: 'dj', labelKey: 'auth.roleDj', hintKey: 'auth.roleDjHint' },
];

const CLUB_NAMES = ['Barrio Latino', 'Noche Club', 'Vinyl Room', 'Basement 9', 'Sala Norte', 'Nebula Club'] as const;
const DJ_NAMES = ['DJ Noche', 'DJ Vela', 'DJ Kairo', 'DJ Mirlo', 'DJ Lumen', 'DJ Senda'] as const;

/** Readable alphabet: no 0/o/1/l so a demo password can be retyped from screen. */
const TOKEN_ALPHABET = 'abcdefghijkmnpqrstuvwxyz23456789';

function randomToken(length: number): string {
  const values = new Uint8Array(length);
  if (typeof crypto !== 'undefined' && typeof crypto.getRandomValues === 'function') {
    crypto.getRandomValues(values);
  } else {
    for (let index = 0; index < length; index += 1) values[index] = Math.floor(Math.random() * 256);
  }
  let token = '';
  for (const value of values) token += TOKEN_ALPHABET.charAt(value % TOKEN_ALPHABET.length);
  return token;
}

function pick<T>(items: readonly T[], fallback: T): T {
  return items[Math.floor(Math.random() * items.length)] ?? fallback;
}

/** `demo+<rand>@club.test` / `@dj.test` per the signup contract. */
function generateIdentity(role: Role): GeneratedIdentity {
  const id = randomToken(6);
  const base = pick(role === 'club' ? CLUB_NAMES : DJ_NAMES, role === 'club' ? 'Demo Club' : 'DJ Demo');
  return {
    name: `${base} ${id.slice(0, 3).toUpperCase()}`,
    email: `demo+${id}@${role}.test`,
    password: `demo-${randomToken(10)}`,
  };
}

function describeSignupError(err: unknown, scope: SignupError['scope'], t: Translate): SignupError {
  if (err instanceof ApiRequestError) {
    switch (err.code) {
      case 'email_taken':
        return {
          scope,
          code: err.code,
          message: t('auth.emailTaken'),
          hint: scope === 'quick' ? t('auth.emailTakenHintQuick') : t('auth.emailTakenHintManual'),
        };
      case 'validation_failed':
        return {
          scope,
          code: err.code,
          message: err.message || t('auth.validationFailed'),
          hint: t('auth.validationFailedHint'),
        };
      case 'rate_limited':
        return {
          scope,
          code: err.code,
          message: t('auth.rateLimited'),
          hint: t('auth.rateLimitedHint'),
        };
      case 'club_not_found':
        return {
          scope,
          code: err.code,
          message: t('auth.clubSlugNotFound'),
          hint: t('auth.clubSlugNotFoundHint'),
        };
      default:
        return { scope, code: err.code, message: err.message || t('auth.signupRejected') };
    }
  }
  if (err instanceof Error) return { scope, message: t('auth.serverNoAnswer'), hint: err.message };
  return { scope, message: t('auth.signupFailed') };
}

/** The two steps, translated at render time (the rail is read, not navigated). */
const STEPS = (t: Translate): ReadonlyArray<WizardStep> => [
  { title: t('auth.stepRole'), note: t('auth.stepRoleNote') },
  { title: t('auth.stepAccount'), note: t('auth.stepAccountNote') },
];

export default function RegisterPage() {
  const { t } = useLocale();
  const navigate = useNavigate();
  const [searchParams] = useSearchParams();
  const requestedRole = searchParams.get('role');
  const [role, setRole] = useState<Role | null>(requestedRole === 'dj' || requestedRole === 'club' ? requestedRole : null);
  const [manual, setManual] = useState({ name: '', email: '', password: '', clubSlug: '' });
  const [error, setError] = useState<SignupError | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const [signupClosed, setSignupClosed] = useState(false);

  // A live session does not need the funnel.
  useEffect(() => {
    if (isAuthenticated('dj')) navigate('/dj', { replace: true });
    else if (isAuthenticated()) navigate('/sessions', { replace: true });
  }, [navigate]);

  const roleMeta = ROLE_OPTIONS.find((option) => option.value === role);
  const busy = submitting || signupClosed;

  const chooseRole = (next: Role) => {
    if (next === role) return;
    setRole(next);
    setManual({ name: '', email: '', password: '', clubSlug: '' });
    setError(null);
  };

  const changeRole = () => {
    setRole(null);
    setError(null);
  };

  const fillFromGenerator = () => {
    const generated = generateIdentity(role ?? 'club');
    setManual((current) => ({ ...current, name: generated.name, email: generated.email, password: generated.password }));
    setError(null);
  };

  const createAccount = async (
    scope: SignupError['scope'],
    payload: { name: string; email: string; password: string; clubSlug?: string },
  ) => {
    if (!role) return;
    setError(null);
    setSubmitting(true);
    try {
      if (role === 'club') {
        await registerClub({ name: payload.name, email: payload.email, password: payload.password });
        navigate('/sessions', { replace: true });
      } else {
        const slug = payload.clubSlug?.trim();
        await registerDj({
          display_name: payload.name,
          email: payload.email,
          password: payload.password,
          ...(slug ? { club_slug: slug } : {}),
        });
        navigate('/dj', { replace: true });
      }
    } catch (err) {
      // The flag is a server switch, so it is reported once for the whole page.
      if (err instanceof ApiRequestError && err.code === 'signup_disabled') setSignupClosed(true);
      else setError(describeSignupError(err, scope, t));
      setSubmitting(false);
    }
  };

  const onSubmitManual = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const name = manual.name.trim();
    const email = manual.email.trim();
    if (name === '') {
      setError({
        scope: 'manual',
        message: role === 'club' ? t('auth.clubNameRequired') : t('auth.djNameRequired'),
      });
      return;
    }
    if (!/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email)) {
      setError({ scope: 'manual', message: t('auth.emailInvalid') });
      return;
    }
    if (manual.password.length < 8) {
      setError({ scope: 'manual', message: t('auth.passwordTooShort') });
      return;
    }
    void createAccount('manual', { name, email, password: manual.password, clubSlug: manual.clubSlug });
  };

  return (
    <AuthWizard
      role={t('auth.panelsEyebrow')}
      steps={STEPS(t)}
      step={role ? 2 : 1}
      title={t('auth.registerTitle')}
      subtitle={t('auth.registerIntro')}
      asideFooter={
        <AuthFacts
          facts={[
            { label: t('auth.factSignupLabel'), value: t('auth.factSignupValue') },
            { label: t('auth.factSessionLabel'), value: t('auth.sessionMemoryNote') },
            { label: t('auth.factStandLabel'), value: t('auth.factStandValue') },
          ]}
        />
      }
      footer={
        // Each link is wrapped, so the spaces between them are real text nodes:
        // two inline-flex links with no gap between their boxes rendered as
        // "DJ sign-inSupport". The separators are the only glue.
        <>
          <span>{t('auth.haveAccount')}</span>{' '}
          <Link to="/login" className="auth-link">{t('auth.clubSignIn')}</Link>
          <span className="auth-foot-sep" aria-hidden="true">·</span>
          <Link to="/dj/login" className="auth-link">{t('auth.djSignIn')}</Link>
          <span className="auth-foot-sep auth-link-optional" aria-hidden="true">·</span>
          <Link to="/connect" className="auth-link auth-link-muted auth-link-optional">{t('auth.publicSite')}</Link>
          <span className="auth-foot-sep" aria-hidden="true">·</span>
          <a href="mailto:clubs@dropit.music" className="auth-link auth-link-muted">{t('auth.support')}</a>
        </>
      }
    >
      {signupClosed ? (
        <AuthAlert tone="note" title={t('auth.signupClosedTitle')}>
          {t('auth.signupClosedBodyLead')}{' '}
          <span className="font-mono">signup_disabled</span>
          {t('auth.signupClosedBodyTail')}
          <span className="auth-links">
            <button type="button" className="auth-link" onClick={() => setSignupClosed(false)}>
              {t('common.tryAgain')}
            </button>
            <Link className="auth-link auth-link-muted" to="/login">{t('auth.clubSignIn')}</Link>
            <Link className="auth-link auth-link-muted" to="/dj/login">{t('auth.djSignIn')}</Link>
          </span>
        </AuthAlert>
      ) : null}

      <div className="auth-field" role="radiogroup" aria-label={t('auth.roleLegend')}>
        <span className="field-label" aria-hidden="true">{t('auth.roleLegend')}</span>
        <div className="auth-choice-grid" style={{ marginTop: 6 }}>
          {ROLE_OPTIONS.map((option) => {
            const selected = role === option.value;
            return (
              <button
                key={option.value}
                type="button"
                role="radio"
                aria-checked={selected}
                className={`auth-choice ${selected ? 'is-selected' : ''}`.trim()}
                onClick={() => chooseRole(option.value)}
              >
                <span className="auth-choice-head">
                  <span className="auth-choice-title">{t(option.labelKey)}</span>
                  <span className="auth-choice-mark" aria-hidden="true" />
                </span>
                <span className="auth-choice-note">{t(option.hintKey)}</span>
              </button>
            );
          })}
        </div>
      </div>

      {role && roleMeta ? (
        <section className="auth-form" aria-labelledby="signup-step-two">
          <div className="auth-field-row">
            <h2 id="signup-step-two" className="auth-step-title" style={{ color: 'var(--c-text)', fontWeight: 600 }}>
              {t('auth.stepTwo', { role: t(roleMeta.labelKey) })}
            </h2>
            <button
              type="button"
              className="auth-link auth-link-muted"
              onClick={changeRole}
              disabled={submitting}
            >
              {t('auth.changeRole')}
            </button>
          </div>

          {/* One form, one action. The screen used to carry a "create in one
              click" block with the generated identity inside it and then the
              same account fields below a divider: two primaries for one job,
              and the person had to read a password to use the top one. Now the
              generator only fills the fields, so what is created is always the
              account visible on screen. */}
          <form className="auth-form" onSubmit={onSubmitManual} noValidate>
            <div className="auth-field-row">
              <span className="auth-hint">{t('auth.manualHint')}</span>
              <button
                type="button"
                className="auth-link auth-link-muted"
                onClick={fillFromGenerator}
                disabled={submitting}
              >
                {t('auth.generate')}
              </button>
            </div>

            <div className="auth-field">
              <label htmlFor="signup-name" className="field-label">
                {role === 'club' ? t('auth.clubNameLabel') : t('auth.djNameLabel')}
              </label>
              <input
                id="signup-name"
                type="text"
                autoComplete={role === 'club' ? 'organization' : 'nickname'}
                className="field-input auth-input"
                value={manual.name}
                onChange={(event) => setManual({ ...manual, name: event.target.value })}
              />
            </div>

            <div className="auth-field">
              <label htmlFor="signup-email" className="field-label">{t('auth.email')}</label>
              <input
                id="signup-email"
                type="email"
                autoComplete="email"
                className="field-input auth-input"
                value={manual.email}
                onChange={(event) => setManual({ ...manual, email: event.target.value })}
              />
            </div>

            <PasswordField
              id="signup-password"
              label={t('auth.password')}
              autoComplete="new-password"
              hint={t('auth.passwordHint')}
              value={manual.password}
              onChange={(value) => setManual({ ...manual, password: value })}
            />

            {role === 'dj' ? (
              <div className="auth-field">
                <label htmlFor="signup-club" className="field-label">
                  {t('auth.clubSlugLabel')}{' '}
                  <span className="auth-hint">— {t('common.optional')}</span>
                </label>
                <input
                  id="signup-club"
                  type="text"
                  autoComplete="off"
                  placeholder="barrio-latino"
                  className="field-input auth-input font-mono"
                  value={manual.clubSlug}
                  onChange={(event) => setManual({ ...manual, clubSlug: event.target.value })}
                />
                <span className="auth-hint">{t('auth.clubSlugHint')}</span>
              </div>
            ) : null}

            {error?.scope === 'manual' ? (
              <AuthAlert tone="error" title={error.message} code={error.code}>
                {error.hint}
              </AuthAlert>
            ) : null}

            {/* The one action of the screen: it creates exactly the account
                shown in the fields above it. */}
            <button type="submit" className="btn-primary" disabled={busy}>
              {submitting ? t('auth.creating') : t('auth.createAccount')}
            </button>
            <span className="auth-hint">{t('auth.accountReady')}</span>
          </form>
        </section>
      ) : null}
    </AuthWizard>
  );
}
