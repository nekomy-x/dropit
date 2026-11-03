# DROP IT Web Product UI

> **2026-10-27 — panel redesign in force.** All three authenticated panels
> (club, admin, DJ console) now share one token layer with a real light and
> dark theme. The authoritative contract is `docs/ui/PANEL-DESIGN.md`; the
> canonical sources are in `packages/ui/` and are copied into each app with
> `node scripts/sync-ui-tokens.mjs`. Where this file and that one disagree
> about colour, radius or layout, `PANEL-DESIGN.md` wins, because it is
> measured by `scripts/panel-audit.mjs`.

## Direction

Calm, precise operational software with a light-first dashboard shell and a
deliberately nocturnal dark theme. The visual language is specific to DROP IT,
not a copy of ChatGPT, OpenAI, a landing page, or an agency portfolio: a quiet
rail, a single white card surface per group, one accent for actions and live
state, and enough space to scan the room. DROP IT green is reserved for primary
actions, selected states, and live health indicators — never for decoration.

## Tokens

`packages/ui/tokens.css` is the reference implementation, copied byte-for-byte
into both apps (`apps/web-club/src/styles/club-tokens.css`,
`apps/web-admin/src/styles/panel-tokens.css`). Every panel — club, admin, DJ
console — reads these values, and no authenticated surface declares a raw
colour, radius or font size of its own. The table below describes the **dark**
theme; the light values are in the same file under
`:root[data-theme='light']` and in `docs/ui/PANEL-DESIGN.md`. Values marked
**(changed)** replaced the previous token set in the club panel redesign of
2026-10-26; **(themed)** values changed again on 2026-10-27 when the light
theme was introduced; **(neutral, 2026-10-28)** replaces the blue-tinted
graphite with a pure-grey ramp taken from the owner's swatch — page `#080808`,
panel `#181818`, divider reading `#313131` on the panel. The old ramp
(`#0A0C11` / `#12151C`) read blue next to every other black surface.

- Canvas: `#080808` **(neutral — was `#0A0C11`; light `#F5F5F5`)**
- Navigation layer: `#0E0E0E` **(neutral — light `#FFFFFF`)**
- Panel layer: `#181818` **(neutral)**; raised `#202020`; inset `#111111`.
  In the light theme a panel is white on `#F5F5F5` and is separated by
  `--c-panel-shadow`, because a surface ramp is invisible on white.
- Floating layer (menus and dropdown lists): `--c-popover` `#232323`, light
  `#FFFFFF`, with `--c-popover-shadow`. This is the only surface allowed to shadow on
  both themes: it leaves the page ramp, so it needs its own step and its own elevation.
  It is never a panel, and a panel never uses it.
- Dividers: `rgba(255,255,255,.11)` hairline (it lands on `#313131` over the
  `#181818` panel, the divider in the swatch) and `rgba(255,255,255,.07)` for row
  separators **(neutral — was the cool-white `rgba(235,240,250,…)` pair)**
- Primary text: `#F2F2F2` **(neutral — was `#F1F4F7`)**
- Secondary text: `#B8B8B8` **(neutral — was `#B0B8C3`)**
- Muted text: `#949494` at 12px and above, `#A2A2A2` at 11–12px **(neutral — was
  `#858E9C` / `#9AA3B0`)**. `#949494` is the lightest value that still measures
  4.6:1 on `--c-seg-active` (`#2C2C2C`), the darkest surface it ever labels. **`#666666`
  is removed: it measures 3.06:1 on the dark surfaces, below the 4.5:1 AA floor, so it is
  not usable for text at any size.**
- DROP IT green: `#00FF87` (hover `#00D873`; tints at 8% and 14% alpha).
  **In the light theme the accent is `#087A4C`** (hover `#066B42`): the neon
  green measures ~1.4:1 on white, so it cannot carry text or read as a button.
  One green per theme —
  **`#10B981` and `#22C55E` are removed from the club panel (changed)**, because "green"
  was simultaneously meaning live, primary and success.
- Warning: `#FFC65B` at a 10% tint; error: `#FF6B6B` at a 10% tint; never communicate
  state by colour alone — every state carries a word.
