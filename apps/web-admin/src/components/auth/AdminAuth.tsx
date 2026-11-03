/**
 * Admin auth kit — the same screen the club panel signs in with. The sign-in
 * card is one card on every surface: the DROP IT lockup, the title, the form and
 * the ways out. What differs is the facts, not the skin — the admin panel has no
 * self-service registration and no reset (its accounts are created by an
 * operator and always carry 2FA), and the ways out lead to the club panel and to
 * the operator who grants access.
 *
 * Styling is the shared `auth.css` and `brand-logo.css` (canonical:
 * packages/ui/), copied into both apps by scripts/sync-ui-tokens.mjs. Nothing
 * here declares a colour or a radius of its own.
 */
import { useId, useRef, useState, type KeyboardEvent, type ReactNode } from 'react';
import { CircleAlert, CircleCheck, Eye, EyeOff, Info } from 'lucide-react';
import BrandLogo from '../BrandLogo';
import { ThemeToggle } from '../../theme';
import { useLocale } from '../../locale';
import { DeploymentPill } from '../DeploymentStatus';
import { IS_DEMO } from '../../app-mode';
import '../../styles/auth.css';
import '../../styles/brand-logo.css';

/** The sign-in card: brand, title, the form, the ways out, and the corner
 *  cluster — the theme control. The environment fact (test / maintenance) is the
 *  fixed bottom strip mounted once at the app root, never a second badge in the
 *  corner. */
export function AdminAuthScreen({
  title,
  subtitle,
  children,
  links,
  footer,
}: {
  title: string;
  /** Short qualifier only; the sign-in card carries no intro paragraph. */
  subtitle?: string;
  children: ReactNode;
  /** Recovery and role links, in the club kit's row: primary then muted. */
  links?: ReactNode;
  footer?: ReactNode;
}) {
  const { t } = useLocale();
  return (
    <div className="auth-screen">
      <div className="auth-screen-top">
        <DeploymentPill demoBuild={IS_DEMO} />
        <ThemeToggle />
      </div>
      <div className="auth-card">
        {/* The lockup is its own accessible name ("DROP IT" from the wordmark
            plus the role label); an aria-label here would override the visible
            text and fail WCAG 2.5.3. Like the club card, the mark leads to the
            product's public page rather than to either panel. */}
        <a href="/connect" className="auth-brand auth-card-brand">
          <BrandLogo size="small" />
          <span className="auth-brand-role">{t('login.adminPanel')}</span>
        </a>
        <div className="auth-head">
          <h1 className="auth-title auth-title-sm">{title}</h1>
          {subtitle ? <p className="auth-sub">{subtitle}</p> : null}
        </div>
        <div className="auth-card-body">{children}</div>
        {links ? <div className="auth-links">{links}</div> : null}
        {footer ? <p className="auth-card-foot">{footer}</p> : null}
      </div>
    </div>
  );
}

/** Password field with a reveal control (same contract as the club kit). */
export function AdminPasswordField({
  id,
  label,
  value,
  onChange,
  autoComplete,
}: {
  id: string;
  label: string;
  value: string;
  onChange: (value: string) => void;
  autoComplete: 'current-password' | 'new-password';
}) {
  const { t } = useLocale();
  const [revealed, setRevealed] = useState(false);
  return (
    <div className="auth-field">
      <label htmlFor={id} className="field-label">{label}</label>
      <div className="auth-password">
        <input
          id={id}
          type={revealed ? 'text' : 'password'}
          className="field-input auth-input"
          autoComplete={autoComplete}
          value={value}
          onChange={(event) => onChange(event.target.value)}
        />
        <button
          type="button"
          className="auth-reveal"
          aria-pressed={revealed}
          aria-label={revealed ? t('login.hidePassword') : t('login.showPassword')}
          title={revealed ? t('login.hidePassword') : t('login.showPassword')}
          onClick={() => setRevealed((current) => !current)}
        >
          {revealed ? <EyeOff size={16} /> : <Eye size={16} />}
        </button>
      </div>
    </div>
  );
}

/** Six-digit TOTP entry as boxes; admin 2FA is mandatory, so this is the norm. */
export function AdminOtpInput({
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
  const digits = Array.from({ length: 6 }, (_, index) => value[index] ?? '');

  const setDigit = (index: number, digit: string) => {
    const next = digits.slice();
    next[index] = digit;
    onChange(next.join('').slice(0, 6));
  };
  const focusAt = (index: number) => {
    const target = refs.current[Math.max(0, Math.min(5, index))];
    target?.focus();
    target?.select();
  };
  const onInput = (index: number, raw: string) => {
    const clean = raw.replace(/\D/g, '');
    if (clean === '') { setDigit(index, ''); return; }
    if (clean.length > 1) {
      onChange((digits.join('').slice(0, index) + clean).slice(0, 6));
      focusAt(index + clean.length);
      return;
    }
    setDigit(index, clean);
    if (index < 5) focusAt(index + 1);
  };
  const onKeyDown = (index: number, event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === 'Backspace' && digits[index] === '' && index > 0) {
      event.preventDefault();
      setDigit(index - 1, '');
      focusAt(index - 1);
    } else if (event.key === 'ArrowLeft' && index > 0) {
      event.preventDefault();
      focusAt(index - 1);
    } else if (event.key === 'ArrowRight' && index < 5) {
      event.preventDefault();
      focusAt(index + 1);
    }
  };

  return (
    <div className="auth-otp" role="group" aria-label={t('login.otpGroup')}>
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
          aria-label={t('login.otpDigit', { index: index + 1, count: 6 })}
          onChange={(event) => onInput(index, event.target.value)}
          onKeyDown={(event) => onKeyDown(index, event)}
          onFocus={(event) => event.currentTarget.select()}
        />
      ))}
    </div>
  );
}

export function AdminAuthAlert({
  tone,
  title,
  code,
  children,
}: {
  tone: 'error' | 'note' | 'ok';
  title: string;
  code?: string | null;
  children?: ReactNode;
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
