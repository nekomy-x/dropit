# @dropit/web-admin — DROP IT Admin Panel (C4)

React + TypeScript + Vite + Tailwind client for the DROP IT admin panel.
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
  main.tsx               # entry: BrowserRouter + StrictMode
  App.tsx                # routes: /login /dashboard /clubs /djs /guests
                         #         /payments /payouts /incidents /audit-log
                         #         /platform-settings
                         #         (default redirect → /payouts, the live surface)
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
