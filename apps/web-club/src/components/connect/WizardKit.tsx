import type { ReactNode } from 'react';
import { Link } from 'react-router-dom';
import Icon, { type IconName } from './Icons';
import Select from '../ui/Select';
import { useLocale } from '../../connect/locale';

/* ---------------------------------------------------------------------------
   Small building blocks shared by the three onboarding steps.
   Geometry mirrors the .pen design: 24px step markers, 48px fields, 14px
   cards, mint accent only on the active/complete state.
   ------------------------------------------------------------------------ */

export type StepState = 'done' | 'current' | 'todo';

export function Stepper({
  steps,
  current,
}: {
  steps: string[];
  current: number;
}) {
  const { t } = useLocale();
  return (
    <ol className="flex flex-wrap items-center gap-2" aria-label={t('steps')}>
      {steps.map((label, index) => {
        const number = index + 1;
        const state: StepState =
          number < current ? 'done' : number === current ? 'current' : 'todo';
        return (
          <li key={label} className="flex items-center gap-2">
            {index > 0 && (
              <span
                aria-hidden="true"
                className={`h-px w-8 transition-colors duration-500 ${
                  state === 'todo' ? 'bg-drop-line' : 'bg-drop-accent/60'
                }`}
              />
            )}
            <span
              className={`flex h-6 w-6 items-center justify-center rounded-[var(--r-tile)] font-mono text-[10px] transition-all duration-300 ${
                state === 'todo'
                  ? 'border border-drop-line text-drop-dim'
                  : state === 'current'
                    ? 'border border-drop-accent text-drop-accent'
                    : 'border border-drop-accent bg-drop-accent text-drop-ink'
              }`}
            >
              {state === 'done' ? <Icon name="check" size={11} strokeWidth={2.6} /> : `0${number}`}
            </span>
            <span
              className={`text-[12px] transition-colors duration-300 ${
                state === 'todo'
                  ? 'text-drop-dim'
                  : state === 'current'
                    ? 'text-drop-text'
                    : 'text-drop-muted'
              }`}
              aria-current={state === 'current' ? 'step' : undefined}
            >
              {label}
            </span>
          </li>
        );
      })}
    </ol>
  );
}

export function FormCard({
  eyebrow,
  title,
  number,
  icon,
  children,
}: {
  eyebrow: string;
  title: string;
  number: string;
  icon: IconName;
  children: ReactNode;
}) {
  return (
    <section className="flex-1 rounded-card border border-drop-line bg-drop-surface p-7">
      <header className="flex items-center gap-3.5">
        <span className="flex h-5 w-5 items-center justify-center text-drop-accent">
          <Icon name={icon} size={18} />
        </span>
        <div className="min-w-0 flex-1">
          <p className="connect-mono-label">{eyebrow}</p>
          <p className="mt-1 line-clamp-2 text-[13px] text-drop-text">{title}</p>
        </div>
        <span className="font-mono text-[22px] leading-none text-drop-accent">{number}</span>
      </header>

      <div className="mt-6 flex flex-col gap-5">{children}</div>
    </section>
  );
}

interface FieldProps {
  label: string;
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  error?: string;
  icon?: IconName;
  type?: 'text' | 'email' | 'tel' | 'url' | 'number';
  inputMode?: 'text' | 'numeric' | 'tel' | 'email' | 'url';
  autoComplete?: string;
  hint?: string;
  /** Renders the shared Select with these options instead of an <input>. */
  options?: readonly string[];
  className?: string;
}

