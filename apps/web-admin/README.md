# @dropit/web-admin — DROP IT Admin Panel (C4)

React + TypeScript + Vite + Tailwind skeleton for the DROP IT admin panel
(phase **F7**). Routes, typed API client, loading/error states and
401 → `/login` handling are real; data arrives when the backend phases
F1/F2/F5 are ready.

## What is inside

```
src/
  main.tsx               # entry: BrowserRouter + StrictMode
  App.tsx                # routes: /login /dashboard /clubs /djs /guests
                         #         /payments /incidents /audit-log
                         #         /platform-settings
  index.css              # Tailwind layers + shared component classes
  api/client.ts          # typed API client (httpOnly cookie + X-CSRF-Token)
  hooks/useAsyncData.ts  # fetch hook: loading / error / 401 → /login / retry
  components/            # Layout (grouped sidebar), Loading, ErrorState
  pages/                 # one page per route (EN copy, ES to follow — Q9)
```

## Auth model

Session = **httpOnly cookie** + CSRF token read from
`<meta name="csrf-token">` (tech map Part I §3). Admin accounts require
mandatory 2FA — the TOTP screen ships with the F1 backend; the login page
already handles the `otpRequired` response flag.

## Running (requires network — NOT executed in this phase)

```bash
npm install
npm run dev        # http://localhost:5174, proxies /v1 → http://localhost:8080
npm run typecheck  # tsc --noEmit
npm run build      # typecheck + production build to dist/
npm run preview    # serve the built bundle
```

## API contract used (tech map Part IV, admin)

- `POST /v1/auth/admin/login`
- `GET /v1/admin/dashboard/now`
- `GET/POST /v1/admin/clubs`, `GET/PATCH /v1/admin/clubs/:id`
- `GET /v1/admin/djs`, `GET /v1/admin/guests`
- `GET /v1/admin/payments?filters`
- `GET /v1/admin/incidents`, `POST /v1/admin/incidents/:id/resolve`
- `GET /v1/admin/audit-log`
- `GET/PATCH /v1/admin/platform-settings`

## Platform settings validation

The form validates client-side before PATCHing (API re-validates):

- **basis-point fields** (commission percent, DJ share): integer in
  `0..10000` (10000 = 100%);
- **money fields**: non-negative integers in cents (ADR-003 — no floats);
- **min price < max price** (strictly);
- commission mode is exclusive: percent uses `commissionPercentBp`,
  fixed uses `commissionFixedMinor`.

## Known stubs (to be replaced in F7 proper)

- **TOTP screen**: login detects `otpRequired` and shows a note (F1).
- **Guest/DJ block & suspend actions**: read-only tables until F8
  (anti-fraud phase).
- **Payment split drill-down** (`GET /v1/admin/payments/:id/splits`):
  reserved, lands with the F5 ledger.
- **Club delete** is sent as a status action stub (soft actions: the
  project uses status fields, not deletions).
- Money formatting is a plain `(minor / 100).toFixed(2)` helper; a proper
  localized formatter arrives with i18n (Q9: EN + ES).

Stubs are marked in code comments per project rule 5 (mock/stub must be
explicit).
