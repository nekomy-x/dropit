/**
 * Segmented — a single-choice control with a roving tabindex (§4.9, §5.3).
 *
 * Contract: the container stamps `data-control="segment"` and
 * `role="radiogroup"`; each option is `role="radio"` with `aria-checked`.
 * Arrow keys move selection (Left/Right, Up/Down), Home/End jump to the ends,
 * and only the checked option (or the first, when none is checked) is tabbable,
 * so the control costs one tab stop. The runner moves with `transform` only,
 * 180ms, and is removed under prefers-reduced-motion by the token layer.
 */
import { useCallback, useEffect, useId, useLayoutEffect, useRef, useState, type KeyboardEvent } from 'react';

export interface SegmentedOption<T extends string> {
  value: T;
  label: string;
  /** Optional accessible description; the visible label is short by design. */
  detail?: string;
}

export interface SegmentedProps<T extends string> {
  options: ReadonlyArray<SegmentedOption<T>>;
  value: T;
  onChange: (value: T) => void;
  /** Accessible name for the group. */
  label: string;
  className?: string;
  disabled?: boolean;
}

export default function Segmented<T extends string>({
  options,
  value,
  onChange,
  label,
  className,
  disabled = false,
}: SegmentedProps<T>) {
  const groupId = useId();
  const containerRef = useRef<HTMLDivElement | null>(null);
  const optionRefs = useRef<Array<HTMLButtonElement | null>>([]);
  const [runner, setRunner] = useState<{ left: number; width: number } | null>(null);

  const selectedIndex = Math.max(0, options.findIndex((option) => option.value === value));

  const measure = useCallback(() => {
    const node = optionRefs.current[selectedIndex];
    if (!node) return;
    setRunner({ left: node.offsetLeft, width: node.offsetWidth });
  }, [selectedIndex]);

  useLayoutEffect(() => {
    measure();
    const node = containerRef.current;
    if (!node || typeof ResizeObserver === 'undefined') return;
    const observer = new ResizeObserver(measure);
    observer.observe(node);
    return () => observer.disconnect();
  }, [measure]);

  // A late web-font swap changes option widths; re-measure once it settles.
  useEffect(() => {
    const fonts = (document as Document & { fonts?: FontFaceSet }).fonts;
    if (!fonts || typeof fonts.ready?.then !== 'function') return;
    let cancelled = false;
    fonts.ready.then(() => { if (!cancelled) measure(); }).catch(() => undefined);
    return () => { cancelled = true; };
  }, [measure]);

  const focusOption = (index: number) => {
    const clamped = Math.min(options.length - 1, Math.max(0, index));
    optionRefs.current[clamped]?.focus();
  };

  const onKeyDown = (event: KeyboardEvent<HTMLButtonElement>, index: number) => {
    if (disabled) return;
    const last = options.length - 1;
    switch (event.key) {
      case 'ArrowRight':
      case 'ArrowDown': {
        event.preventDefault();
        const next = index === last ? 0 : index + 1;
        const option = options[next];
        if (option) onChange(option.value);
        focusOption(next);
        break;
      }
      case 'ArrowLeft':
      case 'ArrowUp': {
        event.preventDefault();
        const next = index === 0 ? last : index - 1;
        const option = options[next];
        if (option) onChange(option.value);
        focusOption(next);
        break;
      }
      case 'Home': {
        event.preventDefault();
        const option = options[0];
        if (option) onChange(option.value);
        focusOption(0);
        break;
      }
      case 'End': {
        event.preventDefault();
        const option = options[last];
        if (option) onChange(option.value);
        focusOption(last);
        break;
      }
      default:
        break;
    }
  };

  return (
    <div
      ref={containerRef}
      className={['club-segmented', className].filter(Boolean).join(' ')}
      role="radiogroup"
      aria-label={label}
      data-control="segment"
      data-disabled={disabled ? 'true' : undefined}
    >
      {runner ? (
        <span
          className="club-segmented-runner"
          aria-hidden="true"
          style={{ width: runner.width, transform: `translateX(${runner.left}px)` }}
        />
      ) : null}
      {options.map((option, index) => {
        const checked = option.value === value;
        return (
          <button
            key={option.value}
            ref={(node) => { optionRefs.current[index] = node; }}
            type="button"
            role="radio"
            id={`${groupId}-${option.value}`}
            aria-checked={checked}
            aria-label={option.detail ? `${option.label} — ${option.detail}` : option.label}
            tabIndex={checked || (!options.some((o) => o.value === value) && index === 0) ? 0 : -1}
            className="club-segmented-option"
            disabled={disabled}
            data-control="button"
            onClick={() => { if (!disabled) onChange(option.value); }}
            onKeyDown={(event) => onKeyDown(event, index)}
          >
            {option.label}
          </button>
        );
      })}
    </div>
  );
}
