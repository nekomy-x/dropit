# G0 — Baseline: club panel before the redesign

> Owner: **auditor** (read-only on `src/**`). Task `task-1`.
> Server measured: **http://localhost:5199** — pristine frozen copy at
> `/tmp/club-baseline/web-club`, `vite --mode demo`.
> Tooling: Playwright 1.64.1, `@axe-core/playwright` 4.14.0, Chromium 1248.
> Raw data: `output/club-redesign/before/baseline.json`
> (machine-readable), screenshots in the same directory.
> Status: **evidence only — nothing here is a fix.**

Scope of this report: verify, deepen and correct the first pass; do not redo it
blindly. Every number below was re-measured on `:5199`. Where the first pass and
this pass disagree, both are printed.

---

## 0. How this baseline was taken, and why `:5199` matters

| Fact | Value |
|---|---|
| Measured server | `http://localhost:5199` (frozen copy, `vite --mode demo`) |
| Server of the **Lead's pass 1** | `http://localhost:5173` — recorded host list in `baseline-lead-pass1.json` is `localhost:5173` |
| Do the two sources agree? | **Yes, byte-for-byte at measurement time.** At 19:22 MSK, `diff -r /tmp/club-baseline/web-club/src apps/web-club/src` → identical. Every measurement and screenshot below was taken between 19:24 and 19:45 MSK against that identical tree. |
| ⚠️ Tree has since diverged | From ~19:33 MSK G2 began writing `src/styles/club-tokens.css` and `src/components/club/*`; re-running the same `diff` at 19:55 shows `App.tsx`, `Icon.tsx`, `index.css`, `operations.css` changed. **This does not invalidate the baseline** — it is exactly why `:5199` (the frozen copy) exists. It does mean the `:5173` numbers in pass 1 and the current `src` can no longer be re-derived from the working tree. |
| Therefore | Pass-1 numbers are *valid*, but their provenance is `:5173`, which is being edited live. This report re-measured everything on `:5199` so the "before" column cannot drift. |
| Demo backend | `localhost:8081` is **down** → `GET /v1/club/dashboard` → **500** on every load. The error banner is the honest baseline state. |
| Requests on load | `/v1/club/dashboard` is called **twice** at t=0 (React `StrictMode` double-effect in `main.tsx:15`), both 500 |

Reproduce: `node /tmp/measure-baseline.mjs`, `measure2..5.mjs` (harnesses kept
outside the repo so the tree stays clean). They re-measure **`:5199`**, which is
immune to the `src` churn above.

---

## 1. Measurement tables

### 1.1 Page height and structural counts

| Metric | 1440×900 | 1024×768 | 768×1024 | 390×844 | 320×844 | §7.4 threshold |
|---|---|---|---|---|---|---|
| `document.body.scrollHeight` | **2348** | **2618** | **3380** | **4952** | **5093** | ≤1700 / ≤3000 (§4.2) |
| Horizontal overflow (px) | 0 | 0 | 0 | 0 | 0 | 0 (§7.1) |
| Panels (`section` in `.club-dashboard-grid`) | 8 | 8 | 8 | 8 | 8 | ≤7 (01-contract §4) / ≤8 (§4.2) |
| Distinct border-radius values (per element) | **8** | 8 | 8 | **9** | **9** | ≤4 (§7.4) |
| `[data-surface]` | **0** | 0 | 0 | 0 | 0 | required by §7.2 #1 |
| `[data-control]` | **0** | 0 | 0 | 0 | 0 | §7.2 #1 |
| `[data-metric]` / `[data-source]` | **0 / 0** | 0 / 0 | 0 / 0 | 0 / 0 | 0 / 0 | §7.2 #11 |
| `[data-preview-banner]` | **0** | 0 | 0 | 0 | 0 | §7.2 #11 |

**Cross-check §3.1 / §7.4:** heights `2348 / 2618 / 4952` reproduce **exactly**.
`8 radii` reproduces exactly. Overflow-free at all five widths reproduces.
The 9th and 10th radius at mobile is new (§3 only reported the desktop set).

### 1.2 Per-panel geometry and the empty band at the bottom (1440×900)

`gapVisible` = panel bottom − bottom of the last rendered descendant.
`gapBeyondPadding` = the same, minus the panel's own `padding-bottom`.
`text` = measured to the last **text line** box, which is the number §3 quotes.

| # | Panel | Grid | Radius | Border | Height | `gapVisible` | `gapBeyondPadding` | `gapText` | §4.2 limit |
|---|---|---|---|---|---|---|---|---|---|
| 1 | Live venue pulse | span 8, **row span 2** | 28px | 1px | 526 | 27 | **1** | 30 | ≤24 |
| 2 | On the decks | span 4, **row span 2** | 28px | 1px | 526 | 82 | **56** | 94 | ≤24 |
| 3 | Zones | span 4 | 28px | 1px | 535 | **138** | **112** | 141 | ≤24 |
| 4 | Money tonight | span 4 | 28px | 1px | 535 | 27 | 1 | 31 | ≤24 |
| 5 | Guest QR | span 4 | 28px | 1px | 535 | **195** | **169** | 197 | ≤24 |
| 6 | Event & lineup scheduler | span 8 | 28px | 1px | 376 | 102 | **76** | 116 | ≤24 |
| 7 | Most active tables | span 4 | 28px | 1px | 376 | 27 | 1 | 30 | ≤24 |
| 8 | Club rules & music policy | span 12 | 28px | 1px | 335 | 27 | 1 | 41 | ≤24 |

**Correction to §3.1/V5.** §3 states *"«Guest QR» 169px, «Zones» 112px,
«Event & lineup scheduler» 91px, «On the decks» 73px"*. Measured here:
**Guest QR 169 ✓, Zones 112 ✓, Scheduler 76 (not 91), On the decks 56 (not 73)**.
The first two are the `gapBeyondPadding` metric; the last two in §3 were taken to
the last *text* box minus padding, giving 90 and 68 respectively — neither matches
91/73 under any consistent definition. **Use 169 / 112 / 76 / 56 (gap beyond
padding) or 197 / 141 / 116 / 94 (to last text line).** The *direction* of V5 is
correct and the two headline numbers are exact; two of the four numbers in §3
are not reproducible.

The cause is confirmed structurally, not by eye: at 1440 only panels 1 and 2 are
`grid-row: span 2` (`operations.css:464-465`), so panels 3/4/5 all stretch to the
tallest of the row (535 px) and panels 6/7 to theirs (376 px). Panels 2, 3, 5, 6
are therefore stretched by their neighbours, not by their content.

**Mobile is clean.** At 390 and 320 every panel reports `gapBeyondPadding = 1`
(`grid-row: auto` at ≤1023 px, `operations.css:1100-1110`). The mobile emptiness
is *total page height* (4952 / 5093), not per-panel slack.

### 1.3 Border-radius histogram

Per **element** (corner values collapsed), 1440×900 — 8 distinct:

| Radius | Count | Where |
|---|---|---|
| `999px` | 32 | chips, meter, genre buttons, toggle, demo tag, state chip |
| `12px` | 14 | action / control / icon-action, clock, mode container |
| `10px` | 14 | mini-action, avatar, link-action container |
| `28px` | **8** | **every panel** (`operations.css:458`), incl. `-panel-full` (`:741`) |
| `9px` | 4 | active venue-mode button (`operations.css:704`) |
| `50%` | 2 | live dot, chart point |
| `18px` | 1 | error notice (`operations.css:1188`) |
| `16px` | 1 | QR frame (`operations.css:985`) |

At **390×844 → 9 values**: the same set with `24px` ×7 (mobile panel override,
`operations.css:602`) **and `28px` ×1 surviving** — `.club-dashboard-panel-full`
gets `24px` from the media query but the base rule at `:741` also sets `28px`;
the last one wins per screen. Measured result: **7 panels at 24px, the rules
panel still at 28px**. §3.1's "8 radii, all panels 28px" is right for desktop and
**wrong for mobile** — mobile ships a mixed 24/28 panel set today.
At **320×844 → 9 values** (identical to 390).

Modal radius: **32px** (`operations.css:565`), measured live — §3.1 said 32 for
modals, confirmed.

