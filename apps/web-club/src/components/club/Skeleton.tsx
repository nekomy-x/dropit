/**
 * Skeleton — structural placeholder (§4.9).
 *
 * Only structural blocks: no shimmer, no gradient, brightness capped at 6%
 * white so a skeleton never reads as content. Compose with `SkeletonText` for
 * a couple of lines or `SkeletonMetric` for a tile-shaped number.
 */
export interface SkeletonProps {
  /** CSS height; defaults to 14px. */
  height?: number | string;
  width?: number | string;
  className?: string;
  /** Blocks are presentational; a wrapper should carry the loading label. */
  label?: string;
}

export default function Skeleton({ height = 14, width = '100%', className, label }: SkeletonProps) {
  return (
    <span
      className={['club-skeleton', className].filter(Boolean).join(' ')}
      style={{ height, width }}
      aria-hidden={label ? undefined : true}
      role={label ? 'status' : undefined}
      aria-label={label}
    />
  );
}

export function SkeletonText({ lines = 2 }: { lines?: number }) {
  return (
    <span className="club-skeleton-stack">
      {Array.from({ length: lines }).map((_, index) => (
        <span
          key={index}
          className={`club-skeleton club-skeleton-line${index === lines - 1 ? ' club-skeleton-line-short' : ''}`}
          style={{ height: 12 }}
        />
      ))}
    </span>
  );
}

export function SkeletonMetric() {
  return (
    <span className="club-skeleton-stack">
      <span className="club-skeleton club-skeleton-line-short" style={{ height: 12 }} />
      <span className="club-skeleton club-skeleton-value" style={{ height: 28 }} />
    </span>
  );
}
