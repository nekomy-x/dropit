import Sparkline from './Sparkline';
import type { AnalyticsKpi } from './types';
import { useLocale } from '../../locale';

export interface KpiCardProps extends AnalyticsKpi {
  className?: string;
}

/**
 * One KPI, in the shared `stat-card` composition.
 *
 * Label and trend on the head row, the value at 28px tabular, then the change
 * with its period label. The change string is supplied by the page — this card
 * never computes or infers a comparison (docs/ui/PANEL-DESIGN.md §Data honesty).
 */
export default function KpiCard({ label, value, change, changeLabel, trend, tone = 'neutral', description, className = '' }: KpiCardProps) {
  const { t } = useLocale();
  const deltaClass = tone === 'critical' ? 'is-down' : tone === 'positive' ? 'is-up' : 'is-flat';
  const trendTone = tone === 'critical' ? 'critical' : tone === 'warning' ? 'warning' : tone === 'neutral' ? 'neutral' : 'accent';

  return (
    <article className={`stat-card ${className}`.trim()}>
      <div className="stat-head">
        <p className="stat-label">{label}</p>
        {trend && trend.length > 0 ? (
          <Sparkline values={trend} tone={trendTone} label={t('analytics.trendLabel', { label })} className="h-8 w-16 shrink-0" />
        ) : null}
      </div>
      <p className="stat-value">{value}</p>
      {change || changeLabel ? (
        <div className="stat-row">
          {change ? <span className={`stat-delta ${deltaClass}`}>{change}</span> : null}
          {changeLabel ? <span className="stat-foot">{changeLabel}</span> : null}
        </div>
      ) : null}
      {description ? <p className="stat-foot" style={{ marginTop: 6 }}>{description}</p> : null}
    </article>
  );
}
