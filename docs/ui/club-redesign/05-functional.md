# G5 — Functional honesty: the data layer, and what was removed

> Owner: functional (G5), tail finished by the finisher.
> Authority: `01-contract.md` §6–§8 and
> `docs/ui/club-panel-redesign-prompt.md` §5.1, §5.2, §5.4, §7.1.
> This note documents the honest-data layer: the `useClubBoard()` contract, the
> deleted fake telemetry (with the file:line where it used to live), the
> `getCurrentClubSession()` decision and its SQL, and the `useAsyncData` change.

## 0. Gate status

| Check | Result |
|---|---|
| `npm run typecheck` | **exit 0** |
| `npm run build` / `npm run build:demo` | **exit 0 / exit 0** |
| `grep -rn "Math.random" apps/web-club/src/dashboard` | **1 line, a comment** (`useClubBoard.ts:23`) |
| orphaned mock modules referenced anywhere | **0** |
| `/dashboard` honest (no backend) | one ErrorState banner, every API metric `unavailable`, zones `preview`, no fabricated number |

## 1. `useClubBoard()` — the contract the board consumes

File: `apps/web-club/src/dashboard/useClubBoard.ts` (form fixed by the Lead in
`01-contract.md` §7; the board renders against it).

Rules the hook enforces:

- **No synthetic series.** No `Math.random()`, no crowd ticker, no
  `floorEnergy`/tips/“guest connections” derived from one. The only series-like
  value is the preview zone list, and it is labelled `preview`.
- **`lastFetchedAt` is set only after a successful `await`.** The board renders
  “updated Ns ago” from it (5s clock, not a 1s ticker); when it is `null` the
  copy is “no successful request yet”. The hook never advances it on a failure.
- **One honest error per board.** `error = { message, code? } | null`; on failure
  every API-backed metric becomes `source: 'unavailable', value: null`. Previous
  values are **not** substituted under a live label (contract §7.1: stale display
  is forbidden).
- **`sessionQr` is the real token** from `generateSessionQr(sessionId)`; the hook
  never builds `dropit.app/room/...`. If there is no session (or the call fails)
  the source is `unavailable` and the tile points at `/sessions`.
- **Preview is explicit and counted.** `zones` is the only preview block (there
  is no zone endpoint); `previewCount > 0` is what makes the board render the
  single `PreviewBanner`.
- **`getCurrentClubSession()`** is the only session lookup used by the board —
  see §3.

The hook is consumed by `src/pages/DashboardPage.tsx` through
`src/components/club/board/*`; every figure is rendered by the `Metric`
primitive, which stamps `data-metric` + `data-source`.

## 2. What was deleted, and why

The board used to be a frontend-only simulation. The following modules were
**orphaned** after the board rewrite (grep-proven: zero imports) and were deleted
in the finisher pass:

| File | What it contained |
|---|---|
| `src/dashboard/useVenueState.ts` | the whole mutable “venue”: events, DJ shifts, zones, tables, music rules, QR switches, toast queue, clock/freshness ticker |
| `src/dashboard/mock.ts` | fixtures: `VENUE_NAME`/`VENUE_ROOM` (:19–20), `INITIAL_ZONES` (:22), `INITIAL_DJS` (:52), `INITIAL_EVENTS` (:59), `INITIAL_TABLES` (:98), `INITIAL_RULES` (:107), `GENRE_LIBRARY` (:116), `INITIAL_CROWD` (:130), `INITIAL_PULSE` (:141) |
| `src/dashboard/types.ts` | the local venue model (`VenueMode`, `VenueZone`, `VenueTable`, `MusicRules`, `CrowdPoint`, `VenuePulse`, …) |
| `src/dashboard/format.ts` | `formatEuro`, `formatCount`, `formatMinutes`, `VENUE_MODE_LABEL` |

`src/dashboard/useFocusTrap.ts` was **kept** (grep: it has no importers today,
but it is a shared utility the shell may use; deleting it would remove working
code outside this task). `src/dashboard/useClubBoard.ts` is the only board hook.

### The fake telemetry that used to be there

File:line below refer to the pre-cleanup tree (the modules are now deleted; the
citations come from `docs/ui/club-panel-redesign-prompt.md` §3.3, which recorded
them before removal).