- Radius: `16px` surfaces, `12px` nested tiles, `10px` controls, `999px` only for a status
  chip or a segment runner. **Nothing above 16px (changed — the club panel used 28px
  panels and 32px dialogs).** **The same four values apply to every surface in
  `apps/web-club` — the Connect flow, the auth screens and the DJ console included
  (changed 2026-10-27: those screens still carried 4/5/6/7/8/12px corners, so the same
  button was 5px on the landing and 10px in the panel).** Data bars and progress
  indicators are square; only chips, pills and circular markers use `999px`.
- Typography: Inter, self-hosted as one variable `woff2` (`font-weight: 100 900`,
  `font-display: swap`), for all product UI. **`Geist` is declared nowhere in
  `apps/web-club` and is not loaded (changed): before the redesign the club panel asked
  for a font that did not exist, downloaded a second one from a CDN and painted neither.**
  Monospace is the system stack (`ui-monospace, SFMono-Regular, Menlo`) and is used only
  for IDs, timestamps and money. Six type steps: `28 / 20 / 15 / 13 / 12 / 11`; nothing
  below 11px.

## Layout

- Desktop shell uses a fixed `248px` role rail and a flexible content column.
- Header is a quiet `64px` top bar with a command field, the theme switch, system
  status and the operator chip.
- Content is constrained to a readable operational width and uses open spacing rather than stacked decorative cards.
- Mobile navigation is a burger button in the top bar that opens a left drawer
  (`packages/ui/nav-drawer.tsx`). The drawer carries the whole rail — same groups,
  same labels, same order — with 44px targets. **There is no bottom navigation
  bar (changed 2026-10-28):** it held five abbreviated destinations, so two thirds
  of the admin panel and two of the club's seven were unreachable on a phone, and
  it permanently occupied the edge the deployment strip already uses.
- Tables may scroll horizontally on narrow screens; controls remain keyboard accessible.

## Components

- Primary button: green fill, dark text, `36px` minimum height on desktop and `44px` at
  `≤640px`, `10px` radius. Do not make every control a pill.
- Secondary button: transparent graphite control; on club surfaces it has **no border** —
  the `raised` surface and a hover state carry it **(changed)**.
- Active navigation: graphite surface plus a narrow green leading rule.
- Panel: on club surfaces a flat `#11131A` surface, **no border and no shadow
  (changed — it was `#191919` with a thin border)**, `16px` radius. Separation comes from
  the surface ramp and distance. Do not nest panels or build decorative card stacks.
- Checkbox: painted by the kit, not the OS (`appearance: none`) — `1px` hairline
  border, `--r-mark` (`4px`) radius on the dense `16px` box and `--r-control`
  (`10px`) on the `44px` touch box, accent fill plus a dark tick when checked, a
  dash when indeterminate. A native checkbox ignores `border-radius`, so its `2px`
  focus ring was the one place where the ring did not follow the shape of the
  control; with the kit's box the ring follows the corner. Both gates accept `4px`
  in the radius set for exactly this mark.
- Focus: controls (buttons, links, checkboxes, radios, select triggers, custom
  widgets) take the `2px` accent ring with a `2px` offset. **Text entry does not
  (changed 2026-10-28)** — an input, textarea or OTP cell is marked by the accent
  caret plus its own accent rule, with no outline, no box and no halo. A field that
  drew all three at once (ring + border + `3px` tint) read as a text selection rather
  than as focus, and it was the box people complained about in the command palettes.
  One rule decides this for every surface: the caret block in
  `packages/ui/panel.css`; `scripts/panel-audit.mjs` and `scripts/club-ui-check.mjs`
  accept the accent caret as a visible focus marker on text fields and still require
  the ring everywhere else.