export function Field({
  label,
  value,
  onChange,
  placeholder,
  error,
  icon,
  type = 'text',
  inputMode,
  autoComplete,
  hint,
  options,
  className = '',
}: FieldProps) {
  const id = `f-${label.replace(/[^a-zA-Zа-яА-Я0-9]+/g, '-').toLowerCase()}`;
  const invalid = Boolean(error);

  return (
    <div className={`flex flex-col gap-2 ${className}`}>
      <label htmlFor={id} className="connect-mono-label">
        {label}
      </label>

      <div className="relative">
        {icon && (
          <span className="pointer-events-none absolute left-3.5 top-1/2 -translate-y-1/2 text-drop-dim">
            <Icon name={icon} size={15} />
          </span>
        )}

        {options ? (
          <Select
            id={id}
            className="connect-select"
            menuClassName="connect-select-menu"
            ariaLabel={label}
            value={value}
            onChange={onChange}
            invalid={invalid}
            placeholder={placeholder}
            options={options.map((option) => ({ value: option, label: option }))}
          />
        ) : (
          <input
            id={id}
            type={type}
            inputMode={inputMode}
            autoComplete={autoComplete}
            value={value}
            placeholder={placeholder}
            aria-invalid={invalid}
            onChange={(e) => onChange(e.target.value)}
            className={`connect-field ${icon ? 'pl-10' : ''}`}
          />
        )}
      </div>

      {error ? (
        <p className="flex items-center gap-1.5 text-[11px] text-red-400" role="alert">
          <Icon name="info" size={12} />
          {error}
        </p>
      ) : hint ? (
        <p className="text-[11px] text-drop-dim">{hint}</p>
      ) : null}
    </div>
  );
}

export function ChipGroup({
  options,
  selected,
  onToggle,
  error,
}: {
  options: readonly string[];
  selected: readonly string[];
  onToggle: (value: string) => void;
  error?: string;
}) {
  return (
    <div className="flex flex-col gap-2">
      <div className="flex flex-wrap gap-2">
        {options.map((option) => {
          const active = selected.includes(option);
          return (
            <button
              key={option}
              type="button"
              onClick={() => onToggle(option)}
              aria-pressed={active}
              className={`connect-chip ${active ? 'connect-chip-active' : ''} ${
                active ? 'scale-[1.02]' : ''
              }`}
            >
              {active && <Icon name="check" size={11} strokeWidth={2.6} className="mr-1.5" />}
              {option}
            </button>
          );
        })}
      </div>
      {error && (
        <p className="flex items-center gap-1.5 text-[11px] text-red-400" role="alert">
          <Icon name="info" size={12} />
          {error}
        </p>
      )}
    </div>
  );
}

/** Sticky footer bar of the wizard: back link, autosave hint, primary action. */
export function WizardFooter({
  backLabel,
  backTo,
  hint,
  nextLabel,
  onNext,
  nextDisabled,
  loading,
}: {
  backLabel?: string;
  backTo?: string;
  hint?: string;
  nextLabel: string;
  onNext: () => void;
  nextDisabled?: boolean;
  loading?: boolean;
}) {
  const { t } = useLocale();
  return (
    <div className="sticky bottom-0 z-20 border-t border-drop-line bg-[#0B0D0BF2] backdrop-blur-md">
      <div className="mx-auto flex w-full max-w-[1440px] flex-col gap-3 px-4 py-3 sm:flex-row sm:items-center sm:justify-between sm:gap-4 sm:px-6 sm:py-5 lg:px-12">
        {backTo ? (
          <Link
            to={backTo}
          className="group inline-flex min-h-11 items-center gap-2 text-[13px] text-drop-muted transition-colors hover:text-drop-text focus:outline-none focus-visible:ring-2 focus-visible:ring-drop-accent/50"
          >
            <Icon
              name="arrowRight"
              size={15}
              className="rotate-180 transition-transform duration-200 group-hover:-translate-x-0.5"
            />
            {backLabel}
          </Link>
        ) : (
          <span />
        )}

        <div className="flex w-full flex-col gap-3 sm:w-auto sm:flex-row sm:items-center sm:gap-4">
          {hint && (
            <span className="hidden font-mono text-[10px] text-drop-dim sm:inline">{hint}</span>
          )}
          <button
            type="button"
            onClick={onNext}
            disabled={nextDisabled || loading}
            className="connect-btn-primary w-full sm:min-w-[207px] sm:w-auto"
          >
            {loading ? t('wizardSending') : nextLabel}
            {!loading && (
              <Icon
                name="arrowRight"
                size={15}
                strokeWidth={2}
                className="transition-transform duration-200 group-hover:translate-x-0.5"
              />
            )}
          </button>
        </div>
      </div>
    </div>
  );
}
