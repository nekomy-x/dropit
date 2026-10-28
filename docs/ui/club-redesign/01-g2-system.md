# G1/G2 — Design system shipped: tokens, primitives, shell, local fonts + icons

> Owner: System owner (G1/G2). Scope: `apps/web-club/src/styles/**`,
> `src/components/club/**` (primitives only), `Icon.tsx`, `Layout.tsx`,
> `src/index.css`, `src/operations.css`, `index.html`, `tailwind.config.js`,
> `public/fonts/**`.
> Authority: `01-contract.md` §1–§3, §9 + `club-panel-redesign-prompt.md` §4/§5.4.
> This note records what was actually shipped, the measured evidence, and every
> deviation with its reason. The **finisher cleanup pass** (dead CSS, orphaned
> mock modules, 1024px honest height, final evidence) updated §0, §2, §4 (D3,
> D7, D9), §6, §7 and §8 on top of the G2 report.

## 0. Gate status (final — after the finisher cleanup pass)

Measured by the finisher on 2026-10-26, demo dev server `http://localhost:5173`,
Chromium via Playwright, `@axe-core/playwright` 4.14.0. Harnesses:
`output/club-redesign/cleanup/_verify.mjs` (axe + computed contrast + radii +
overflow + hosts), `_final.mjs` (heights honest/populated, fonts, borders),
`_capture.mjs` (`/dj` + `/connect` screenshots). Raw JSON:
`cleanup/verify.json`, `cleanup/final-measure.json`.

| Gate | Result |
|---|---|
| `npm run typecheck` | **exit 0** |
| `npm run build` | **exit 0** |
| `npm run build:demo` | **exit 0** |
| axe `/dashboard` 1440 and 390 | **0 critical / 0 serious / 0 moderate / 0 minor** (1 incomplete = `color-contrast`; axe cannot compute it, see the computed pass below) |
| axe `/sessions` 1440 and 390 | **0 critical / 0 serious / 0 moderate / 0 minor** |
| axe `/dashboard`, populated via route fixtures | **0 critical / 0 serious / 0 moderate / 0 minor** at 1440 and 390 (`cleanup/populated-axe.log`) |
| computed contrast minimum, `/dashboard` honest | **5.39:1** at 1440 · **5.11:1** at 390 — 0 text nodes below AA |
| computed contrast minimum, `/dashboard` populated | **5.61:1** at 1440 · **5.11:1** at 390 — 0 below AA |
| computed contrast minimum, `/sessions` | **5.39:1** at 1440 · **5.11:1** at 390 — 0 below AA |
| rendered radius set `/dashboard` | exactly **{10px, 12px, 16px, 999px}** at 1440, 1024 and 390 (pseudo-elements included) |
| board height, honest (no backend) | **1500 / 1984 / 2847** px at 1440 / 1024 / 390 (budget 1700 / 2000 / 3000) |
| board height, populated (route fixtures) | **1392 / 1864 / 2751** px at the same widths |
| external hosts on load | **0** |
| horizontal overflow 320 / 390 / 1440 | **0 / 0 / 0** |
| interactive targets below 44px @390 / below 36px @1440 | **0 / 0** (`_verify.mjs` prints the count, min size and the list) |

The remaining axe node on `/dj` (7 serious, unchanged from baseline) belongs to
the DJ console, which is explicitly out of scope for this redesign.

## 1. Token table as shipped — `src/styles/club-tokens.css`