- Selection: **a control is chrome, body text is copy (changed 2026-10-28)**. A
  drag across a button, a nav row or the account switcher used to paint a text
  selection over the label, so a slightly sloppy click left "Mark played" or the
  account name highlighted after the action had already run. Chrome (buttons,
  links, menu items, segmented options, chips, table headers, labels) is
  `user-select: none`; paragraphs, table cells, hints and every input stay
  selectable. A pointer press on a control also clears a selection left over
  from the page (`packages/ui/pointer-guard.ts`, installed from each app entry),
  because a click outside a selection means "deselect" everywhere else in the
  platform. Selection inside a field, and a press _inside_ the selection, are
  left to the browser.
- Loading: structural skeletons or a quiet inline status, never a decorative animation.
- Error: restrained red inline panel with a visible retry action.
- Dropdown: **one component, `packages/ui/Select.tsx` — a native `<select>` is not
  used on any product surface (changed 2026-10-27)**, because the OS popup cannot take
  the product's surface, row height or density, and its height is not controllable.
  The trigger is the field itself: the same `1px` rule under the value and the same
  `38px` height as `.field-input` (`44px` at `≤640px`), so a select and a text input sit
  on one baseline in a form grid or a toolbar. The list opens **directly under the field, `6px`
  below it** (`position: fixed`, measured from the trigger rect), so no scrolling table
  or overflow container can clip it; when the viewport has no room below, it flips above,
  caps its own height (`≤340px`, `≤55%` of the viewport) and scrolls inside. It is never
  narrower than its trigger, never leaves the viewport, and it is the one floating layer
  that carries `--c-popover` and `--c-popover-shadow` instead of a border. Keyboard:
  `↑/↓` move, `Home/End` jump, `Enter` selects, `Esc` closes and returns focus,
  type-ahead works. Verified by `scripts/select-audit.mjs`.

## Account and language

- The account lives in one place: a single control in the rail foot opens one
  menu (Settings, then Sign out). The settings dialog holds the account facts,
  the interface language, the theme, and sign-out. No account chip in the top
  bar, no workspace switcher that switches nothing.
- Language is one product-wide preference (`dropit.language`), not a per-surface
  setting: the panels, the DJ console and the public Connect flow read the same
  key in six languages (en, es, fr, de, pt, ru).
- English is the reference catalog; a missing translation renders English, never
  a key. `node scripts/i18n-check.mjs` is the gate.
- Translated labels are longer than English ones. Controls that hold a name or a
  role truncate with an ellipsis rather than wrapping a rail onto three rows;
  money, dates and numbers follow the chosen language via `Intl`.

## Motion

Use `120ms` (fast), `180ms` (base) and `260ms` (slow) transitions only for state changes
and direct feedback, with an ease-out curve **(changed — was 150–250ms; the club panel
motion tokens are `--dur-fast/base/slow`)**. Animate `transform`, `opacity` and colour
only — never `width`, `height`, `top`, `left` or `margin`. Do not add page-load reveals,
scroll choreography, parallax, glow, or decorative animation. Never hide useful content
until an animation runs. `prefers-reduced-motion: reduce` drops every duration to `1ms`
in the token layer, and the club panel then reports zero running animations. Keep live
status changes understandable without motion.

## Auth surfaces (one card)

Every auth screen is the same object: one card, `min(404px, 100%)`, centred on
the canvas — brand lockup and role label, title, the form, the links out, one
honest line of footer. Sign-in, open registration and password recovery are
different jobs, but they are all "fill this in, then get to the work"; a second
layout for the account screens made registration look like a different product
from the sign-in the same person had just used. There is no side rail, no step
list and no facts column. The card lives in `packages/ui/auth.css`, copied into
both apps by `node scripts/sync-ui-tokens.mjs`; nothing on it declares a colour,
radius or font size of its own.

`/login`, `/dj/login`, `/admin/login` fill the card with two fields and one
action. `/register` adds the role tiles and, once a role is chosen, a second
section under a hairline titled "Step 2 · {role}" with the "change role" action
beside it — the position indicator is that caption, not a rail.
`/reset-password` and `/reset-password/:token` fill the same card with one
field and one action each. The fixed corner cluster (test-mode pill, theme
toggle) sits above the card's top padding, so a card that grows past the
viewport starts below it rather than underneath it.

