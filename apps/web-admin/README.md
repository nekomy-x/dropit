# @dropit/web-admin — DROP IT Admin Panel (C4)

React + TypeScript + Vite + Tailwind client for the DROP IT admin panel.

## Run modes

- `npm run dev:demo` opens the local-first Control Center without sign-in. All sections use demo/local state and never call live mutation endpoints.
- `npm run dev` uses production authentication behavior. Every admin route requires an admin session with mandatory 2FA.
- `npm run build` builds production-auth mode. `npm run build:demo` builds the local-only preview mode.

Never deploy the demo build where real platform data is available.

## Deployment path — served under `/admin/`

On the production origin the admin panel is served next to the club panel:

| URL | App |
| --- | --- |
| `http://HOST/` | club panel (`apps/web-club`) |
| `http://HOST/admin/` | admin panel (this app) |
| `http://HOST/dj/` | static DJ console mock (`apps/web-dj`) |

How the base path is wired:

- `vite.config.ts` sets `base: '/admin/'`, so every emitted asset is
  `/admin/assets/...` and the favicon resolves to `/admin/favicon.svg`.
  `dist/` is uploaded as-is; nginx maps `location /admin/` to it and falls
  back to `index.html` for the SPA routes (e.g. `/admin/login`).
- `src/main.tsx` derives the router basename from
  `import.meta.env.BASE_URL` (`/admin/` → `/admin`, `/` during `vite dev`),
  so the same bundle works both locally and under the prefix.
- `src/vite-env.d.ts` pulls in `vite/client` for `import.meta.env` typing.
- Internal navigation uses router `navigate`/`Link` rather than absolute
  `window.location.assign('/admin#...')`, so hash sections stay under the
  basename.
- API calls are unchanged: `request()` fetches `/v1/...` from the origin
  root, which nginx proxies to the Rust gateway. Do not prefix them with
  `/admin`.

`npm run dev` still serves at `http://localhost:5174/admin/` because of
`base`. Admin login is `http://HOST/admin/login`; it is a separate app from
the club panel and keeps its own mandatory-2FA flow.

The API client matches the **actual backend contracts implemented through
F8** (docs/openapi.yaml v0.5.0 + `crates/gateway-api` routes) — live
endpoints are really called; everything else is an explicit
"backend not implemented yet" 501 stub.

## Live functions (backend implemented, real calls)

- **Login (2-step, 2FA mandatory)** — `POST /v1/auth/admin/login
  {email, password}` → `TotpChallenge {mfa_required: true}`; then
  `POST /v1/auth/admin/2fa {email, totp_code}` → `TokenPair`.
- **Silent token rotation** — `POST /v1/auth/refresh` on 401 (one retry;
  reuse detection on the backend revokes the family).
- **Payouts (`/payouts`)** — `GET /v1/admin/payouts` → `{items: [{id,
  payee_type, payee_id, amount_minor, status, attempts, is_final}]}`;
  `POST /v1/admin/payouts/run` → `{payouts_created, total_minor}`
  (Run cycle button); `POST /v1/admin/reconciliation/run` →
  `{run_id, status}`.
- **Incidents (`/incidents`)** — `GET /v1/admin/incidents` →
  `{items: [{id, kind, message}]}` (open incidents only);
  `POST /v1/admin/incidents/:id/resolve` → `{resolved: true}`.

These two pages are the only fully live admin surfaces and are marked
"live" in the sidebar.

## Waiting for the backend (local 501 stubs, empty state in the UI)

- `GET /v1/admin/dashboard/now` — dashboard page.
- CRUD `/v1/admin/clubs` — clubs page (create form is a stub).
- `GET /v1/admin/djs`, `GET /v1/admin/guests`.
- `GET /v1/admin/payments` (F5 implements the guest confirm flow and
  Stripe webhooks, not the admin read endpoint).
- `GET /v1/admin/audit-log`.
- `GET/PATCH /v1/admin/platform-settings`.

Stubs reject locally with `NotImplementedStubError` (code
`not_implemented`, status 501) — no network call is made. Pages render a
"waiting for backend" empty state.

## Auth model

Bearer tokens from the backend (access JWT 15 min + refresh 30 days).
Admin 2FA is mandatory. Tokens are held **in memory only** — never
`localStorage` (security rule: no tokens in persistent web storage), so
a page refresh requires re-login.

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
  main.tsx               # entry: BrowserRouter basename from BASE_URL (/admin)
  App.tsx                # routes: /login /dashboard /clubs /djs /guests
                         #         /payments /payouts /incidents /audit-log
                         #         /platform-settings
                         #         (default redirect → /payouts, the live surface)
  vite-env.d.ts          # vite/client types for import.meta.env
  index.css              # Tailwind layers + shared component classes
  api/client.ts          # typed API client: live endpoints + 501 stubs
  hooks/useAsyncData.ts  # fetch hook: loading / error / 401 → /login / retry
  components/            # Layout (grouped sidebar, "live" badges), Loading, ErrorState
  pages/                 # one page per route (EN copy, ES to follow — Q9)
```

## Running (requires network — NOT executed in this phase)

```bash
npm install
npm run dev        # http://localhost:5174, proxies /v1 → http://localhost:8080
npm run typecheck  # tsc --noEmit
npm run build      # typecheck + production build to dist/
npm run preview    # serve the built bundle
```

The dev proxy points `/v1` at the local `dropit-gateway-api` (adjust in
`vite.config.ts`).

## Known stubs (explicit per project rule 5)

- **Admin reference data** (dashboard/clubs/DJs/guests/payments
  read/audit-log/platform-settings): local 501 stubs, see "Waiting for
  the backend" above.
- **Guest/DJ block & suspend actions**: read-only until F8 (anti-fraud).
- **Payout provider**: transfers are mock (Test Mode) until ADR-004 —
  the payouts UI reflects real DB state either way.
- Money formatting is a plain `(minor / 100).toFixed(2)` helper; a proper
  localized formatter arrives with i18n (Q9: EN + ES).
