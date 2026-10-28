/**
 * Auth kit — the two skins the auth screens share.
 *
 *   AuthScreen   skin A: signing in. One narrow card, no reading required.
 *   AuthWizard   skin B: creating an account / resetting a password. A step
 *                rail that says where the person is, the form beside it.
 *
 * The split is a job split, not a style choice: a returning operator wants two
 * fields and a button; someone creating an account needs to know what is
 * expected of them and what this stand does and does not do. Sharing one layout
 * for both produced the old screen — a registration form's worth of explanation
 * wrapped around a login form.
 *
 * Styling lives in styles/auth.css (canonical: packages/ui/auth.css) and reads
 * the same tokens as the panels. Nothing here declares a colour or a radius.
 */
import {
  useId,
  useRef,
  useState,
  type FormEvent,
  type KeyboardEvent,
  type ReactNode,
} from 'react';
import { Link } from 'react-router-dom';
import { CircleAlert, CircleCheck, Eye, EyeOff, Info } from 'lucide-react';
import BrandLogo from '../BrandLogo';
import { useLocale } from '../../locale';
import { DeploymentPill } from '../DeploymentStatus';
import { IS_DEMO } from '../../app-mode';
import { ThemeToggle } from '../../theme';
import '../../styles/auth.css';

/* -------------------------------------------------------------------------- */
/* Shells                                                                     */
/* -------------------------------------------------------------------------- */

/** Corner cluster shared by both skins: the "not production" pill and the theme
 *  control. The pill is the product-wide indicator the owner asked for (a small
 *  orange-outlined label in the corner of every screen); the bottom band carries
 *  maintenance only. */
function AuthCorner({ fixedClass }: { fixedClass: string }) {
  return (
    <div className={fixedClass}>
      <DeploymentPill demoBuild={IS_DEMO} />
      <ThemeToggle />
    </div>
  );
}

export function AuthScreen({
  role,
  title,
  subtitle,
  children,
  links,
  footer,
}: {
  /** Shown beside the wordmark: "Club panel", "DJ console", … */
  role: string;
  title: string;
  subtitle: string;
  children: ReactNode;
  links?: ReactNode;
  footer?: ReactNode;
}) {
  return (
    <div className="auth-screen">
      <AuthCorner fixedClass="auth-screen-top" />
      <div className="auth-card">
        <Link to="/connect" className="auth-brand auth-card-brand" aria-label="DROP IT — public site">
          <BrandLogo size="small" />
          <span className="auth-brand-role">{role}</span>
        </Link>
        <div className="auth-head">
          <h1 className="auth-title auth-title-sm">{title}</h1>
          <p className="auth-sub">{subtitle}</p>
        </div>
        {children}
        {links ? <div className="auth-links">{links}</div> : null}
        {footer ? <p className="auth-card-foot">{footer}</p> : null}
      </div>
    </div>
  );
}

export interface WizardStep {
  title: string;
  note?: string;
}

/**
 * Skin B shell. `step` is 1-based; steps before it read as done, so the rail is
 * a position indicator rather than a set of navigation controls — jumping
 * around a credential flow is never what the person wants.
 */
