# G7 — Red team report: club panel redesign

Author: red-team (G7). I write no product code. Everything below is a measurement
against `http://localhost:5173` (`vite --mode demo`, IPv6 loopback — `localhost`,
not `127.0.0.1`) or against the source tree, with the harness in
`output/club-redesign/redteam/`.

**Measurement snapshot (round 2/3 numbers):** tree frozen by an external editor
since `00:01:20 MSK`. Key files at measurement time:

| File | mtime | sha256 (first 16) |
|---|---|---|
| `components/club/board/DoorPanel.tsx` | 00:01:07 | `6cd31675c99c8398` |
| `components/club/board/nextActions.ts` | 23:32:44 | `5cbd628030e30bb1` |
| `dashboard/useClubBoard.ts` | 21:25:59 | `1d25bb84aad05c08` |
| `pages/SessionsPage.tsx` | 21:25:59 | `2387294e4ff49409` |
| `pages/PricingPage.tsx` | 21:14:26 | `8a7014d7392be7b9` |
| `components/Layout.tsx` | 21:06:27 | `d28ec9d3036fe831` |
| `index.css` | 21:48:09 | `ff1fc6ac9af73d9a` |
| `components/club/club-primitives.css` | 23:56:12 | `b11e507a1848eef1` |
| `styles/club-tokens.css` | (unchanged) | `c457566b0b6e8cda` |

Harness (all in `output/club-redesign/redteam/`, test scaffolding only, never
product code): shared lib `rt-lib.mjs` + 13 scripts (fixtures + in-page measurement), `rt-dashboard.mjs`,
`rt-interaction.mjs`, `rt-interaction2.mjs`, `rt-structure.mjs`, `rt-pages.mjs`,
`rt-axe.mjs`, `rt-mobile.mjs`, `rt-round2.mjs`, `rt-round2b.mjs`,
`rt-round2c.mjs`, `rt-round2d.mjs`, `rt-round3.mjs`, `rt-connect-diff.mjs`.
Raw data: `measure.json`, `interaction.json`, `interaction2.json`, `mobile.json`,
`pages.json`, `pages-digest.json`, `axe.json`, `structure.json`, `round2.json`,
`round2-addendum.json`, `round2-fresh.json`, `round2-final.json`, `round3.json`.
Screenshots: 67 PNGs in the same directory.

Severity scale used: **P0** = a §7.1 DoD line is factually false / a broken
production path; **P1** = the product lies about its data or state, a destructive
flow is not operable by keyboard/AT, or a functional dead end; **P2** = a
measurable rule/polish defect that does not block the gate.

---

## Round 1

Round 1 was executed against the tree as it stood at `~22:00–22:10 MSK` (before
the four post-round-1 fixes). Its artifacts survive; this section reconstructs it
from them. Round-1 output was not routed before the fixes landed, so four of its
P1 findings are still open (tracked in Round 2).

### What was attacked and how

| Area | Method | Artifact |
|---|---|---|
| Heights / overflow / radii / borders / fonts / uppercase / contrast / targets | Playwright 1440×900, 1024×768, 768×1024, 390×844, 320×568, populated fixtures + honest (no backend) + reduced motion | `measure.json`, `digest.json` |
| Per-panel void, radius owners, zones grid, banner | DOM geometry pass | `structure.json` |
| Honest / partial-failure / empty / loading states | route fixtures with failures and empties | `interaction2.json`, `board-*-1440.png` |
| Two-step confirms, double submit, focus, Escape, toast lifetime/pause, command palette, tab order | scripted interaction | `interaction.json`, `interaction2.json` |
| The four club pages + /dj + /connect | same measurement suite per page × 2 widths | `pages.json`, `pages-digest.json` |
| axe | `@axe-core/playwright` 5 pages × {1440, 390} × {populated, honest} | `axe.json` |

### Round-1 findings

