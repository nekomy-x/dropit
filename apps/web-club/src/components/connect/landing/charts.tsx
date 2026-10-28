/**
 * Charts for the public Connect page.
 *
 * The panels draw their data with `packages/ui/charts.tsx`: a 2px accent line
 * over one flat area, dots, an 11px axis, hairline grid. That grammar is the
 * product's, and a visitor who has seen the panel should recognise this page.
 * The earlier revision drew solid columns with a printed value under every
 * weekday — a chart type the product uses nowhere, and the reason the blocks
 * read as a generic dashboard.
 *
 * These two small SVGs carry the same grammar without importing the panel kit
 * (the landing has its own dependency surface):
 *
 *   - trend: one accent series as line + flat fill, an optional previous series
 *     as a quiet line, an optional dashed mean, a dot per point and the peak
 *     annotated with its exact value;
 *   - labels on the x axis only — the tooltip carries the exact numbers, so
 *     seven printed values do not fight the axis;
 *   - split: a part-of-whole ring for a payment, not a second bar chart.
 *
 * Honesty rules from `scenario.ts` and `DESIGN.md §Charts` still hold: a chart
 * plots the points it is given, never generates or smooths over missing data,
 * the curve is monotone so it cannot draw a value that was not measured, and
 * there is no entrance or data animation. Under reduced motion nothing changes,
 * because nothing was moving.
 *
 * Interaction is one tooltip per point, on hover and with ←/→ when the figure
 * has focus. The figure carries the full series in its `aria-label`; the
 * picture itself is decorative.
 *
 * Two things here look like detail and are not:
 *
 *   - the x labels are thinned by measuring their rendered boxes, not by
 *     counting characters (English "MON" is 27 px at 11 px type, and the first
 *     and last labels are anchored to the plot edge, so an arithmetic estimate
 *     was wrong by 1.3 px on a 360 px screen);
 *   - every hook runs before the empty-series return, so a series that empties
 *     out (a future filter, an unavailable week) cannot change the hook count.
 */
import { useEffect, useLayoutEffect, useMemo, useRef, useState, type KeyboardEvent } from 'react';

export interface TrendPoint {
  label: string;
  value: number;
  /** Extra tooltip line, e.g. "63 × €1.70". */
  note?: string;
}

interface ChartFrameProps {
  height?: number;
  /** Compact reading for the peak annotation, e.g. "464". */
  valueFormat: (value: number) => string;
  /** Y-axis tick, e.g. "500". */
  axisFormat: (value: number) => string;
  /** Full value in the tooltip and the peak annotation, e.g. "€464.10". */
  tooltipFormat?: (value: number) => string;
  ariaLabel: string;
}

export interface TrendChartProps extends ChartFrameProps {
  points: readonly TrendPoint[];
  /** Same length as `points`: the comparison series, drawn as a line only. */
  previous?: readonly number[];
  /** Dashed reference line, in the same unit as the points. */
  mean?: number;
  /** Legend wording reused in the tooltip; falls back to a value row. */
  seriesLabels?: { current: string; previous?: string };
  /** Exact value above the peak dot (default on). Hidden while a point is active. */
  annotatePeak?: boolean;
}

export interface DonutSegment {
  label: string;
  value: number;
  /** `accent` is the series, `muted` the remainder. One accent per chart. */
  tone?: 'accent' | 'muted';
}

export interface DonutSplitProps {
  segments: readonly DonutSegment[];
  ariaLabel: string;
  size?: number;
  thickness?: number;
  /** Centre reading, e.g. "€2.00". */
  centerValue?: string;
  /** Centre caption, e.g. the fee description. */
  centerLabel?: string;
}

/** Room around the plot. Left holds a three-digit tick; bottom holds the day row. */
const PAD = { top: 18, right: 10, bottom: 30, left: 46 };
const NICE_BASES = [1, 2, 2.5, 5] as const;

/**
 * Ticks for a "nice" axis: the smallest step that keeps the chart to five
 * intervals or fewer. 464 → 0…500 by 100; 1 666 → 0…2 000 by 500.
 */
