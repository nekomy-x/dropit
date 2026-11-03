# Panel design — the reference layout, applied to all three surfaces

Status: authoritative for the light/dark panel redesign.
Canonical source: `packages/ui/` (tokens, panel.css, Icon, theme, charts).
Copy into apps: `node scripts/sync-ui-tokens.mjs` (never edit the copies).

## What this replaces

The three panels (club, admin, DJ console) had three unrelated visual
languages: a dense dark "ops" style in admin, a dark board with boxy tiles in
club, and a separate single-file console for DJ. They also had exactly one
theme — dark — with colours written as literals in ~400 places, so a light
theme was impossible without a rewrite.

The new language is a **light-first dashboard shell with a real dark theme**:

* a white rail, a white top bar, a near-white canvas;
* content as white cards on that canvas, separated by shadow (light) or by a
  surface step (dark) — never by borders on every edge;
* one accent (the DROP IT green), used only for action, live state and the
  chart series;
* a KPI row of four cards, then a wide chart panel beside a narrow panel, then
  two tables.

## Theme contract

```html
<html data-theme="dark">   <!-- default -->
<html data-theme="light">
```

* `index.html` sets the attribute before first paint from
  `localStorage['dropit-theme']`; there is no flash and no layout shift.
* `ThemeToggle` (from `theme.tsx`) is the only control that writes it. It sits
  in the top bar of every panel and in the login screen footer.
* No component may read `prefers-color-scheme` directly, and no component may
  hardcode a colour. Everything reads `--c-*` from `tokens.css`.

### Token map (the only colours allowed)

| token | dark | light | use |
| --- | --- | --- | --- |
| `--c-canvas` | `#080808` | `#F5F5F5` | page background |
| `--c-rail` | `#0E0E0E` | `#FFFFFF` | sidebar, navigation drawer |
| `--c-surface` | `#181818` | `#FFFFFF` | cards |
| `--c-raised` | `#202020` | `#F0F0F0` | chips, secondary buttons, thumbs |
| `--c-inset` | `#111111` | `#F7F7F7` | wells, bar tracks |
| `--c-hairline` | `rgba(255,255,255,.11)` | `#E4E4E4` | one divider per edge |
| `--c-accent` | `#00FF87` | `#087A4C` | primary action, live chip, series |
| `--c-text` | `#F2F2F2` | `#121212` | primary text |
| `--c-text-2` | `#B8B8B8` | `#4A4A4A` | secondary text |
| `--c-text-muted` | `#949494` | `#676767` | labels (≥12px) |
| `--c-text-muted-sm` | `#A2A2A2` | `#5B5B5B` | micro text (11–12px) |

The ramp is **neutral grey, not blue-grey** (changed 2026-10-28). The previous
dark set (`#0A0C11` page, `#12151C` panel, cool-white hairlines) carried a blue
cast that made the panel look blue beside every other black surface; the owner's
swatch is plain `#080808` / `#181818` with a `#313131` divider on the panel, and
that is what the tokens now paint. Every value keeps its measured contrast: the
muted step is the lightest grey that still clears 4.5:1 on the darkest surface
it labels (`--c-seg-active` `#2C2C2C`, 4.6:1).
| `--c-ok` / `--c-warn` / `--c-error` | see file | see file | state, never as a second accent |

The light accent is `#087A4C`, not `#00FF87`: the neon green measures ~1.4:1
on white and cannot carry text or read as a button. Same hue family, 5.4:1 on
white. This is the one place where the two themes are not a pure value swap.

## Geometry, type, motion

* Radii: exactly four — `--r-surface` 16, `--r-tile` 12, `--r-control` 10,
  `--r-chip` 999. No 24px panels, no 20px buttons.
* Type: Inter (self-hosted variable, `public/fonts/inter-latin-var.woff2`),
  six sizes — 28 / 20 / 15 / 13 / 12 / 11. Nothing below 11px. Numbers are
  `tabular-nums`.
* Motion: `--dur-fast` 120ms for state, `--dur-base` 180ms for panels, and
  nothing animates on load. `prefers-reduced-motion` collapses all of it.

## Components (in `panel.css`)