| # | Fake behaviour | Where it used to be |
|---|---|---|
| F3 | Venue mode, zones, tables, DJs, QR, events and rules lived in `useState` — no network, no persistence | `useVenueState.ts:49–65` |
| F4 | **Ten success toasts for actions that were never saved** — including “Music policy saved for tonight” | `useVenueState.ts:162, 178, 213, 231, 247, 259, 272, 286, 319, 324` |
| F5 | “Snapshot Ns ago” ticker: `+1` every second, reset every 30s, announced by `aria-live` every second — not tied to any request | `useVenueState.ts:57, 83–89` |
| F6 | Random crowd chart (`Math.random()` every 15s) presented as “15 min buckets / last two hours”; “guest connections” and “floor energy” derived from it | `useVenueState.ts:93–111` (`Math.random()` at **:98**), consumed in the old `DashboardPage.tsx:48–50, 68` |
| F8 | Hidden side effects: “Take over” silently set the room to `peak_hours`; “Go live” set `doors_open` and demoted another live event to `scheduled` | `useVenueState.ts:161, 174, 177` |
| F9 | “N left in the set” computed as `minutesOf(end) − minutesOf(clock)` — broken across midnight, never rendered on current data | `useVenueState.ts:345–352`; `mock.ts:53` |
| F10 | Table arithmetic: `held` counted in neither “free” nor “booked or seated” → “5 of 6” with six tables | `useVenueState.ts:132–133` |
| F11 | Hard-coded venue identity “Club Nebula” / “Main room”, “Doors opened 22:00” | `mock.ts:19–20, 144` |
| F12 | “Save policy” only toasted; rules mutated on click and were lost on reload | `useVenueState.ts:293–320` |
| F7 | Board QR was a hand-built `dropit.app/room/...` URL; rotation silently un-paused a table. The real token lives on `/sessions` | old `DashboardPage.tsx:162–164, 279–287` |
| F1 | “Club net (settled) · last 24 h, from earnings API” — the number is all-time and comes from `/v1/club/dashboard` | old `DashboardPage.tsx:396–399`; `api/client.ts:461–469` |
| F2 | “Paid requests” — actually all requests in 24h, no payment filter | old `DashboardPage.tsx:402–407` |
| F17 | Toast lifetime 3.6s, no hover/focus pause, no undo, double screen-reader announcement, ~18px close button | `useVenueState.ts:35, 68–75`; old `DashboardPage.tsx:683–698` |

Also deleted with the modules: `toggleTableQr` / `setTableStatus`
(`useVenueState.ts:234–251`; contract §9.7 — zero callers), `saveRules`
(`:318–320`) and `setMode` (`:322–325`).

The new board keeps only what has a real source or an explicit `preview` label:
`getClubDashboard().session`, `requests_today`, `earned_total_minor`,
`getDjs()`, `getPricing()`, `generateSessionQr()` and the local zone preview.
There is no second source of truth for any number.

Grep proof after deletion:

```sh
grep -rn "useVenueState\|dashboard/mock\|dashboard/types\|dashboard/format\|VENUE_NAME\|VENUE_ROOM\|GENRE_LIBRARY" \
  apps/web-club/src --include=*.ts --include=*.tsx     # 0
grep -rn "Math.random" apps/web-club/src/dashboard     # only useClubBoard.ts:23 (comment)
```

## 3. `getCurrentClubSession()` — why the dashboard is the source

File: `src/api/client.ts:591–594`.

```ts
export async function getCurrentClubSession(): Promise<{ id: string; status: SessionStatus } | null> {
  const dashboard = await getClubDashboard();
  return dashboard.session;
}
```

Reasoning:

1. **There is no `GET /v1/club/sessions` list route.** `sessions_routes.rs` exposes
   `POST /v1/club/sessions`, `POST …/:id/qr/generate|activate|deactivate` and
   `PATCH …/:id/close` — but no list/current endpoint for the club role.
2. **`GET /v1/sessions/current` is guest-scoped.** It authenticates a guest
   device token or a guest JWT and returns `404` otherwise; a club Bearer would
   be rejected (`403`). It also filters to `status IN ('active',
   'paused_acceptance') AND opened_at IS NOT NULL`, which would hide a shift that
   exists but has not been opened yet.
