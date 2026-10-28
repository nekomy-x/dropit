# G6 — Verification: measured result of the club panel redesign

> Role: verification. The harness in this report is `scripts/club-ui-check.mjs`.
> The verifier teammate was provisioned but its turn ended before it produced
> this file; the Lead wrote and ran the harness instead, so this section is
> explicit about who measured what and how to reproduce it independently.
> Every number below is reproducible with one command.

## 0. How to reproduce

```sh
cd apps/web-club && npm run dev:demo      # terminal 1 — http://localhost:5173
node scripts/club-ui-check.mjs            # terminal 2 — from the repo root
```

`scripts/club-ui-check.mjs` measures the page in two states:

| State | How | Why |
|---|---|---|
| **populated** | Playwright intercepts `**/v1/club/**` and answers with fixtures shaped like the real DTOs | The demo build has no backend. Fixtures are test scaffolding so the board renders the content it would render against a live API — the tallest, most complete state, therefore the real height budget test. |
| **honest** | no interception; the demo build's `/v1/club/**` returns 500 | Verifies the unavailable path: one banner, every API metric marked `unavailable`, no fabricated values, no thrown errors. |

Exit code 0 = green, 1 = at least one failing assert. Report artifacts land in
`output/club-redesign/after/` (`report.json`, `report.md`, screenshots).

## 1. Gate results

| Gate | Result | Evidence |
|---|---|---|
| `scripts/club-ui-check.mjs` (16 checks) | **GREEN — 0 failing asserts** | `output/club-redesign/after/report.md` |
| `npm run typecheck` | **exit 0** | `tsc --noEmit` |
| `npm run build` | **exit 0** | vite production build |
| `npm run build:demo` | **exit 0** | vite demo build |
| axe `/dashboard` 1440 + 390, populated + honest | **0 violations** | `@axe-core/playwright` 4.14.0 |
| axe `/sessions`, `/djs`, `/pricing`, `/earnings` 1440 + 390 | **0 violations** | same |
| Horizontal scroll 320/390/768/1024/1440 | **0 px at every width** | check 4 |
| Board height budget | **pass at all three viewports** | check 3 |

## 2. §7.4 before/after table — measured, not estimated

"Before" comes from `docs/ui/club-redesign/00-baseline.md` (auditor's independent
re-measurement on a frozen copy at `:5199`; it corrected two numbers in §3 and
those corrections are used here). "After" comes from
`output/club-redesign/after/report.json` plus the per-page measurement below.

| Metric | Before | After | Threshold | Verdict |
|---|---|---|---|---|
| Board height 1440×900 | **2348 px** | **1356 px** populated / 1500 px honest | ≤1700 | pass |
| Board height 1024×768 | **2618 px** | **1880 px** populated / 1984 px honest | ≤2000 | pass |
| Board height 390×844 | **4952 px** | **2671 px** populated / 2847 px honest | ≤3000 | pass |
| Panels on Overview | **8** (incl. rules form, tables list) | **7** | ≤7 | pass |
| Distinct radii in the rendered DOM | **8 desktop / 9 mobile** (28, 24, 18, 16, 12, 10, 9, 999, 50%) | **4** — exactly {10, 12, 16, 999} | ≤4 | pass |
| Elements with a border inside the board | **81**, of which **65 nested** | **5**, all 1 px dividers at ≤6 % white (header rule, one `dl` rule, three metric row rules) | ≤20, 0 nested | pass |
| Distinct font sizes on the board | **13** (10–30 px) | **6** — 11, 12, 13, 15, 20, 28 | ≤6, min 11 px | pass |
| Interactive elements < 44×44 | **40 of 41** | **0** at 390×844 | 0 on mobile | pass |
| Text contrast minimum | **3.90:1** page minimum (`#666666` token measured 3.06:1 but never actually rendered) | **5.61:1** at 1440, **5.1:1** at 390 | ≥4.5 | pass |
| axe critical/serious | **0 critical / 3 serious rule families, 10 nodes** (7 in the shell/rail) | **0 / 0** on all five club pages | 0 | pass |
| False save-labels and save-toasts | **12** (10 fake success toasts + 2 false data labels) | **0** | 0 | pass |
| External CDN requests on load | **4** (fonts.googleapis, fonts.gstatic, code.iconify.design, api.iconify.design) | **0** | 0 | pass |

Two rows were already green before the work started and are **not** claimed as an
improvement: horizontal overflow (0 px at all widths before and after) and
`prefers-reduced-motion` (the old page had 0 running animations; the new one also
has 0).

## 3. Per-page measurement (1440×900, populated)