`panel-shell` / `panel-rail` / `panel-topbar` / `panel-content` /
`panel-page-head` · `stat-card` · `panel-card` · `chip` · `btn-*` ·
`field-*` · `field-select*` · `seg` · `data-table` · `list-row` · `chart-*` ·
`chart-*` · `donut-*` · `bar-*` · `panel-error` / `panel-note` / `panel-empty` /
`skeleton` · `panel-nav-trigger` / `panel-nav-drawer` / `panel-nav-sheet*`
(the mobile drawer — see *Mobile navigation*).

### Charts

Data graphics are `packages/ui/charts.tsx` — `TrendChart`, `DonutChart` with
`LegendRow`, and `CapacityBars`. All three are SVG in real pixels: the drawing is
measured with a `ResizeObserver`, so the 11px axis labels stay 11px and nothing
is stretched. Curves are monotone cubic, so a series that bottoms out never draws
below zero. `DESIGN.md` §Charts has the motion contract and the reasons behind
the non-obvious choices.

A chart draws itself in once on mount (line traces, dots settle, ring arcs
appear, bars grow) and then holds still: no idle motion, no animation on data
updates, no moving numbers. Every keyframe is inside
`@media (prefers-reduced-motion: no-preference)`, so under reduced motion the
finished chart is what renders and nothing is left running in
`document.getAnimations()`.

### Dropdowns

A native `<select>` is not used on any product surface: the OS popup cannot
take the product's surface, row height or density, and its height is not
controllable. Every dropdown is `packages/ui/Select.tsx` +
`field-select*` in `panel.css`, so one implementation owns the input, the
height of the open layer and the behaviour on every screen:

* the trigger is the field itself — same `1px` rule under the value and
  `38px` height as `.field-input` (`44px` at `≤640px`), so it sits on the
  same baseline as a text input in a form grid or a toolbar;
* the list opens **directly under the field, `6px` below it**. It is
  `position: fixed` and measured from the trigger rect, so no scrolling table
  or overflow container can clip it; when the viewport has no room below it
  flips above, caps its own height (`≤340px`, `≤55%` of the viewport) and
  scrolls inside;
* it is never narrower than its trigger and never leaves the viewport;
* it is a floating layer: `--c-popover` + `--c-popover-shadow`, no border.
  That is the one surface allowed to shadow on both themes;
* keyboard: `↑/↓` move, `Home/End` jump, `Enter` selects, `Esc` closes and
  returns focus, type-ahead works; rows are 44px tall at touch widths.

`node scripts/select-audit.mjs` verifies all of the above on both themes at
1440 and 390.

The existing class names `.btn-primary`, `.btn-secondary`, `.field-input`,
`.field-label`, `.card` are kept as the product-wide action API, now defined
once in `panel.css`. Pages do not need to be rewritten to keep working.

### Fields

A field is a line, not a box. Every text control in the product — input,
textarea, the `Select` trigger, OTP cell, auth and admin variants — is the same
`1px` rule under the value (`border-bottom`), with no fill, no radius and no
side or top border. The rule is the state: `--c-hairline-strong` at rest,
`--c-text-muted` on hover, `--c-accent` on focus (and while a dropdown is open),
`--c-error` when invalid. Focus adds no frame and no halo — the accent rule and
the caret are the whole focus marker. Definition: the Fields block in
`panel.css`; the app-level variants (`.club-field`, `.connect-field`,
`.admin-control`, `.auth-otp-input`) follow it, and the Flutter guest/DJ apps
use `UnderlineInputBorder` for the same reason.

**The auth field.** `apps/web-club/src/index.css` carries one unlayered
`field-input.auth-input` rule, because an unlayered rule the app did not own
beat `panel.css` and drew a 1px frame, a 10px radius, an inset fill and a
bottom rule on the same input — three separators where the contract has one.
The auth field is now the same line as every other field, at 40px
(`--hit-mobile` at ≤640px) because an auth form is the only thing on its
screen. Anything that adds box chrome to `.field-input` is a regression of a
documented decision, not a local style choice.

### Selection

