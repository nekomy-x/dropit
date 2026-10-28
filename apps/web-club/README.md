# @dropit/web-club — Club Panel (C3)

React + TypeScript + Vite + Tailwind client for the DROP IT club panel.

## Run modes

- `npm run dev:demo` starts demo mode. `/` opens the DJ console without login; DJ/club actions use local mock/demo state.
- `npm run dev` starts the normal app with production authentication behavior. Direct `/dj` requires DJ login and protected club routes require club login.
- `npm run build` builds the production-auth mode. `npm run build:demo` builds the no-login demo mode.

Demo mode is an explicit Vite mode, not a production authentication bypass. Never deploy the demo build where real club or DJ data is exposed.

The API client matches the **actual backend contracts implemented through
F8** (docs/openapi.yaml v0.5.0 + `crates/gateway-api` routes) — live
endpoints are really called; everything else is an explicit
"backend not implemented yet" 501 stub.

## Open registration (`/register`) — Test Mode stand

Public route, no auth. Contract: `docs/signup-contract.md`.

- **Step 1 — role.** Клуб / DJ as a two-option radio group; `/register?role=dj`
  preselects the DJ role so the DJ login link lands on the right branch.
- **Step 2 — one click.** A generated identity (`demo+<rand>@club.test` /
  `@dj.test`, a 10-char password, a demo name) is shown before submit, then
  posted to `POST /v1/auth/club/register` or `POST /v1/auth/dj/register`.
  The 201 body is a `TokenPair` + created entity, so the tokens are stored
  in memory and the panel opens straight away — `/sessions` for a club,
  `/dj` for a DJ. "Другие данные" regenerates the identity.
- **Step 2 — manual form.** Name, e-mail, password (≥ 8) and, for DJs, an
  optional `club_slug`. "Сгенерировать" fills the fields from the same
  generator.
- **Errors** are mapped from the contract codes and shown inline next to the
  form that produced them: `email_taken`, `validation_failed`,
  `rate_limited`, `club_not_found`. `signup_disabled` (403) replaces the form
  with a standing note that open signup is off on the server, a
  "Попробовать снова" retry and links to both login screens.
- **Deployment strip** (`components/DeploymentStatus.tsx`, canonical
  `packages/ui/DeploymentStatus.tsx`) — one fixed band on the bottom edge of
  every screen of the app (panel, auth, public Connect). It has three states and
  no fourth: `test` (orange «Тестовый режим работы · открытая регистрация ·
  платежи мок»), `maintenance` (red «Технические работы · возможны неполадки и
  перебои в работе сервиса»), and `normal`, which renders nothing at all. It is
  not dismissible: hiding it would let a stand or a degraded service read as
  production. It is the only deployment indicator — there is no corner pill,
  badge or inline footnote saying the same thing anywhere else.
- Tokens stay in memory (project rule), so a page refresh requires a login
  with the generated e-mail; the page says so next to the credentials.

Both login screens link to `/register`. The old "club access is granted after
verification" line on `/login` was replaced by that link — the account is
active immediately on this stand.

`apps/web-dj/index.html` (static mock, served at `/dj-mock/`) keeps its own
inline «Тестовый режим» line under the top bar: it is a design prototype with no
backend, so it does not read `/v1/status`.

## Live functions (backend implemented, real calls)

- **Login (2-step)** — `POST /v1/auth/club/login {email, password}` →
  `TokenPair` or `TotpChallenge {mfa_required: true}`; then
  `POST /v1/auth/club/2fa {email, totp_code}` → `TokenPair`.
- **Silent token rotation** — `POST /v1/auth/refresh` on 401 (one retry;
  reuse detection on the backend revokes the family).
- **Sessions (`/sessions`)** — `POST /v1/club/sessions {timezone, dj_id?,
  pricing_policy_id?}` → `SessionResponse {id, status, timezone, dj_id?}`
  (201); `POST /v1/club/sessions/:id/qr/generate` → `{session_id,
  qr_token, expires_at}`; `PATCH /v1/club/sessions/:id/close` →
  `SessionResponse`.

## Club dashboard (`/dashboard`) — venue board