export function AuthWizard({
  role,
  steps,
  step,
  title,
  subtitle,
  children,
  footer,
  asideFooter,
}: {
  role: string;
  steps: ReadonlyArray<WizardStep>;
  step: number;
  title: string;
  subtitle: string;
  children: ReactNode;
  footer?: ReactNode;
  asideFooter?: ReactNode;
}) {
  const { t } = useLocale();
  return (
    <div className="auth-wizard">
      <AuthCorner fixedClass="auth-wizard-top" />
      <aside className="auth-aside">
        <Link to="/connect" className="auth-brand" aria-label="DROP IT — public site">
          <BrandLogo size="small" />
          <span className="auth-brand-role">{role}</span>
        </Link>
        <ol className="auth-steps" aria-label={t('auth.progress')}>
          {steps.map((item, index) => {
            const position = index + 1;
            const state = position === step ? 'is-current' : position < step ? 'is-done' : '';
            return (
              <li
                key={item.title}
                className={`auth-step ${state}`.trim()}
                aria-current={position === step ? 'step' : undefined}
              >
                <span className="auth-step-mark" aria-hidden="true">
                  {position < step ? <CircleCheck size={12} strokeWidth={2.4} /> : String(position).padStart(2, '0')}
                </span>
                <span>
                  <span className="auth-step-title">{item.title}</span>
                  {item.note ? <span className="auth-step-note">{item.note}</span> : null}
                </span>
              </li>
            );
          })}
        </ol>
        <div className="auth-aside-fill" />
        {asideFooter}
      </aside>

      <main className="auth-main">
        <div className="auth-wizard-card">
          <div className="auth-progress" aria-hidden="true">
            {steps.map((item, index) => (
              <span
                key={item.title}
                className={`auth-progress-seg ${index + 1 <= step ? 'is-done' : ''}`.trim()}
              />
            ))}
          </div>
          <div className="auth-head">
            <h1 className="auth-title">{title}</h1>
            <p className="auth-sub">{subtitle}</p>
          </div>
          {children}
          {footer ? <p className="auth-foot">{footer}</p> : null}
        </div>
      </main>
    </div>
  );
}

/** Facts block for the wizard's step rail — what this stand actually does. */
export function AuthFacts({ facts }: { facts: ReadonlyArray<{ label: string; value: string }> }) {
  return (
    <dl className="auth-facts">
      {facts.map((fact) => (
        <div key={fact.label} className="auth-fact">
          <dt>{fact.label}</dt>
          <dd>{fact.value}</dd>
        </div>
      ))}
    </dl>
  );
}

/* -------------------------------------------------------------------------- */
/* Fields and messages                                                        */
/* -------------------------------------------------------------------------- */

/**
 * Password field with a reveal control.
 *
 * The button is a real `<button type="button">` with `aria-pressed`, so a screen
 * reader announces the state and Space does not submit the form.
 */
export function PasswordField({
  id,
  label,
  value,
  onChange,
  autoComplete,
  hint,
  describedBy,
  invalid,
  placeholder,
}: {
  id: string;
  label: string;
  value: string;
  onChange: (value: string) => void;
  autoComplete: 'current-password' | 'new-password';
  hint?: string;
  describedBy?: string;
  invalid?: boolean;
  placeholder?: string;
}) {
  const { t } = useLocale();
  const [revealed, setRevealed] = useState(false);
  const hintId = hint ? `${id}-hint` : undefined;
  return (
    <div className="auth-field">
      <div className="auth-field-row">
        <label htmlFor={id} className="field-label">
          {label}
        </label>
        {hint ? (
          <span id={hintId} className="auth-hint">
            {hint}
          </span>
        ) : null}
      </div>
      <div className="auth-password">
        <input
          id={id}
          type={revealed ? 'text' : 'password'}
          className="field-input auth-input"
          autoComplete={autoComplete}
          value={value}
          placeholder={placeholder}
          aria-invalid={invalid || undefined}
          aria-describedby={describedBy}
          onChange={(event) => onChange(event.target.value)}
        />
        <button
          type="button"
          className="auth-reveal"
          aria-pressed={revealed}
          aria-label={revealed ? t('auth.hidePassword') : t('auth.showPassword')}
          title={revealed ? t('auth.hidePassword') : t('auth.showPassword')}
          onClick={() => setRevealed((current) => !current)}
        >
          {revealed ? <EyeOff size={16} /> : <Eye size={16} />}
        </button>
      </div>
    </div>
  );
}

export type AuthMessageTone = 'error' | 'note' | 'ok';