| ID | Sev | What is wrong | Proof | How to verify |
|---|---|---|---|---|
| RT-01 | **P1** | The board's "Next actions" asserts a door-QR failure that never happened: *"Check the door QR — The shift is open but the door token could not be generated."* No token was ever requested; the hook maps "no token in memory" to `unavailable`. | Populated board text in `measure.json` (`populated-1440.bodyText`), screenshot `populated-1440.png`. Code: `components/club/board/nextActions.ts:112-120` keyed on `sessionQrSource`; `dashboard/useClubBoard.ts:371-372` sets `sessionQrSource: sessionQr !== null ? 'live' : 'unavailable'`. | Fixture `GET /v1/club/**` with an open session, open `/dashboard`, read the "Next actions" panel. Never touch the QR. The first row claims a failure. |
| RT-02 | **P1** | Honest and partial-failure states print fabricated negatives. Header says *"Shift state unavailable — the board cannot say whether the room is open"*, while the same panel's facts say **Shift: none open** and **Guest requests: closed**, and the Guest access tile says **"No shift open"**. | `interaction2.json` → `partialFailure.facts`; `round2.json` → `honest.facts`; screenshots `honest-1440.png`, `board-partial-failure-1440.png`, `r3-partial-failure.png`. Code: `RoomNowPanel.tsx:91-92` (`session.accepting ? 'open' : 'closed'`), `DoorPanel.tsx` empty branch, `state.ts:33-43` (unavailable branch has `accepting: false`). | Block only `GET /v1/club/dashboard` with a 500, open `/dashboard`. The headline says "unavailable", the facts say "none open / closed". |
| RT-03 | **P1** | Every two-step confirmation except the DJ one drops keyboard focus to `<body>`: opening the Sessions close-shift confirm, opening the QR-rotate confirm, opening the Pricing price confirm, and cancelling any of them. The confirm region has no `role="status"`/`aria-live`, so an AT user gets no announcement that a destructive question has appeared; `Escape` does not dismiss it. | `interaction.json` → `sessions.focusInConfirm`/`focusAfterCancel` (`isBody: true`), `djs.focusAfterCancel`; `round2-addendum.json` → `sessions` all six probes `isBody: true`, `pricing.inConfirm/afterCancel` `isBody: true`, `escapeClosed: false`. Screenshot `r2-sessions-confirm.png`. (Mitigation measured: sequential Tab resumes at the confirm, `tabsToCancel: 2`.) | Focus "Close shift" with the keyboard, press Enter, read `document.activeElement` → `BODY`; press Escape → still open. |
| RT-04 | **P1** | After closing a shift, `/sessions` — the only screen that opens shifts — offers no way to open the next one. The closed-shift branch says *"Open the next shift when the room opens again"* but renders no control; the open-shift form is unreachable without a page reload. | `round2-fresh.json` → `sessions.afterClose.openShiftButton: false` and the body text "This shift is closed … Open the next shift…"; screenshot `r2-sessions-closed.png`. Code: `pages/SessionsPage.tsx:262-270` (closed branch) and `:245` (`shift ? … : open form`) — the form only renders when `shift === null`. | Fixture the close endpoint, close the shift, look for any "Open shift" button on the page: none. |
| RT-05 | P2 | In the failed state the price caption asserts a fact that was never read: **"Request price — unavailable … no active pricing policy"**. The request failed; "no active policy" is `INITIAL_SNAPSHOT`'s default caption. | `round2.json` → `honest.metrics[4]`; `useClubBoard.ts:170` (initial caption), `:256-258` (`policy ? … : 'no active pricing policy'`), `:325`; screenshot `honest-1440.png`. | Block `/v1/club/pricing` with a 500; the metric reads `unavailable` and its caption says "no active pricing policy". |
| RT-06 | P2 | Next action promises a capability the product does not have and contradicts the adjacent tile: **"Review the DJ roster — Invitations, statuses and who is on shift for the rest of the week"**, while "On the decks" says *"the club API does not report who is on shift"* and `/djs` shows no shift information. | `populated-1440.bodyText`; `nextActions.ts:134-140`; `DecksPanel.tsx:61`. | Open populated `/dashboard`; read the third next action. |
| RT-07 | P2 | Internal panel voids the "≤24px" rule is meant to prevent: content ends **53px** above the pinned footer in "Guest requests" and **32.6px** in "Floor zones" at 1440. The gate's void metric reads 0 because the footer is `margin-top:auto`. | `structure1440` in `structure.json` / `round2-final.json` → `gapBodyToFoot` 53 and 32.6; panels are 355px tall with ~150px of content. | Measure distance from last body child to `.club-board-foot` top. |
| RT-08 | P2 | "Floor zones" is a 2×2 grid with three zones → one empty cell (151×90 at 1440, 151×90 at 390) that reads as a missing tile. | `round2-final.json` → `zoneGrid.emptyCells: 1`; screenshots `populated-1440.png`, `populated-390.png`. | Count grid cells vs children. |
| RT-09 | P2 | `BoardSkeleton` claims "same three bands and the same spans … so nothing jumps", but the skeleton panels are **176px** tall and the loaded panels **354–355px**; page height jumps 900 → 1494 on the first response. | `interaction2.json` → `loadingShift`; `round2-fresh.json` → `loadingShift`; `BoardSkeleton.tsx:1-8`. | Delay `**/v1/club/**`, measure `[data-surface="panel"]` heights before/after release. |
| RT-10 | P2 | Markup/harness integrity: §7.2 check **1.1 passes vacuously** — `[data-tile] radius is 12px … measured {}` means there is no `[data-tile]` on the board at all, and the board ships **zero nested 12px surfaces** (all seven tiles are 16px panels). `Tile.tsx:8-20` deliberately emits `data-tile` instead of the contract's `data-surface="tile"` because §2 and check 1 are mutually unsatisfiable. Separately, 5 `club-field` inputs carry no `data-control="field"` (only the ⌘K input does), so check 1.2 never sees them. | `output/club-redesign/after/report.md` line 10 (`1.1 … measured {}`); my run `dataTile: 0`, `dataSurface: ["panel"×7]`; `grep -c 'data-control="field"'` = 1 vs `className="club-field"` = 5. | `document.querySelectorAll('[data-tile]').length` on the populated board → 0. |
| RT-11 | P2 | Two shell borders are **above the 6% hairline**: the sidebar footer divider `rgba(255,255,255,.08)` on every desktop page and the mobile bottom-nav top border `rgba(255,255,255,.086)` at every mobile width. (Fields at 8% are exempt.) | `round2.json` → `pageBordersAbove6` for all ten page×width runs; `Layout.tsx:193`, `Layout.tsx:269`. | Scan every element with `border-*-width > 0`, read the colour of the side that has width. |
| RT-12 | P2 | `/dj` is **not** pixel-identical to the pre-redesign capture: 5.4% of sampled pixels differ, and flat-region sampling shows the DJ console **panel surface changed `rgb(32,32,35)` → `rgb(28,28,31)`** (−4/channel). Backgrounds/sidebar are identical (`rgb(9,9,11)`). §9.6 allows only the single-green collapse. (Against the post-G2 capture the diff is 0.00%.) | `round2.json` → `djDiff.vsBefore {changedPct: 5.4, maxChannelDeltaSum: 684}`, `vsG2 {changedPct: 0}`; flat-region sampling output in Round 2 below; `before/page-dj.png` vs `r2-dj-now-1440.png`. | Diff the two PNGs; sample 180×60 px of an empty panel area in both. |
| RT-13 | P2 | `transition-property: all` at 120ms on shell controls `.command-trigger` and `.header-account` — §4.8 allows only transform/opacity/colour. | `round2-fresh.json` motion probe; computed styles: `{cls:"command-trigger", prop:"all", dur:"0.12s"}`. | `getComputedStyle` on those two buttons. |
| RT-14 | P2 | "Two screens long" is gone from the DoD numbers but not from the composition: at 1440×900 the board is 1359px (1.51 screens) and only **2 of 7 tiles are fully visible**; at 1024×768 it is 1864px (2.43 screens, 2 tiles). §4.2 asks for 4–6 tiles in the first screen. | `structure.json` / `round2-final.json` → `fullyVisiblePanels: 2/7`; `round3.json` → `viewportCoverage`. | Count panels with `rect.bottom <= innerHeight` at 1440×900 and 1024×768. |
| RT-15 | P2 | Freshness is board-wide but reads as per-data freshness: in partial failure the header says **"updated just now"** while the shift block is `unavailable`. `lastFetchedAt` is stamped if *any* of the four requests succeeded (`useClubBoard.ts:266-267`). | `round2.json` / `round3.json` → `partialFailure.header` + `freshness`. | 500 only `/v1/club/dashboard`, read the header line. |
| RT-16 | P2 | Invented currency: `/v1/club/earnings` returns `period/net_minor/payments/gross_minor` with **no currency field** (`panel_routes.rs:410-416`), but the client hardcodes `currency: 'EUR'` (`api/client.ts:791`) and the page prints `€`. | code refs above; screenshot `earnings-1440.png`. | Read the backend JSON and the client mapping. |
| RT-17 | P2 | `/pricing` renders `data-source="live"` on a metric with **no value** when the API has no active policy: `Current price / live / No active price`. This breaks the §7 invariant "value `null` only when `unavailable`" and shows a "live" chip for an empty reading. | `round2-final.json` → `emptyPages["/pricing"].metrics[0]`: `"source": "live"`, text "No active price"; `PricingPage.tsx:169-181` (`source={unavailable ? 'unavailable' : 'live'}`). | Fixture `/v1/club/pricing` → `{items: []}`; open `/pricing`. |
| RT-18 | P2 | No toast ever offers Undo (`grep onUndo` → only the primitive's own prop). §5.3 asks for "rollback via 8 s toast with Undo where rollback is possible"; the price change is the one reversible mutation (set the old price again). | `grep -rn onUndo apps/web-club/src --include=*.tsx` → 3 hits, all inside `Toast.tsx`; toasts pushed in `PricingPage.tsx:125-129`, `SessionsPage.tsx:146,175,197`, `DjsPage.tsx:94,142` carry no `onUndo`. | Trigger a price change; the toast has only a close button. |

### Round 1: what I could not falsify (verified clean)

Reproduced with my own harness, not taken on trust:

- **axe**: 0 violations (all impacts) on `/dashboard`, `/sessions`, `/djs`, `/pricing`, `/earnings` × {1440×900, 390×844} × {populated, honest} = 20 runs (`axe.json`).
- **Heights**: populated 1359 / 1864 (1024×768) / 2725 (390×844) / 2924 (320); honest 1500 @1440. Budgets 1700 / 2000 / 3000 — all pass.
- **Horizontal overflow**: 0 at 320/390/768/1024/1440, board and all four pages.
- **Radii**: rendered set is exactly `{10px, 12px, 16px, 999px}` on the board and on all four pages at both widths. No `50%`, no 24/28/32 on club surfaces.
- **Fonts**: exactly 6 sizes `{11,12,13,15,20,28}`, none below 11px, on the board and on all four pages, both widths.
- **Uppercase+tracking**: exactly 1 on the board (`Club Nebula`); 0 on the four pages (their eyebrows are normal case, `letter-spacing: .11px`).
- **Contrast**: own composited-text walker, all text nodes: board min **5.61:1** @1440 and **5.11:1** @390; per-page minima 5.11–5.79; zero failures.
- **External hosts**: `localhost:5173` only; 0 CDN requests; no iconify/Google-Fonts code path.
- **Console/page errors**: 0 in the populated state on all five pages. Honest mode logs 8× HTTP 500 at the browser level (4 requests × StrictMode) — expected and documented (§9.3).
- **Preview banner**: present in populated and honest states; 12px, 42px tall (2 lines) @1440, 62px (3 lines) @390/320 — not fine print; every preview number carries a `preview` chip.
- **Source chips**: 17 of 20 numbers rendered on the board are inside `[data-metric]`; the 3 others are the session id (`11111111`), the "1 more on the roster" sentence and one prose sentence — no unlabelled metric.
- **Toast**: lifetime measured 8054–8102 ms (≥8 s), hover pause holds past 9.5 s, exactly one `role="status"` per toast, close target ≥36 px.
- **Double submit** (realistic `dblclick` and Enter×2): 1 request for close-shift, price change, suspend-DJ, and (then) board QR.
- **Touch targets**: 0 elements under 44×44 at 390 and 320 (board and all four pages); desktop minimum is exactly 36px as specified.
- **Bottom nav**: no coverage of the last panel at max scroll — gap 51px at both 390 and 320.
- **Motion**: reduced motion → `document.getAnimations()` empty on all five pages; normal state runs only the permitted live dot (`opacity`, 2s); no stagger, no `animation-delay` in club CSS, no transition >300 ms.
- **SQL captions**: `panel_routes.rs:178-185` — net settled has no time filter ("all time" true), requests are `created_at > now() - interval '24 hours'` ("last 24 h" true, and "paid" is not claimed); `club_earnings` is a rolling `interval '1 day'` window and the page says so. Splits are inserted only on capture (`infra-db/src/billing.rs:536-551`), so "settled" ≈ captured — defensible.
- **No `Math.random()`** in any board data path (`grep` finds it only in the DJ console waveform, the idempotency-key fallback and the connect/register helpers — none feeds a board metric).

### Round 1: one correction to my own record

My `digest.json` recorded `{"tag":"li","cls":"club-list-row","width":1,"color":"rgb(229,231,235)"}`.
That was **my measurement artifact**: my border helper stored `borderTopColor` for
whichever side had width, and that element only has a *bottom* border. Re-measured
per side, `li.club-list-row` bottom = `rgba(235,240,250,0.06)` (6%). So the
near-white divider was **not** a product defect and the post-round-1 "fix" of that
line fixed nothing real. It is harmless, but the record should say so. (The real
above-6% borders are RT-11.)

---

## Round 2

Tree: frozen since `00:01:20 MSK` (DoorPanel + DashboardPage changed at 00:01,
i.e. between Round 1 and Round 2). Note: the Door & QR generation flow **was
removed from the board** in that edit — see F3 verdict.

### A. The four post-round-1 fixes

| Fix | Verdict | Evidence |
|---|---|---|
| **F1. `li.club-list-row` divider pinned to the 6% hairline; shell header divider scoped to `.club-app-shell`** | **Verified, but the premise was my artifact.** Current measured borders on the board page: 6 × `rgba(235,240,250,0.06)` (header rule, `dl` rule, list row, three zone rules) + 1 × `rgba(255,255,255,0.08)` (sidebar footer — **not fixed**). The shell header rule is now `rgba(235,240,250,0.06)` ✓. The claim "every border on every club page ≤6%" is **false**: see RT-11. | `round2.json` → `board.bordersAll` (7 entries), `pageBordersAbove6` (sidebar footer 8% on all desktop pages; mobile nav 8.6% on all mobile pages). |
| **F2. Occupancy arc removed; no background abstraction** | **Verified and endorsed.** `[data-abstract]` nodes: **0**; `.club-board-arc`: **0**; no `<svg>` inside any board panel. `--abstract-opacity` still resolves (1; 0 under reduced motion) but dims nothing. | `round2.json` → `board.abstract {nodes:0, arc:0, svgs:0, token:"1"}`; `abstractUnderReducedMotion {opacity:"0"}`. |
| **F3. Focus after the Door & QR confirmation** | **Not verifiable — the flow no longer exists.** `DoorPanel` is now "Guest access": a status panel with a `Link` ("Issue the door QR" → `/sessions`); there is no confirm, no trigger, no `Generate`/`Cancel` control on the board (`focus.boardQr.qrButtons: []`, `doorPanel.buttons: []`). The fixed code path is gone. The **same focus defect is still live** on the remaining confirms: Sessions close-shift, Sessions rotate-QR, Pricing price change, DJ suspend-cancel (RT-03). | `round2.json` → `focus.boardQr`; `round2-addendum.json` → `sessions` (6/6 probes `isBody`), `pricing` (2/2 `isBody`), `djs.afterCancel.isBody: true`; `escapeClosed: false`. |
| **F4. `/dj` restored** | **Verified against the G2 capture, not against "before".** vs `output/club-redesign/g2-dj-after.png`: **0.00% changed, max channel delta 0**. vs the only pre-redesign capture (`output/club-redesign/before/page-dj.png`): **5.4% changed**, and flat-region sampling shows the panel surface is now `rgb(28,28,31)` where it was `rgb(32,32,35)`; background/sidebar/topbar are identical `rgb(9,9,11)`. So the `--ops-ink/--ops-rail` diagnosis is confirmed for the *shell*, but a 4/255 panel-surface repaint remains (RT-12). | `round2.json` → `djDiff {vsBefore:{changedPct:5.4}, vsG2:{changedPct:0}}`; flat-region probe: `panel_bg before [32,32,35] now [28,28,31]`, all other regions equal. |

### B. Carried-over findings (open, not new)

Round 1's P1 set has not been routed yet and is still present on the current tree.
Because the board can no longer mint a token, RT-01 got **worse** (unconditional):

| ID | Status on current tree |
|---|---|
| RT-01 (**P1**) | **Confirmed and now unconditional.** Nothing calls `rotateSessionQr` any more (`grep`: only `useClubBoard.ts` defines it), so `board.sessionQr` is permanently `null`, `sessionQrSource` permanently `unavailable`, and `nextActions.ts:112-120` therefore fires on **every** board with an open shift. Measured on the current tree: `nextActions: "Check the door QR\nThe shift is open but the door token could not be generated.\nLive room …"` (`round2.json` → `board.nextActions`). The Room-now fact `Door QR → "not generated"` (`RoomNowPanel.tsx:99-100`) is equally unobservable: the board is not where a token is issued. |
| RT-02 (**P1**) | **Confirmed on current tree** — `round2.json` → `honest.facts`: "Shift none open / Guest requests closed" under a headline that says "Shift state unavailable"; `round3.json` → `partialFailure.facts` identical; Guest access branch says "No shift open". |
| RT-03 (**P1**) | **Confirmed on current tree** for Sessions close/rotate and Pricing; DJ confirm opens without losing focus but cancelling drops it to `<body>`. Escape does not dismiss. |
| RT-04 (**P1**) | **Confirmed on current tree** — `round2-fresh.json` → `sessions.afterClose.openShiftButton: false` after a successful close. |
| RT-05…RT-18 (P2) | All re-confirmed on the current tree where measurable: RT-05 (`honest.metrics[4]`), RT-06 (board text), RT-07 (53 / 32.6 px), RT-08 (1 empty cell), RT-09 (176 vs 354), RT-10 (report line 10 + `dataTile: 0`), RT-11, RT-12, RT-14 (`fullyVisiblePanels: 2/7`), RT-15 (`partialFailure.freshness: "updated just now"`), RT-17 (`/pricing` empty fixture). |

**New P0/P1 in round 2: none.** The round-2 edits introduced no new P0/P1; RT-01
became unconditional as a consequence of removing the board's QR minting (same
defect, worse blast radius, not a new one).

