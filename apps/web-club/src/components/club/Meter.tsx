/**
 * Meter — a 6px progress bar (§4.9).
 *
 * Track is `inset`, fill is the accent, and the number is always present as
 * text next to it so the bar is never the only carrier of the value. The fill
 * animates with `transform: scaleX()` only.
 */
export interface MeterProps {
  /** 0–1. Values outside the range are clamped. */
  value: number;
  /** Text shown to the right of the bar, e.g. "62 of 180". */
  label?: string;
  /** Accessible description of what is being measured. */
  ariaLabel?: string;
  className?: string;
}

export default function Meter({ value, label, ariaLabel, className }: MeterProps) {
  const ratio = Number.isFinite(value) ? Math.min(1, Math.max(0, value)) : 0;
  return (
    <div className={['club-meter', className].filter(Boolean).join(' ')}>
      <div
        className="club-meter-track"
        role="meter"
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={Math.round(ratio * 100)}
        aria-label={ariaLabel}
      >
        <span className="club-meter-fill" style={{ transform: `scaleX(${ratio})` }} />
      </div>
      {label ? <span className="club-meter-value">{label}</span> : null}
    </div>
  );
}