export function AuthAlert({
  tone,
  title,
  children,
  code,
}: {
  tone: AuthMessageTone;
  title: string;
  children?: ReactNode;
  code?: string | null;
}) {
  const Glyph = tone === 'ok' ? CircleCheck : tone === 'note' ? Info : CircleAlert;
  return (
    <div className={`auth-alert auth-alert-${tone}`} role={tone === 'error' ? 'alert' : 'status'}>
      <Glyph size={15} className="auth-alert-icon" aria-hidden="true" />
      <div className="auth-alert-body">
        <span className="auth-alert-title">{title}</span>
        {children ? <span className="auth-alert-text">{children}</span> : null}
        {code ? <span className="auth-alert-code">{code}</span> : null}
      </div>
    </div>
  );
}

/* -------------------------------------------------------------------------- */
/* Six-digit code                                                             */
/* -------------------------------------------------------------------------- */

const OTP_LENGTH = 6;

/**
 * TOTP entry as six boxes.
 *
 * Each box is its own input so a phone can show the numeric keypad and a code
 * arriving by paste or SMS autofill fills the row. The value is still one string
 * owned by the parent, which is what the API takes.
 */
export function OtpInput({
  value,
  onChange,
  disabled,
}: {
  value: string;
  onChange: (value: string) => void;
  disabled?: boolean;
}) {
  const { t } = useLocale();
  const groupId = useId();
  const refs = useRef<Array<HTMLInputElement | null>>([]);
  const digits = Array.from({ length: OTP_LENGTH }, (_, index) => value[index] ?? '');

  const setDigit = (index: number, digit: string) => {
    const next = digits.slice();
    next[index] = digit;
    onChange(next.join('').slice(0, OTP_LENGTH));
  };

  const focusAt = (index: number) => {
    const target = refs.current[Math.max(0, Math.min(OTP_LENGTH - 1, index))];
    target?.focus();
    target?.select();
  };

  const onInput = (index: number, raw: string) => {
    const clean = raw.replace(/\D/g, '');
    if (clean === '') {
      setDigit(index, '');
      return;
    }
    // A paste into one box can carry the whole code.
    if (clean.length > 1) {
      const next = (digits.join('').slice(0, index) + clean).slice(0, OTP_LENGTH);
      onChange(next);
      focusAt(index + clean.length);
      return;
    }
    setDigit(index, clean);
    if (index < OTP_LENGTH - 1) focusAt(index + 1);
  };

  const onKeyDown = (index: number, event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === 'Backspace' && digits[index] === '' && index > 0) {
      event.preventDefault();
      setDigit(index - 1, '');
      focusAt(index - 1);
    } else if (event.key === 'ArrowLeft' && index > 0) {
      event.preventDefault();
      focusAt(index - 1);
    } else if (event.key === 'ArrowRight' && index < OTP_LENGTH - 1) {
      event.preventDefault();
      focusAt(index + 1);
    }
  };

  return (
    <div className="auth-otp" role="group" aria-label={t('auth.otpGroup')}>
      {digits.map((digit, index) => (
        <input
          key={`${groupId}-${index}`}
          ref={(node) => { refs.current[index] = node; }}
          className="auth-otp-input"
          type="text"
          inputMode="numeric"
          autoComplete={index === 0 ? 'one-time-code' : 'off'}
          maxLength={1}
          value={digit}
          disabled={disabled}
          aria-label={t('auth.otpDigit', { index: index + 1, count: OTP_LENGTH })}
          onChange={(event) => onInput(index, event.target.value)}
          onKeyDown={(event) => onKeyDown(index, event)}
          onFocus={(event) => event.currentTarget.select()}
        />
      ))}
    </div>
  );
}

/* -------------------------------------------------------------------------- */
/* Helpers                                                                    */
/* -------------------------------------------------------------------------- */

/** `onSubmit` that suppresses the browser's implicit submit and runs `run`. */
export function submitHandler(run: () => void | Promise<void>) {
  return (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    void run();
  };
}

/** Link that keeps the recovery flow inside the auth skins. */
export function AuthLink({ to, children }: { to: string; children: ReactNode }) {
  return (
    <Link to={to} className="auth-link">
      {children}
    </Link>
  );
}