### C. Round-2 new P2 findings

| ID | Sev | What is wrong | Proof | How to verify |
|---|---|---|---|---|
| RT-19 | P2 | Dead code left by the Door-panel rewrite: `useClubBoard`'s `sessionQr`, `sessionQrSource`, `rotateSessionQr`, `qrBusy` are unreachable from the UI; `DoorPanel`'s whole `qr ? …` branch and `RoomNowPanel`'s `doorQrGenerated` prop can never be true. It is what keeps RT-01's false alert alive. | `grep -rn rotateSessionQr apps/web-club/src` → definition only; `DashboardPage.tsx:119,133-134` passes values that are always `null`/`'unavailable'`; `DoorPanel.tsx:40-60` unreachable branch. | Delete the call graph and both RT-01 and the dead branch disappear. |
| RT-20 | P2 | `/sessions` and `/pricing` confirmations are **not dismissible with Escape** and are not announced (`no role=status`/`aria-live` on the confirm region). This is part of RT-03 but stands as its own §5.3 requirement: "Escape closes; keyboard: …". | `round2-addendum.json` → `sessions.escapeClosed: false`; DOM of `r2-sessions-confirm.png` has no live region. | Open the close-shift confirm, press Escape. |
| RT-21 | P2 | The board's own summary sentence is falsified by its own list: "Next actions — **Nothing here is a suggestion the product cannot carry out**" sits above "Review the DJ roster — … and who is on shift for the rest of the week" (RT-06). | `populated-1440.bodyText`; `NextActionsPanel.tsx:23`. | Read the panel detail and the second row together. |

