# G4 — The four operational pages: one language, and F13–F16 fixed

> Owner: pages implementer (G4). Files: `src/pages/{Sessions,Djs,Pricing,Earnings}Page.tsx`,
> `src/components/{PageHeading,SectionHeading,StatusBadge}.tsx`.
> Contract: `01-contract.md` §1–§2, §4.3–§4.10, §5; requirements from
> `docs/ui/club-panel-redesign-prompt.md` §5.3.

## 1. Shared language

All four pages now render from the same primitives as the board
(`src/components/club/`): `Panel`, `Tile`, `Metric`, `SourceChip`, `Chip`,
`Segmented`, `ListRow`, `Meter`, `Toast`, `Skeleton`, `EmptyState`,
`ErrorState`. Measured on every page at 1440×900:

| Page | Height | Radii in the rendered DOM | Distinct font sizes | Borders in main |
|---|---|---|---|---|
| `/sessions` | 900 | {10, 12, 16, 999} | 5 | 1 |
| `/djs` | 900 | {10, 12, 16, 999} | 5 | 5 |
| `/pricing` | 900 | {10, 12, 16, 999} | 5 | 2 |
| `/earnings` | 900 | {10, 12, 16, 999} | 5 | 1 |

Before this pass each of these pages used 5–8 px Tailwind radii, `#10B981`/
`#00e887` accents in places, and a different panel vocabulary from the board
(the "two design systems" problem, E1 in the diagnosis). They are now on the same
token layer, with no page-level stylesheet of their own.

## 2. Functional defects fixed

### F13 — Live room (`src/pages/SessionsPage.tsx`)

| Defect | Fix |
|---|---|
| **F13a** the open shift vanished on reload: `session` lived only in `useState` | On mount the page calls `getCurrentClubSession()` (`src/api/client.ts`), which returns `getClubDashboard().session`. The backend's `club_dashboard` SQL selects the newest session with `status <> 'closed'`, so this is the honest way to recover the shift. There is no `GET /v1/club/sessions/:id` on the backend and none was invented. |
| **F13b** closing a shift was one click and irreversible on the backend | Closing is now a two-step inline confirmation that states the action is irreversible and that pending requests expire. No browser `confirm()`. |
| **F13c** QR rotation invalidates the previous code silently | Rotation confirms first, then calls `generateSessionQr()`; the one-time token and `expires_at` are shown. The page never renders a hand-built URL. |
| Async actions could be double-submitted | Every mutating action is disabled while in flight and guarded by a busy flag. |

### F14 — DJs (`src/pages/DjsPage.tsx`)

- Suspend/Restore takes a confirmation and reports the outcome (success toast or
  inline error) instead of silently reloading the list.
- The invitation expiry is the API's `inviteExpiresAt`, formatted — the
  hardcoded "expires in 7 days" is gone.
- "Copied" resets after a timeout instead of staying forever.
- A failed request never renders a success state.

### F15 — Earnings (`src/pages/EarningsPage.tsx`)

- The backend's `today` is a **rolling 24 hours** (`now() - interval '1 day'` in
  `crates/gateway-api/src/panel_routes.rs`), not a calendar day. The page says
  "Last 24 hours" and explains the window in the panel detail.
- Money is formatted with `Intl.NumberFormat` (thousands separators, currency),
  not `toFixed`.
- The net figure is labelled as the club's share after platform commission, and
  the panel states what it is; it does not imply a DJ share is included.
- Empty state when there are no paid requests.

### F16 — Pricing (`src/pages/PricingPage.tsx`)

- Changing the guest price requires a confirmation that shows **the old price and
  the new price side by side**.
- The 1–10 EUR validation is kept.
- The copy that the change applies to new requests only is next to the action,
  not buried.

## 3. What was deliberately not changed

- Routes, route paths, navigation labels and the auth flow are untouched.
- `/connect` was not restyled.
- The DJ console (`/dj`) was not restyled; it keeps its own vocabulary and only
  inherits the single accent through the shared token layer.
- No page gained a local stylesheet: all styling comes from the shared token and
  primitive layer.

## 4. Verified

- `npm run typecheck`, `npm run build`, `npm run build:demo`: exit 0.
- axe on `/sessions`, `/djs`, `/pricing`, `/earnings` at 1440 and 390:
  **0 violations** (baseline had 10 and 6 serious nodes on `/sessions`).
- No console errors on any route.
- Screenshots: `output/club-redesign/after/{sessions,djs,pricing,earnings}-1440.png`.
