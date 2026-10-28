import type { AnalyticsPeriod } from './types';
import { useLocale } from '../../locale';

export interface PeriodSwitcherProps {
  value: AnalyticsPeriod;
  onChange: (period: AnalyticsPeriod) => void;
  label?: string;
  className?: string;
}

const PERIODS: ReadonlyArray<{ value: AnalyticsPeriod; key: string }> = [
  { value: '24h', key: 'analytics.period24h' },
  { value: '7d', key: 'analytics.period7d' },
  { value: '30d', key: 'analytics.period30d' },
];

/**
 * Reporting-period control — the shared `seg` segmented switcher.
 *
 * The current period is a real pressed state (`aria-pressed`), so the control is
 * readable without colour; the label names what the period applies to.
 */
export default function PeriodSwitcher({ value, onChange, label, className = '' }: PeriodSwitcherProps) {
  const { t } = useLocale();
  const caption = label ?? t('analytics.reportingPeriod');
  return (
    <div className={`flex flex-wrap items-center gap-2 ${className}`.trim()}>
      <span className="admin-eyebrow">{caption}</span>
      <div className="seg" role="group" aria-label={caption}>
        {PERIODS.map((period) => {
          const selected = period.value === value;
          return (
            <button
              key={period.value}
              type="button"
              aria-pressed={selected}
              onClick={() => onChange(period.value)}
              className={`seg-item ${selected ? 'is-active' : ''}`.trim()}
            >
              {t(period.key)}
            </button>
          );
        })}
      </div>
    </div>
  );
}