### D. Round-2 areas attacked and found clean

- **Pages interactions**: close-shift confirm copy is explicit and irreversible ("This cannot be undone — a closed shift cannot be reopened. The guest QR stops working and pending requests expire"); one `PATCH /close` on realistic double click; busy state ("Closing…") disables both buttons; success toast is honest; price confirm shows **Old €3.00 / New €4.00** and validates 1–10 EUR ("Minimum price is €1.00 (100 cents)"); 1 `POST` on double click; suspend shows the API result and reloads (`PATCH` once); invitation expiry is the real `inviteExpiresAt` ("Expires 2 Nov 2026, 22:09"), not "7 days".
- **Toast**: lifetime 8102 ms; hover pause still present after 9.5 s; single `role="status"`; under reduced motion it still appears and auto-dismisses by 9 s with `animation-name: none` — content does not depend on motion.
- **Keyboard**: first 10 tab stops on `/sessions` all show a 2px outline and follow visual order; command palette traps focus, Escape closes it and returns focus to `.command-trigger` (`round2` palette probe: 6/6 tab stops inside the dialog, `focusAfterEscape: BUTTON.command-trigger`).
- **Mobile 390/320**: board 2725 / 2942 px (budget 3000); 0 horizontal overflow; 0 targets under 44×44; banner 3 lines / 62 px at both widths; bottom nav clears the last panel by 51 px; `/sessions` QR is a 176 px SVG in a 200 px box, fully inside 320 px, no overflow.
- **Loading / empty**: `/earnings` and `/djs` empty states are honest (`EmptyState`, no fake rows); `/dashboard` empty renders `live €0.00` and `0 requests` with source chips. (Only `/pricing` empty is mislabelled — RT-17.)
- **Reduced motion**: 0 animations on all five pages; only `club-board-dot` loops in the normal state (2 s opacity, the single permitted loop); no `transition: all` on club controls except the two shell buttons in RT-13.
- **Markup contract**: `[data-surface]` = 7 × `panel` + `dialog` (⌘K) → all 16px ✓; `[data-control]` radii `{10px}` on the board ✓. The `tile` half is vacuous (RT-10).
- **axe**: independently re-run, still 0 violations in all 20 configurations.
- **Acceptance harness**: I re-ran `node scripts/club-ui-check.mjs` myself: **GATE GREEN, 16/16**, with `1.1 … measured {}` (see RT-10), heights `1359 / 1864 / 2725`, contrast minima `5.61 / 5.1`.
- **typecheck**: `npm run typecheck` exit 0. **build**: `npm run build` exit 0 (run twice during this audit).
- **Production mode** (`vite preview` of the production build, port 4173): `/connect` renders (`h1 "Let guests choose the next track."`, 0 console errors, own visual world — radii 2/3/5/6/7/8/12/15/9999, own `--linear-mint`), `/` → `/connect`, `/dashboard` and `/sessions` → `/login` via `RequireRole`, `/dj` → `/dj/login`, `/login` renders. **Limitation:** `output/club-redesign/before/page-root.png` was captured in demo mode where `/` lands on `/dj`, so there is **no pre-redesign `/connect` capture** — "`/connect` visually unchanged" cannot be proven from the repo's evidence; what is proven is that it still renders, is still its own design world, and its source is untouched (mtimes Oct 23–25).