Corner-level histogram (×4 per element, kept for §7.2 #1 comparability):
`999px` 128 · `12px` 56 · `10px` 56 · `28px` 32 · `9px` 16 · `50%` 8 · `18px` 4 · `16px` 4.

CSS declaration counts (source, for G1/G2 cleanup):

| File | Distinct `border-radius` values | `!important` |
|---|---|---|
| `operations.css` (1201 lines) | 17 (`0,1,4,5,6,8,9,10,12,16,18,24,28,32,50%,999px,inherit`) | 11 |
| `index.css` (1778 lines) | 10 (`0,5,6,7,8,9,11,15,50%,999px,0 2px 2px 0`) | 15 |

§3.1 said "17 + 10, `!important` 11 + 15" — **confirmed exactly.**

### 1.4 Borders

| Metric | 1440 | 1024 | 768 | 390 | 320 |
|---|---|---|---|---|---|
| Elements with a border on **any** side | **81** | 81 | 81 | 81 | 81 |
| …of which nested inside a panel | **65** | 65 | 65 | 65 | 65 |
| Full 4-side boxes (`border` shorthand) | 62 | — | — | — | — |
| Bottom-only hairlines (row separators) | 15 | — | — | — | — |
| Top-only hairlines | 4 | — | — | — | — |

> **Reconciliation with §3.** §3.1 says "**66** elements with a border, **58**
> nested". Both numbers are **low by ~15/7**. Measured: **81 / 65** — see the
> breakdown above. §3's 66 most likely counted only *fully boxed* elements
> (measured: 62) plus the 4 panels-with-border that share a class name, i.e. it
> undercounted the 15 bottom-hairline row separators. The §7.4 "before" cell
> should read **81 (65 nested)**, not 66 (58).
>
> Every panel itself carries `border: 1px solid var(--club-dashboard-border)`
> (`operations.css:457`) — the single largest source: 8 panels + 1 notice + 1
> header rule + 1 modal + 1 QR frame.

Non-border "fake borders" (a rule §4.4 forbids equally): **2 inset box-shadows**
used *as* borders — `.club-dashboard-mode.is-active`
(`box-shadow: inset 0 0 0 1px …`, `operations.css:713`) and
`.club-dashboard-timeline-row.is-live` (`inset 2px 0 0`, `operations.css:1009`).

Nested-border inventory by class (any side): `chip` 10 · `genre` 10 ·
`mini-action` 9 · `icon-action` 6 · `avatar` 4 · `action` 3 · `control` 3 ·
lineup/table/timeline/toggle rows 10 · inputs 2 · plus 1 each of clock, modes,
demo-tag, state chip, QR frame, deck, shift.

### 1.5 Typography

Board shell (`.club-dashboard-shell`), text-bearing elements, 1440×900:

| font-size | nodes | | font-size | nodes |
|---|---|---|---|---|
| 10px | **13** | | 16px | 1 |
| 11px | **45** | | 17px | 3 |
| 12px | 61 | | 18px | 1 |
| 13px | 21 | | 20px | 3 |
| 14px | 4 | | 22px | 1 |
| 15px | 10 | | 26px | 1 |
| | | | 30px | 1 |

- **13 distinct sizes** on the board (10–30 px). §3.1 said 12 sizes (10,11,12,13,
  14,15,16,17,18,20,26,30) — this pass additionally finds **22px** (the min-tip
  stepper value, `operations.css:1160`). §3's "13 elements 10px, 45 at 11px" is
  **confirmed exactly**.
- **Text nodes < 12px: 58** (13 at 10px + 45 at 11px). **< 11px: 13.**
- §7.4 asks for ≤6 sizes and min 11px: the board is **2.2× over** on sizes and
  **has 13 nodes below the 11px floor**.
- At 390×844: 13 sizes, **62 nodes < 12px**, 13 nodes < 11px.
- Whole document (shell + rail + header + mobile nav), 1440×900: **15 distinct
  sizes**, histogram adds `9px ×3` (`.club-sidebar-label`) and `23px ×1`;
  **18 text nodes below 11px** document-wide.

Fonts — and this is the part §3.1 got right in direction but the measurements
now prove:

| Probe | Result |
|---|---|
| `getComputedStyle(body).fontFamily` | `Geist, ui-sans-serif, system-ui, …` |
| Family buckets in the shell | `Geist` ×324, `ui-monospace` ×19 |
| `document.fonts` @font-face entries with family `Geist` | **0** |
| Canvas width of `600 30px Geist` | **458.99** |
| Canvas width of `600 30px ZZNotAFont123` | **458.99** ← identical |
| Canvas width of `600 30px system-ui` | 500.14 |
| `document.fonts.load('600 30px Geist')` | no face exists |
| Inter faces found in `document.fonts` | 28 (400/500/600/700, from Google Fonts) |
| Inter faces **loaded** | **1 of 28** (`700`); `document.fonts.check('16px Inter')` → **false** |
| What actually paints | **system fallback** (`ui-sans-serif` → SF Pro on macOS) |

So the product renders in a font nobody chose, while declaring a font that
does not exist **and** loading a third font (Inter) from a CDN that it never
applies. §3.1/V8 is **confirmed and sharpened**: it is not "Geist is declared but
Inter is loaded" — it is "Geist is a fiction, Inter is downloaded and unused, and
system UI font paints the screen."

### 1.6 Interactive targets

| Metric | 1440 | 1024 | 768 | 390 | 320 |
|---|---|---|---|---|---|
| Interactive elements in shell (`a,button,input,select,textarea,[tabindex]`) | 41 | 41 | 41 | 41 | 41 |
| Width **or** height < 44px | **40** | **40** | **40** | **40** | **40** |
| Width **or** height < 36px | **27** | 27 | 27 | **15** | **15** |
| < 44px after resolving to the nearest `label`/`a`/`button` wrapper | 38 | — | — | — | — |

**§3.1's "41 interactive, 40 under 44×44" is confirmed exactly.**

Smallest targets at 1440 (from the tab-order capture, so these are real
`Tab` stops):

| Target | Size | Source |
|---|---|---|
| `Pause QR` / `Activate QR` | **52×17** / 64×17 | `.club-dashboard-link-action`, `operations.css:929-937` |
| genre chip (`House`…) | 61×30 … 87×30 | `.club-dashboard-genre`, `operations.css:1050-1052` (`min-height:30px`) |
| venue mode (`Closed`…) | 64×32 | `.club-dashboard-mode`, `operations.css:700-701` (`min-height:32px`) |
| `Take over` | 80×32 | `.club-dashboard-mini-action`, `operations.css:909-914` (`min-height:32px`) |
| `Rotate code`, `Pause requests`, `Go live`, `Create event` (scheduler) | 32px tall | same |
| icon buttons (`Manage <table>`, stepper ±) | 42×42 | `.club-dashboard-icon-action`, `operations.css:487-507` (`width:42px; flex:0 0 42px`) |
| avatar | 30×30 | `.club-dashboard-avatar`, `operations.css:874-878` |
| toggle switch | 38×22 | `operations.css:1093-1102` |
| toast dismiss | **18×28** | measured live; `operations.css:1139` (`padding:2px`, no min-size) |

At 390 the mobile nav items are **76×64** — compliant. Only 15 targets are under
36px on mobile because `.club-dashboard-mini-action` grows to 40px and the mode
buttons to 36px at ≤640px (`operations.css:1150-1155`); the 17px `link-action`,
the 30px genre chips and the 42px icon buttons remain non-compliant.

Tab order is 48 stops at 1440 and follows visual order. No target is
`display:none`-hidden from keyboard.

### 1.7 Uppercase + letter-spacing

Elements whose own text is uppercased **with** tracking:

| Scope | Count | Instances |
|---|---|---|
| Board shell (`.club-dashboard-shell`) | **2** | `CLUB NEBULA / MAIN ROOM` (`operations.css:646`, 10px/0.12em), `DEMO DATA` (`operations.css:737`, 10px/.04em) |
| Whole document, 1440×900 | **6** | + `WORKSPACE`, `MANAGE`, `TOOLS` (`index.css:192`, 9px/1.26px), `CLUB WORKSPACE` (header, `Layout.tsx:131`, 10px/.18em) |
| Whole document, 390×844 | **3** | rail labels hidden; eyebrow, demo tag, header kicker remain |

§7.2 #10 requires **≤1 per screen**. Document count is **6** at 1440 — 6× over.
§3.1/V7's list of six uppercase levels is **confirmed**, with exact sources.
Note `.club-dashboard-state` ("Peak hours") and `.club-dashboard-chip` ("On
shift") are **not** uppercase (measured `text-transform:none`) — §3.1/V7 lists
`ON SHIFT` and `QR ACTIVE` as uppercase levels, which is **incorrect**:
`.club-dashboard-chip` sets `text-transform` nowhere and renders sentence case
(`operations.css:890-900`). `.club-dashboard-chip` does add `letter-spacing:.02em` (`operations.css:898`)
but with no uppercase, so it does not count under §7.2 #10.

### 1.8 External hosts on load

| Host | Requests | Kind |
|---|---|---|
| `localhost:5199` | 81 | app + HMR |
| **`fonts.googleapis.com`** | 1 | CSS `@font-face` for Inter (`index.html:13`) |
| **`fonts.gstatic.com`** | 1 | Inter `.woff2` (`primary`, 1 of 4 weights) |
| **`code.iconify.design`** | 1 | `iconify-icon` 2.3.0 script (`index.html:14`) |
| **`api.iconify.design`** | 1 | lucide icon JSON, one payload for all icons |

**4 external hosts — §3.1 and §7.4 confirmed exactly.** `preconnect` hints exist
for both Google hosts (`index.html:11-12`), i.e. 2 of the 4 are pure latency for
a font that never applies (§1.5). 25 `<iconify-icon>` elements render, all 25 with
SVG, all fetched from `api.iconify.design` at runtime — **the board's entire icon
set is a network dependency**; if that host is slow the UI renders blank icon
boxes with no local fallback.

### 1.9 Contrast

Two scopes, because the two answer different questions.

**(a) Board only (`.club-dashboard-shell`) — 252 text nodes:**

| Metric | Value |
|---|---|
| Minimum ratio | **5.61:1** |
| Text nodes below 4.5:1 (or 3:1 for large) | **0** |
| Worst offender | `--club-dashboard-muted` `#858E9C` on `#11131A` = 5.61:1 (0.5px margin over AA) |

The board's own palette **passes AA**, but with 11px text at exactly 5.61:1 the
margin is thin. §7.4's contrast row should therefore say "board shell min 5.61:1
(**pass**, but `#858E9C` used down to 10px leaves no headroom)" — the §7.4
value `#666666 = 3.06:1` is a **token** number, not a number measured on this
screen: `#666666` (and the `rgb(102,102,102)` family) **does not appear in any
computed colour on `/dashboard`** (measured: 0 nodes). That row as written is
misleading for the Verifier.

**(b) Whole page — 273 text nodes, 7 below 4.5:1:**

| Ratio | Size | Text | Colour / surface | Source |
|---|---|---|---|---|
| **3.90** | 11px | `Owner account` | `#73737e` / `#141517` | `Layout.tsx:73` `text-neutral-600` |
| **3.91** | 9px | `Workspace` | `#71717a` / `#101113` | `Layout.tsx:79` → `index.css:192` |
| **3.91** | 9px | `Manage` | `#71717a` / `#101113` | `index.css:192` |
| **3.91** | 9px | `Tools` | `#71717a` / `#101113` | `index.css:192` |
| **4.03** | 11px | `Sign out` | `#73737e` / `#101113` | `Layout.tsx:123` |
| **4.25** | 10px | `Club workspace` | `#73737e` / `#09090b` | `Layout.tsx:131` |
| **3.96** | 10px | `⌘ K` | `#71717a` / `#0f0f11` | `CommandMenu` `kbd`, `index.css` |

This reconciles **exactly** with axe (§2): axe reports 7 `color-contrast` nodes at
1440 and 1 at 390, with the same values (3.89 / 3.9 / 4.03 / 4.24). Two
independent implementations agree to ±0.02. **All 7 failures are in the shell /
rail, none on the board** — so §7.1's "no text token below 4.5:1" is currently a
*shell* defect, and a redesign that only repaints the board will still fail it.

---

## 2. axe results on `/dashboard`

`axe-core` via `@axe-core/playwright`, tags `wcag2a, wcag2aa, wcag21a, wcag21aa,
best-practice`. **0 critical, 3 violation rules, all `serious`.**

### 2.1 1440×900 — 42 passes, 3 violations, 10 nodes

| Impact | Rule | Nodes | Target / detail |
|---|---|---|---|
| serious | `color-contrast` | **7** | `.venue-switcher …text-[11px]` 3.89 · `.club-sidebar-label` ×3 **3.9 (9px)** · `.account-row …text-[11px]` 4.03 · header `.uppercase` 4.24 (10px) · `kbd` 3.96 |
| serious | `definition-list` | **1** | `.club-dashboard-money` — `<dl>` has direct children that are not `<dt>`/`<dd>`: `<div><dt>…<dd>…<span>…</span></div>` (`DashboardPage.tsx:381-409`) |
| serious | `label-content-name-mismatch` | **2** | `.command-trigger` (`aria-label="Open command menu"` but visible text `Jump to… ⌘K`) · `.header-account` (`aria-label="Sign out"` but visible `CL`) — `Layout.tsx:134`, `:139` |

`incomplete` (needs review, not a violation): `color-contrast` **3** nodes —
`text[fill="#F2F7F5"]` (the SVG chart legend label painted as text),
`.club-dashboard-gauge-label > strong` (26px on a gradient-free surface),
`.club-dashboard-gauge-label > span` (`floor energy`, 10px). The two gauge nodes
are *inside* absolutely-positioned overlays that axe cannot resolve against a
background; my own compositing walk (§1.9) resolves them and finds both pass.

### 2.2 390×844 — 43 passes, 3 violations, 6 nodes

| Impact | Rule | Nodes | Target |
|---|---|---|---|
| serious | `color-contrast` | **1** | header `.uppercase` 4.24 |
| serious | `definition-list` | **1** | `.club-dashboard-money` |
| serious | `label-content-name-mismatch` | **4** | `.header-account`, `.mobile-nav-item-active` (`aria-label="Overview"`, visible `Home`), `a[aria-label="Pricing"]` (visible `Price`), `a[aria-label="Earnings"]` (visible `Income`) — `Layout.tsx:167-177` |

`incomplete`: `color-contrast` 2 (gauge nodes).

**Why this matters for the gate:** §7.1 requires *0 critical, 0 serious* on
`/dashboard` **and `/sessions`**. Today `/dashboard` has 3 serious rules / 10
nodes at desktop and `/sessions` **cannot be measured at all** (§3.1 below).
A passing G6 must therefore bring at least `definition-list`,
`label-content-name-mismatch` and the shell contrast failures to zero — none of
which are in §3's defect list, and two of which live in `Layout.tsx`, i.e. in
G4's scope, not G3's.

---

## 3. DATA MAP — every number on the board and where it really comes from

### 3.1 The routing fact that blocks acceptance (§7.1 requires axe on /sessions)

`apps/web-club/src/App.tsx:21-37` — when `IS_DEMO`:

```tsx
<Route element={<Layout />}>
  <Route path="/dashboard" element={<DashboardPage />} />
  <Route path="*" element={<Navigate to="/dashboard" replace />} />
</Route>
```

There is **one** club route and a catch-all. Measured, navigating directly to each
URL on `:5199` and reading `location.href` after load:

| Requested | Final URL | `<h1>` | Page height | Has `.club-dashboard-shell` |
|---|---|---|---|---|
| `/dashboard` | `/dashboard` | Tonight at Club Nebula | 2348 | yes |
| **`/sessions`** | **`/dashboard`** | Tonight at Club Nebula | 2348 | yes |
| **`/djs`** | **`/dashboard`** | Tonight at Club Nebula | 2348 | yes |
| **`/pricing`** | **`/dashboard`** | Tonight at Club Nebula | 2348 | yes |
| **`/earnings`** | **`/dashboard`** | Tonight at Club Nebula | 2348 | yes |
| `/dj` | `/dj` | Request queue | 900 | no (`.dj-app-shell`) |
| `/` | `/dj` | Request queue | 900 | no |

Evidence: `page-sessions.png`, `page-djs.png`, `page-pricing.png`,
`page-earnings.png` — all four are byte-identical renders of the dashboard
(368,878 bytes each, vs 368,921 for `dashboard-1440x900.png`: the 43-byte delta is
the freshness ticker digit).

**This is a P0 acceptance blocker, not a styling issue:**
- §7.1 asks for axe 0 serious on `/dashboard` **and `/sessions`**. In demo mode
  `/sessions` is unreachable. G6 can only satisfy that line by *changing routing*,
  by *measuring the production build with a backend*, or by *amending §7.1*. §3
  of the prompt does not mention it, and neither does 01-contract §8 (which
  correctly notes API metrics are `unavailable` but not that four pages 404 into
  the board).
- The nav in `Layout.tsx:19-25` still renders **five** links (`Live room`, `DJs`,
  `Pricing`, `Earnings`) that all land on `/dashboard`. On the live-edit server a
  user clicking "Earnings" gets the Overview screen with the Overview item
  *still highlighted* — visible in `dashboard-1440-first-screen.png` and in
  `page-earnings.png`. This is a **working-looking navigation that does nothing**,
  which §5.3/§7.1 ("no placeholders instead of working elements") forbids.
- `/dj` **is** reachable and renders the real DJ console shell. So the demo build
  is a 2-route app wearing a 5-item navigation, and G4/G8 cannot honestly
  screenshot "the other club pages" from the demo build.

### 3.2 Two data planes on one screen

The board mixes **(A) three live fields** with **(B) a 400-line local-state
module**. Nothing on the rendered screen distinguishes them — there is no
`data-source`, no chip, no banner (all measured 0, §1.1).

### 3.3 Real API numbers (only three)

| # | Rendered on screen | Element | Source chain |
|---|---|---|---|
| A1 | `€12,845.00` — **Club net (settled)** | `.club-dashboard-money-value` (`DashboardPage.tsx:396`) | `DashboardPage.tsx:158` `data?.earnedTotalMinor` ← `api/client.ts:569` `Number(r.earned_total_minor ?? 0)` ← `GET /v1/club/dashboard` ← `panel_routes.rs:184-185` |
| A2 | `137` — **Paid requests** | `DashboardPage.tsx:405` | `DashboardPage.tsx:159` `data?.requestsToday` ← `api/client.ts:568` ← `panel_routes.rs:182-183` |
| A3 | `ACTIVE` + `Session 0192f3b0-1` | `.club-dashboard-shift` (`DashboardPage.tsx:412-415`) | `DashboardPage.tsx:160` `data?.session?.status` ← `api/client.ts:565-567` ← `panel_routes.rs:179-181` |

*(Values above are from the `populated` fixture run required by 01-contract §8;
in the honest demo state the same three render `Unavailable` / `38` / `No open
session` — see §3.6.)*

### 3.4 Mock numbers — every other number on the board

All from `src/dashboard/mock.ts`, held in `useState` by
`src/dashboard/useVenueState.ts:49-65`. **No network, no persistence, gone on
reload.**

| Rendered | Value | mock.ts line | Path to screen |
|---|---|---|---|
| `451 inside of 670` | 451 / 670 | `:22-50` zones summed | `useVenueState.ts:129-143` → `DashboardPage.tsx:247` |
| `· 67% capacity` | 67 | computed `:138` | `DashboardPage.tsx:248` |
| `Guest connections` `212` | 212 | `:145` | `:104` (overwritten by ticker) → `DashboardPage.tsx:260` |
| `Requests tonight` `38` | 38 | `:147` | `DashboardPage.tsx:264` |
| `Tips tonight` `€124.80` | 12480 | `:148` | `DashboardPage.tsx:268` |
| `On deck` `Ava Laurent` | — | `:53` | `:114` → `DashboardPage.tsx:272` |
| `floor energy 88` | 88 | `:146` / `:105` | `DashboardPage.tsx:276,96` |
| chart series (8→12 points) | 96…356 + random | `:130-139`, `:98` | `DashboardPage.tsx:279` |
| `Peak bucket 356` | max(crowd) | — | `DashboardPage.tsx:153-156,281` |
| `Average tip €3.28` | 12480/38 | computed `DashboardPage.tsx:157` | `:389` |
| lineup (4 DJs) | — | `:52-57` | `DashboardPage.tsx:313-334` |
| `Next up: Marco Ruiz · 01:00` | — | `:54` | `useVenueState.ts:337-343` → `:307` |
| zones (3) + `316 / 420 · 75%` etc. | — | `:22-50` | `DashboardPage.tsx:344-371` |
| `Booth 01 · v3` QR | qrVersions=3 | `useVenueState.ts:59-61` | `DashboardPage.tsx:455`; payload built at `:162-164` |
| `4 of 6 table codes active` | 4 | computed `:134` | `DashboardPage.tsx:471` |
| events (3) + `316/420 guests` | — | `:59-96` | `DashboardPage.tsx:488-519` |
| `Most active tables` + `€48.00 · 9 requests` | — | `:98-105` | `useVenueState.ts:332-335` → `DashboardPage.tsx:532-553` |
| `1 tables free · 4 booked or seated` | 1 / 4 | computed `:132-133` | `DashboardPage.tsx:556` |
| `Minimum tip €3.00` | 300 | `:108` | `DashboardPage.tsx:626,676` |
| `2 genres blocked` | 2 | `:110` | `DashboardPage.tsx:676` |
| `Club Nebula`, `Main room`, `Doors opened 22:00` | — | `:19-20`, `:144` | `DashboardPage.tsx:182-185` |
| `Snapshot Ns ago` | 4→30 loop | `useVenueState.ts:57,86` | `DashboardPage.tsx:220` |
| clock `19:28` | real clock | `useVenueState.ts:44-46,85` | `DashboardPage.tsx:191` |
| `left in the set` | computed | `:345-352` (`mock.ts:53` setEnd `01:00`) | `DashboardPage.tsx:299-304` — **never renders**: `end−now ≤ 0` for the whole evening, measured `deckRemaining: false` |

**Available endpoints that the board does not call** (targets for §5.2):

| Function | `api/client.ts` line | Used by the board? |
|---|---|---|
| `getDjs()` | `:688` | **no** — lineup is `INITIAL_DJS` |
| `getPricing()` | `:651` | **no** — min tip is `INITIAL_RULES` |
| `generateSessionQr()` | `:577` | **no** — QR payload is a hand-built `dropit.app/{venue}?table=…` string (`DashboardPage.tsx:162-164`) |
| `getProfile()` | `:600` | **no** — venue name is `mock.ts:19` |
| `getEarnings(period)` | `:743` | **no** — this file is only imported by `EarningsPage.tsx:11` |

### 3.5 F1 — CONFIRMED, and it is worse than "last 24 h"

Label rendered: **`Club net (settled)`** with sub-label **`last 24 h, from
earnings API`** (`DashboardPage.tsx:393`, `:399`).

```sql
-- panel_routes.rs:184-185, inside club_dashboard (GET /v1/club/dashboard)
(SELECT COALESCE(sum(sp.amount_minor - sp.reversed_minor), 0)::bigint FROM split sp
 WHERE sp.payee_type = 'club' AND sp.payee_id = $1)
```

| Claim in the label | Reality | Verdict |
|---|---|---|
| "last 24 h" | The SQL has **no time filter at all** — no `JOIN payment`, no `captured_at`, no interval. It sums **every row ever** created for this club. | **FALSE** |
| "from earnings API" | The number is read from `getClubDashboard()` → `GET /v1/club/dashboard` (`client.ts:563-571`); that endpoint is `panel_routes.rs:171`. `GET /v1/club/earnings` (`panel_routes.rs:373`) **does** have a period filter, and the board **never calls it**. | **FALSE** |
| "settled" | `split` has `status` ∈ `pending / paid_out / partially_reversed / reversed` (`0005_payments.sql:58`) and the query filters **none** of them; it also never checks `payment.status = 'captured'`. Uncaptured and reversed splits are included. | **FALSE (misleading)** |
| "net" | It is `amount − reversed + …` = a pending/gross mix, not a settlement. | **misleading** |

**F1 confirmed on all three clauses.** The correct caption for this exact number
is **"all time, all split statuses, gross of reversals"** — and per §5.1.3 the
fix is to caption by fact (no API-contract change). Note the *period* label is
defensible nowhere: `earnings?period=today` maps to `interval '1 day'` =
**rolling 24 h**, not a calendar day (`panel_routes.rs:384-388`), so even the
endpoint the label names would not mean what "last 24 h" reads like at 03:00.

### 3.6 F2 — CONFIRMED

Label rendered: **`Paid requests`** (`DashboardPage.tsx:402`), sub-label
`last 24 h` when the API answered, `demo counter` when it did not (`:407`).

```sql
-- panel_routes.rs:182-183
(SELECT count(*) FROM request r JOIN session ss ON ss.id = r.session_id
 WHERE ss.club_id = $1 AND r.created_at > now() - interval '24 hours')
```

| Claim | Reality | Verdict |
|---|---|---|
| "Paid" | No join to `payment`, no `payment.status = 'captured'`. `request.price_minor` defaults to **0** (`0004_requests.sql:46`) and free requests (`F4: без денег = 0`) are counted identically to paid ones. | **FALSE** — the number is *unfiltered request volume* |
| "last 24 h" | `created_at > now() - interval '24 hours'` — a **rolling** 24 h window, correct against the sub-label but not "tonight". | **TRUE for the sub-label**, misleading against the panel's "Money tonight" title |
| Fallback | When the API fails (demo), it silently renders `pulse.requestCount` = **38** from `mock.ts:147` with the caption `demo counter` — a *mock number in the same visual slot* as the API number. | **additional defect**: the caption says "demo counter" but `Paid requests 38` reads as a count of paid requests. §5.1 requires a `preview` chip, not a 11px footnote. |

**F2 confirmed.** Even after captioning, the *panel* title "Money tonight"
contradicts a 24-hour rolling window; the §7.1 line "«Club net» и «Paid
requests» подписаны по фактическому периоду" cannot be satisfied by editing the
sub-label alone.

### 3.7 Cross-check of the schema (why "Paid" cannot be derived from `request`)

- `request` (`0004_requests.sql:40-52`) has **no payment column** — only
  `price_minor` (a snapshot price, default 0) and `status`
  (`pending/accepted/rejected/played/expired`, i.e. DJ workflow, not money).
- `payment` (`0005_payments.sql:27-42`) holds `status` and `captured_at`.
- `split` (`:51-59`) holds `payee_type/payee_id/amount_minor/reversed_minor/status`.

A correct "paid requests" count is `count(DISTINCT request.id)` over
`payment.status='captured'` — exactly the shape `club_earnings` already uses at
`panel_routes.rs:395-397`. So the board could be honest **without any API
change** by *switching endpoint* for this one metric, which §5.2 allows.

---

## 4. Spec-vs-reality conflicts found beyond §3

New findings, ordered by how much they endanger the gate. §3 does not mention
any of these.

| ID | Finding | Evidence | Why it matters |
|---|---|---|---|
| **N1** | **Demo mode has exactly one club route.** `/sessions`, `/djs`, `/pricing`, `/earnings` all land on `/dashboard`; `/` lands on `/dj`. | `App.tsx:21-37`; measured final URLs in §3.1; 4 screenshots | Breaks §7.1 axe-on-`/sessions`, breaks G4 "screenshot each page", and makes the 5-item rail a lie. **Highest-impact unlisted finding.** |
| **N2** | **The 500 is a *design* element of the baseline, and it gets the whole first screen wrong.** The error banner occupies y=310–464 (154px) and pushes the first board panel to **y=480**; only "Live venue pulse" and "On the decks" are above the fold, and **neither is fully visible** (both 526px tall, bottom at y=1006 vs fold 900). | measured `errorBanner.top/height`; `firstScreen` both `fullyVisible:false`; `dashboard-1440-first-screen.png` | §4.2 requires "4–6 tiles in the first screen at 1440×900". The baseline has **0 fully visible tiles**. The budget cannot be met by shrinking tiles alone — the banner + 260px header block must shrink too. Also: `ErrorState` is rendered *inside* the flow with `rounded-md` (6px) while everything around it is 28px, and the copy is a raw `Internal Server Error http_500` (developer text, `ErrorState.tsx:16-22`) — §4.9 requires "честный текст, что произошло и что делать". |
| **N3** | **`Layout.tsx` fully re-themes the board's neighbours**, and it is the source of *all 7* contrast failures and *4 of 6* uppercase instances. The board sits on `#09090b`-family surfaces while `body` is `#0a0a0a` and `.club-dashboard-shell` itself is **transparent** (`background: transparent` wins at `:625` over `:437`). | `index.css:32,192`; `Layout.tsx:73,79,123,131`; measured `shellBg: rgba(0,0,0,0)` | G2's token layer cannot be "board-only": 3 of the 3 axe rule families at desktop involve the rail/header. And because the shell is transparent, any board re-surface must also define what is behind it. |
| **N4** | **The CSS contains two competing `.club-dashboard-shell` blocks** (`:422-440` defines the tokens + `background: var(--club-dashboard-canvas)`; `:625-632` re-declares `width/max-width/margin/padding` and `background: transparent`). Panel radius is declared **twice** (`:458` `28px`, `:741` `28px` again for `-panel-full`), and mobile radius is overridden at `:602` (`24px`) yet `-panel-full` keeps `28px`. | measured: rules panel = 28px while the other 7 = 24px at 390 | This is the mechanism behind the mobile 24/28 split (§1.3). G2 must delete, not override — otherwise the "≤4 radii" assert will pass on desktop and fail on mobile. |
| **N5** | **The board's "design system inside a design system" is not just variables — it declares its own font that does not exist and inherits a third font from a CDN.** 324 of 343 shell elements compute `font-family: Geist`; 0 `Geist` faces exist; 1 of 28 Inter faces ever loads; system UI paints. | §1.5 canvas-width proof | §7.1's "шрифты и иконки локальные" is currently satisfied only in the sense that *the wrong font is local*. G2 must self-host and **actually apply** one family; deleting `Geist` from CSS without adding a `@font-face` would leave the same invisible fallback. |
| **N6** | **`/dj` shares the same CSS files and the same green/radius vocabulary**, and is reachable in demo. `DjConsolePage.css:2` also hardcodes `Geist`; `operations.css:196` aliases `--dj-green: var(--ops-green)`. The rail's "DJ console" link is the only nav item that works. | `page-dj.png`; `Layout.tsx:107-118` | §6.1 gives G4 `DjSidebar.css` but **not** `DjConsolePage.css` / `DjConsoleView.css`, though §2 lists all four stylesheets as the club panel's. Any token migration that touches `--ops-*` will silently repaint the DJ console. Needs a scope decision from the Lead — §3 does not raise it. |
| **N7** | **The command palette has no focus trap.** Measured: `Tab` from the 6th `.command-result` goes to `.club-dashboard-action` **outside** the dialog (4 of 10 tabs escape), and `Escape` drops focus onto `BUTTON.club-dashboard-mode` in the board, not the trigger. Arrow keys do nothing (0 result movement on `ArrowDown`). | `measure3` `palette.tabSeq`, `escaped:4`; `Layout.tsx:142-161` (no `useFocusTrap`, unlike the dashboard dialogs which *do* use it at `DashboardPage.tsx:138-139`) | §3.3/A3 says "нет focus-trap и возврата фокуса" — **confirmed**, and now with the exact escape point. Note the asymmetry: the same repo already has a working `useFocusTrap` (`dashboard/useFocusTrap.ts`) used by the board's modals, just not here. |
| **N8** | **The event modal does not lock background scrolling** and has no `aria-describedby`. Measured: `body`/`html` `overflow: visible`; `scrollY` goes 0 → **900** after one wheel with the dialog open; modal is `radius 32px`, `border 1px`, `box-shadow none`. | `measure3.modal`; `DashboardPage.tsx:711`; `operations.css:559-571` (`border-radius:32px` at `:565`) | §3.3/A2 says "фон не блокируется от прокрутки; у диалогов нет `aria-describedby`" — **confirmed with the 900px number**. Add: the modal is 32px, i.e. **above** the §4.3 ceiling and bigger than the panels it covers. |
| **N9** | **Toasts cannot be paused and steal focus.** TTL `3600ms` (`useVenueState.ts:35`); measured disappearance **3410ms while hovered** and **3486ms while the dismiss button had focus**; after removal focus falls to `<body>`. Double-announce: container has `aria-live="polite"` **and** each toast has `role="status"` (3 live regions, 2 status roles on the page). No undo. Dismiss target **18×28**. | `measure3.toast`; `DashboardPage.tsx:683-700`; `operations.css:1139` | §3.3/F17 said 3.6s / no pause / double announce / ~18px — **all confirmed**; the measured 3410/3486 ms and the focus loss to `<body>` are new evidence. |
| **N10** | **Two dead controls.** `venue.toggleTableQr()` and `venue.setTableStatus()` are implemented (`useVenueState.ts:234-251`) and exported, but **no element on the board calls them** (grep of `DashboardPage.tsx`: only `openTable` at `:547` and `saveTable` at `:848`). Table QR state is only mutable through the modal's `<select>`. | no caller in `DashboardPage.tsx` (only `openTable` at `:547`, `saveTable` at `:848`) | Not in §3. Dead code that §6.1 lets G5/G3 delete — but it also means the "table QR" control the board *looks* like it has (the `4 of 6 table codes active` footnote) has no per-table affordance at all. |
| **N11** | **`getClubDashboard` is called twice per load** and both 500s are logged to console. | measured `dashboardCalls: 2` at t=0,0ms; `main.tsx:15` `<StrictMode>` | §7.2 #14 requires "no `console.error` on load". The baseline **cannot** pass #14 in demo mode: the 500 itself is logged by the browser (`Failed to load resource … 500`) regardless of app code. G6 must either allowlist the demo 500 or measure #14 in the populated run — a decision §7.2 does not make. **Conflict between 01-contract §8 ("unavailable is correct, not a defect") and §7.2 #14.** |
| **N12** | **Loading state is nearly invisible.** With the API at 0ms failure the skeleton flashes for <120ms; `loading.png` captured at 120ms shows **no** `.club-dashboard-skeleton` element. The only loading affordance for the two API metrics is a 76×20px shimmer inline (`operations.css:978`), and the rest of the board renders confident mock numbers meanwhile. | `measure2.loadingShot.hasSkeleton:false`; `loading.png`; `DashboardPage.tsx:395,404` | §4.9 asks for `Skeleton` on structural blocks. Today a slow/failed request looks **identical** to a successful one except for two small placeholders → this *is* the F18 mechanism, and it is why "the board never looks broken". |
| **N13** | **The board is full-bleed but the shell is 1128px wide at every desktop size, and never reaches its own 1360px cap.** At 1440 the content column is 1128px (`main` 1208 − 2×40 padding); at 1920 the shell is **1200px** (capped by `index.css:334` `max-width:1280px` on `.app-shell main > div` minus padding), so the board *stops growing* 120px short of its declared 1360. | measured `widthChain` at 1440 and 1920; `index.css:334`; `operations.css:156,627` | §3.5/E4 names the three caps; this adds the **measured consequence**: the 1360px cap is unreachable dead CSS, and the effective cap is 1280 on the *parent*, so the panel spans 8/4/4/4 columns are computed from 1128px, not 1360. |

Additional smaller conflicts:

| ID | Finding | Evidence |
|---|---|---|
| N14 | **`.club-workspace-content` (the shell class every other page uses) does not exist in the dashboard DOM** — measured `found:false`. Only `SessionsPage`/`PricingPage`/`EarningsPage`/`DjsPage` use it (`operations.css:156`). So 01-contract §5 ("`club-dashboard-shell` inherits from `.club-workspace-content`") is **not true today**; it would have to be *created*. | `widthChain[0].found=false` |
| N15 | **`.club-dashboard-eyebrow` is used twice in two different roles**: page eyebrow (`DashboardPage.tsx:182`) and modal eyebrow (`:714`, `:832`). §4.6 allows exactly one uppercase element per screen; the modal adds a second instance of the same one. | `DashboardPage.tsx:714,832`; `operations.css:642` |
| N16 | **`StatusBadge` has no tone for `created`** — falls back to `status-badge-muted` (`StatusBadge.tsx:23`), i.e. "room not opened yet" renders identically to "closed" and to "invited"/"played"/"offline". Measured only indirectly here (no session in demo) but the map is static. | `StatusBadge.tsx:1-19` |
| N17 | **The `definition-list` axe rule stems from `<span>` inside `<dl><div>`** — a 5-second fix but it *is* a `serious` violation and §7.1 gates on serious = 0. | `DashboardPage.tsx:385,390,399,407` |
| N18 | **`prefers-reduced-motion` already passes**: `document.getAnimations()` → **0** with `reducedMotion:'reduce'`. Counts as a *pass* in the baseline — §7.1's reduced-motion line should not be recorded as "before: failing", and G6 should not claim credit for it. | `measure2.reducedMotion`; `operations.css:609-617` |
| N19 | **`EarningsPage` is not a "today" page**: it calls `getEarnings('today')`, which the backend maps to `interval '1 day'` = rolling 24 h, then prints the word `today` as the period. Same class of label lie as F1/F2, on a page §3.3/F15 mentions only for the commission wording. | `EarningsPage.tsx:11,31`; `panel_routes.rs:384-388` |
| N20 | **`/dj` console headline height is 900px at 1440** while the board is 2348px — the two halves of the same product feel like different applications, which is exactly what §4.1 #4 ("одинаковые элементы выглядят одинаково на всех страницах") forbids. | `page-dj.png` vs `dashboard-1440x900.png` |

---

## 5. Defect table

Severity: **P0** = blocks §7.1 as written · **P1** = violates §4/§5 and will be
found by the red team · **P2** = quality/hygiene, fix if the file is opened anyway.
Rule column cites §4/§5 (prompt) and, where relevant, 01-contract.

### P0 — blocks the acceptance gate

| ID | P | Symptom (measured) | file:line | Rule violated |
|---|---|---|---|---|
| **D1** | P0 | Board height **2348 / 2618 / 4952** px vs ≤1700 / ≤2000 / ≤3000 | whole board; main driver `operations.css:464-465` (`grid-row: span 2`) + 8 panels | §4.2 height budget, §7.1, §7.4 |
| **D2** | P0 | **8** distinct radii (9 mobile); panels 28px, modals 32px; mobile mixes 24px and 28px | `operations.css:458`, `:565`, `:602`, `:704`, `:741`, `:985`, `:1188` | §4.3, §7.1, §7.2 #1, §7.4 (≤4) |
| **D3** | P0 | **81** border-bearing elements, **65** nested; every panel boxed | `operations.css:457` (panel `border:1px solid`) + boxed controls/chips at `:489`, `:535`, `:564`, `:676`, `:696`, `:731`, `:748`, `:880`, `:895`, `:916`, `:984`, `:1053`, `:1099` + row hairlines at `:843`, `:865`, `:871`, `:941`, `:958`, `:975`, `:1006`, `:1034`, `:1088` | §4.4 "панель без рамки", §7.1, §7.4 (≤20 / 0 nested) |
| **D4** | P0 | **F1**: "last 24 h, from earnings API" over an **all-time, unfiltered `split` sum** from `/club/dashboard` | `DashboardPage.tsx:399`; `panel_routes.rs:184-185`; `client.ts:563-571` | §5.1.3, §5.2, §7.1 |
| **D5** | P0 | **F2**: "Paid requests" is **all requests in 24 h**, no payment filter | `DashboardPage.tsx:402`; `panel_routes.rs:182-183` | §5.1.7, §7.1 |
| **D6** | P0 | **0** `[data-surface]`/`[data-control]`/`[data-metric]`/`[data-source]`/`[data-preview-banner]` → §7.2 checks 1, 2, 11 cannot run at all | whole board; contract defined in 01-contract §2 | §7.2 #1,#2,#11, §5.1.1 |
| **D7** | P0 | **13 sizes** on board, **58** text nodes <12px, **13** <11px; document 15 sizes / 18 <11px | `operations.css:474-476`, `:479`, `:585-586`, `:642`, `:729-737`, `:795`, `:825`, `:888`, `:955`, `:960`, `:978`, `:1020`, `:1039`, `:1175` | §4.6 scale 28/20/15/13/12/11, §7.2 #9, §7.4 (≤6) |
| **D8** | P0 | **40 of 41** interactive targets <44px; 27 <36px on desktop; `Pause QR` **52×17**; toast dismiss **18×28** | `operations.css:929` (link-action, no min-height), `:914` (mini-action 32px), `:701` (mode 32px), `:1051` (genre 30px), `:487`+`:507` (icon-action 42px), `:1139` (toast dismiss) | §5.3 "≥44 mobile / ≥36 desktop", §7.2 #6, §7.4 |
| **D9** | P0 | **All mock numbers presented as telemetry**: zones/tables/DJs/crowd/tips/policy, no chip, no banner | `mock.ts:19-158`; `useVenueState.ts:49-65,93-111`; `DashboardPage.tsx` panels 1,3,5,7,8 | §5.1.1–§5.1.2, §7.1 |
| **D10** | P0 | **`Math.random()` drives crowd series, "guest connections" and "floor energy"** while labelled "Guest connections, 15 min buckets"; drift proven (±5 within 16s, peak 356→361) | `useVenueState.ts:98`; `DashboardPage.tsx:49,68,279` | §5.1.4, §7.1 ("ни одного `Math.random()`") |
| **D11** | P0 | **`/sessions` unreachable in demo** → §7.1's axe-on-`/sessions` is unmeasurable; 4 nav items dead-end on `/dashboard` | `App.tsx:21-37`; `Layout.tsx:19-25` | §7.1 axe line, §5.3, §7.2 #15 (per-page screenshots) |
| **D12** | P0 | axe **3 serious rules / 10 nodes** at 1440 (`color-contrast` 7, `definition-list` 1, `label-content-name-mismatch` 2); 6 nodes at 390. All 7 contrast failures are **shell/rail**, not board. | `Layout.tsx:73,79,123,131,134,139,167-177`; `index.css:192`; `DashboardPage.tsx:385-407` | §7.1 "axe 0 critical 0 serious", §7.4 |
| **D13** | P0 | **Font fiction**: 324/343 shell elements compute `Geist`; **0** Geist faces exist; Inter downloaded but **1 of 28 faces loaded** and never applied; system UI paints. Icon set entirely from `code.iconify.design` + `api.iconify.design`. | `index.css:32`; `operations.css:53,438`; `index.html:11-14`; `Icon.tsx:21-27` | §4.6, §5.4 "шрифты/иконки локально", §7.1, §7.4 (4→0 CDN) |
| **D14** | P0 | **Stretched panels**: 169/112/76/56px of unjustified emptiness in Guest QR / Zones / Scheduler / On the decks (197/141/116/94 to the last text line) | `operations.css:464-465` (`grid-row: span 2`); panels 2,3,5,6 | §4.2 "≤24px от последнего элемента" |
| **D15** | P0 | **Two greens** on the board: `#10B981` ×7 (CTA bg, meters), `#22C55E` ×25 (dot, chart, gauge, toggles, chips) + `#7ee2a8` ×11 (chip text). Brand green `#00FF87` appears on the **rail** only. Document total **4 greens** (`+#70FFB9`), 5 at mobile (`+#6DF0B2`). | `operations.css:432-433` (token defs), `:482` (`--green-bright` text), `:496-497` (CTA), `:583-584` (chart), `:727` (live dot), `:749` (state chip), `:890-900` + `:1095` (chips/genres), `:1119`/`:1133` (checked toggle, focus ring); `index.css:17`; `tailwind.config.js:12` | §4.5, §7.1, §7.4 |
| **D16** | P0 | **6 uppercase+tracked labels** on one screen (2 on board + 4 in shell) vs ≤1 | `operations.css:642` (eyebrow), `:729-737` (demo tag); `index.css:192` (rail labels); `Layout.tsx:131` (header kicker) | §4.6, §7.2 #10 |
| **D17** | P0 | **10+ false success toasts** for unsaved actions, incl. `Music policy saved for tonight` for a function that does not exist; no undo; no pause | `useVenueState.ts:319` (+ `:162,178,213,231,247,259,272,286,324`) | §5.1.6, §5.3, §7.1, §7.4 (12→0) |

### P1 — will be found by the red team

| ID | P | Symptom | file:line | Rule |
|---|---|---|---|---|
| D18 | P1 | "Snapshot Ns ago" is a **ticker** (4→30 loop, resets every 30s) with `aria-live="polite"` updated every 1s — announced ~60×/min to screen readers | `useVenueState.ts:57,83-89`; `DashboardPage.tsx:218-222` | §5.1.5, §7.1 |
| D19 | P1 | "left in the set" is **dead code**: `minutesOf(end) − minutesOf(clock)` is negative all evening, so it never renders (measured: absent) | `useVenueState.ts:345-352`; `mock.ts:53` | §5.1.10 |
| D20 | P1 | Table arithmetic does not close: `held` is excluded from both `available` and `occupied` → footnote prints **"1 tables free · 4 booked or seated"** for 6 tables (also ungrammatical "1 tables") | `useVenueState.ts:132-133`; `DashboardPage.tsx:556` | §5.1.10, §7.1 |
| D21 | P1 | Command palette: **4 of 10 Tab presses escape** the dialog; `Escape` returns focus to a board button, not the trigger; arrows do nothing; yet the **same repo ships a working `useFocusTrap`** | `Layout.tsx:142-161`; compare `DashboardPage.tsx:138-139` | §5.3 keyboard, §7.2 #7 |
| D22 | P1 | Modal: **background scrolls** (0→900px on one wheel), no `aria-describedby`, radius 32px | `DashboardPage.tsx:711`; `operations.css:559-571` (`border-radius:32px` at `:565`) | §5.3, §4.3 |
| D23 | P1 | Toast: dies at **3410ms hovered / 3486ms focused**, dismiss 18×28, **double** announcement (`aria-live` container + `role="status"` item), no undo, focus lost to `<body>` | `useVenueState.ts:35,68-75`; `DashboardPage.tsx:683-700`; `operations.css:1139` | §5.3, §4.9 `Toast`, §7.1 |
| D24 | P1 | Error state: raw `Internal Server Error`/`http_500` shown to staff; past-success data is not demoted, board keeps printing mock numbers | `ErrorState.tsx:16-22`; `DashboardPage.tsx:225-233`; `useAsyncData.ts:49-60` | §4.9 `ErrorState`, §5.1.1, §7.1 |
| D25 | P1 | QR on the board is a hand-built `dropit.app/room/…` URL, not a session token; rotation silently un-pauses the table | `DashboardPage.tsx:162-164`; `useVenueState.ts:279-287` | §5.1.8, §7.1 |
| D26 | P1 | Hidden side effects: `Take over` sets venue mode to `peak_hours`; `Go live` sets mode to `doors_open` and demotes the other live event | `useVenueState.ts:161,174,177` | §5.1.9, §7.1 |
| D27 | P1 | `Save policy` is a toast-only no-op while every rule already mutates on click | `useVenueState.ts:318-320`; `DashboardPage.tsx:566` | §5.1.6, §5.3 |
| D28 | P1 | "Money tonight" panel titles a **rolling 24 h** number; `EarningsPage` prints `today` for the same rolling window | `panel_routes.rs:384-388`; `EarningsPage.tsx:11,31` | §5.1.3, §4.1 #7 |
| D29 | P1 | Venue identity hardcoded (`Club Nebula`, `Main room`, `Doors opened 22:00`) while `getProfile()` exists | `mock.ts:19-20,144`; `DashboardPage.tsx:182-185`; `client.ts:600` | §5.2, §7.1 |
| D30 | P1 | Lineup from `INITIAL_DJS` while `getDjs()` exists; `Take over` has a UI affordance and no endpoint | `mock.ts:52-57`; `DashboardPage.tsx:313-334`; `client.ts:688` | §5.2, §7.1 |
| D31 | P1 | Min tip / rules from mock while `getPricing()` exists; rules belong on `/pricing` per §4.2 | `mock.ts:107-114`; `DashboardPage.tsx:560-680`; `client.ts:651` | §4.2 "убрать форму правил", §5.2 |
| D32 | P1 | `StatusBadge` has no `created` tone → "not open yet" reads like "closed" | `StatusBadge.tsx:1-19,23` | §5.3 "статус не только цветом" |
| D33 | P1 | `dl` structure invalid (`<span>` inside `<dl><div>`) — a serious axe violation | `DashboardPage.tsx:385,390,399,407` | §7.1 axe |
| D34 | P1 | Two dead controls exported but never wired (`toggleTableQr`, `setTableStatus`) | `useVenueState.ts:234-251`; 0 callers in `DashboardPage.tsx` | §5.4 "no placeholders" |
| D35 | P1 | Design-system-in-a-design-system: duplicate `.club-dashboard-shell` blocks; the board's canvas is **transparent**, so it inherits `body #0a0a0a` rather than its own `#090a0f` token | `operations.css:422-440` vs `:625-632` | §5.4 token layer, §4.4 surfaces |
| D36 | P1 | `!important` **26** total (11 + 15), incl. `font-family: inherit !important` inside the board's own metric name | `operations.css:806` (`font-family: inherit !important`), `:613-616` (reduced-motion block); 11 + 15 total across `operations.css` / `index.css` | §5.4 "`!important` запрещён" |
| D37 | P1 | Every table row's only action is a 42×42 gear that opens a modal whose "Save table" is a local-only mutation with a success toast | `DashboardPage.tsx:544-551,903-910`; `useVenueState.ts:227-232` | §5.1.6, §5.3 |
| D38 | P1 | `Sessions`/`Djs`/`Pricing`/`Earnings` pages exist and are styled, but are **unreachable** in demo — so their own defects (F13–F16) cannot be verified or screenshotted by G6/G7 | `App.tsx:21-37` | §7.1, §7.2 #15 |

### P2 — hygiene, fix when the file is open

| ID | P | Symptom | file:line | Rule |
|---|---|---|---|---|
| D39 | P2 | Three competing width caps (1280 parent / 1320 content / 1360 board); **1360 is unreachable** — shell measures 1128 at 1440 and 1200 at 1920 | `index.css:334`; `operations.css:156,627` | §5.4, §4.2 |
| D40 | P2 | `.club-workspace-content` absent from the dashboard DOM, so the "inherits from" story in 01-contract §5 is aspirational | measured `found:false`; `operations.css:156` | 01-contract §5 |
| D41 | P2 | `font-size: clamp(24px,2.4vw,30px)` title + `24.576px` at 1024 → a **fractional** size in the histogram; responsive type adds an unbounded size family | `operations.css:648-655` (title `clamp(24px,2.4vw,30px)`); `:477-481` (`.club-dashboard-value` `clamp(24px,3vw,34px)`) | §4.6 fixed scale |
| D42 | P2 | `.club-dashboard-eyebrow` reused as modal eyebrow (2 uppercase instances per screen when a modal is open) | `DashboardPage.tsx:714,832` | §4.6 |
| D43 | P2 | `motion` and `@formkit/auto-animate` are dependencies but the board animates with raw CSS `@keyframes` (skeleton shimmer, toast in, modal in) | `operations.css:522,539,546,557,570` | §5.4, §4.8 |
| D44 | P2 | Gradient on the skeleton (`linear-gradient(90deg, … #222630 …)`) — §4 forbids gradients outright | `operations.css:520` (`background: linear-gradient(90deg, #191c24 0%, #222630 48%, …)`) | §4.5 "градиенты запрещены" |
| D45 | P2 | `ErrorState` uses `rounded-md` (6px) inside a 28px world and `btn-secondary` (a `.club-dashboard-notice .btn-secondary { border-radius:10px }` patch at `:1195` proves the mismatch) | `ErrorState.tsx:13,24`; `operations.css:1187-1195` | §4.3 |
| D46 | P2 | `.club-dashboard-title` sets `text-wrap: balance` and the subtitle is capped at `62ch` (both **comply**), but the panel `detail` line is capped at `46ch` only below 640px | `operations.css:655-662`, `:1168` | §4.6 |
| D47 | P2 | `Icon` renders `<span aria-hidden>` wrapping `<iconify-icon>`; 25/25 render but the icon font-dependency means a CDN stall blanks every icon with no `alt`/fallback | `Icon.tsx:21-27` | §5.4 |
| D48 | P2 | `useAsyncData` fires the fetcher twice on mount under `StrictMode` and both errors hit the console | `main.tsx:15`; `useAsyncData.ts:34-68` | §7.2 #14 |

---

## 6. Baseline numbers for the §7.4 "before" column

These are the cells G6 should copy. **Bold = differs from the prompt's §7.4 /
§3.1 text.**

| Metric | Before (measured, `:5199`) | §7.4 threshold |
|---|---|---|
| Board height 1440×900 | **2348** px | ≤1700 |
| Board height 1024×768 | 2618 px | ≤2000 |
| Board height 768×1024 | **3380** px | (not gated) |
| Board height 390×844 | 4952 px | ≤3000 |
| Board height 320×844 | **5093** px | (not gated) |
| Panels on Overview | 8 (incl. rules form) | ≤7 |
| Distinct radii (per element) | **8** desktop / **9** mobile | ≤4 |
| Border-bearing elements | **81** (**65** nested) | ≤20, 0 nested |
| Distinct font sizes (board text) | **13** (10–30px) | ≤6, min 11 |
| Font sizes, whole document | **15** | — |
| Text nodes <11px | **13** board / **18** document | 0 |
| Interactive elements <44×44 | **40 of 41** | 0 on mobile |
| Interactive elements <36×36 (desktop) | **27** | 0 on desktop |
| Contrast min — board shell | **5.61:1 (pass)** | ≥4.5 |
| Contrast failures — whole page | **7 nodes, min 3.90:1** (all in shell/rail) | 0 |
| axe critical / serious (1440) | **0 / 3 rules (10 nodes)** | 0 / 0 |
| axe critical / serious (390) | **0 / 3 rules (6 nodes)** | 0 / 0 |
| False save-labels & success toasts | **2 labels + 10 toasts = 12** | 0 |
| External CDN hosts | **4** | 0 |
| Uppercase+tracked labels (doc) | **6** | ≤1 |
| `[data-surface]` / `[data-metric]` | **0 / 0** | required |
| `prefers-reduced-motion` animations | **0 (already passing)** | 0 |
| Horizontal overflow 320/390/768/1024/1440 | **0 (already passing)** | 0 |
| Fully visible tiles in first screen 1440×900 | **0** | 4–6 |

---

## 7. What this baseline means for the gate (for the Lead)

1. **Two §7.4 rows are already green** — reduced motion and horizontal overflow.
   G6 must not spend a round "fixing" them, and the red team should not report
   them as regressions.
2. **`/sessions` is unmeasurable in demo mode (D11).** Either §7.1's axe line gets
   scoped to `/dashboard` + a production-build check, or routing must change. That
   is a **Lead decision**, not an implementer one, and it is not in §3.
3. **Four of the P0s are outside the board file**: D12 (axe/shell contrast),
   D13 (fonts/icons in `index.html` + `Layout.tsx`), D15/D16 partially (rail
   green + rail eyebrow), D11 (routing). §6.1 gives these to G2/G4, so G3 alone
   cannot close the gate — the task graph in §6.2 assumes G3 is the critical path,
   and it is not.
4. **The "0 serious" axe target needs `Layout.tsx` work** (3 rule families, 7 of
   10 nodes). 01-contract §4 gives Layout to G4 and lets G4 run in parallel with
   G5 — the gate is still G2→G3→G6 sequential, so G4's axe work must be pulled
   earlier or G6 will fail on a shell nobody owns.
5. **`populated` baselines are included** (`populated-1440x900.png`,
   `populated-1024x768.png`, `populated-390x844.png` + heights 2178 / 2448 / 4762)
   exactly as 01-contract §8 asks, so the "after" populated measurement has a
   like-for-like "before". Use those numbers for the height row if the Verifier
   follows §8 (the banner-free height is **2178**, 170px shorter than the honest
   demo height of 2348).

---

## 8. Artefacts produced

`output/club-redesign/before/`

| File | Contents |
|---|---|
| `baseline.json` | **machine-readable** — every table above, per viewport |
| `baseline-lead-pass1.json` | the Lead's first pass, preserved untouched for audit |
| `dashboard-1440x900.png` | full-page 1440×900 |
| `dashboard-1024x768.png` | full-page 1024×768 |
| `dashboard-390x844.png` | full-page 390×844 |
| `dashboard-320x844.png` | full-page 320×844 (overflow check) |
| `dashboard-768x1024.png` | full-page 768×1024 (overflow check) |
| `dashboard-1440-first-screen.png` | viewport-only 1440×900 — the fold argument |
| `dashboard-390-first-screen.png` | viewport-only 390×844 |
| `error-state.png` | the 500 banner, element-cropped |
| `error-state-fullpage-1440.png` | the whole board in its honest error state |
| `command-palette.png` | ⌘K open (6 results) |
| `event-modal.png` | Create-event dialog, 32px radius |
| `toast.png` | toast after `Peak hours` |
| `loading.png` | 120ms after navigation — no skeleton present |
| `reduced-motion.png` | `prefers-reduced-motion: reduce`, 0 animations |
| `page-sessions.png` · `page-djs.png` · `page-pricing.png` · `page-earnings.png` | **all four land on `/dashboard`** |
| `page-dj.png` · `page-root.png` | `/dj` DJ console (reachable); `/` → `/dj` |
| `populated-1440x900.png` · `populated-1024x768.png` · `populated-390x844.png` | route-fixture run required by 01-contract §8 |