A control is chrome; body text is copy. Buttons, links, menu rows, segmented
options, chips and table headers are `-webkit-user-select: none`, so dragging
across a control cannot highlight its label. Paragraphs, table cells, hints and
every input keep `user-select: text`, because that text is what people copy.
`packages/ui/pointer-guard.ts` (installed from each app entry point) clears a
selection left over from the page when a control is pressed — a click outside a
selection is a deselect. A press inside a text field or inside the selection
itself is left to the browser.

## Layout rules

* Rail 248px, sticky, full height; content max 1320px, 24px padding.
* Phones ≤640px use a single 12px page gutter and a 14px panel inset. Auth
  cards keep a 16px inset. Tablet gutters remain 16px; desktop spacing stays
  unchanged. Do not add another page gutter inside a panel body.
* Stacked table records put the label above its value at ≤480px, so addresses,
  notes and actions can use the full row instead of a reserved label column.
* Top bar 64px, sticky, holds the command/search field, theme toggle, account.
* KPI row: 4 columns ≥1280px, 2 at 1024–1279, and 2 on phones (the compact
  card — 14px padding, 20px value — fits down to 360px); 1 column below 360.
  Four full-width KPI cards pushed the live state two screens down on a phone.
* Wide+narrow rows: `2fr 1fr` for the chart row, `1.45fr 1fr` for the table row;
  both collapse to one column below 1280px.
* Below 1024px the rail is replaced by a **burger button + left drawer**, not a
  bottom bar. The burger sits first in the top bar and opens
  `packages/ui/nav-drawer.tsx` — a native `<dialog>` sheet carrying the rail's
  own three groups, its own labels, its own order and its live badges. Every
  destination keeps a 44px hit area. See *Mobile navigation* below.

## Mobile navigation (below 1024px)

One component, `packages/ui/nav-drawer.tsx`, copied into both panels. It is the
mobile counterpart of the rail, and it obeys one rule: **it is the rail, not a
summary of it.**

| | old bottom bar (removed) | drawer (current) |
| --- | --- | --- |
| destinations | first five, fixed | all of them, from the same source list |
| labels | abbreviated (`mobileKey`: "Home", "Room", "Price") | the rail's own labels |
| admin coverage | 5 of 17 | 17 of 17 |
| club coverage | 5 of 7 | 7 of 7 |
| bottom edge | permanently occupied | free — the strip owns it |
| live badges | none | `panel-nav-trailing`, same as the rail |

Why it went: a five-slot bar cannot describe a seventeen-destination panel. The
admin panel silently dropped Users, DJs, Clubs, Transactions, Roles, Feature
flags, Economics, Integrations and Mail on every phone; the club panel dropped
QR studio and the DJ console. The bar also fought the deployment strip for the
bottom edge, so both had to spend vertical offset on each other.

**Structure.** The trigger (`.panel-nav-trigger`, hidden ≥1024px) is a 44px
icon button with a real accessible name (`nav.drawerOpen`) that flips
`aria-expanded`. The panel is a `<dialog>`: focus containment, Escape, the
backdrop and focus return to the trigger are the browser's job, exactly as in
`modal.tsx`. `Escape` closes, a tap on the scrim closes, a tap inside never
does, and choosing a destination (`data-nav-close`) closes.

**Geometry.** A full-height sheet, `min(320px, 88vw)` wide, against the left
edge on `--c-rail` with a single right hairline. Header 64px — the same height
as the top bar it came from — then the scrolling list. No radius: it is a
surface pinned to the viewport edge, not a floating card.

**Contract carried into it.** The list reuses `panel-nav-group` /
`panel-nav-label` / `panel-nav-list` / `panel-nav-link`, so a destination cannot
look or behave differently in the drawer than in the rail. Group labels are the
rail's (`nav.workspace` / `nav.manage` / `nav.tools`), never invented. The
drawer's subtitle names the workspace the navigation belongs to (the club name,
or the platform workspace) and its `actions` slot holds the theme switch, so a
phone does not need a second theme control in the top bar.

**Motion.** 180ms slide + scrim fade, inside
`@media (prefers-reduced-motion: no-preference)` only; under reduced motion the
sheet appears directly. No entrance animation gates content and no animation is
left running after the panel is open.

