/**
 * Charts — the panels' only data graphics, all 2D SVG.
 *
 * No chart library: three small components, no extra dependency, and every
 * colour comes from the token layer (`--c-chart-*`, `--c-series-*`), which is
 * what makes them work in both themes and match the surrounding panel.
 *
 * The graphics are drawn in *real pixels*: the SVG is measured with a
 * `ResizeObserver` and the viewBox is the measured box, so nothing is scaled
 * non-uniformly and the 11px axis labels stay 11px at any panel width.
 *
 * Motion (DESIGN.md §Charts): a chart draws itself in once, when it mounts —
 * the line traces left to right, the dots settle, the ring assembles, the bars
 * grow. There is no idle motion and no motion on data updates beyond the same
 * one-shot draw. Every keyframe lives inside `prefers-reduced-motion:
 * no-preference`, so with reduced motion the final state is what renders and
 * `document.getAnimations()` is empty — not a 1ms animation that still runs.
 *
 * Honesty rules (DESIGN.md §Data honesty): a chart receives points and renders
 * them; it never generates, interpolates or "smooths over" missing data. The
 * curve is monotone, so it never overshoots a point it was not given. An empty
 * series must be handled by the caller with an `unavailable` state.
 */
import { useCallback, useEffect, useRef, useState, type ReactNode } from 'react';

export interface ChartPoint {
  label: string;
  value: number;
}

export type SeriesIndex = 1 | 2 | 3 | 4 | 5;

/**
 * The token behind a series. Exported so a caller drawing its own compact
 * summary (a share strip, a legend) uses the same colour as the charts instead
 * of reaching for a literal.
 */
export const seriesColor = (series: SeriesIndex) => `var(--c-series-${series})`;

/**
 * A round top for the axis, and the ticks that go with it.
 *
 * The value is rounded up to a whole number of steps, so the axis reads
 * 0/100/200/300/400/500 rather than 0/125/250/375/500: a quarter of an
 * arbitrary maximum is not a number anyone reads at a glance. At most five
 * intervals are used, and the step is picked from the 1/2/2.5/5 family so the
 * labels stay short.
 */
function niceScale(value: number): { max: number; ticks: number[] } {
  if (!Number.isFinite(value) || value <= 0) return { max: 1, ticks: [0, 1] };
  const magnitude = 10 ** Math.floor(Math.log10(value));
  for (const base of [1, 2, 2.5, 5, 10] as const) {
    const step = base * magnitude;
    const intervals = Math.ceil(value / step);
    if (intervals <= 5) {
      const max = step * intervals;
      return { max, ticks: Array.from({ length: intervals + 1 }, (_, index) => step * index) };
    }
  }
  const max = magnitude * 10;
  return { max, ticks: Array.from({ length: 6 }, (_, index) => (max / 5) * index) };
}

/**
 * Width of the box the chart is drawn in.
 *
 * Charts fill a card whose width is decided by the grid, so the drawing has to
 * follow the box rather than a fixed viewBox. Returns `0` until the first
 * measurement; the caller renders nothing for that one frame instead of a chart
 * at the wrong scale.
 */
function useMeasuredWidth(): [React.RefObject<HTMLDivElement>, number] {
  const ref = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(0);

  const measure = useCallback(() => {
    const node = ref.current;
    if (!node) return;
    const next = Math.round(node.getBoundingClientRect().width);
    setWidth((current) => (Math.abs(current - next) > 1 ? next : current));
  }, []);

  useEffect(() => {
    measure();
    const node = ref.current;
    if (!node || typeof ResizeObserver === 'undefined') {
      window.addEventListener('resize', measure);
      return () => window.removeEventListener('resize', measure);
    }
    const observer = new ResizeObserver(measure);
    observer.observe(node);
    return () => observer.disconnect();
  }, [measure]);

  return [ref, width];
}

/**
 * Monotone cubic through the points (Fritsch–Carlson tangents).
 *
 * A plain Catmull-Rom curve overshoots: on a series that dips to zero it draws
 * below the axis, which is a value the data never had. The monotone variant
 * keeps every segment inside the range of its two endpoints.
 */