function niceScale(maxValue: number): { max: number; ticks: number[] } {
  if (!Number.isFinite(maxValue) || maxValue <= 0) {
    return { max: 1, ticks: [0, 1] };
  }
  let step = 1;
  let found = false;
  for (let exponent = -2; exponent <= 7 && !found; exponent += 1) {
    const magnitude = 10 ** exponent;
    for (const base of NICE_BASES) {
      const candidate = base * magnitude;
      if (Math.ceil(maxValue / candidate) <= 5) {
        step = candidate;
        found = true;
        break;
      }
    }
  }
  const max = Math.ceil(maxValue / step) * step;
  const ticks: number[] = [];
  for (let value = 0; value <= max + step / 2; value += step) {
    ticks.push(Math.round(value * 100) / 100);
  }
  return { max, ticks };
}

/** Measures the box the SVG will fill; charts live below the fold, so a
 *  first paint at zero width is invisible and harmless. */
function usePlotWidth() {
  const ref = useRef<HTMLDivElement | null>(null);
  const [width, setWidth] = useState(0);

  useEffect(() => {
    const node = ref.current;
    if (!node) return undefined;
    const measure = () => setWidth(node.getBoundingClientRect().width);
    measure();
    if (typeof ResizeObserver === 'undefined') {
      window.addEventListener('resize', measure);
      return () => window.removeEventListener('resize', measure);
    }
    const observer = new ResizeObserver(measure);
    observer.observe(node);
    return () => observer.disconnect();
  }, []);

  return { ref, width };
}

/**
 * Monotone cubic through the points (Fritsch–Carlson tangents) — the same
 * curve as the panel's `TrendChart`.
 *
 * A plain Catmull-Rom spline overshoots: on a series that dips to zero it
 * draws below the axis, which is a value the data never had. The monotone
 * variant keeps every segment inside the range of its two endpoints.
 */