Seven panels in three bands (room state / money and requests / people and access),
built from the club primitives. Every figure on the board is either read from the
club API or explicitly labelled as a local preview; nothing is invented.

| Module | Responsibility |
| --- | --- |
| `src/dashboard/useClubBoard.ts` | Single data hook: session, net, 24 h requests, request price, DJs, real session QR, plus the preview zone model. Tracks `lastFetchedAt`, one board-wide `error`, and `previewCount`. |
| `src/dashboard/useFocusTrap.ts` | Keeps dialog focus inside and restores it on close. |
| `src/components/club/` | Primitives: `Panel`, `Metric`, `Tile`, `Chip`, `SourceChip`, `Segmented`, `ListRow`, `Meter`, `Toast`, `Skeleton`, `EmptyState`, `ErrorState`, `PreviewBanner`. They stamp the `data-surface` / `data-control` / `data-metric` / `data-source` contract. |
| `src/components/club/board/` | The seven panels (`RoomNowPanel`, `DecksPanel`, `MoneyPanel`, `RequestsPanel`, `ZonesPanel`, `DoorPanel`, `NextActionsPanel`), the occupancy arc, and `board.css`. |
| `src/pages/DashboardPage.tsx` | Composition only: three bands, loading / error / preview states. |

What the board reads, and from where:

| Figure | Source |
| --- | --- |
| Shift state and session id | `GET /v1/club/dashboard` → `session` |
| Requests (last 24 h) | `GET /v1/club/dashboard` → `requests_today` (rolling 24 h window, **not** filtered by payment) |
| Net settled | `GET /v1/club/dashboard` → `earned_total_minor` (all-time sum of club splits, no period filter) — captioned "all time, settled" |
| Request price | `GET /v1/club/pricing` (policy with `valid_to = null`) |
| Lineup | `GET /v1/club/djs` |
| Guest QR | `POST /v1/club/sessions/:id/qr/generate` — only when a session exists |
| Floor zones | **preview** — the backend has no zone/occupancy tables, so these values are local and the board shows a preview banner while any is on screen |

A failed request turns that metric into `unavailable` with no value instead of
showing a stale number, and the board shows one error banner with a retry. There
is no random data, no crowd chart, no energy gauge and no self-incrementing
freshness counter: the "updated N s ago" label is derived from the last
successful fetch and says so when there has not been one.

The board is the one surface with a Bento composition and a background
abstraction (the occupancy arc, switched off by `--abstract-opacity`). The
radius scale is 16/12/10/999, panels have no border, and there is one green
`#00FF87` — see `DESIGN.md` → "Club dashboard exception". The other club screens
(`/sessions`, `/djs`, `/pricing`, `/earnings`) and the DJ console use the same
tokens without the board grid.

Design evidence and acceptance numbers: `docs/ui/club-redesign/` (contract,
baseline, verification, §7.4 before/after table). The acceptance harness is
`scripts/club-ui-check.mjs`.

## Public Connect flow (`/connect`)

The marketing site + club onboarding wizard for club owners. **No auth** —
it is the entry point for a visitor who does not have a panel account yet.

| Route | Screen | What it does |
| --- | --- | --- |
| `/connect` | Landing ("Start") | Hero, mint ticker, benefits, revenue calculator, panel demo, 4 steps, stats, FAQ, final CTA |
| `/connect/club` | Step 01 · Клуб | Club profile: name, city, address, site, contact, party genres |
| `/connect/venue` | Step 02 · Площадка | Venue: name, type, address, city/district, capacity/seats/area |
| `/connect/qr` | Step 03 · QR и тариф | Guest price, zones, QR-kit preview, tariff terms → submit |
| `/connect/connected` | Step 04 · Готово | Application id, what happens next, access & support |

Behaviour:

- **Auto-saved draft** — the three steps write to
  `localStorage['dropit.connect.draft.v1']` 600 ms after the last edit
  (no secrets, so web storage is fine here; auth tokens still live in
  memory only) and the footer bar shows the save time.
- **Validation** — per-step validators in `src/connect/types.ts`
  (`validateClub` / `validateVenue` / `validateQr`): required fields,
  e-mail-or-phone contact, numeric capacity, price range 1–50 €, at least
  one zone, tariff consent.