Shared atoms: `PasswordField` (reveal control with `aria-pressed`), `OtpInput`
(six boxes, paste and backspace behave), `AuthAlert` (error / note / ok, each
with its own word), `.auth-choice` role tiles, `.auth-next-step` for the second
section of registration. Titles on this surface stay on the 20px step; a 28px
title belongs to a landing page, not to a form.

## "Not production" indicator

One fixed strip on the bottom edge of the viewport, mounted once per app —
`packages/ui/DeploymentStatus.tsx` + `packages/ui/status-bar.css`, copied into
both panels. It is a property of the whole installation, so it is present on
every screen (panel, auth, public Connect) and cannot be dismissed. `test` reads
"Test mode │ open signup · mock payments" carrying `chip-warn`; `maintenance`
reads "Maintenance │ faults and interruptions are possible" carrying
`chip-error`; `normal` renders nothing at all.

The band is persistent chrome — the same job as the rail and the mobile
navigation — so it is drawn on `--c-rail` behind a single top hairline, and the
state is the product's one status primitive, `.chip`. Two earlier versions
invented a register the product does not have: a full-bleed saturated fill made
the banner the loudest object on every screen, and a 10% tone wash mixed into
`--c-surface` turned muddy brown in dark theme and read as a smudge along the
bottom edge. The tone now never touches the band: it lives in the chip, so a
deployment state looks exactly like a DJ status or a failed payout.

There is no leading dot. The chip's word already names the mode, so a coloured
disc next to it encoded nothing; a hairline rule separates the state from its
facts instead.

This replaces the small `Test stand` corner pill (26px, `--c-warn` outline and
tint, `--r-chip` radius) that used to sit on every auth screen and inline in
every panel top bar. The pill was a second, weaker copy of the same fact and it
carried a 5px disc that duplicated the word next to it. The strip is now the only
deployment indicator in the product: there is no corner badge, pill or inline
footnote saying the same thing anywhere else.

The fact is read, not assumed: `__APP_MODE__` cannot tell the truth about the
stand, because the stand is built with `npm run build`. `/v1/status`
(resolved mode, admin setting or maintenance flag) answers first, `/readyz`
(`open_signup`, `payments`) is the fallback for an older gateway, and the demo
build short-circuits to "stand" without waiting for the network. A failed probe
renders nothing: an unknown environment is not a licence to claim either
"production" or "test".

## Scope

These rules apply to every authenticated surface: the club panel, the admin
control center, and the DJ console. All three now read the same token layer and
the same component kit, so a screen cannot be "the other design" by accident.

Two surfaces are deliberately **not** on this token set:

- The public Connect flow (`apps/web-club/src/connect`) keeps its own `drop`
  palette in `apps/web-club/tailwind.config.js`. It is a marketing surface with
  a different job; it was out of scope for the panel redesign.
- `apps/web-dj/index.html` is a standalone static mock. It is not a product
  surface and is not built or deployed with the panels.

The admin panel's previous private theme (`--admin-accent: #10b981`,
`--admin-panel-radius: 24px`, `Geist`) is **gone**: `admin-theme.css` now only
maps those legacy names onto the shared tokens.

## Admin operations layout

The control center uses the same shell as the club panel, so operators move
between roles without relearning navigation. Its pages are page head → KPI row
→ panels and tables. `AdminOverviewPage` is the reference: four `stat-card`s,
a wide trend panel with a share donut beside it, then a session table beside the
"needs attention" list. Simulated figures (`simulated` chip) and store figures
(`live store` chip) are never mixed without a label.

## Club dashboard exception (venue board)

The club overview (`/dashboard`) is the one surface where a Bento composition and a
background abstraction were explicitly requested by the user and approved. This is a
**documented exception to the "no bento / no decoration" rule in `AGENTS.md`**, granted
by direct user instruction with the budget below. Everything else in `AGENTS.md` stays
in force on this surface: no gradients, no glow, no glass, no grain, no parallax, no
decorative illustration, no fake metrics.

It is a scoped exception, not a new house style. The board uses the same token layer as
every other club screen; what differs is the composition (a 12-column board of panels)
and the abstraction allowance. The other club screens (`/sessions`, `/djs`, `/pricing`,
`/earnings`) and the DJ console keep the compact operational language — panels and list
rows, no board grid, no abstraction.

