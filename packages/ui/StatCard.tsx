/**
 * StatCard — one KPI with its source.
 *
 * The card is the reference layout's unit: label, icon tile, value, and a
 * source row. It never invents a comparison: when there is no previous period
 * to compare against, the row shows the source chip and the honest caption
 * instead of a percentage.
 *
 * `source` is required and stamped as `data-source` on `[data-metric]`, so the
 * acceptance contract (docs/ui/PANEL-DESIGN.md §Data honesty) can verify that
 * every figure on a screen says where it came from.
 */
import type { ReactNode } from 'react';
import Icon from '../Icon';

export type MetricSource = 'live' | 'preview' | 'unavailable';

export interface StatCardProps {
  label: string;
  value: ReactNode;
  icon: `lucide:${string}`;
  source: MetricSource;
  /** Source chip or period note, rendered next to the value. */
  meta?: ReactNode;
  /** One honest sentence under the value (period, filter, caveat). */
  caption?: ReactNode;
  valueMuted?: boolean;
  /** Overrides the rendered value block (used for the unavailable state). */
  children?: ReactNode;
}

export default function StatCard({ label, value, icon, source, meta, caption, valueMuted, children }: StatCardProps) {
  return (
    <article className="stat-card" data-metric="" data-source={source}>
      <div className="stat-head">
        <p className="stat-label">{label}</p>
        <span className="stat-icon"><Icon name={icon} size={17} /></span>
      </div>
      {children ?? (
        <>
          <p className={`stat-value ${valueMuted ? 'is-muted' : ''}`.trim()}>{value}</p>
          {meta ? <div className="stat-row">{meta}</div> : null}
        </>
      )}
      {caption ? <p className="stat-foot">{caption}</p> : null}
    </article>
  );
}