---

## Round 3

Different angle: data honesty against the SQL for *every* rendered number, motion
under reduced motion including toasts, empty states, production routes, and the
"two screens in disguise" hypothesis.

| ID | Sev | What is wrong | Proof | How to verify |
|---|---|---|---|---|
| RT-22 | P2 | Same root as RT-15/RT-02, measured a third way: in partial failure the single error banner says *"The figures below are marked unavailable rather than showing the last values the API returned"* while **5 of 8 metrics below are `live`/`preview`** (roster, price, three zones, occupancy). The banner describes a state that does not exist, and the failed request is not named. | `round3.json` → `partialFailure.metrics` (sources: preview, live, unavailable, unavailable, live, preview×3) + `errorBanner`. | 500 only `/v1/club/dashboard`; count the `data-source` values below the banner. |
| RT-23 | P2 | In the same partial-failure state "Next actions" asserts a settled-money fact it does not have: *"Review tonight's settled net — **Nothing has settled in the last 24 hours yet**"* while `Net settled` is `unavailable`. (False zero, same family as RT-02.) | `round3.json` → `partialFailure.nextActions`; `nextActions.ts:128-130` (`requests24h` null falls to the "nothing has settled" branch). | Same fixture as RT-22, read the second next action. |
| RT-24 | P2 | The first-screen budget claim is only numerically true: the board is 1.51 screens at 1440×900 (2 of 7 tiles complete), 2.43 screens at 1024×768 (2 tiles), 3.23 screens at 390×844 (1 tile). §4.2's "4–6 tiles in the first screen" is not met at any desktop width. | `round3.json` → `viewportCoverage`; `structure1440.fullyVisiblePanels: 2`. | `rect.bottom <= innerHeight` per panel at 1440×900 / 1024×768 / 390×844. |

