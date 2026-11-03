/**
 * Auth kit — one shell for every auth screen.
 *
 *   AuthScreen   brand lockup, optional title caption, the form, and the links
 *                that lead out of it. Sign-in, the club application, invitation
 *                activation and password recovery all use it: one narrow card,
 *                no side rail to read.
 *
 * An auth screen states the job and the fields. The subtitle is optional and is
 * used only for a fact the form cannot show (which account the two-factor code
 * belongs to); it is not a paragraph explaining the product. Invitation screens
 * repeat the approved application in `AuthSummary` rows, so the person never
 * retypes what the club already sent.
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

/** Corner cluster shared by every auth screen: the "not production" pill and
 *  the theme control. The pill is the product-wide indicator the owner asked for
 *  (a small orange-outlined label in the corner of every screen); the bottom
 *  band carries maintenance only. */
function AuthCorner() {
  return (
    <div className="auth-screen-top">
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
  media,
}: {
  /** Shown beside the wordmark: "Club panel", "DJ console", "Recovery". */
  role: string;
  title: string;
  /** A single fact the form cannot show (e.g. which account gets the 2FA code).
   *  Not an introduction: the screens do not carry marketing copy. */
  subtitle?: string;
  children: ReactNode;
  links?: ReactNode;
  footer?: ReactNode;
  /**
   * Optional photograph column on the left (the club application is the only
   * screen that uses it). Rendered after the card in the DOM — the form is the
   * job, the photo is context — and placed in the first column above 1100px;
   * below that width it is not rendered at all (`.auth-media` is `display:
   * none`), so a phone keeps the same centred card with the brand lockup on it.
   */
  media?: ReactNode;
}) {
  return (
    <div className={`auth-screen${media ? ' auth-screen-split' : ''}`}>
      <AuthCorner />
      <div className="auth-card">
        {/* The lockup is its own accessible name ("DROP IT" from the wordmark
            plus the role label). An aria-label here overrode that name with a
            sentence the visible text did not contain, which fails WCAG 2.5.3 —
            the rule is that the name starts from what is on screen. */}
        <Link to="/connect" className="auth-brand auth-card-brand">
          <BrandLogo size="small" />
          <span className="auth-brand-role">{role}</span>
        </Link>
        <div className="auth-head">
          <h1 className="auth-title auth-title-sm">{title}</h1>
          {subtitle ? <p className="auth-sub">{subtitle}</p> : null}
        </div>
        <div className="auth-card-body">{children}</div>
        {links ? <div className="auth-links">{links}</div> : null}
        {footer ? <p className="auth-card-foot">{footer}</p> : null}
      </div>
      {media ? (
        <aside className="auth-media">
          <div className="auth-media-inner">{media}</div>
        </aside>
      ) : null}
    </div>
  );
}

/**
 * Read-only rows under an invitation title: the facts the application already
 * carries. A definition list, not a card and not disabled inputs — nothing here
 * is editable, so it must not look like a field.
 */
export function AuthSummary({
  rows,
}: {
  rows: ReadonlyArray<{ label: string; value: ReactNode }>;
}) {
  return (
    <dl className="auth-summary">
      {rows.map((row) => (
        <div key={row.label} className="auth-summary-row">
          <dt className="auth-summary-label">{row.label}</dt>
          <dd className="auth-summary-value">{row.value}</dd>
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