3. **`GET /v1/club/dashboard` already returns exactly the shift the panel needs.**
   The handler (`panel_routes.rs:171–212`) selects it in a CTE:

```sql
WITH s AS (
  SELECT id, status
  FROM session
  WHERE club_id = $1 AND status <> 'closed'
  ORDER BY created_at DESC
  LIMIT 1
)
SELECT (SELECT id FROM s),
       (SELECT status FROM s),
       (SELECT count(*) FROM request r JOIN session ss ON ss.id = r.session_id
         WHERE ss.club_id = $1 AND r.created_at > now() - interval '24 hours'),
       (SELECT COALESCE(sum(sp.amount_minor - sp.reversed_minor), 0)::bigint
          FROM split sp
         WHERE sp.payee_type = 'club' AND sp.payee_id = $1)
```

   `$1` is `ctx.subject_id()` — the club id from the Bearer token.

So “current session” in the club panel means **the latest non-closed session of
this club** (`created`, `active`, `paused_acceptance` all count; `closed` never
does). That matches the `BoardSession.status` union in contract §7 and keeps
“shift exists but is not open yet” visible instead of turning it into “no
session”. The trade-off is explicit: the dashboard response carries only
`{id, status}`, so callers must not expect a full `SessionResponse` (no
`timezone`, no `dj_id`). The board only needs `{id, status}` — the id drives the
`generateSessionQr(sessionId)` call, and the status drives the headline.

## 4. `useAsyncData` — additive honesty (F18)

File: `src/hooks/useAsyncData.ts`.

The old hook kept `previous.data` on error and returned only
`{data, loading, error, reload}`. A page could not distinguish:

- “there is no data now” (`data === null`), from
- “we are showing values from an earlier successful fetch” (`data !== null`).

The finisher pass added two **additive** fields. Existing call sites
(`SessionsPage`, `DjsPage`, `PricingPage`, `EarningsPage`, and any page written
later) compile and behave exactly as before; `data`, `loading`, `error` and
`reload` semantics are unchanged.

```ts
export interface AsyncState<T> {
  data: T | null;
  loading: boolean;
  error: { message: string; code?: string; notImplemented?: boolean } | null;

  /** ms epoch of the latest failed fetch; null while the latest attempt succeeded. */
  failedAt: number | null;

  /**
   * ms epoch of the successful fetch that produced the `data` currently on
   * screen; set only while the latest attempt failed.
   * `data === null` + `staleSince === null` = "no data now".
   * `data !== null` + `staleSince !== null` = "this is old data".
   */
  staleSince: number | null;
}

export interface AsyncDataResult<T> extends AsyncState<T> {
  reload: () => void;
}
```

Behaviour:

- success → `failedAt: null`, `staleSince: null`, `lastFetchedAt` ref updated;
- failure → `failedAt: Date.now()`, `staleSince` = timestamp of the last
  successful fetch if `data !== null`, otherwise `null`; `data` is still
  preserved for callers that want it, but the page can now say “showing data
  from HH:MM” instead of presenting it as current;
- starting a retry keeps the two fields until the outcome is known, so a page
  that renders “showing data from …” does not flip back to “live” while the
  request is in flight;
- `401` still redirects to `/login` before any state write; polling cadence is
  unchanged (timer scheduled only after completion, no overlap).

Typecheck proof: `npm run typecheck` exit 0 with all existing call sites
unchanged (`grep -rn "useAsyncData<" src` → 5 hooks in 4 files:
`SessionsPage` ×2, `DjsPage`, `PricingPage`, `EarningsPage`).

## 5. What is still intentionally honest-but-empty in demo

`npm run dev:demo` has no backend (`localhost:8081` is not running), so
`/v1/club/**` returns 500. The board then renders one ErrorState banner, marks
API metrics `unavailable`, and keeps the zone preview labelled. That is the
contract §8 “honest” state, not a defect. The “populated” screenshots in
`output/club-redesign/cleanup/populated-*.png` are produced by Playwright route
fixtures inside the test harness — they are test scaffolding, not a product
mock.