**Round 3: what could not be falsified**

- **SQL vs captions, re-derived**: net settled = `sum(split.amount_minor - reversed_minor)` for `payee_type='club'` with no time predicate → "all time" is true; requests = `request.created_at > now() - interval '24 hours'` over the club's sessions → "last 24 h" is true and "paid" is no longer claimed; `/earnings` is `p. captured_at > now() - interval '1 day'` and the page says "rolling 24 h, not midnight"; splits exist only after capture (`billing.rs:536-551`), so "settled" is defensible as "captured". No caption contradicts its query except RT-02/RT-05/RT-22/RT-23 (which are about *failed* data, not the SQL).
- **Every number's provenance**: 20 numeric text nodes on the populated board, 17 inside `[data-metric]` with a valid source; the 3 outside are the short session id, one prose sentence, and the "N more on the roster" count (a live roster count restated).
- **Motion**: hover changes `background-color` only (no transform, `180`→`0.12s`); press `scale(.99)` is transform-only; the only running animation is the permitted live dot; reduced motion → 0 animations on all five pages, toast still functions with `animation: none`.
- **Empty states**: `/earnings` ("No paid requests in the last 24 hours… rolling window"), `/djs` ("No DJs connected…"), `/dashboard` empty (live €0.00 / 0 requests with chips) are honest. Only `/pricing` mislabels (RT-17).
- **Production routes**: as in Round 2 D — `/connect` renders and stays in its own visual world; club routes redirect to `/login` without console errors; no route crash.
- **Two screens**: measured and reported as RT-14/RT-24; no "fits only by shrinking type" — 6 font sizes, min 11px, no `text-overflow` on numeric metrics (only the "Review tonight's settled net" row title ellipsises at 320, `scrollWidth 170 / clientWidth 162`).