| Page | Height | Borders in main | Radii | Font sizes |
|---|---|---|---|---|
| `/dashboard` | 1356 | 5 (dividers only) | {10,12,16,999} | 6 |
| `/sessions` | 900 | 1 | {10,12,16,999} | 5 |
| `/djs` | 900 | 5 | {10,12,16,999} | 5 |
| `/pricing` | 900 | 2 | {10,12,16,999} | 5 |
| `/earnings` | 900 | 1 | {10,12,16,999} | 5 |

The four secondary pages fit one screen; the board is one screen plus a partial
second band, down from 2.6 screens.

## 4. What the harness asserts (16 checks, §7.2)

1. `[data-surface]` radius == {16px}; `[data-tile]` == {12px}; `[data-control]` ⊆ {10,12,999}px; the rendered radius set on the page is exactly {10,12,16,999}.
2. No `[data-surface]` has a border or a blurred box-shadow (inputs/`[data-allow-border]` exempt).
3. Height budget at 1440×900 (≤1700), 1024×768 (≤2000), 390×844 (≤3000), populated.
4. No horizontal overflow at 320/390/768/1024/1440.
5. Every interactive element has an accessible name.
6. Touch targets ≥44×44 at 390 (inline links exempt).
7. Focus ring `outline-width ≥2px` on the first 10 tab stops.
8. `prefers-reduced-motion: reduce` → no running animations.
9. ≤6 distinct font sizes, none below 11px, at 1440 and 390.
10. ≤1 element with `text-transform: uppercase` + positive letter-spacing.
11. Every `[data-metric]` carries `data-source` ∈ {live, preview, unavailable}; when any preview exists, `[data-preview-banner]` is present — asserted in both the populated and the honest state.
12. Contrast ≥4.5:1 (≥3:1 at ≥24px) per text node, computed from computed styles.
13. Zero requests to `code.iconify.design`, `fonts.googleapis.com`, `fonts.gstatic.com`.
14. No `console.error` and no `pageerror` in the populated state.
15. Screenshots at 1440/1024/390 + reduced motion + honest state are written.
16. `report.json` + `report.md` are written.

## 5. What could not be verified in this environment, and why

| Item | Why | Consequence |
|---|---|---|
| Behaviour against a live API | No backend on this machine (`localhost:8081` is down). | Every API path is exercised through fixtures that mirror `src/api/client.ts` DTOs, and the honest path is exercised without fixtures. The *shape* is verified; the *server* is not. |
| Real payment/settlement numbers | Same. | The board labels the figure "all time, settled" and the SQL in `crates/gateway-api/src/panel_routes.rs` was read to confirm the label; the number itself cannot be cross-checked here. |
| Session restore after a real reload with a real shift | Same. | The code path (`getCurrentClubSession()` → `getClubDashboard().session`) is wired and type-checked; the round-trip is not observed. |
| DJ console axe cleanliness | Out of scope by §9.3 (the DJ console is not part of this task). | Baseline had 7 serious nodes there; unchanged. Documented as out of scope rather than silently ignored. |

## 6. Known limitations of the harness

- The populated fixtures are test scaffolding, not product code. If the real API
  returns a different shape, the harness will not notice; `npm run typecheck`
  will, because the hook is typed against `src/api/client.ts`.
- axe reports `color-contrast` as `incomplete` (it cannot resolve stacked
  translucent layers), so check 12 computes contrast itself from computed
  styles. Both are reported.
- Check 10 counts elements that *render* uppercase with tracking; an element
  whose text is already uppercase in the source does not count.

## 7. Post-red-team fixes and the closing gate

The red team (G7) opened four P1s and one border-policy finding after the first
green run. All five were fixed and re-measured; the round-4 record with proof is
`03-redteam.md ## Round 4` and `output/club-redesign/redteam/round4.json`.

| ID | Defect | Fix | Re-measured |
|---|---|---|---|
| RT-01 | The board asserted a door-QR failure it never observed (and, after the QR flow moved to Live room, unconditionally). | The door-QR next action was removed; the Room-now fact reads "issued in Live room". | `doorQrAlert: false` |
| RT-02 | Honest and partial-failure states printed "Shift: none open" / "Guest requests: closed" / "No shift open" under an "unknown" headline. | `sessionSource` is threaded into `RoomNowPanel` and `DoorPanel`; when it is `unavailable` they print "not read" / "Shift state not read". | `noneOpen: false`, `noShiftOpen: false`, `notRead: true` |
| RT-03 | The two-step confirmations dropped focus to `<body>`; Escape did not dismiss. | Focus moves into each confirmation, returns to its trigger on cancel/Escape; Escape handler added on Sessions (close + rotate), Pricing (price change) and DJs (suspend/restore). | focus lands on a `BUTTON` in all four confirms, before and after Escape |
| RT-04 | After closing a shift, `/sessions` had no way to open the next one. | A "Start the next shift" control drops the closed shift locally so the setup form returns. | `startNextShiftOffered: true`, `openFormBack: true` |
| RT-11 | Sidebar footer (8 %) and mobile bottom-nav (8.6 %) dividers were brighter than the 6 % hairline. | A single `.club-shell-rule` hairline (6 %) replaced both, club-scoped. | 0 border sides above 6 % white on all five club pages |