- **Prefill** — a logged-in owner gets the form prefilled from
  `GET /v1/club/profile` (best effort, failures are ignored).
- **Submit** — `POST /v1/club/onboarding/apply` (`src/api/connect.ts`).
  The endpoint is **not on the backend yet** (only the admin-only
  `POST /v1/admin/clubs` exists), so the call degrades to a 501
  `NotImplementedStubError` and the success screen says honestly that the
  draft stayed in the browser. `TODO(club-onboarding-endpoint)` marks the
  place to drop the stub branch.
- **Money** — the guest price is integer minor units everywhere
  (`priceMinor`), the platform fee is a 0.30 € constant, and formatting
  happens only in `formatEuro()`.

Motion (all CSS/rAF, no animation library; every effect is disabled under
`prefers-reduced-motion`): scroll reveals via `IntersectionObserver`,
marquee ticker, animated calculator counters (`useCountUp`), hover wipes
on benefit rows, floating hero chips + hero parallax, stepper state
transitions, step slide-in, drawn check + pulse ring on the success
screen, accordion grid-rows animation on the FAQ.

## Waiting for the backend (local 501 stubs, empty state in the UI)

- `POST /v1/club/djs/invite` — DJ invite flow (listing itself is live).
- `POST /v1/club/onboarding/apply` — public club application (Connect
  wizard), see above.

Stubs reject locally with `NotImplementedStubError` (code
`not_implemented`, status 501) — no network call is made. Pages render a
"waiting for backend" empty state.

## Auth model

Bearer tokens from the backend (access JWT 15 min + refresh 30 days).
Tokens are held **in memory only** — never `localStorage` (security rule:
no tokens in persistent web storage), so a page refresh requires
re-login.

> TODO(httpOnly-cookie-auth): the backend does not implement httpOnly
> cookie sessions + CSRF yet; when it does, switch the client to
> `credentials: 'include'` + an `X-CSRF-Token` header from
> `<meta name="csrf-token">` and drop the in-memory Bearer store.

Errors are parsed into the `ErrorEnvelope {code, message, details}`
contract (`crates/common/src/error.rs`) — codes are stable i18n keys
(`session_closed`, `rate_limited`, `forbidden`, …). 401 redirects to
`/login`.

## What is inside

```
src/
  main.tsx               # entry: BrowserRouter + StrictMode
  App.tsx                # routes: /connect/* (public), /login, /register,
                         #         /dj/login, /dj/invite/:token, /dashboard,
                         #         /sessions /pricing /djs /earnings
                         #         (default redirect → /sessions, the live flow)
  index.css              # Tailwind layers + shared component classes
  api/client.ts          # typed API client: live endpoints + 501 stubs
  api/connect.ts         # public onboarding apply + profile prefill
  connect/               # Connect flow logic: draft store, validators,
                         #   money formatting, motion hooks
  components/            # Layout (sidebar), Loading, ErrorState, TestModeNote
  components/connect/    # Connect UI: nav/footer, wizard kit, landing sections
  pages/connect/         # StartPage + the four wizard screens
  pages/                 # one page per panel route (EN copy, ES to follow — Q9)
                         #   RegisterPage is the Test Mode open-signup screen (RU)
```

## Running (requires network — NOT executed in this phase)

```bash
npm install
npm run dev        # http://localhost:5173, proxies /v1 → http://localhost:8080
npm run typecheck  # tsc --noEmit
npm run build      # typecheck + production build to dist/
npm run preview    # serve the built bundle
```

The dev proxy points `/v1` at the local `dropit-gateway-api` (adjust in
`vite.config.ts`).

## Known stubs (explicit per project rule 5)

- **QR generator**: `/sessions` calls the real QR endpoint, but the QR
  image is a placeholder box; the raw `qr_token` is displayed once (the
  server stores only its sha256). The encoder module lands with the
  design pass.
- **Club reference data** (profile/pricing/DJs/earnings): local 501
  stubs, see "Waiting for the backend" above.
- **Stripe onboarding link** on the dashboard is a non-functional anchor.
- Money formatting is a plain `(minor / 100).toFixed(2)` helper; a proper
  localized formatter arrives with i18n (Q9: EN + ES).