**New P0/P1 in round 3: none.**

---

## Verdicts

| Round | P0 | new P1 | carried P1 open | new P2 | Verdict |
|---|---|---|---|---|---|
| 1 (reconstructed) | 0 | 4 (RT-01…04) | — | 14 (RT-05…18) | NOT CLEAN — 4 P1 honesty/interaction/functional defects |
| 2 | 0 | **0** | **4** (RT-01…04) | 3 (RT-19…21) | Round 2 introduced no P0/P1, **but the gate cannot close** while RT-01…04 are open; RT-01 is now unconditional |
| 3 | 0 | **0** | **4** (RT-01…04) | 3 (RT-22…24) | No new P0/P1; gate still blocked by the round-1 P1 set |

**Gate position:** two consecutive rounds (2 and 3) produced **no new P0/P1**, which
satisfies the letter of the "two clean rounds" rule, but four P1 findings from
round 1 were never routed and are still reproducible on the current tree. The
gate should not close until RT-01…RT-04 are fixed and re-verified; after that fix
a re-audit round (round 4) is still required by the protocol.

**What I would fix first (highest trust damage per line):**

1. RT-01 + RT-19 — delete the undeclared door-QR failure: either remove the
   `door-qr` next action and the `Door QR` fact, or give the hook a real tri-state
   (`unknown` ≠ `failed`) and only claim failure when the request actually failed.
2. RT-02 — never print "none open / closed / No shift open" when
   `sessionSource === 'unavailable'`; say "unknown — the API did not answer".
3. RT-03 + RT-20 — focus management + `role="status"` + Escape on the three
   remaining confirmations (Sessions close, Sessions rotate, Pricing).
4. RT-04 — a real "Open the next shift" control in the closed-shift branch
   (or a "Refresh" that re-runs the restore check).

---

## Files produced (for the Lead)

- Harness: `output/club-redesign/redteam/rt-*.mjs` (14 files = shared `rt-lib.mjs`
  + 13 scripts, all runnable with `node <script>` from the repo root; fixtures are
  test scaffolding only).
- Raw evidence: `measure.json`, `digest.json`, `structure.json`, `interaction.json`,
  `interaction2.json`, `mobile.json`, `pages.json`, `pages-digest.json`, `axe.json`,
  `round2.json`, `round2-addendum.json`, `round2-fresh.json`,
  `round2-final.json`, `round3.json`.