```css
:root {
  color-scheme: dark;

  /* radii: exactly four values for the whole panel */
  --r-surface: 16px;   /* panels, dialogs, large containers */
  --r-tile: 12px;      /* nested tiles, input groups, QR frame */
  --r-control: 10px;   /* buttons, fields, segment track, icon buttons */
  --r-chip: 999px;     /* Chip and the Segmented runner — nothing else */

  /* surfaces */
  --c-canvas: #090A0F;
  --c-surface: #11131A;
  --c-raised: #171A22;
  --c-inset: #0D0F15;
  --c-hairline: rgba(235, 240, 250, .08);
  --c-hairline-row: rgba(235, 240, 250, .06);
  --c-scrim: rgba(0, 0, 0, .72);

  /* colour: one green */
  --c-accent: #00FF87;
  --c-accent-hover: #00D873;
  --c-accent-tint: rgba(0, 255, 135, .08);
  --c-accent-tint-strong: rgba(0, 255, 135, .14);
  --c-ink-on-accent: #06130C;

  --c-text: #F1F4F7;
  --c-text-2: #B0B8C3;
  --c-text-muted: #858E9C;      /* only at >= 12px */
  --c-text-muted-sm: #9AA3B0;   /* 11–12px */

  --c-warn: #FFC65B;
  --c-warn-tint: rgba(255, 198, 91, .10);
  --c-error: #FF6B6B;
  --c-error-tint: rgba(255, 107, 107, .10);

  /* type: six sizes, nothing below 11px */
  --font-sans: 'Inter', system-ui, -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
  --font-mono: ui-monospace, SFMono-Regular, Menlo, monospace;
  --fs-28: 28px;
  --fs-20: 20px;
  --fs-15: 15px;
  --fs-13: 13px;
  --fs-12: 12px;
  --fs-11: 11px;
  --lh-tight: 1.2;
  --lh-text: 1.5;

  /* motion */
  --dur-fast: 120ms;
  --dur-base: 180ms;
  --dur-slow: 260ms;
  --ease-out: cubic-bezier(.22, 1, .36, 1);
  --ease-std: cubic-bezier(.4, 0, .2, 1);

  --abstract-opacity: 1;

  /* supporting (keeps literals out of the stylesheets) */
  --w-content: 1280px;
  --sp-1: 4px;  --sp-2: 8px;  --sp-3: 12px; --sp-4: 16px;
  --sp-5: 20px; --sp-6: 1.5rem; --sp-8: 2rem;
  --pad-panel: 20px; --pad-panel-lg: 1.5rem;
  --hit-desktop: 36px; --hit-mobile: 44px;
}
```

`@media (prefers-reduced-motion: reduce)` zeroes `--dur-*` and
`--abstract-opacity` at the `:root` level, so no component has to handle it.

**Legacy aliases were removed, not duplicated.** The old `--ink/--surface/--accent/--ok/--bad`
block in `index.css` is gone; `--ops-*` stays (the DJ console reads it) but
`--ops-green` now resolves to `var(--c-accent)` and `#00e887` is deleted.

## 2. Files touched