Gate after those fixes: `scripts/club-ui-check.mjs` **GATE GREEN 16/16**,
`typecheck` / `build` / `build:demo` **exit 0**, axe **0 violations** on all five
club routes in populated and honest states.

Final board measurements: **1359 px** @1440×900, **1864 px** @1024×768,
**2725 px** @390×844 (populated); 1500 / 1984 / 2847 honest. 7 panels, radii
exactly `{10, 12, 16, 999}`, 5 border sides all ≤6 % white, 6 font sizes with a
11 px minimum, contrast 5.61:1 @1440 and 5.1:1 @390, 0 external hosts, 0 console
errors populated, 0 running animations under reduced motion, 0 touch targets
under 44×44 at 320 and 390.

**Red team status: three consecutive rounds with no new P0/P1** (rounds 2 and 3
by the red-team agent, round 4 by the Lead after that agent's turn ended; the
attribution and the harness are stated in `03-redteam.md`). The gate asks for two.

### Known, accepted, not fixed

| ID | Severity | What | Why it stays |
|---|---|---|---|
| R4-03 | P2 | Only 2 panels are fully visible in the first screen at 1440×900; §4.2 asks 4–6. | Meeting it would need either a type size below the 6-step scale or targets below 44 px, both forbidden. Needs a dedicated composition pass (e.g. collapsing Floor zones to one row), not a late trim. |
| R4-04 | P2 | Two panels end with 27 px / 41 px of unused space (§4.2 asks ≤24). | 3 px and 17 px over a soft budget on two tiles. |
| RT-10 | P2 | Harness check 1.1 reports `measured {}` because the board ships no nested 12 px surface; four `club-field` inputs carry no `data-control="field"`. | The check is honest about "no instances"; the missing `data-control` on those inputs is a real gap, reported rather than silently patched. |


## 8. App-wide radius unification (2026-10-27, after the user's follow-up)

The user reported that the same design problems (off-scale corners) existed "in many
places", not just on the board. Measured before: `/connect` rendered **eight** radius
values at once (`5px`×14, `6px`×10, `7px`×7, `8px`×19, `12px`×9, `50%`×4,
`3px 3px 0 0`×21, `2px`×2), and the DJ console mixed `4/5/6/7/8/9/11/12/14/18/24/28px`.

What changed:
- `packages/ui/panel.css` (the canonical kit; copies are generated by
  `node scripts/sync-ui-tokens.mjs`): 3 raw radii moved to tokens, theme-toggle
  buttons get 44px targets at ≤640px, and the narrow topbar lets the search field
  give up width so 320px stops scrolling the page.
- `apps/web-club/src/{index,operations}.css`, `pages/DjConsolePage.css`,
  `components/DjConsoleView.css`, `components/DjSidebar.css`: ~110 radius
  declarations mapped by role — surfaces 16, tiles 12, controls 10, chips/dots 999,
  data bars 0. No raw radius remains in any web-club CSS.
- `tailwind.config.js`: the radius scale is remapped (`sm|md|DEFAULT → 10px`,
  `lg|xl → 12px`, `2xl|3xl|card → 16px`, `full → 999px`, `field → 10px`) so no
  utility can reintroduce an off-scale corner.
- Inline `borderRadius: 4` on the DJ meters → `var(--r-chip)`; `rounded-t-[3px]`
  chart bars → square.

Measured after: `/connect` → {10px:20, 12px:28, 16px:9, 999px:67};
`/dashboard` → exactly {10,12,16,999}; DJ console → {10px, 12px, 16px, 999px}.
`apps/web-admin` still renders 6px×4 on its login screen: it is outside this task's
scope and is reported, not silently changed.

Gate after the change: `club-ui-check` **GATE GREEN**, axe **0 violations** on all five
club routes at 1440 and 390, `typecheck`/`build`/`build:demo` exit 0 for `web-club`
and `web-admin`.