function monotonePath(xs: readonly number[], ys: readonly number[]): string {
  const count = xs.length;
  if (count === 0) return '';
  if (count === 1) return `M ${xs[0]} ${ys[0]}`;

  const dx: number[] = [];
  const slope: number[] = [];
  for (let i = 0; i < count - 1; i += 1) {
    const step = (xs[i + 1] ?? 0) - (xs[i] ?? 0);
    dx.push(step);
    slope.push(step === 0 ? 0 : ((ys[i + 1] ?? 0) - (ys[i] ?? 0)) / step);
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

  let d = `M ${(xs[0] ?? 0).toFixed(2)} ${(ys[0] ?? 0).toFixed(2)}`;
  for (let i = 0; i < count - 1; i += 1) {
    const third = (dx[i] ?? 0) / 3;
    const c1x = (xs[i] ?? 0) + third;
    const c1y = (ys[i] ?? 0) + (tangent[i] ?? 0) * third;
    const c2x = (xs[i + 1] ?? 0) - third;
    const c2y = (ys[i + 1] ?? 0) - (tangent[i + 1] ?? 0) * third;
    d += ` C ${c1x.toFixed(2)} ${c1y.toFixed(2)}, ${c2x.toFixed(2)} ${c2y.toFixed(2)}, ${(xs[i + 1] ?? 0).toFixed(2)} ${(ys[i + 1] ?? 0).toFixed(2)}`;
  }
  return d;
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(Math.max(value, min), max);
}

interface TooltipRow {
  label: string;
  value: string;
  tone: 'current' | 'previous';
}

/**
 * One tooltip for both series.
 *
 * It measures itself and clamps inside the plot instead of guessing a
 * half-width: the Russian labels ("прошлая неделя") are wider than the English
 * ones, so a hardcoded margin let the box cross the frame edge and lose its
 * border and rounded corner. When the box cannot fit beside its point — a
 * narrow phone, or a very wide label — it spans the plot instead of
 * overflowing it.
 */
function ChartTooltip({
  anchorX,
  plotWidth,
  title,
  rows,
  note,
  stacked,
}: {
  /** Centre of the hovered point, in plot pixels. */
  anchorX: number;
  plotWidth: number;
  title: string;
  rows: readonly TooltipRow[];
  note?: string;
  /**
   * Label above value instead of beside it. Two long Russian labels and two
   * amounts need 193 px and a 320 px phone gives the plot 178 px, so a
   * side-by-side row would push the box past the frame. Stacked, the same
   * reading needs the width of one label and stays inside.
   */
  stacked?: boolean;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [boxWidth, setBoxWidth] = useState(0);

  useLayoutEffect(() => {
    const node = ref.current;
    if (!node) return;
    setBoxWidth(node.getBoundingClientRect().width);
  }, [title, rows, note, plotWidth]);

  const spans = boxWidth === 0 ? plotWidth < 420 : boxWidth > plotWidth - 16;
  const left = spans
    ? undefined
    : Math.min(Math.max(anchorX - boxWidth / 2, 0), Math.max(0, plotWidth - boxWidth));

  return (
    <div
      ref={ref}
      className={`pointer-events-none absolute top-0 z-10 rounded-lg border border-drop-line bg-drop-rail px-2.5 py-2 ${
        spans ? 'w-auto' : 'w-max'
      }`}
      // When the box cannot fit beside its point it fills the *plot*, not the
      // whole frame: starting at the axis would print the tooltip on top of the
      // y-axis numbers, which reads as a broken chart rather than a reading.
      style={spans ? { left: PAD.left, right: PAD.right } : { left }}
    >
      <p className="text-[11px] leading-none text-drop-muted">{title}</p>
      {rows.map((row) => (
        // A single-series figure has no legend wording to print, so the row is
        // the reading itself — a dot beside an empty label would be noise.
        <div
          key={row.label || row.tone}
          className={`mt-1.5 gap-4 ${
            stacked ? 'flex flex-col items-start' : `flex items-center ${row.label ? 'justify-between' : 'justify-end'}`
          }`}
        >
          {row.label ? (
            <span className="flex items-center gap-1.5 text-[11px] leading-none text-drop-muted">
              <span
                className={`h-1.5 w-1.5 rounded-full ${
                  row.tone === 'current' ? 'bg-drop-accent' : 'bg-drop-muted'
                }`}
              />
              {row.label}
            </span>
          ) : null}
          <span
            className={`text-[12px] font-medium leading-none text-drop-text tabular ${
              stacked && row.label ? 'ml-3.5' : ''
            }`}
          >
            {row.value}
          </span>
        </div>
      ))}
      {note ? <p className="mt-1.5 text-[11px] leading-none text-drop-muted">{note}</p> : null}
    </div>
  );
}

export function TrendChart({
  points,
  previous,
  mean,
  seriesLabels,
  annotatePeak = true,
  height = 220,
  valueFormat,
  axisFormat,
  tooltipFormat,
  ariaLabel,
}: TrendChartProps) {
  const { ref, width } = usePlotWidth();
  const [active, setActive] = useState<number | null>(null);

  const count = points.length;

  const peakIndex = useMemo(() => {
    let index = 0;
    points.forEach((point, position) => {
      if (point.value > (points[index]?.value ?? 0)) index = position;
    });
    return index;
  }, [points]);

  const maxValue = Math.max(
    ...points.map((point) => point.value),
    ...(previous ?? []),
    mean ?? 0,
    0,
  );
  const { max, ticks } = niceScale(maxValue);
  const baseline = height - PAD.bottom;
  const plotHeight = Math.max(1, baseline - PAD.top);
  const plotWidth = Math.max(1, width - PAD.left - PAD.right);
  const step = count > 1 ? plotWidth / (count - 1) : 0;
  const xAt = (index: number) =>
    count > 1 ? PAD.left + (index / (count - 1)) * plotWidth : PAD.left + plotWidth / 2;
  const yAt = (value: number) => baseline - (value / max) * plotHeight;

  const xs = points.map((_, index) => xAt(index));
  const ys = points.map((point) => yAt(point.value));
  const line = monotonePath(xs, ys);
  const area =
    count > 0
      ? `${line} L ${(xs[count - 1] ?? 0).toFixed(2)} ${baseline} L ${(xs[0] ?? 0).toFixed(2)} ${baseline} Z`
      : '';
  const previousLine =
    previous && previous.length > 0
      ? monotonePath(
          previous.map((_, index) => xAt(index)),
          previous.map((value) => yAt(value)),
        )
      : '';
  const format = tooltipFormat ?? valueFormat;
  const activePoint = active === null ? null : points[active] ?? null;
  const activePrevious = active === null ? null : previous?.[active] ?? null;
  const showPeak = annotatePeak && active === null;

  const moveFocus = (event: KeyboardEvent<HTMLElement>) => {
    if (event.key === 'ArrowRight' || event.key === 'ArrowLeft') {
      event.preventDefault();
      const stepDirection = event.key === 'ArrowRight' ? 1 : -1;
      setActive((current) => {
        const base = current ?? (stepDirection > 0 ? -1 : count);
        return clamp(base + stepDirection, 0, count - 1);
      });
    } else if (event.key === 'Escape') {
      setActive(null);
    }
  };

  const pick = (event: React.PointerEvent<SVGRectElement>) => {
    if (count < 2) {
      setActive(0);
      return;
    }
    const box = event.currentTarget.getBoundingClientRect();
    const offset = event.clientX - box.left;
    setActive(clamp(Math.round(offset / step), 0, count - 1));
  };

  const peakX = xs[peakIndex] ?? PAD.left;
  const anchor =
    peakX < PAD.left + 34 ? 'start' : peakX > width - PAD.right - 34 ? 'end' : 'middle';

  /*
   * Seven day names in 178 px: measured, not estimated.
   *
   * At 320 px the plot is about 178 px wide and an English "MON" is 27 px, so
   * neighbours collided by 8 px. Guessing the width from the character count
   * got 360 px wrong (the estimate said "fits", the render overlapped by
   * 1.3 px), because the first and last labels are anchored to the edge and a
   * three-letter word is not a fixed number of pixels. So the row is measured:
   * every label renders once, `getBBox` reports its true width, and the ones
   * that would overlap are dropped before paint (a `useLayoutEffect`, so no
   * flash). Readings are not lost — the peak prints its exact value, the
   * tooltip names every day, and the figure's `aria-label` spells out all seven.
   */
  const labelNodes = useRef<Array<SVGTextElement | null>>([]);
  const labelKey = points.map((point) => point.label).join('|');
  const [visibleLabels, setVisibleLabels] = useState<number[] | null>(null);
  // A web font changes glyph widths after first layout: measured against the
  // fallback the labels fit, measured against Inter they did not (360 px in
  // English overlapped by 1.3 px). So the row is measured again once the fonts
  // the page actually uses have loaded.
  const [fontsReady, setFontsReady] = useState(false);

  useEffect(() => {
    const fonts = typeof document === 'undefined' ? undefined : document.fonts;
    if (!fonts) return undefined;
    let cancelled = false;
    fonts.ready.then(() => {
      if (!cancelled) setFontsReady(true);
    });
    return () => {
      cancelled = true;
    };
  }, []);

  useLayoutEffect(() => {
    // A stale row is what a resized window leaves behind: `width` is a
    // dependency, but the boxes are read after layout, so the list is rebuilt
    // from scratch on every pass instead of being carried forward.
    if (width === 0) return;
    const nodes = labelNodes.current;
    if (nodes.length !== count || nodes.some((node) => node === null)) {
      setVisibleLabels(null);
      return;
    }

    // An even stride, chosen by measuring: for k = 1, 2, … keep every k-th
    // label plus the final one, then check the real gaps between those boxes.
    // The first k that clears every pair wins, so the axis thins evenly
    // ("MON · WED · FRI · SUN") instead of developing holes. The first and last
    // labels are anchored to the plot edges rather than centred, which is why
    // they collide first — and why the arithmetic estimate was off by 1.3 px.
    const boxes = nodes.map((node) => node!.getBBox());
    // 3 px of air between two 11 px words: enough to read as separate, small
    // enough that a 390 px phone still shows all seven days.
    const MIN_GAP = 3;
    const last = count - 1;
    const clears = (indexes: number[]) => {
      for (let i = 1; i < indexes.length; i += 1) {
        const left = boxes[indexes[i - 1]!];
        const right = boxes[indexes[i]!];
        if (!left || !right) return false;
        if (right.x - (left.x + left.width) < MIN_GAP) return false;
      }
      return true;
    };

    let keep: number[] = [0];
    for (let stride = 1; stride <= count; stride += 1) {
      const candidate: number[] = [];
      for (let index = 0; index < count; index += stride) candidate.push(index);
      if (candidate[candidate.length - 1] !== last) candidate.push(last);
      keep = candidate;
      if (clears(candidate)) break;
    }

    setVisibleLabels((current) =>
      current && current.length === keep.length && current.every((value, i) => value === keep[i])
        ? current
        : keep,
    );
    // `labelKey` stands in for the label text: a new language is a new row.
  }, [width, count, labelKey, plotWidth, fontsReady]);

  const showLabel = (index: number) => visibleLabels === null || visibleLabels.includes(index);

  // After every hook, never between them: an early return above them would
  // change the hook count on an empty series (React throws on that).
  if (count === 0) return null;

  return (
    <figure
      className="m-0 w-full min-w-0 cursor-crosshair rounded-lg outline-none focus-visible:ring-2 focus-visible:ring-drop-accent/40"
      tabIndex={0}
      role="img"
      aria-label={ariaLabel}
      onKeyDown={moveFocus}
      onFocus={() => setActive((current) => current ?? peakIndex)}
      onBlur={() => setActive(null)}
    >
      <div ref={ref} className="relative w-full">
        {width > 0 ? (
          <svg width={width} height={height} role="presentation" className="block">
            {ticks.map((tick) => (
              <line
                key={tick}
                x1={PAD.left}
                x2={width - PAD.right}
                y1={yAt(tick)}
                y2={yAt(tick)}
                className="text-drop-line"
                stroke="currentColor"
                strokeOpacity={tick === 0 ? 1 : 0.55}
                strokeWidth={1}
                shapeRendering="crispEdges"
              />
            ))}

            {active !== null ? (
              <rect
                x={clamp((xs[active] ?? 0) - step / 2, PAD.left, width - PAD.right - step)}
                y={PAD.top}
                width={Math.max(step, 8)}
                height={plotHeight}
                className="text-drop-text"
                fill="currentColor"
                fillOpacity={0.035}
              />
            ) : null}

            {/* Flat fill at one alpha, the same 18 % as the panel's
                `--c-chart-fill`: a vertical gradient would make the same value
                look different at different heights (DESIGN.md §Charts). */}
            <path d={area} className="text-drop-accent" fill="currentColor" fillOpacity={0.18} />

            {previousLine ? (
              <path
                d={previousLine}
                className="text-drop-muted"
                fill="none"
                stroke="currentColor"
                strokeOpacity={0.65}
                strokeWidth={1.5}
                strokeDasharray="4 3"
                strokeLinecap="round"
                strokeLinejoin="round"
              />
            ) : null}

            {/* The mean is a reference, not a series: it sits above the area so
                it stays a crisp hairline, and it is the only dashed rule when
                it is present (the previous series is dashed instead, never both). */}
            {mean !== undefined && mean > 0 ? (
              <line
                x1={PAD.left}
                x2={width - PAD.right}
                y1={yAt(mean)}
                y2={yAt(mean)}
                className="text-drop-accent"
                stroke="currentColor"
                strokeOpacity={0.45}
                strokeDasharray="3 4"
                strokeWidth={1}
              />
            ) : null}

            <path
              d={line}
              className="text-drop-accent"
              fill="none"
              stroke="currentColor"
              strokeWidth={2}
              strokeLinecap="round"
              strokeLinejoin="round"
            />

            {active !== null ? (
              <line
                x1={xs[active] ?? 0}
                x2={xs[active] ?? 0}
                y1={PAD.top}
                y2={baseline}
                className="text-drop-text"
                stroke="currentColor"
                strokeOpacity={0.18}
                strokeWidth={1}
              />
            ) : null}

            {points.map((point, index) => (
              // Every point carries the same dot. The panel's "last point"
              // emphasis marks the latest reading of a live series; a week has
              // no latest point, so Sunday would get an emphasis it did not
              // earn — the peak annotation is what the eye should find.
              <circle
                key={`dot-${point.label}-${index}`}
                cx={xs[index] ?? 0}
                cy={ys[index] ?? 0}
                r={2.5}
                className="text-drop-accent stroke-drop-surface"
                fill="currentColor"
                stroke="currentColor"
                strokeWidth={2}
              />
            ))}

            {active !== null ? (
              <circle
                cx={xs[active] ?? 0}
                cy={ys[active] ?? 0}
                r={4}
                className="text-drop-accent stroke-drop-surface"
                fill="currentColor"
                stroke="currentColor"
                strokeWidth={2}
              />
            ) : null}

            {ticks.map((tick) => (
              <text
                key={`tick-${tick}`}
                x={PAD.left - 10}
                y={yAt(tick) + 4}
                textAnchor="end"
                className="fill-current text-[11px] text-drop-muted tabular"
              >
                {axisFormat(tick)}
              </text>
            ))}

            {points.map((point, index) => {
              const peak = index === peakIndex;
              return (
                <text
                  key={`label-${point.label}-${index}`}
                  ref={(node) => {
                    labelNodes.current[index] = node;
                  }}
                  x={xs[index] ?? 0}
                  y={baseline + 18}
                  textAnchor={index === 0 ? 'start' : index === count - 1 ? 'end' : 'middle'}
                  className={`fill-current text-[11px] ${peak ? 'font-medium text-drop-text' : 'text-drop-muted'}`}
                  // Rendered but not shown: `getBBox` needs a laid-out node, and
                  // a hidden row still measures. See the measurement effect.
                  style={{ visibility: showLabel(index) ? 'visible' : 'hidden' }}
                >
                  {point.label}
                </text>
              );
            })}

            {showPeak ? (
              <text
                x={clamp(peakX, PAD.left + 2, width - PAD.right - 2)}
                y={Math.max(14, (ys[peakIndex] ?? 0) - 9)}
                textAnchor={anchor}
                className="fill-current text-[11px] font-medium text-drop-text tabular"
              >
                {format(points[peakIndex]?.value ?? 0)}
              </text>
            ) : null}

            <rect
              x={PAD.left}
              y={PAD.top}
              width={plotWidth}
              height={plotHeight}
              fill="transparent"
              onPointerMove={pick}
              onPointerDown={pick}
              onPointerLeave={() => setActive(null)}
            />
          </svg>
        ) : null}

        {activePoint && width > 160 ? (
          <ChartTooltip
            anchorX={xs[active ?? 0] ?? 0}
            plotWidth={width}
            title={activePoint.label}
            // `plotWidth` is the whole frame; the plot itself is narrower.
            stacked={plotWidth - PAD.left - PAD.right < 200}
            rows={
              previous
                ? [
                    {
                      label: seriesLabels?.current ?? '',
                      value: format(activePoint.value),
                      tone: 'current' as const,
                    },
                    {
                      label: seriesLabels?.previous ?? '',
                      value: format(activePrevious ?? 0),
                      tone: 'previous' as const,
                    },
                  ]
                : [
                    {
                      label: seriesLabels?.current ?? '',
                      value: format(activePoint.value),
                      tone: 'current' as const,
                    },
                  ]
            }
            note={activePoint.note}
          />
        ) : null}
      </div>
    </figure>
  );
}

export function DonutSplit({
  segments,
  ariaLabel,
  size = 132,
  thickness = 12,
  centerValue,
  centerLabel,
}: DonutSplitProps) {
  const total = segments.reduce((sum, segment) => sum + segment.value, 0);
  const radius = (size - thickness) / 2;
  const circumference = 2 * Math.PI * radius;
  let offset = 0;

  return (
    <div className="relative grid shrink-0 place-items-center" style={{ width: size, height: size }}>
      <svg width={size} height={size} viewBox={`0 0 ${size} ${size}`} role="img" aria-label={ariaLabel}>
        <title>{ariaLabel}</title>
        <circle
          cx={size / 2}
          cy={size / 2}
          r={radius}
          fill="none"
          strokeWidth={thickness}
          className="text-drop-line"
          stroke="currentColor"
        />
        {total > 0
          ? segments.map((segment) => {
              const length = (segment.value / total) * circumference;
              const dash = Math.max(0, length - 2);
              const element = (
                <circle
                  key={segment.label}
                  cx={size / 2}
                  cy={size / 2}
                  r={radius}
                  fill="none"
                  strokeWidth={thickness}
                  strokeLinecap="butt"
                  strokeDasharray={`${dash} ${circumference - dash}`}
                  strokeDashoffset={-offset}
                  transform={`rotate(-90 ${size / 2} ${size / 2})`}
                  className={segment.tone === 'muted' ? 'text-drop-muted' : 'text-drop-accent'}
                  stroke="currentColor"
                  // 0.7 over the card: ~4.7:1, above the 3:1 a graphic needs to
                  // be read rather than guessed. At 0.45 the remainder measured
                  // ~2.8:1 — the size of the fee would be a colour guess.
                  strokeOpacity={segment.tone === 'muted' ? 0.7 : 1}
                />
              );
              offset += length;
              return element;
            })
          : null}
      </svg>
      {centerValue || centerLabel ? (
        <div className="pointer-events-none absolute inset-0 grid place-content-center text-center">
          {centerValue ? (
            <p className="font-display text-[19px] leading-none text-drop-text tabular">{centerValue}</p>
          ) : null}
          {centerLabel ? (
            <p className="mt-1.5 text-[11px] leading-none text-drop-muted">{centerLabel}</p>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}