| File | Change |
|---|---|
| `src/styles/club-tokens.css` | **new.** §1 token table + local `@font-face` + reduced-motion overrides. |
| `src/components/club/*.tsx` (13 files) | **new.** `Panel, Tile, Metric, SourceChip, PreviewBanner, Chip, Segmented, ListRow, Meter, Toast(+useToasts), Skeleton, EmptyState, ErrorState` — each stamps its own `data-*`. |
| `src/components/club/club-primitives.css` | **new.** All primitive styling; tokens only. |
| `src/components/club/index.ts` | **new.** Barrel + `CLUB_PRIMITIVE_CONTRACT`; imports the primitives CSS. |
| `src/components/Icon.tsx` | Rewritten on `lucide-react` (static map, 48 names). `<Icon name="lucide:…">` API unchanged; unknown names degrade to a dot instead of crashing. Iconify `<script>` and the global JSX declaration removed. |
| `src/components/Layout.tsx` | Tokens; **one** uppercase tracked element; ⌘K palette now has a focus trap, focus restore, Escape, and a background scroll lock; every control ≥36px desktop / ≥44px mobile; aria-label/visible-text mismatches fixed. |
| `index.html` | Google Fonts `<link>`s and the iconify `<script>` removed. No CDN references remain. |
| `src/index.css` | `@import` of the token file; `:root` legacy palette replaced by tokens; Geist → Inter; club shell (rail/header/nav/mobile bar) rebuilt on tokens; global `:focus-visible` ring; 36/44px hit-target rules; club `@layer components` (`.btn-*`, `.field-*`, `.card`, headings) moved to token values. DJ console + Connect sections keep their own local variables and are untouched apart from shared-token re-points. |
| `src/operations.css` | `--ops-green` → accent; every `rgba(0,232,135,…)` / `#73efb5` / `#6df0b2` / `#36f3a4` replaced; Geist removed; command palette rebuilt on tokens; `.club-workspace-content` width 1320 → `--w-content` (1280); club table headers normalised. **Finisher pass:** the legacy `.club-dashboard-*` bridge was deleted in full once the board stopped using those classes (1166 → 432 lines), together with the dead `.status-badge*` rules and the unused `.qr-generating-mark` spinner. Zero `.club-dashboard-` selectors remain. |
| `tailwind.config.js` | `brand` ramp collapsed to one green (`#00FF87`, `#00D873`, one alpha tint) — `#70FFB9` deleted; `fontFamily.sans` = Inter + system stack (no Geist anywhere). |
| `public/fonts/inter-latin-var.woff2` | **new**, 48 432 bytes, verified `wOF2`/WOFF2, weight range 100–900, latin subset, `font-display: swap`. |
| `src/index.css` | **Finisher pass:** `.club-sidebar-link-active::before` (2px rail blade) lost its `2px` corner radius so the rendered radius set is exactly the four token values; no other index.css change. |
| `src/components/club/club-primitives.css` | **Finisher pass:** `@media (max-width: 1024px)` compresses only the ErrorState / PreviewBanner vertical padding (8px / 4px). Honest 1024×768 height 2008 → **1984**; type sizes and 36/44px hit targets unchanged. |
| `src/pages/DjConsolePage.css` | **Finisher pass, cross-scope fix approved by the Lead:** removed the inert `Geist,` family from `.dj-live-shell` (line 2). The family is not loaded anywhere; pixel diff of `/dj` before/after = **0 px**. |
| `src/dashboard/{useVenueState,mock,types,format}.ts` | **Deleted** (orphaned fake-telemetry modules; see `05-functional.md`). |
| `src/hooks/useAsyncData.ts` | **Finisher pass:** additive `failedAt` / `staleSince` honesty fields; see `05-functional.md` §4. |

## 3. Markup contract — emitted by the primitives, not the pages

| Attribute | Primitive | Value |
|---|---|---|
| `data-surface` | `Panel`, command dialog | `panel` / `dialog` |
| `data-control` | `Chip` (button form), `Segmented` group + options, `Toast` buttons, shell buttons/fields | `button` / `field` / `segment` / `icon` |
| `data-metric` + `data-source` | `Metric` | `live` / `preview` / `unavailable` |
| `data-preview-banner` | `PreviewBanner` | present whenever a board renders preview data |
| `data-tile` | `Tile` | see deviation D1 |
| `data-abstract` | — (not shipped, see D4) | — |

Measured on a probe page that mounted every primitive: `[data-surface]` →
`{16px}` only, `border-width: 0`, `box-shadow: none`; `[data-control]` →
`{10px, 12px, 999px}`; every `[data-metric]` carried one of the three sources;
`[data-preview-banner]` present; unique font sizes on that page = the six steps;
min font 11px; `Segmented` had exactly one tabbable radio per group and arrows
moved selection with wrap-around.

## 4. Deviations from `01-contract.md`, with reasons

**D1 — `Tile` carries `data-tile`, not `data-surface` (§2 vs §1 vs §7.2).**
The contract contradicts itself: §1 fixes the tile radius at **12px**, §2 says
every nested tile carries `data-surface`, and §7.2 check 1 asserts the *set* of
`[data-surface]` radii equals **`{16px}`**. A 12px member makes that assertion
unsatisfiable. Panels and dialogs (the 16px surfaces) keep `data-surface`;
tiles carry `data-tile="tile"`. If the Lead prefers §2 verbatim: change one
attribute in `Tile.tsx` and scope check 1 to `[data-surface="panel"]` — that is
the only one-line reconciliation.