function monotonePath(xs: readonly number[], ys: readonly number[]): string {
  const count = xs.length;
  if (count === 0) return '';
  if (count === 1) return `M ${xs[0]} ${ys[0]}`;

  const dx: number[] = [];
  const slope: number[] = [];
  for (let i = 0; i < count - 1; i += 1) {
    const step = xs[i + 1]! - xs[i]!;
    dx.push(step);
    slope.push(step === 0 ? 0 : (ys[i + 1]! - ys[i]!) / step);
  }

  const tangent: number[] = new Array(count).fill(0);
  tangent[0] = slope[0] ?? 0;
  tangent[count - 1] = slope[count - 2] ?? 0;
  for (let i = 1; i < count - 1; i += 1) {
    const previous = slope[i - 1] ?? 0;
    const next = slope[i] ?? 0;
    tangent[i] = previous * next <= 0 ? 0 : (previous + next) / 2;
  }
  // Clamp each tangent to the secants around it, which is what removes the
  // overshoot the plain average would still allow.
  for (let i = 0; i < count - 1; i += 1) {
    const secant = slope[i] ?? 0;
    if (secant === 0) {
      tangent[i] = 0;
      tangent[i + 1] = 0;
      continue;
    }
    const a = (tangent[i] ?? 0) / secant;
    const b = (tangent[i + 1] ?? 0) / secant;
    const magnitude = Math.hypot(a, b);
    if (magnitude > 3) {
      tangent[i] = (3 / magnitude) * a * secant;
      tangent[i + 1] = (3 / magnitude) * b * secant;
    }
  }

  let d = `M ${xs[0]!.toFixed(2)} ${ys[0]!.toFixed(2)}`;
  for (let i = 0; i < count - 1; i += 1) {
    const third = dx[i]! / 3;
    const c1x = xs[i]! + third;
    const c1y = ys[i]! + (tangent[i] ?? 0) * third;
    const c2x = xs[i + 1]! - third;
    const c2y = ys[i + 1]! - (tangent[i + 1] ?? 0) * third;
    d += ` C ${c1x.toFixed(2)} ${c1y.toFixed(2)}, ${c2x.toFixed(2)} ${c2y.toFixed(2)}, ${xs[i + 1]!.toFixed(2)} ${ys[i + 1]!.toFixed(2)}`;
  }
  return d;
}

// ---------------------------------------------------------------------------
// TrendChart — a series over time
// ---------------------------------------------------------------------------

export interface TrendChartProps {
  points: readonly ChartPoint[];
  /** Accessible summary. Include the numbers: the picture is not announced. */
  ariaLabel: string;
  height?: number;
  valueFormat?: (value: number) => string;
  /** Unit caption at the top of the axis, e.g. "requests". Rendered once. */
  unit?: string;
}

/**
 * Room around the plot. The top pad grows when there is a unit caption: the
 * caption sits above the highest tick, and at the default pad the two would
 * print on top of each other.
 */
const trendPad = (unit?: string) => ({ top: unit ? 30 : 16, left: 46, right: 12, bottom: 26 });

