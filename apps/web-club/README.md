# @dropit/web-club — Club Panel (C3)

React + TypeScript + Vite + Tailwind skeleton for the DROP IT club panel
(phase **F7**). Everything is wired to the REST contract from the technical
map (Part IV): routes, API client, loading/error states and 401 → `/login`
handling are real; the data itself arrives when the backend phases F1/F2 are
ready.

## What is inside

```
src/
  main.tsx               # entry: BrowserRouter + StrictMode
  App.tsx                # routes: /login /dashboard /sessions /pricing /djs /earnings
  index.css              # Tailwind layers + shared component classes
  api/client.ts          # typed API client (httpOnly cookie + X-CSRF-Token)
  hooks/useAsyncData.ts  # fetch hook: loading / error / 401 → /login / retry
  components/            # Layout (sidebar), Loading, ErrorState
  pages/                 # one page per route (EN copy, ES to follow — Q9)
```

## Auth model

Session = **httpOnly cookie** + CSRF token read from
`<meta name="csrf-token">` (tech map Part I §3). Every mutating request
(POST/PATCH) sends `X-CSRF-Token`; money-carrying POSTs also send
`Idempotency-Key` (generated with `crypto.randomUUID()`).

## Running (requires network — NOT executed in this phase)

```bash
npm install
npm run dev        # http://localhost:5173, proxies /v1 → http://localhost:8080
npm run typecheck  # tsc --noEmit
npm run build      # typecheck + production build to dist/
npm run preview    # serve the built bundle
```

The dev proxy points `/v1` at the local `dropit-gateway-api` (adjust in
`vite.config.ts`). Until F2 lands, pages will show their error state —
that is expected for a skeleton.

## API contract used (tech map Part IV, club)

- `POST /v1/auth/club/login`
- `GET/PATCH /v1/club/profile`
- `GET/POST /v1/club/pricing` (new price = new policy row, Q6)
- `POST /v1/club/sessions`, `POST /v1/club/sessions/:id/qr/generate`
- `GET /v1/club/djs`, `POST /v1/club/djs/invite`
- `GET /v1/club/earnings?period=`

## Known stubs (to be replaced in F7 proper)

- **QR generator**: the button calls the real endpoint, but the QR image is
  a placeholder box — the encoder module lands with the design pass.
- **TOTP screen**: login detects `otpRequired` and shows a note; the actual
  2FA flow ships with F1 backend.
- **Stripe onboarding link** on the dashboard is a non-functional anchor.
- Money formatting is a plain `(minor / 100).toFixed(2)` helper; a proper
  localized formatter arrives with i18n (Q9: EN + ES).

Stubs are marked in code comments per project rule 5 (mock/stub must be
explicit).
