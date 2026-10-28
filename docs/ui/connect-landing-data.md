# Connect landing — the data blocks

Status: describes the `/connect` marketing page (`apps/web-club`), sections
`#revenue` and `#guest`. Read `AGENTS.md` first; this file only covers the two
blocks that show numbers.

## Why this file exists

The landing's calculator, its weekday chart and its payout card each used to
carry their own invented numbers, and they contradicted each other and the
product's own price model:

- the calculator said 2,00 € − 0,30 € = **1,70 €** to the club;
- the stats card said "280 requests, €140 for the club" — that is **0,50 €**
  per request;
- the payout card said "642 requests × €0.50 = €1,284.50", which is **321**,
  not 1 284,50;
- the weekday bars were weighted so that their mean was 157 while the caption
  promised "avg. 140".

A visitor could check two of those numbers against each other in ten seconds.
Marketing copy may be illustrative; it may not be arithmetically false.

## One scenario, derived once

`apps/web-club/src/components/connect/landing/scenario.ts` is the only place a
sample figure is written. Everything the two sections print is computed from
it, and the components format it with `Intl.NumberFormat` in the active
language — no number lives inside a translated string.

| Input | Value | Consequence |
| --- | --- | --- |
| service fee | 0,30 € per payment | the only money DROP IT keeps |
| default request price | 2,00 € | club share 1,70 € |
| scans per night | 140 | — |
| nights per month | 26 | 3 640 payments, 6 188,00 € a month |
| weekday weights | sum to exactly 7 | mean night = 140 scans, so the average line matches the control |

The sample week is the weekday series at 140 scans (980 payments, 1 666,00 € to
the club). The week before is 903 payments, so the comparison chart shows a
real +8.5 % rather than a decorative second series. The top-requests card
lists five tracks totalling 312 of those 980 payments.

The panel preview in `#panel` uses the same price model: 87 requests × 1,70 €
= 147,90 €, which is the value its KPI already shows.

## Charts

`landing/charts.tsx` is a small SVG kit that mirrors the panel's chart grammar
(`packages/ui/charts.tsx`, `DESIGN.md §Charts`). It is not an import of the panel
kit — the landing keeps its own small dependency surface — but it draws the same
three things the product draws, and nothing else:

- `TrendChart` — one accent series as a 2 px line over a **flat** 18 % area
  (monotone cubic, so it never dips below a value that was not measured), an
  optional previous series as a quiet dashed line, an optional dashed mean, one
  dot per point, and the peak annotated with its exact value.
- Labels sit under the x axis **only**: the earlier revision also printed a value
  under every weekday, which turned a seven-point trend into a table and is the
  reason the block read as a generic dashboard rather than as this product.
- `DonutSplit` — the part-of-whole ring for one payment (club share vs fee). A
  two-value split is a ring in the panels, not a second bar chart.
- The kit measures itself in real pixels (`ResizeObserver`), so 11 px axis text
  stays 11 px at any width.
- **Label thinning is measured, not guessed.** At 320 px seven English day names
  do not fit 178 px of plot. Every label is laid out, `getBBox` reports its true
  width, and the smallest even stride whose labels clear each other wins
  (`MON · WED · FRI · SUN`). It re-measures after `document.fonts.ready`, because
  a web font changes glyph widths after first layout. No reading is lost: the
  peak prints its value, the tooltip names every day, and the `aria-label`
  spells out all seven.
- One tooltip per point on hover; the figure is focusable and `←`/`→` walk the
  points, `Esc` clears. The figure's `aria-label` spells out every value, so the
  picture is decorative and the numbers are not. The tooltip measures itself and
  clamps inside the plot, so the longer Russian labels cannot be clipped.
- No data animation and no entrance animation: reduced motion changes nothing,
  and `document.getAnimations()` is empty.
- Below 420 px the tooltip fills the plot instead of floating over a point, and
  below ~200 px of plot width it stacks the label above the value: side by side,
  "прошлая неделя 426,70 €" needs 193 px and a 320 px phone leaves 178 px.

## Copy keys

New in `connect/messages.ts` (all six languages): `calcChartTitle`,
`feeSplitTitle`, `statsLegendCurrent`, `statsLegendPrevious`, `statsScenario`,
`statsWeekTotal`, `statsPayments`, `statsAvgNight`, `topRequestsMeta`,
`payoutNext`, `payoutGuestSpend`, `payoutServiceFee`, `payoutClubNet`.
`payoutDescription` now takes `{payments} {share} {fee}`.

Removed because they described figures the page no longer claims: `fridayPeak`,
`payoutMonday`, `clubRevenue`, `requestVolume`, `calcQrWeek`, `revenueChip1..3`
(the three decorative chips under the revenue copy).

`scripts/i18n-check.mjs` does not parse this catalog (its keys are unquoted), so
`node scripts/connect-catalog-check.mjs` covers it: key-set parity across the
six blocks, duplicate keys, and placeholder parity. Run both.

## Verification

- `output/connect-charts-review/` — the measurement harness for this change:
  `audit.mjs` (5 widths × `en`/`ru`, reduced motion, painted-text only),
  `baseline.json` + `baseline/` (pre-change, replayed from a pinned build),
  `audit.json` + `after/`, and `report.md` with the numeric thresholds.
- `output/connect-data-review/` — the earlier pass: screenshots at
  320/390/768/1024/1440 in `en` and `ru`, hover and keyboard states, and the
  arithmetic reconciliation of every printed figure.
- `node scripts/i18n-check.mjs` and `node scripts/connect-catalog-check.mjs`,
  `cd apps/web-club && npm run typecheck`, `npm run build` must be green.