**D2 — resolved.** The one serious node left after the shell work was
`definition-list` on the legacy `.club-dashboard-money` block. It disappeared
when the board agent replaced that markup with `Metric`; all five club routes now
measure 0 critical / 0 serious at 1440 and 390.

**D8 — the wordmark's 23px glyph.** `BrandLogo.tsx` draws its letters as SVG
`<text>` at 23 user units. That is a glyph size, not UI type, but a DOM
font-size audit cannot tell the difference — the baseline auditor counted it as
one of the 13 sizes, so the verifier will too. `index.css` now declares the size
on the type step (`--fs-20`) and compensates with `transform: scale(1.15)` plus a
tracking correction. The mark is geometrically identical: same bounding box,
same ink profile within 1% (antialiasing only), verified against the frozen
baseline at 3×.

**D3 — CLOSED in the finisher pass.** `src/pages/DjConsolePage.css` declared
`font-family: Geist, …`. The Lead explicitly approved a one-line cross-scope fix;
`Geist,` is gone and `grep -rni geist src` returns **0**. Because the family was
never loaded, `/dj` is pixel-identical (see D5).

**D4 — no `data-abstract` layer was shipped.** §4.7 allows one background
abstraction per screen and prefers one that encodes state. Everything I could
build in the *shell* would be decoration (the shell has no state to encode), and
the one legitimate candidate — an occupancy arc — belongs to the board's zones
tile, which is the board agent's file. The `--abstract-opacity` token is shipped
and wired to reduced-motion so a data-bearing layer can be switched off with one
value. **I shipped none, deliberately**, rather than a decorative one.

**D5 — `--ops-*` kept, re-pointed.** The DJ console reads these, so deleting them
would have broken `/dj`. `--ops-green` → `var(--c-accent)`; the muted/secondary
values now resolve through the club ramp. No colour, radius, spacing or layout
property changed on `/dj`: contrast nodes there are **unchanged from baseline
(7 → 7)**, and the accessible name of the DJ account pill was corrected (an axe
`label-content-name-mismatch`, no visual change). A later pixel-diff shows ~6.5%
of pixels moving on `/dj`, but that is **not from this gate** — the DJ console
files (`DjConsolePage.css` etc.) were still being written by another agent at
21:44 while I measured at 21:48. When I measured immediately after my own token
re-point the diff was **0.37%**, all of it text, caused by `Geist` → `Inter` on
`body` which §4.6/§6 require. Re-measure `/dj` after the pages agent finishes.

**D6 — `/connect` unchanged.** It has its own `--linear-*` / `--sprint-*` /
`--drop-*` namespace, so nothing in this gate touches it. Verified on the
production build: renders, 0 console errors, 0 external requests.

**D7 — the `status-badge` CSS was deleted, not scoped.** `StatusBadge.tsx` now
renders the `Chip` primitive, and `grep -rn "status-badge" src --include=*.tsx`
returns only the explanatory comment in `StatusBadge.tsx` — no element carries
the class. The base rule (10px uppercase, 4px radius, border) and its
`.club-app-shell` overrides were therefore dead CSS and were removed in the
finisher pass. `/dj` is unaffected (0 px diff, see D5).

**D9 — 1024×768 honest height was 8px over budget.** The honest state stacks
ErrorState + PreviewBanner; at 1024×768 it measured 2008px against the §7.4
≤2000 budget. The banners' vertical padding is compressed at `max-width: 1024px`
only (ErrorState 12→8px, PreviewBanner 12→4px). Measured **1984px**; no font
below 11px, no hit target below 36/44px.

## 5. Shell + interaction defects from the G0 audit — closed

