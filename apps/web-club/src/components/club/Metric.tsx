/**
 * Metric — one number with an honest source.
 *
 * `data-metric` is on the container and `data-source` carries one of
 * `live | preview | unavailable`, which is exactly what §7.2 check 11 reads.
 * Value is tabular (mono) at 28px, or 20px when `compact`. A metric without a
 * reading shows words, never a zero: `unavailable` content is the caller's
 * honest placeholder, and the source chip always says what happened.
 */
import type { ReactNode } from 'react';
import SourceChip, { type MetricSource } from './SourceChip';

export interface MetricProps {
  label: ReactNode;
  /** Rendered value. Pass `null`/undefined together with source="unavailable". */
  value: ReactNode;
  source: MetricSource;
  /** Honest period/filter caption, e.g. "all time, settled" or "last 24 h". */
  caption?: ReactNode;
  /** Unit shown at 13px next to the value ("requests", "€"). */
  unit?: ReactNode;
  compact?: boolean;
  /** Extra chip next to the source chip (e.g. a period Chip). */
  meta?: ReactNode;
  className?: string;
  /** Placeholder text when there is no reading. */
  unavailableLabel?: string;
}

export default function Metric({
  label,
  value,
  source,
  caption,
  unit,
  compact = false,
  meta,
  className,
  unavailableLabel = 'No reading',
}: MetricProps) {
  const classes = ['club-metric', compact ? 'club-metric-compact' : null, className].filter(Boolean).join(' ');
  const empty = source === 'unavailable' || value === null || value === undefined;
  return (
    <div className={classes} data-metric="" data-source={source}>
      <div className="club-metric-head">
        <span className="club-metric-label">{label}</span>
        <SourceChip source={source} />
        {meta}
      </div>
      <div className="club-metric-body">
        {empty ? (
          <span className="club-metric-value club-metric-value-unavailable">{unavailableLabel}</span>
        ) : (
          <>
            <span className="club-metric-value">{value}</span>
            {unit ? <span className="club-metric-unit">{unit}</span> : null}
          </>
        )}
      </div>
      {caption ? <p className="club-metric-caption">{caption}</p> : null}
    </div>
  );
}
