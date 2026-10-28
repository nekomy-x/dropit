import type { CSSProperties } from 'react';
import Sparkline from './Sparkline';
import type { AnalyticsPoint } from './types';
import { useLocale } from '../../locale';

export interface ResponsiveChartProps {
  title: string;
  points: ReadonlyArray<AnalyticsPoint>;
  valueFormatter?: (value: number) => string;
  tone?: 'accent' | 'positive' | 'warning' | 'critical' | 'neutral';
  className?: string;
  caption?: string;
  /** Says where the series comes from; the panel rules require it to be visible. */
  sourceNote?: string;
}

/**
 * Bar panel for a series the page hands in.
 *
 * Honesty rules (docs/ui/PANEL-DESIGN.md §Data honesty): this component plots
 * exactly the points it is given — no interpolation, no generated series — and
 * the caller is responsible for labelling a simulated or preview series via
 * `caption` / `sourceNote`. Every bar is a button with its exact value in the
 * accessible name, so the chart is readable without hovering.
 */
const TONE_COLOR: Record<NonNullable<ResponsiveChartProps['tone']>, string> = {
  accent: 'var(--c-series-1)',
  positive: 'var(--c-series-1)',
  warning: 'var(--c-warn)',
  critical: 'var(--c-error)',
  neutral: 'var(--c-text-muted)',
};

export default function ResponsiveChart({
  title,
  points,
  valueFormatter = (value) => String(value),
  tone = 'accent',
  className = '',
  caption,
  sourceNote,
}: ResponsiveChartProps) {
  const { t } = useLocale();
  const values = points.map((point) => point.value);
  const max = Math.max(...values, 0);
  const min = Math.min(...values, 0);
  const range = max - min || 1;
  const titleId = `${title.replaceAll(' ', '-').toLowerCase()}-heading`;
  const chartStyle = { '--chart-tone': TONE_COLOR[tone] } as CSSProperties;

  return (
    <section className={`panel-card ${className}`.trim()} aria-labelledby={titleId}>
      <header className="panel-card-head">
        <div>
          <h2 id={titleId} className="panel-card-title">{title}</h2>
          {caption ? <p className="panel-card-sub">{caption}</p> : null}
        </div>
        <div className="panel-card-tools">
          <span className="chip chip-neutral">{t('analytics.intervals', { count: points.length })}</span>
        </div>
      </header>

      <div style={chartStyle} role="group" aria-label={t('analytics.chartLabel', { title })}>
        <div className="grid gap-3" style={{ gridTemplateColumns: 'auto minmax(0, 1fr)', height: 176 }}>
          <div
            aria-hidden="true"
            className="is-mono"
            style={{
              display: 'flex',
              flexDirection: 'column',
              justifyContent: 'space-between',
              textAlign: 'right',
              color: 'var(--c-chart-axis)',
              fontFamily: 'var(--font-mono)',
              fontSize: 'var(--fs-11)',
              fontVariantNumeric: 'tabular-nums',
            }}
          >
            <span>{valueFormatter(max)}</span>
            <span>{valueFormatter(min + range / 2)}</span>
            <span>{valueFormatter(min)}</span>
          </div>
          <div
            style={{
              position: 'relative',
              display: 'flex',
              minWidth: 0,
              alignItems: 'flex-end',
              gap: 6,
              borderBottom: '1px solid var(--c-hairline-strong)',
              borderLeft: '1px solid var(--c-hairline-strong)',
              paddingInline: 4,
            }}
          >
            {points.map((point) => {
              const height = Math.max(4, ((point.value - min) / range) * 100);
              return (
                <button
                  key={point.label}
                  type="button"
                  className="group relative flex min-h-11 min-w-0 flex-1 items-end self-stretch"
                  aria-label={`${point.label}: ${valueFormatter(point.value)}`}
                  title={`${point.label}: ${valueFormatter(point.value)}`}
                >
                  <span
                    aria-hidden="true"
                    className="relative w-full"
                    style={{ height: `${height}%`, borderRadius: '6px 6px 0 0', background: 'var(--chart-tone)' }}
                  />
                  <span
                    aria-hidden="true"
                    className="pointer-events-none absolute bottom-full left-1/2 z-10 mb-1 hidden -translate-x-1/2 whitespace-nowrap rounded px-2 py-1 group-hover:block group-focus-visible:block"
                    style={{
                      border: '1px solid var(--c-hairline-strong)',
                      borderRadius: 'var(--r-control)',
                      background: 'var(--c-raised)',
                      color: 'var(--c-text)',
                      fontFamily: 'var(--font-mono)',
                      fontSize: 'var(--fs-11)',
                    }}
                  >
                    {valueFormatter(point.value)}
                  </span>
                </button>
              );
            })}
          </div>
        </div>
        <div className="mt-2 grid gap-3" style={{ gridTemplateColumns: 'auto minmax(0, 1fr)' }} aria-hidden="true">
          <span />
          <div style={{ display: 'flex', minWidth: 0, gap: 6, paddingInline: 4 }}>
            {points.map((point) => (
              <span
                key={point.label}
                style={{
                  minWidth: 0,
                  flex: 1,
                  overflow: 'hidden',
                  textOverflow: 'ellipsis',
                  whiteSpace: 'nowrap',
                  textAlign: 'center',
                  color: 'var(--c-chart-axis)',
                  fontFamily: 'var(--font-mono)',
                  fontSize: 'var(--fs-11)',
                }}
              >
                {point.label}
              </span>
            ))}
          </div>
        </div>
      </div>

      <details style={{ marginTop: 12, borderTop: '1px solid var(--c-hairline-row)', paddingTop: 10 }}>
        <summary style={{ cursor: 'pointer', color: 'var(--c-text-2)', fontSize: 'var(--fs-12)', fontWeight: 500, paddingBlock: 4 }}>
          {t('analytics.exactValues')}
        </summary>
        <ul
          className="grid gap-x-4 gap-y-2 sm:grid-cols-3"
          style={{ marginTop: 8, padding: 0, listStyle: 'none', gridTemplateColumns: 'repeat(2, minmax(0, 1fr))' }}
        >
          {points.map((point) => (
            <li key={point.label} style={{ display: 'flex', minWidth: 0, justifyContent: 'space-between', gap: 8, fontSize: 'var(--fs-12)' }}>
              <span style={{ overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap', color: 'var(--c-text-muted)' }}>{point.label}</span>
              <span style={{ flex: '0 0 auto', fontFamily: 'var(--font-mono)', fontVariantNumeric: 'tabular-nums', color: 'var(--c-text)' }}>
                {valueFormatter(point.value)}
              </span>
            </li>
          ))}
        </ul>
      </details>

      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 12, marginTop: 12 }}>
        <p className="panel-card-note" style={{ margin: 0 }}>
          {sourceNote ?? t('analytics.chartNote')}
        </p>
        <Sparkline values={values} tone={tone} label={t('analytics.compactTrend', { title })} className="h-6 w-20" />
      </div>
    </section>
  );
}