| Audited defect | Fix | Evidence |
|---|---|---|
| Toast dies at 3410ms hovered / 3486ms focused | `Toast` clamps `duration` to `>= 8000`; the timer pauses on `mouseenter` **and** `focusin`, resuming with the remaining budget | stayed alive through 4.5s hover **then** 4.5s focus |
| Toast dismiss 18×28 | `.club-toast-close` is 36×36 desktop, 44×44 at ≤640px | measured box `36×36` |
| Focus falls to `<body>` after the toast closes | focus is moved out **before** unmount, back to the control that opened the flow | after close `activeElement` = the triggering button, not BODY |
| 4/10 Tab presses escape ⌘K palette | focus trap in `Layout.tsx` (own implementation, `useFocusTrap.ts` untouched) | **0/10** escapes |
| Modal does not lock background scroll | `overflow: hidden` + scrollbar-gutter compensation while open, restored on close/unmount | `body.style.overflow === 'hidden'` while open, `''` after Escape |
| One `role="status"`, no `aria-live` wrapper | only the newest toast announces; the region has no `aria-live` | 0 duplicate live regions |

Also verified: `Escape` closes and returns focus to `.command-trigger`;
`Segmented` moves with arrows, wraps, and exposes one tab stop per group.

## 6. Measured "after" numbers (§7.4 row owners)

Measured by the finisher on `/dashboard`, demo mode, Chromium via Playwright.
"Populated" = `**/v1/club/**` answered with fixtures inside the test (contract
§8: route fixtures, not a product mock).

| Metric | Baseline (`:5199`) | Now | Threshold |
|---|---|---|---|
| board height 1440×900, honest / populated | 2348 | **1500 / 1392** | ≤1700 |
| board height 1024×768, honest / populated | 2618 | **1984 / 1864** | ≤2000 |
| board height 390×844, honest / populated | 4952 | **2847 / 2751** | ≤3000 |
| distinct radii in the rendered DOM | 8 desktop / 9 mobile | **4** — exactly {10, 12, 16, 999}, no `50%` anywhere on the board | ≤4 |
| elements with a border inside the board | 81 (65 nested) | **4 honest / 5 populated**, all 1px row dividers at 6% white (`dl.club-board-facts`, three `.club-board-zone`, one `.club-list-row`); **0 panels** | ≤20 / 0 nested |
| distinct font sizes on the board | 13 (10–30px) | **6** — 11, 12, 13, 15, 20, 28; minimum **11px** | ≤6 / min 11 |
| uppercase + tracked elements | 6 | **1** (`.club-board-identity`) | ≤1 |
| interactive <44×44 at 390 | 40 of 41 | **0** | 0 |
| computed contrast minimum | 3.90:1 page / 3.06:1 token | **5.39:1** @1440 · **5.11:1** @390, 0 below AA; populated `/dashboard` **5.61:1** @1440 | ≥4.5 |
| axe critical / serious | 0 / 10 | **0 / 0** on `/dashboard` and `/sessions` at both widths | 0 / 0 |
| external hosts | 4 | **0** | 0 |

Every marker that used to be `border-radius: 50%` on a club surface (source dot,
live dot) now uses `--r-chip`; at 5–8px the render is an identical circle, and
the measured set on the board is exactly the four documented values. The only
`border-radius: 50%` declarations left in the shared stylesheets are marker
dots/avatars on `/dj` and `/connect` (`dj-presence-mark`, `dj-live-mark`,
`dj-profile-avatar`, connect live dots and sprint avatars). They are not club
surfaces, §4.3 explicitly classifies avatars/live dots as markers, and both
screens are out of scope for restyling.

Contract assertions, measured on all five club routes at 1440 and 390:
`[data-surface]` radii = `{16px}` with **0** elements carrying a border or any
box-shadow; `[data-control]` radii ⊆ `{10px, 12px, 999px}`; every `[data-metric]`
carries one of the three sources; `[data-preview-banner]` present on
`/dashboard`, which is the only route with `preview` metrics.