export function TrendChart({
  points,
  ariaLabel,
  height = 220,
  valueFormat = (value) => value.toLocaleString('en-US'),
  unit,
}: TrendChartProps) {
  const [ref, width] = useMeasuredWidth();
  const pad = trendPad(unit);
  const count = points.length;
  const innerW = Math.max(0, width - pad.left - pad.right);
  const innerH = Math.max(0, height - pad.top - pad.bottom);
  const scale = niceScale(Math.max(...points.map((point) => point.value), 0));
  const max = scale.max;
  const xAt = (index: number) => pad.left + (count <= 1 ? innerW / 2 : (index / (count - 1)) * innerW);
  const yAt = (value: number) => pad.top + innerH - (value / max) * innerH;

  const xs = points.map((_, index) => xAt(index));
  const ys = points.map((point) => yAt(point.value));
  const line = monotonePath(xs, ys);
  const baseline = pad.top + innerH;
  const area =
    count > 0 ? `${line} L ${xs[count - 1]!.toFixed(2)} ${baseline} L ${xs[0]!.toFixed(2)} ${baseline} Z` : '';
  const ticks = scale.ticks;
  /**
   * How many x labels actually fit beside each other.
   *
   * The step used to be a fixed `count / 7`, which is correct on a 900px panel
   * and wrong on a phone: at 390px the plot is ~250px wide, so the first two
   * 11px labels printed into each other and the axis read `00:0004:0008:00`.
   * The count now comes from the measured plot width and the longest label in
   * the series (an 11px axis label is ~6.6px per character, plus a 12px gap),
   * so the same seven
   * points print seven labels on a desktop panel and four on a phone. The last
   * label is kept when it still has room, because "Now" is the reading the eye
   * goes to first.
   */
  const labelChars = points.reduce((longest, point) => Math.max(longest, point.label.length), 1);
  const labelWidth = labelChars * 6.6 + 12;
  const labelSlots = Math.max(2, Math.floor(innerW / labelWidth) + 1);
  const labelStep = Math.max(1, Math.ceil((count - 1) / Math.max(1, labelSlots - 1)));
  const labelIndexes = new Set<number>();
  if (count > 0) {
    for (let index = 0; index < count; index += labelStep) labelIndexes.add(index);
    const lastLabel = count - 1;
    const highest = Math.max(...labelIndexes);
    if (!labelIndexes.has(lastLabel) && lastLabel - highest >= Math.max(1, Math.round(labelStep * 0.6))) {
      labelIndexes.add(lastLabel);
    }
  }
  const lastIndex = count - 1;

  // The hovered point is a reading, not decoration: it names the interval and
  // prints the exact value, which the dots alone cannot do at 2.5px.
  const [active, setActive] = useState<number | null>(null);
  const activePoint = active === null ? null : points[active] ?? null;

  return (
    <div
      className="chart"
      ref={ref}
      style={{ height }}
      onMouseLeave={() => setActive(null)}
    >
      {width > 0 && count > 0 ? (
        <svg
          className="chart-svg"
          width={width}
          height={height}
          viewBox={`0 0 ${width} ${height}`}
          role="img"
          aria-label={ariaLabel}
        >
          <title>{ariaLabel}</title>
          {ticks.map((tick) => (
            <line
              key={tick}
              className="chart-grid-line"
              x1={pad.left}
              x2={width - pad.right}
              y1={yAt(tick)}
              y2={yAt(tick)}
            />
          ))}
          {ticks.map((tick) => (
            <text
              key={`tick-${tick}`}
              className="chart-axis-label"
              x={pad.left - 10}
              y={yAt(tick) + 4}
              textAnchor="end"
            >
              {valueFormat(tick)}
            </text>
          ))}
          {unit ? (
            <text className="chart-axis-label chart-axis-unit" x={pad.left - 10} y={12} textAnchor="end">
              {unit}
            </text>
          ) : null}
          <path className="chart-area" d={area} />
          <path className="chart-line" d={line} pathLength={1} />
          {points.map((point, index) =>
            labelIndexes.has(index) ? (
              <text
                key={`label-${point.label}-${index}`}
                className="chart-axis-label"
                x={xAt(index)}
                y={height - 8}
                textAnchor={index === 0 ? 'start' : index === lastIndex ? 'end' : 'middle'}
              >
                {point.label}
              </text>
            ) : null,
          )}
          {points.map((point, index) => (
            <circle
              key={`dot-${point.label}-${index}`}
              className={`chart-dot ${index === lastIndex ? 'is-last' : ''} ${index === active ? 'is-active' : ''}`.trim()}
              cx={xs[index]}
              cy={ys[index]}
              r={index === active ? 5 : index === lastIndex ? 4 : 2.5}
              style={{ ['--dot-index' as string]: index }}
            />
          ))}
          {/* Hit areas last, so they sit above the drawing and every interval
              is reachable, not just the 2.5px dot. */}
          {points.map((point, index) => {
            const half = count <= 1 ? innerW / 2 : innerW / (count - 1) / 2;
            return (
              <rect
                key={`hit-${point.label}-${index}`}
                x={xAt(index) - half}
                y={pad.top}
                width={Math.max(half * 2, 1)}
                height={innerH}
                fill="transparent"
                onMouseEnter={() => setActive(index)}
                onFocus={() => setActive(index)}
                tabIndex={-1}
              />
            );
          })}
        </svg>
      ) : null}
      {activePoint && active !== null && width > 0 ? (
        <div
          className="chart-tooltip"
          style={{
            left: Math.min(
              Math.max((xs[active] ?? pad.left) - 48, pad.left),
              Math.max(pad.left, width - pad.right - 96),
            ),
          }}
          aria-hidden="true"
        >
          <span className="chart-tooltip-label">{activePoint.label}</span>
          <span className="chart-tooltip-value">{valueFormat(activePoint.value)}</span>
        </div>
      ) : null}
    </div>
  );
}

// ---------------------------------------------------------------------------
// DonutChart — part of a whole
// ---------------------------------------------------------------------------

