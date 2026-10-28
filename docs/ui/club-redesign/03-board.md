# G3 — The board: composition, sources, and what was deleted

> Owner: board implementer (G3). Files: `apps/web-club/src/pages/DashboardPage.tsx`
> and `apps/web-club/src/components/club/board/**`.
> Contract: `01-contract.md` §4 (composition) and §7 (data shape).

## 1. Composition

Three bands, seven panels, no band headers. The grid is 12 columns at ≥1280px,
8 at 768–1279, 1 below that.

| Band | Panel | Span | Source of every number |
|---|---|---|---|
| 1 — room state | **Room now** | 8 | session + shift from `GET /v1/club/dashboard` (`live`); floor load summed from the zone preview block (`preview`) |
| 1 | **On the decks** | 4 | `GET /v1/club/djs` (`live`) |
| 2 — money and requests | **Money** | 4 | `earned_total_minor` from `/v1/club/dashboard` — caption **"all time, settled"** (the SQL has no time filter); `requests_today` — caption **"last 24 h"** (the SQL is `now() - interval '24 hours'`, paid or not) |
| 2 | **Guest requests** | 4 | active pricing policy from `GET /v1/club/pricing` (`validTo === null`) |
| 2 | **Floor zones** | 4 | the only `preview` block — no zone/table endpoint exists in migrations 0001–0010 |
| 3 — people and access | **Door & QR** | 4 | real one-time token from `POST /v1/club/sessions/:id/qr/generate`, or an honest CTA to `/sessions` when there is no shift |
| 3 | **Next actions** | 8 | derived from the six panels above; every row links to the page that can actually perform the action |

The page is a composition only: it renders `useClubBoard()` and computes nothing
except the sum of the preview zones.

## 2. Honesty decisions visible on the board

- **Every figure** goes through the `Metric` primitive, which stamps
  `data-metric` + `data-source`. 16 metrics render in the populated state.
- **One preview banner** at the top of the board whenever any metric is
  `preview`. It says the numbers are a local preview, not saved, not from the
  club API.
- **Unavailable is a real state**, not a zero: when a request fails the metric
  shows no value and the word `unavailable`. There is no "stale number labelled
  live" path — `lastFetchedAt` advances only on a successful round-trip.
- **The QR is the real token** or it is absent. The old hand-built
  `https://dropit.app/room/...` payload is gone.
- **"Now playing" is not invented.** The club API does not report which DJ is on
  shift, so the panel shows the roster and says so rather than guessing from
  array order.

## 3. Deleted from the previous page (and why)

| Removed | Reason |
|---|---|
| Crowd-activity chart driven by `Math.random()` | Fabricated telemetry (§5.1.4). No endpoint exists for it. |
| "Floor energy" gauge derived from the same random series | Same. |
| "Guest connections", "Requests tonight", "Tips tonight" as live numbers | Local fixtures presented as telemetry (§5.1.5). |
| "Club net (settled) · last 24 h, from earnings API" label | False on three counts: the number is all-time, it comes from `/v1/club/dashboard`, and no `earnings` endpoint is involved (F1). |
| "Paid requests" label | The query counts every request in 24 h with no payment filter (F2). Relabelled "last 24 h". |
| Music-rules form + "Save policy" toast | Nothing was ever saved (F4, F12); the price lives on `/pricing` and is read from `GET /v1/club/pricing`. |
| "Most active tables" list | Duplicated the floor data and had a status arithmetic bug (F9, F10). |
| Venue-mode switcher that "persisted" a mode | No endpoint, no persistence, and it silently changed other state (F8). |
| "Snapshot Ns ago" ticking counter | Was a 1 s timer, not a network signal (F5). |
| Create-event and manage-table modals | No endpoints; local-only state that vanished on reload. |
| Ten success toasts for unsaved actions | The core trust defect (F4). |
| Hand-built QR URL | Not the backend token (F7). |

## 4. Measured result

| Metric | Before | After |
|---|---|---|
| Height 1440×900 | 2348 px | **1359 px** populated / 1500 honest |
| Height 1024×768 | 2618 px | 1880 / 1984 |
| Height 390×844 | 4952 px | 2741 / 2871 |
| Panels | 8 | 7 |
| Rendered radii | 8 (desktop) | 4 — {10, 12, 16, 999} |
| Bordered elements | 81 (65 nested) | 5, all 1 px dividers ≤6 % white |
| Font sizes | 13 | 6 (11–28 px) |

Measured with `scripts/club-ui-check.mjs`; full numbers in
`02-verification.md` and `output/club-redesign/after/report.json`.

## 5. Abstraction

**The board ships no background abstraction.** The plan was one data-bearing layer
(an occupancy arc encoding the zone-preview ratio), and it was built, measured and
then removed: at 1440×900 its 260px box overlapped 11 text nodes between 11 and
13px — the facts grid, the "67% of capacity" caption, the DJ name. §4.7 forbids
abstraction under text smaller than 14px, and there is no text-free region of that
size inside the panel, so the only compliant form was none. The occupancy `Meter`
(6px, inset track, accent fill) carries the same datum as a first-class component.

The `--abstract-opacity` token remains in `src/styles/club-tokens.css` and is
zeroed under `prefers-reduced-motion`, so a future data-bearing layer has a single
switch. No gradients, glow, grain, glass or animated background exists anywhere on
the board.