**Deployment strip.** The sheet is full-height, so it does not sit *above* the
strip — the list takes the strip's height back as bottom padding
(`html[data-deployment-bar] .panel-nav-sheet-body`), so the last destination
clears the band and the sheet still reaches the bottom edge.

Verified by `node scripts/mobile-nav-check.mjs` (both apps, both themes, 390 and
1440): drawer contents equal rail contents, 44px targets, Escape returns focus,
body scroll locked and restored, contrast ≥4.5:1, radius set inside the
contract, no animation left running under reduced motion.

## Mobile content (below 768px)

The rail is not the only desktop instrument these panels have. A dense data
table is the other one, and on a phone it used to be a 760px grid inside a
326px card: the columns past the fold were clipped mid-word (a status header
reading "St", half an address) and the remainder was reachable only through a
horizontal scroll nobody discovers on touch. The rule below replaces that.

**A table becomes a list of labelled rows.** Mark the table
`data-table--stack` and give every value cell the column's own translated
label:

```html
<table class="data-table data-table--stack">
  <thead><tr><th scope="col">E-mail</th><th scope="col">Status</th></tr></thead>
  <tbody>
    <tr>
      <td class="cell-primary">Nora Vex</td>              <!-- identity: no label -->
      <td data-label="E-mail">nora@nebula.test</td>       <!-- same string as the th -->
      <td data-label="" class="cell-actions">…</td>       <!-- actions: no label -->
    </tr>
  </tbody>
</table>
```

* the label is the header's own string, so a translated column stays translated
  and the two cannot drift apart;
* `cell-primary` is the record's identity line (15px, 600);
* `cell-actions` holds the row's buttons, which stretch to a thumb target;
* the desktop `min-width` on the table stays as written — the shared block
  cancels it below 768px, where the grid no longer exists;
* the `<thead>` is visually hidden but still announced, so a screen reader keeps
  the column name before the value.

A **genuine matrix or a document table** (role × permission, markdown reference)
must not stack. Mark it `data-mobile="scroll"`; it may stay wider than the card,
but it sits in a keyboard-focusable, visibly scrollable container. Any table that
neither stacks nor declares `scroll` fails the audit, because that is exactly the
clipped-grid bug.

**The rest of the phone layout** follows the same "the tool, not the page" rule:

* `.panel-page-head` stacks at ≤640: title, then its actions on their own row;
* `.panel-card` keeps 20px padding (the phone width comes out of the content
  padding, not out of every card);
* `.panel-modal` becomes a sheet: `100vw - 24px` wide, `100dvh - 24px` tall,
  footer buttons stretch;
* `TrendChart` picks its x-label density from the measured plot width (an 11px
  label is ~6.6px per character + a 12px gap), so a seven-point series prints
  seven labels on a desktop panel and four on a phone instead of overlapping;
* every control that is reachable by thumb keeps a 44px target down to 320px.

Verified in `node scripts/panel-audit.mjs` (both apps, every route, both themes):
"mobile tables fit or declare scroll @390" and "stacked rows carry column
labels @390", next to the existing overflow ladder (320/390/768/1024/1440),
touch targets, contrast, radius and console checks.

## Deployment strip (test / maintenance / production)

One fixed band on the bottom edge of the viewport, mounted once per app above
the router (`packages/ui/DeploymentStatus.tsx` + `packages/ui/status-bar.css`,
copied into both panels). It is a property of the whole installation, so it is
present on every screen — panel, auth, public Connect — and it cannot be
dismissed.

| resolved mode | strip | tone |
| --- | --- | --- |
| `test` | "Test mode │ open signup · mock payments" | `chip-warn` |
| `maintenance` | "Maintenance │ faults and interruptions are possible" | `chip-error` |
| `normal` | none — not even a hairline | — |