export interface DonutSegment {
  label: string;
  value: number;
  series: SeriesIndex;
}

export interface DonutChartProps {
  segments: readonly DonutSegment[];
  ariaLabel: string;
  size?: number;
  thickness?: number;
  centerValue?: ReactNode;
  centerLabel?: string;
}

export function DonutChart({
  segments,
  ariaLabel,
  size = 168,
  thickness = 14,
  centerValue,
  centerLabel,
}: DonutChartProps) {
  const total = segments.reduce((sum, segment) => sum + segment.value, 0);
  const radius = (size - thickness) / 2;
  const circumference = 2 * Math.PI * radius;
  let offset = 0;

  return (
    <div className="donut-wrap" style={{ width: size, height: size }}>
      <svg
        className="donut-svg"
        width={size}
        height={size}
        viewBox={`0 0 ${size} ${size}`}
        role="img"
        aria-label={ariaLabel}
      >
        <title>{ariaLabel}</title>
        <circle
          className="donut-track"
          cx={size / 2}
          cy={size / 2}
          r={radius}
          fill="none"
          strokeWidth={thickness}
        />
        {total > 0
          ? segments.map((segment, index) => {
              const length = (segment.value / total) * circumference;
              const dash = Math.max(0, length - 2);
              const element = (
                <circle
                  key={segment.label}
                  className="donut-arc"
                  cx={size / 2}
                  cy={size / 2}
                  r={radius}
                  fill="none"
                  strokeWidth={thickness}
                  strokeLinecap="butt"
                  strokeDasharray={`${dash} ${circumference - dash}`}
                  strokeDashoffset={-offset}
                  transform={`rotate(-90 ${size / 2} ${size / 2})`}
                  style={{
                    stroke: seriesColor(segment.series),
                    ['--arc-index' as string]: index,
                  }}
                />
              );
              offset += length;
              return element;
            })
          : null}
      </svg>
      {centerValue || centerLabel ? (
        <div className="donut-center">
          {centerValue ? <div className="donut-value">{centerValue}</div> : null}
          {centerLabel ? <div className="donut-label">{centerLabel}</div> : null}
        </div>
      ) : null}
    </div>
  );
}

export function LegendRow({
  segment,
  total,
  valueFormat,
}: {
  segment: DonutSegment;
  total: number;
  valueFormat?: (value: number) => string;
}) {
  const share = total > 0 ? Math.round((segment.value / total) * 100) : 0;
  return (
    <li className="legend-item">
      <span className="legend-dot" style={{ background: seriesColor(segment.series) }} aria-hidden="true" />
      <span>{segment.label}</span>
      <span className="legend-value">{valueFormat ? valueFormat(segment.value) : `${share}%`}</span>
    </li>
  );
}

// ---------------------------------------------------------------------------
// CapacityBars — a value against its ceiling
// ---------------------------------------------------------------------------

export interface CapacityBarItem {
  label: string;
  value: number;
  /** The ceiling for this item (capacity, quota, target). `0` hides the fill. */
  max: number;
  /** Exact reading, e.g. "316 / 420 · 75%". Rendered next to the label. */
  valueLabel: string;
  note?: string;
}

export interface CapacityBarsProps {
  items: readonly CapacityBarItem[];
  ariaLabel: string;
}

/**
 * One row per zone: the track is the ceiling, the fill is what is inside it.
 *
 * The share is computed against the item's own ceiling, not against the largest
 * item, so a bar that is full is full. The track width is the ceiling, which is
 * why the rows are only comparable inside a row and never across rows — the
 * exact reading sits next to every label for that reason.
 */
export function CapacityBars({ items, ariaLabel }: CapacityBarsProps) {
  return (
    <div className="bar-list" role="img" aria-label={ariaLabel}>
      {items.map((item, index) => {
        const share = item.max > 0 ? Math.min(1, Math.max(0, item.value / item.max)) : 0;
        return (
          <div key={item.label} className="bar-row">
            <div className="bar-row-head">
              <span className="bar-row-label">{item.label}</span>
              <span className="bar-row-value">{item.valueLabel}</span>
            </div>
            <div className="bar-track">
              <div
                className="bar-fill is-animated"
                style={{ width: `${Math.round(share * 100)}%`, ['--bar-index' as string]: index }}
              />
            </div>
            {item.note ? <p className="bar-row-note">{item.note}</p> : null}
          </div>
        );
      })}
    </div>
  );
}