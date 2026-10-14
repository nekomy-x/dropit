# @dropit/web-club — Club Panel (C3)

React + TypeScript + Vite + Tailwind client for the DROP IT club panel.
The API client matches the **actual backend contracts implemented through
F8** (docs/openapi.yaml v0.5.0 + `crates/gateway-api` routes) — live
endpoints are really called; everything else is an explicit
"backend not implemented yet" 501 stub.

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

## Waiting for the backend (local 501 stubs, empty state in the UI)

- `GET/PATCH /v1/club/profile` — dashboard page.
- `GET/POST /v1/club/pricing` — pricing page (form is a stub).
- `GET /v1/club/djs`, `POST /v1/club/djs/invite` — DJs page.
- `GET /v1/club/earnings` — earnings page.

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
  App.tsx                # routes: /login /dashboard /sessions /pricing /djs /earnings
                         #         (default redirect → /sessions, the live flow)
  index.css              # Tailwind layers + shared component classes
  api/client.ts          # typed API client: live endpoints + 501 stubs
  hooks/useAsyncData.ts  # fetch hook: loading / error / 401 → /login / retry
  components/            # Layout (sidebar), Loading, ErrorState
  pages/                 # one page per route (EN copy, ES to follow — Q9)
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