The band is persistent chrome — the same job as the rail and the mobile
navigation — so it is drawn on `--c-rail` behind a single top hairline, and the
state is the product's one status primitive, `.chip`. Two earlier versions
invented a register the product does not have. A full-bleed saturated fill
(`--c-warn` / `--c-error` at 100%) made the banner the loudest object on every
screen and outranked the live session state it sits under. A 10% tone wash mixed
into `--c-surface` fixed the volume but produced a muddy brown band in dark
theme — `color-mix` of amber into near-black reads as a smudge along the bottom
edge rather than as part of the shell.

Now the tone never touches the band at all: `test` carries `chip-warn`,
`maintenance` carries `chip-error`, so a deployment state looks exactly like a
DJ status, a session state or a failed payout. Words carry the state, the chip's
tone is the second signal, and the facts sit beside it on the muted text ramp.

There is no leading dot. The chip's word already says what the mode is, so a
small `currentColor` disc next to it encoded nothing; a hairline rule separates
the state from its facts instead.

* The mode is decided by the server, never by the build: `GET /v1/status`
  returns `mode` + `source`. An admin setting
  (`platform_settings.runtime_mode`) wins; otherwise `DROPIT_MAINTENANCE`
  forces maintenance; otherwise it is adaptive — open signup or mock payments
  mean `test`, everything else `normal`. An old gateway without `/v1/status`
  falls back to `/readyz` (open signup / payment facts), and a failed probe
  renders nothing: an unknown environment may not claim either state.
* The component writes `<html data-deployment-bar="test|maintenance">`; every
  offset in `status-bar.css` keys off that attribute, so page content, the rail
  foot, the navigation drawer's list and the auth cards move up by exactly the
  strip height and return when it is gone. `normal` leaves the DOM untouched.
* Strip height is `--deployment-bar-h` (34px desktop, 44px under 640px) plus
  `env(safe-area-inset-bottom)`. The component also measures its own rendered
  height and publishes it as `--deployment-bar-pad`; every offset writes
  `max(--deployment-bar-h, --deployment-bar-pad)`, because the copy wraps
  differently per language (the Russian test line is 403px wide). On a phone the
  state is the first line and its facts are the second, so nothing is truncated
  and nothing renders under the band.
  Foreground is `--c-rail` against `--c-text-muted` for the facts, and the chip
  carries its own tone pair. Both are measured against the band on every
  route × theme in `scripts/panel-audit.mjs`. Measured: facts 5.84:1 dark /
  4.95:1 light; chip 10.37:1 / 5.15:1 (`test`) and 6.23:1 / 5.41:1
  (`maintenance`) — all above AA.
* The strip is a state report, not decoration: it carries words, not only
  colour, and it is the only deployment indicator in the product — there is no
  second corner badge saying the same thing.

## Data honesty (unchanged, and non-negotiable)

A redesign may not invent a number. The rules the panels already enforce:

1. Every figure carries a source: `live` (API), `preview` (local model) or
   `unavailable` (request failed).
2. `unavailable` renders the word, muted, and **no value** — never the last
   successful value, never `0`.
3. A comparison ("+12% vs last month") is shown only when a previous period
   was actually read. Otherwise the card shows the period caption instead.
4. Charts plot the points they are given. No interpolation over gaps, no
   generated series, no "demo" curve dressed as data.
5. A failed request produces one `panel-error` with the code and a retry —
   not per-card error copy repeated four times.
6. A chart is markup, not a canvas: it renders wherever the panel renders,
   carries its series in `aria-label`, and never degrades into an empty box or a
   placeholder rectangle.

## Live presence (club roster)

`/djs` shows whether each DJ is in their console. The rule that keeps this
honest: **presence is a server fact with an age, never a client guess.**

* The DJ console posts `POST /v1/dj/presence` every 30 s while its tab is
  visible, and again on `visibilitychange` back to visible. A background tab
  stops beating — it is not "in the console".
* `GET /v1/club/djs` returns `last_seen_at`, a server-computed
  `presence_age_seconds`, and `online` (age ≤ `presence_ttl_seconds`, 90 s).
  The club panel renders the state as a status chip with the word ("Online" /
  "Offline" / "Unknown") and the age as the line beneath it. There is no state
  dot: the chip already carries the state, and the word is what a reader uses.
* `presence_age_seconds === null` means the DJ has never opened the console.
  It renders as **Unknown — never opened the console**, not as "offline": the
  panel does not know, and must say so.