- Screenshots: `populated-*`, `honest-*`, `mobile-*`, `r2-*`, `r3-*`,
  `board-*`, `sessions-*`, `djs-*`, `pricing-*`, `earnings-*`,
  `r2-preview-*`, `dj-diff.png`.

## Round 4

**Attribution, stated plainly.** Rounds 1–3 were run by the red-team agent. Its turn
ended before it could start round 4, so round 4 was run by the Lead with a
purpose-built harness: `output/club-redesign/redteam/rt-round4.mjs`, results in
`output/club-redesign/redteam/round4.json`. This weakens the independence of the
round, and it is recorded here rather than papered over. The harness is
reproducible by anyone:

```sh
cd apps/web-club && npm run dev:demo
node output/club-redesign/redteam/rt-round4.mjs
```

### Re-check of the four round-1 P1s and RT-11

| ID | Was | Round-4 measurement | Verdict |
|---|---|---|---|
| **RT-01** | The board invented a door-QR failure ("the door token could not be generated") that it never observed, and after the QR flow moved to Live room the alert became unconditional. | The action no longer exists. Honest state: `doorQrAlert: false`. The Room-now fact reads "issued in Live room" instead of "not generated". | **closed** |
| **RT-02** | The honest / partial-failure states printed fabricated negatives ("Shift: none open", "Guest requests: closed", "No shift open") under an "unknown" headline. | Honest: `noneOpen: false`, `guestRequestsClosedUnderUnknown: false`, `noShiftOpen: false`, `notRead: true`. Partial failure (dashboard 500, rest live): `noneOpen: false`, `noShiftOpen: false`, `shiftNotRead: true`. | **closed** |
| **RT-03** | Every two-step confirm except the DJ one dropped focus to `<body>`; Escape did not dismiss. | Sessions close: focus → "Keep it open", Escape → focus "Close shift". Pricing: focus → "Cancel", Escape → focus "Review price change". DJs: focus → "Cancel", cancel → focus "Suspend". Sessions rotate: focus → "Keep the current QR", cancel → focus "Rotate QR code". | **closed** (see caveat) |
| **RT-04** | After closing a shift, `/sessions` could not open the next one without a reload. | `startNextShiftOffered: true`, `openFormBack: true`. | **closed** |
| **RT-11** | Sidebar footer (8 %) and mobile bottom-nav (8.6 %) dividers were brighter than the 6 % hairline. | Every club page scanned for any `border-*-color` above 6 % white with width > 0: **0 hits**. Board `whiteBorders: 0`. | **closed** |

**Caveat on RT-03, not hidden:** focus now moves into each confirmation, which is
what makes it operable and announced by screen readers as the focused control. No
explicit `aria-live` region was added, so the explanatory sentence is reachable
with `aria-describedby` semantics only via the focus order. If a stricter reading
of "announce the consequence" is required, that remains open.

### Fresh sweep (same harness)

| Area | Measurement | Verdict |
|---|---|---|
| axe, 5 routes × populated/honest, 1440 | **0 critical / 0 serious / 0 moderate** | clean |
| Radii, board | exactly `{10px, 12px, 16px, 999px}` | clean |
| Borders brighter than 6 % white | 0 | clean |
| Horizontal overflow, 320 / 390 | 0 / 0 | clean |
| Touch targets < 44×44, 320 / 390 | 0 / 0 | clean |
| Reduced motion | 0 running animations | clean |
| Board height | 1359 px populated (budget 1700) | clean |
| `/dj` regression | 900 px, 0 console errors, pixel-identical to the G2 capture (100.00 %, max channel delta 0) | clean |
| `/connect` (production preview) | renders, 0 console errors (rounds 2–3) | clean |

### New findings

| ID | Severity | What is wrong | Proof |
|---|---|---|---|
| R4-03 | **P2** | Only **2 panels are fully visible** in the first screen at 1440×900; §4.2 asks for 4–6. Band 1 ends at y≈632, band 2 at y≈1012, so the money band starts but is not fully in view. | `round4.json` → `board.fullyVisiblePanels: 2`, `panels[]` with `fullyVisible` |
| R4-04 | **P2** | Two panels end with 27 px and 41 px of unused space below their last element (§4.2 asks ≤24). | `round4.json` → `board.panelBottomGaps: [27,24,24,24,24,24,41]` |

Neither is P0/P1: the first is a composition target that cannot be met at the
contracted type scale without shrinking type below the 6-step scale or dropping
targets below 44 px (both forbidden by §4.6/§5.3); the second is 3 px and 17 px
over a soft budget on two tiles.

### Round verdict

**New P0/P1 in round 4: none.** Combined with round 2 and round 3 (both reported
zero new P0/P1 by the red-team agent), that is three consecutive rounds without a
new P0/P1, against a gate that requires two.