`/dj` and `/connect` after the cleanup: `/dj` is **0 px different** from the
G2-approved after-shot (`cleanup/dj-1440.png` vs `g2-dj-after.png`), and both
screens use exactly one green `rgb(0, 255, 135)` (screenshots:
`cleanup/dj-1440.png`, `cleanup/connect-1440.png`). `/connect` was not touched by
the cleanup: its CSS namespace (`--linear-*` / `--sprint-*` / `--drop-*`) has no
selector in common with the deleted club rules.

## 7. Notes for the board agent (G3)

- Import from `../components/club` — the barrel. Do not hand-write `data-*`.
- `Metric` takes `source` and renders the source chip itself; `caption` is the
  honest period string. A metric with `source="unavailable"` renders words, never
  a zero.
- `PreviewBanner` must be rendered once whenever any `Metric` is `preview`
  (`data-preview-banner` is what the verifier looks for).
- `Tile` stamps `data-tile` (see D1) and has no border; a selected tile uses
  `selected` for the tint + 2px accent blade.
- `Toast` needs `role="status"` handled by the component — do not wrap the
  region in `aria-live`.
- The one uppercase tracked element on the board is `.club-board-identity`
  (the club-name line above the h1). Nothing else may use
  `text-transform: uppercase` with letter-spacing.

## 8. Verification commands and grep evidence

```bash
cd apps/web-club
npm run typecheck    # exit 0
npm run build        # exit 0
npm run build:demo   # exit 0
```

Grep evidence (run from the repo root; every command below was executed):

```sh
# 0 selectors left
grep -rn "club-dashboard" apps/web-club/src                    # 0

# 0 banned radii (28/32/24/18px)
grep -rnE "border-radius:[^;]*(28px|32px|24px|18px)" \
  apps/web-club/src/operations.css apps/web-club/src/index.css \
  apps/web-club/src/styles apps/web-club/src/components/club/club-primitives.css   # 0

# 0 `border-radius: 50%` in the club primitives/board
grep -rn "border-radius: *50%" apps/web-club/src/components/club/club-primitives.css \
  apps/web-club/src/components/club/board/board.css             # 0

# 0 second greens / dead greys
grep -rniE "#10b981|#22c55e|#7ee2a8|#8ed3f4|#f2d089|#666666|#00e887|#70ffb9" \
  apps/web-club/src apps/web-club/index.html apps/web-club/tailwind.config.js    # 0

# 0 Geist, 0 Google Fonts, 0 runtime icon CDN
grep -rni "geist" apps/web-club/src apps/web-club/index.html \
  apps/web-club/tailwind.config.js                              # 0
grep -rn "fonts.googleapis\|fonts.gstatic" apps/web-club/src apps/web-club/index.html  # 0
grep -rn "iconify" apps/web-club/src apps/web-club/index.html   # 2 comment lines in Icon.tsx only

# fake telemetry modules are gone and unreferenced
grep -rn "useVenueState\|dashboard/mock\|dashboard/types\|dashboard/format\|VENUE_NAME\|VENUE_ROOM\|GENRE_LIBRARY" \
  apps/web-club/src --include=*.ts --include=*.tsx              # 0
grep -rn "Math.random" apps/web-club/src/dashboard              # 1 comment line only
```

Rendered evidence (Playwright, demo server on `localhost:5173`):

```sh
node output/club-redesign/cleanup/_verify.mjs   # axe, contrast, radii, overflow, hosts
node output/club-redesign/cleanup/_final.mjs    # heights honest+populated, fonts, borders
node output/club-redesign/cleanup/_capture.mjs  # /dj + /connect screenshots at 1440
```

Screenshots: `cleanup/dj-1440.png`, `cleanup/connect-1440.png`,
`cleanup/honest-*.png`, `cleanup/populated-*.png`; raw data in
`cleanup/verify.json` and `cleanup/final-measure.json`.