* The roster re-reads every 20 s, so presence changes without a manual
  refresh. When a poll fails, the rows stay but a line states how old they are
  (`staleSince`) instead of presenting the old value as current.
* `on_shift` is also a server fact (an unclosed session assigned to that DJ),
  which is why the roster's Shift column can be trusted.

## Roster actions (club DJs)

Three actions, three different meanings — do not merge them:

| Action | Effect | Reversible |
| --- | --- | --- |
| Access (`Suspend` / `Restore access`) | DJ stays on the roster, cannot sign in or be assigned to a shift | yes, same row |
| Note (`Add note` / `Edit note`) | The club's private working reminder (≤500 chars); invisible to the DJ and guests | yes |
| `Remove` | Off the roster; access revoked; shifts and payouts keep their history | yes, via "Show removed" |

`Remove` is a soft delete because `session.dj_id` and `split.payee_id` reference
the DJ — history is never destroyed. An invitation that was never accepted has
no history, so it is deleted outright (otherwise `UNIQUE(email)` would block
re-inviting that address). A DJ with an open shift cannot be removed (`409
dj_on_shift`) — close the shift first. Every action confirms and reports its
outcome; nothing mutates silently.

## Account and language

One account control per panel, in the rail foot. It opens one menu: **Account
settings**, then **Sign out**. The settings dialog carries the account facts the
API returned, the interface language, the theme, and sign-out at the bottom.
There is no second place for identity, and the top bar carries no account chip.
Below 1024px the rail is hidden, so the same component renders as a compact
avatar button in the top bar — the same menu, not a second control.

There is no workspace badge above the nav in any panel. One used to sit there in
all three; measured against the account row it never added a fact, only restated
the same two strings in the opposite order (admin: "DROP IT platform / Platform
operator" above, "Platform operator / DROP IT platform" below; club: "Club Nebula
/ Owner account" above, "Club owner / Club Nebula" below). It also could not
switch to anything, so it read as a broken account picker. The facts it carried
still appear where they belong: the club name is the page identity, and the role
sits in the rail foot. A workspace badge is never a switcher — if there is
nothing to switch to, it does not ship, and the panel preview on the Connect
landing page mirrors the real panels.

Language is a product-wide preference, not a panel setting:

* one provider per app (`apps/*/src/locale.tsx` over `packages/ui/i18n.tsx`);
* one storage key, `dropit.language`, read by the panels, the DJ console and the
  public Connect flow — switching in the account dialog switches the public site
  too, and the choice survives a reload;
* six languages, the same set the public site offers: en, es, fr, de, pt, ru;
* English is the reference catalog. A missing translation renders the English
  sentence, never a raw key, and `scripts/i18n-check.mjs` fails the gate listing
  every gap.

Catalog layout: `packages/ui/messages.ts` is the shared shell vocabulary
(navigation, account, common words) copied into each app; `apps/<app>/src/messages/*.ts`
holds one module per screen area. A duplicate key across modules is reported
instead of letting the last file win.

## Verification

```bash
node scripts/sync-ui-tokens.mjs --check      # copies match the canonical kit
node scripts/i18n-check.mjs                  # all six languages complete, no raw keys
node scripts/panel-audit.mjs --app club      # contrast, radii, overflow, focus, console
node scripts/panel-audit.mjs --app admin
node scripts/panel-shot.mjs --app club  --theme both --size 1440x900 --size 390x844
node scripts/panel-shot.mjs --app admin --theme both --size 1440x900 --size 390x844
node scripts/select-audit.mjs                # every dropdown, both themes, 1440 + 390
node scripts/mobile-nav-check.mjs            # burger + drawer, both apps, both themes
node scripts/ui-audit.mjs                    # contrast + focus + hit areas
```

`i18n-check` also verifies placeholders: a `{name}` dropped in one language, or
a placeholder invented in another, is a failure — the first renders an empty
slot on screen, the second renders a literal `{name}`.

Screenshots land in `output/panel-review/<app>/`. A change is done when both
themes pass on desktop and mobile and the contrast audit is clean.
