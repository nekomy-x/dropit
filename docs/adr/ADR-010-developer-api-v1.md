# ADR-010 — Developer API v1, scoped API keys, documentation system

**Status:** accepted (implementation in progress)
**Date:** 2026-10-28
**Owners:** platform/backend + club panel

## Context

Clubs and their developers need to automate the operational loop around a night:
open a session, place and moderate music requests, watch the queue, pull the
night's numbers. Today every one of those actions exists only inside the
internal panel API (`/v1/**`, JWT role auth, session tokens in memory), which
is a browser surface: it is not versioned for external consumers, has no
machine credentials, no per-consumer rate limit and no contract a third party
can build against.

The request: a first, honest **public API v1** for developers and clubs, a
**key/access setting in the profile** whose permissions are exactly what it
grants (a key that may read the docs must not read the club's revenue), and
**full documentation** for the functions that ship.

## Decision

### 1. Surface and namespace

| Surface | Base path | Auth | Contract file |
| --- | --- | --- | --- |
| Internal panel API | `/v1/**` | JWT (club/dj/admin session) | `docs/openapi.yaml` |
| **Developer API v1** | **`/v1/developer/**`** | **API key** | **`docs/api/openapi-developer.yaml`** |
| Panel key management | `/v1/club/api-keys/**` | JWT club | `docs/openapi.yaml` |
| Panel admin key oversight | `/v1/admin/api-keys/**` | JWT admin | `docs/openapi.yaml` |
| Panel developer docs | `/v1/club/developers/**` | JWT club | `docs/openapi.yaml` |

The developer API lives in the same binary (one deployable, one nginx) but is a
separate trust boundary: different authentication, different rate limit,
different response filtering, its own OpenAPI document. The internal contract
test now unions both specs, so neither can drift from the code.

### 2. Credentials

Format: `dit_<env>_<prefix>_<secret>`, e.g.
`dit_live_k4m2p9x7qa_9f8e...` (10 hex prefix + 64 hex secret = 256-bit secret).

* `env` — `live` or `test`.
* `prefix` — public lookup id; safe to show in lists, logs and support tickets.
* `secret` — generated once, returned once in the creation/rotation response
  (`Cache-Control: no-store`), never stored in plaintext.

At rest only `sha256(plaintext)` (hex) is stored, plus the prefix. Verification
is `prefix` lookup + constant-time comparison of the digest. A 256-bit random
secret does not need a password KDF; a leaked database yields no usable key.

Two transports, identical semantics:
`Authorization: Bearer dit_...` and `X-API-Key: dit_...`.

### 3. Scopes — deny by default, no wildcard

A key carries an explicit, ordered set of scopes. There is no `*`, no implicit
admin scope, and a key can never act on another club's data. Unknown scopes are
rejected at creation. Scopes can be **narrowed** on an existing key but never
widened; widening means creating a new key (documented in the panel).

| Scope | Grants |
| --- | --- |
| `docs:read` | developer documentation manifest/sections/openapi endpoints |
| `profile:read` | club profile (name, slug, address, country, phone, logo, status, timezone) |
| `sessions:read` | list/inspect sessions |
| `sessions:write` | create a session, close a session |
| `qr:write` | issue/rotate the session QR token (guest entry) |
| `requests:read` | session queue, request detail, club-wide request list |
| `requests:write` | create a request, accept/reject/played |
| `catalog:read` | catalog search |
| `stats:read` | aggregated overview (sessions, requests, revenue, top tracks) |

Presets shown by the panel: *Docs only* (`docs:read`), *Read-only integration*
(`docs:read`, `profile:read`, `sessions:read`, `requests:read`, `catalog:read`,
`stats:read`), *Queue automation* (read-only + `requests:write`), *Full
automation* (all nine).

### 4. Developer API v1 — resources

Every response is scoped to the key's club. A foreign resource id answers
`404` (not `403`) so ids cannot be enumerated across tenants.

```
GET    /v1/developer/me                        (key identity, no scope)
GET    /v1/developer/docs                      docs:read
GET    /v1/developer/docs/{section}            docs:read
GET    /v1/developer/openapi.json              docs:read (paths filtered by the key's scopes)
GET    /v1/developer/profile                   profile:read
GET    /v1/developer/sessions                  sessions:read
POST   /v1/developer/sessions                  sessions:write
GET    /v1/developer/sessions/{id}             sessions:read
POST   /v1/developer/sessions/{id}/close       sessions:write
POST   /v1/developer/sessions/{id}/qr          qr:write
GET    /v1/developer/sessions/{id}/queue       requests:read
POST   /v1/developer/sessions/{id}/requests    requests:write   (Idempotency-Key required)
GET    /v1/developer/requests                  requests:read
GET    /v1/developer/requests/{id}             requests:read
POST   /v1/developer/requests/{id}/accept      requests:write
POST   /v1/developer/requests/{id}/reject      requests:write
POST   /v1/developer/requests/{id}/played      requests:write
GET    /v1/developer/catalog/search            catalog:read
GET    /v1/developer/stats/overview            stats:read
```

Conventions: cursor pagination (`?limit=&cursor=`), `{ "items": [...],
"next_cursor": null }` for lists, ISO-8601 UTC timestamps, integer minor units
for money, existing `{code,message,details}` error envelope, `Retry-After` and
`X-RateLimit-*` on throttling.

Requests created through the API are attributed to an internal synthetic guest
device per API key (the domain requires a guest identity). They behave exactly
like guest requests in the queue and the DJ console; payment is out of scope for
v1 (documented). `Idempotency-Key` is required on request creation and replays
return the original response.

### 5. Key lifecycle (panel)

`POST /v1/club/api-keys` returns the plaintext once. Keys support rename, scope
narrowing, IP allowlist (exact IP or CIDR, v4/v6), per-key rate limit
(30–600 rpm), expiry (`1..365` days or never), **rotation** (new secret, old key
stays valid for `grace_hours` 0–72, default 24) and immediate **revocation**
(204). Limit: 25 active keys per club. Every create/rotate/revoke/narrow writes
to `audit_log` (no secret material). Admin sees all keys read-only and can
revoke any key.

### 6. Rate limiting and audit

Sliding-ish fixed window per key over `api_key_usage` (last 60 s), default
120/min (`DROPIT_API_DEFAULT_RATE_PER_MIN`, clamp 30–600). Exceeding the limit
returns `429 rate_limited` with `Retry-After: 60`; the panel also surfaces
usage. Every authenticated developer call records method, path, status and time
(30-day retention; purged by the retention job). The raw key is never logged —
only the prefix. Authentication failures answer `401 invalid_api_key` without
distinguishing "wrong secret" from "unknown prefix".

### 7. Environment separation

`test` keys are honest about being test keys: on a deployment whose payment mode
is live, a `test` key is refused with `403 test_key_not_allowed`. On the Test
Mode stand both kinds work and the docs say so. Keys are also refused when the
owning club is not `active` (`403 club_suspended`).

### 8. Documentation system

Three layers, one source of truth:

1. `docs/api/openapi-developer.yaml` — hand-written OpenAPI 3.1 for the public
   surface, every operation carries `x-required-scope`, examples and every
   error response. Served as JSON at `/v1/developer/openapi.json` (filtered by
   the key's scopes) and `/v1/club/developers/openapi.json` (whole, for the
   panel).
2. `docs/api/sections/*.md` — long-form guides with YAML front matter
   (`id`, `title`, `summary`, `requires`, `order`). Compiled into the binary by
   `node scripts/build-api-docs.mjs` → `crates/gateway-api/src/developer_docs_content.rs`.
   Served by `/v1/developer/docs[/{section}]` filtered by scope and by the panel
   docs view unfiltered.
3. `node scripts/api-docs-check.mjs` — gate: front matter valid, scopes known,
   every developer path covered by a guide, every `x-required-scope` present
   and known, generated Rust is up to date.

The panel `/developers` page renders keys, the reference (from the OpenAPI
JSON) and the guides, so the documentation is reachable where the key is
created. "Docs access" is therefore a real scope, not a marketing promise.

### 9. Security invariants (tested)

* keys are never returned by list/get endpoints and never appear in logs;
* a key with `docs:read` only receives `403 insufficient_scope` on every data
  endpoint (route matrix test);
* a key of club A cannot read club B's session/request by id (`404`);
* revocation and expiry are effective on the next request;
* IP allowlist fails closed (invalid rule denies);
* rotation grace is bounded and old secrets die at `expires_at`;
* `test` key on live deployment is refused;
* rate limit is per key, not per club, and answers with headers.

### 10. Out of scope for v1 (documented as such)

Outbound webhooks, payment creation/refunds, guest PII (never exposed), DJ
accounts issuing keys, OAuth2 client-credentials flow. The API is additive:
`v1` grows without breaking; deprecations keep a 6-month window, announced in
`docs/api/sections/changelog.md`.

## Consequences

* One more auth path in the gateway; it is isolated in `api_key_auth.rs` and
  does not touch JWT/extractor behaviour.
* One more table pair (`api_key`, `api_key_usage`) and one migration.
* The contract test reads two specs, so the public contract cannot silently
  drift from the code.
* Documentation is compiled into the binary — the deployed docs are the docs
  the binary was built from.