import { useLocale } from '../../locale';

export interface SparklineProps {
  values: ReadonlyArray<number>;
  tone?: 'accent' | 'positive' | 'warning' | 'critical' | 'neutral';
  label?: string;
  className?: string;
}

/**
 * The compact trend line.
 *
 * The stroke reads a series token, so it is legible on both themes. Warning and
 * critical are state colours, not a second accent: they only appear when the
 * call site has already labelled the value as degraded or failed.
 */
const TONE_COLORS: Record<NonNullable<SparklineProps['tone']>, string> = {
  accent: 'var(--c-series-1)',
  positive: 'var(--c-series-1)',
  warning: 'var(--c-warn)',
  critical: 'var(--c-error)',
  neutral: 'var(--c-text-muted)',
};

export default function Sparkline({ values, tone = 'accent', label, className = '' }: SparklineProps) {
  const { t } = useLocale();
  const safeValues = values.length > 0 ? values : [0];
  const min = Math.min(...safeValues);
  const max = Math.max(...safeValues);
  const range = max - min || 1;
  const points = safeValues.map((value, index) => {
    const x = safeValues.length === 1 ? 50 : 4 + (index / (safeValues.length - 1)) * 92;
    const y = 46 - ((value - min) / range) * 36;
    return `${x},${y}`;
  }).join(' ');
  const color = TONE_COLORS[tone];

  return (
    <svg viewBox="0 0 100 52" preserveAspectRatio="none" className={`h-10 min-w-20 overflow-visible ${className}`.trim()} role="img" aria-label={`${label ?? t('analytics.trend')}: ${safeValues.join(', ')}`}>
      <polyline points={points} fill="none" stroke={color} strokeWidth="2.5" vectorEffect="non-scaling-stroke" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}