### Grid and composition

- 12-column grid at `≥1280px`; tiles span 8 or 4. At `768–1279px` each band becomes two
  columns and the widest tiles span both. Below `768px` it is one column.
- Three bands separated by distance (`24px`), not by rules or eyebrow headings.
- Seven tiles, no settings forms and no duplicate tables. Panels use `20px` padding
  (`24px` from `1024px` up) and leave no unused band below their last element.

### Radius

Four values for the whole panel, nothing above `16px`:

| Token | Value | Use |
|---|---|---|
| `--r-surface` | `16px` | panels, dialogs, large containers |
| `--r-tile` | `12px` | nested tiles, input groups, QR frame |
| `--r-control` | `10px` | buttons, fields, segment track, icon buttons |
| `--r-chip` | `999px` | status chip and segment runner — nothing else |

24/28/32px are gone from the club CSS entirely. A circle (dot, avatar, chart point) uses
`--r-chip`, not `50%`, so the rendered set stays at four values.

### Radius, app-wide (changed 2026-10-27)

The four values are the only radii in `apps/web-club`, not just on the board. The
Tailwind scale is remapped in `tailwind.config.js` so no utility can reintroduce an
off-scale corner: `rounded-sm|md|DEFAULT → 10px`, `rounded-lg|xl → 12px`,
`rounded-2xl|3xl|card → 16px`, `rounded-full → 999px`, `rounded-field → 10px`.
Before this pass the app rendered eight values at once on the Connect landing
(`5px`, `6px`, `7px`, `8px`, `12px`, `50%`, `3px 3px 0 0`) and six on the DJ console.
Measured after: `/connect`, `/login`, `/dj/login` and the club panel render exactly
`{10px, 12px, 16px, 999px}`.

### Borders

A panel has **no border and no shadow**; separation is surface level, distance and type.
A border is allowed in exactly three places: a `1px` rule **under** an input/select/
textarea (fields are lines, not boxes — changed 2026-10-28), a `1px` row separator at
`≤6%` white (`rgba(235,240,250,.06)`), and the `2px` focus ring. No side or top border
on a field, no `inset` box-shadow standing in for a border, no double, no dashed, no
border plus shadow.

### Surfaces and colour

`#090A0F` canvas → `#11131A` panel → `#171A22` raised (tiles, hover) → `#0D0F15` inset
(wells, meter tracks). One green, `#00FF87`, for the primary action, the selected
state and the live indicator; `#10B981` / `#22C55E` are not in the panel.

### Type

Six steps `28 / 20 / 15 / 13 / 12 / 11`, nothing below `11px`; a metric value is `28` or
`20` and tabular. Panel title `15/600`, body `13`, label and caption `12`, micro status
`11`. Exactly **one** uppercase-and-tracked element per screen (the club identity line);
chips, labels and statuses are normal case.

### Data honesty

This is a rule of the surface, not a copy style:

- Every metric carries a source chip: `live` (from the club API), `preview` (local model,
  not saved) or `unavailable` (the request failed). The chip is text, never colour alone.
- If any `preview` metric is on screen, one preview banner sits above the board at reading
  size, not as fine print. Floor zones are the only `preview` block, because the backend
  has no zone/occupancy tables.
- A failed request produces `unavailable` with no value — never a stale or invented
  number, and never a zero that reads as real.
- No fabricated telemetry: no random series, no crowd chart, no energy gauge, no
  "tips tonight", no self-incrementing "snapshot Ns ago". A freshness label is derived
  from the last successful fetch and says so when there was none.
- No success toast without a save behind it. Destructive actions confirm before acting.

### Abstraction budget

At most **one** background element per screen, `pointer-events: none`, contrast to its
surface `≤6%`, no blur, no coloured blobs, and it must encode state rather than decorate.
The shipped element is the occupancy arc: an SVG built from the zone model, drawn at `6%`
opacity with a `5%` track, sitting behind the panel content. It is disabled by a single
token — `--abstract-opacity: 0` removes it, and the token layer sets that to `0` under
`prefers-reduced-motion`.

### Theme

The board is themed like every other screen: `<html data-theme>` drives both
palettes, the occupancy arc reads `--abstract-opacity`, and the light theme
keeps the same composition with the card shadow in place of the surface ramp.

## Motion

`transform`, `opacity` and colour only, at `120 / 180 / 260ms` with an ease-out curve:
hover changes the surface colour, press scales to `.99`, the segment runner translates
behind a `180ms` transition, a toast enters and leaves on `opacity` plus an `8px` offset,
and a metric value cross-fades. No entrance cascade, no stagger, no scroll-linked motion,
no infinite loop other than the live dot's `2s` opacity pulse, which is switched off under
`prefers-reduced-motion`. Everything dies under `prefers-reduced-motion`.

## Charts — 2D, drawn by the interface (2026-10-28)

Every chart in the panels is an SVG drawn by `packages/ui/charts.tsx`:
`TrendChart` (a series over time), `DonutChart` + `LegendRow` (a part-of-whole
split) and `CapacityBars` (a value against its ceiling). The admin overview uses
the first two; the club board's Floor load uses the third. There is no chart
library, no canvas and no WebGL — the charts are markup, so they inherit the
type scale, the token colours and the focus behaviour of everything around them.

Earlier revisions of this file described a WebGL layer over `three` and
`@react-three/fiber`. It was removed on 2026-10-28: on a screen full of tables
and 12px labels a floating solid reads as a different product, it cost a 226 kB
lazy chunk, and the third dimension never carried information the numbers did
not already carry. What is left is the grammar the panels use everywhere else.

### Drawing in real pixels

The SVG is measured with a `ResizeObserver` and the viewBox is the measured box,
so nothing is stretched: a `viewBox` scaled to the container would smear the
11px axis labels horizontally on a wide card and squeeze them on a narrow one.
Until the first measurement the chart renders nothing for one frame rather than
a chart at the wrong scale.

The curve is **monotone cubic** (Fritsch–Carlson tangents), not Catmull-Rom. A
smoothing spline that overshoots draws below zero on a series that bottoms out,
which is a value the data never had; the monotone variant keeps every segment
inside the range of its two endpoints.

### Motion

A chart draws itself in once, when it mounts: the line traces left to right over
620ms, the area fades in behind it, the dots settle in order, the ring's arcs
appear in sequence, the bars grow from the left. That is the whole animation
budget — there is no idle motion, no hover physics, no animation when the data
changes, and the numbers themselves never move. A number that animates is a
number the reader cannot trust at a glance.

Every keyframe lives inside `@media (prefers-reduced-motion: no-preference)`.
With reduced motion the base rules already describe the finished chart, nothing
has to be undone, and `document.getAnimations()` is empty — unlike a "0.01ms
duration" trick, which still starts a transition and still reports as running.

### Honesty rules (enforced by the charts)

- A chart plots the points it is given. No generated series, no smoothing over
  gaps, no animation that reveals a value the data did not have.
- No WebGL means nothing here: the chart is markup and renders everywhere,
  including print and the read-only demo build.
- Colour comes only from the token layer (`--c-chart-*`, `--c-series-*`), so
  both themes are one stylesheet away and no chart carries a literal colour.

### Things that look like style choices but are not

- **The area fill is flat, not a gradient.** A vertical fade is a decorative
  ramp that makes the same value look different at different heights, and the
  rest of the product has no gradients. One token (`--c-chart-fill`) at one
  alpha over the card surface is the whole fill.
- **The donut's legend is a column, not a wrapping strip.** Three percentages
  that wrap at different widths put the values on four different right edges;
  a column lines every number up with the card title.
- **The capacity rows do not share a scale.** Each track is that row's ceiling,
  so a full row is full — the rows compare inside themselves, never across, and
  every row carries its exact reading for that reason.
- **The unit caption sits in the top pad, not on the axis.** Drawn next to the
  highest tick it collides with it; the top pad grows by 14px when a unit is
  present instead.
