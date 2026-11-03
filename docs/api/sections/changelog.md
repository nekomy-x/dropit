---
id: changelog
title: Changelog
summary: Releases of the public Developer API, starting with 1.0.0 on 2026-10-28.
requires: []
order: 160
---

# Changelog

Every release that changes the public contract gets an entry here. The format
follows Keep a Changelog; the compatibility rules are in **Versioning**.

## Release format

```
## <version> — <YYYY-MM-DD>

### Added
- <new endpoint, field or capability, with the scope it needs>

### Deprecated
- <what stops working, and the earliest date it can be removed>

### Removed
- <what no longer answers; only allowed in a new major version>

### Security
- <changes to authentication, storage or permission checks>
```

Dates are UTC. A deprecation entry repeats the planned removal date; the
minimum notice is six months.

## 1.0.0 — 2026-10-28

First public release of the Developer API.

### Added

**Credentials and access**

* Scoped API keys in the club panel: `dit_live_…` / `dit_test_…`, prefix plus
  256-bit secret, one-time reveal, SHA-256 storage.
* Nine scopes: `docs:read`, `profile:read`, `sessions:read`, `sessions:write`,
  `qr:write`, `requests:read`, `requests:write`, `catalog:read`, `stats:read`.
* Four presets in the panel: Documentation only, Read-only integration, Queue
  automation, Full automation.
* Key lifecycle: rename, scope narrowing, IP allowlist (exact IP or CIDR,
  IPv4/IPv6), per-key rate limit (30–600 rpm), expiry (1–365 days or never),
  rotation with a 0–72 hour grace window, immediate revocation.
* Up to 25 active keys per club.
* Per-key rolling-window rate limit, 60 seconds, default 120 rpm, with
  `X-RateLimit-Limit`, `X-RateLimit-Remaining`, `X-RateLimit-Reset` and
  `Retry-After`.
* 30-day usage journal per key (method, path, status, time).

**Endpoints**

| Method | Path | Scope |
| --- | --- | --- |
| GET | `/v1/developer/me` | none |
| GET | `/v1/developer/docs` | `docs:read` |
| GET | `/v1/developer/docs/{section}` | `docs:read` |
| GET | `/v1/developer/openapi.json` | `docs:read` |
| GET | `/v1/developer/profile` | `profile:read` |
| GET | `/v1/developer/sessions` | `sessions:read` |
| POST | `/v1/developer/sessions` | `sessions:write` |
| GET | `/v1/developer/sessions/{id}` | `sessions:read` |
| POST | `/v1/developer/sessions/{id}/close` | `sessions:write` |
| POST | `/v1/developer/sessions/{id}/qr` | `qr:write` |
| GET | `/v1/developer/sessions/{id}/queue` | `requests:read` |
| POST | `/v1/developer/sessions/{id}/requests` | `requests:write` |
| GET | `/v1/developer/requests` | `requests:read` |
| GET | `/v1/developer/requests/{id}` | `requests:read` |
| POST | `/v1/developer/requests/{id}/accept` | `requests:write` |
| POST | `/v1/developer/requests/{id}/reject` | `requests:write` |
| POST | `/v1/developer/requests/{id}/played` | `requests:write` |
| GET | `/v1/developer/catalog/search` | `catalog:read` |
| GET | `/v1/developer/stats/overview` | `stats:read` |

**Conventions available from day one**

* One error envelope, `{ code, message, details }`, with stable codes for
  authentication, permission, validation, state conflicts and throttling.
* Cursor pagination on every list, `limit` 1–100, stable descending order.
* ISO-8601 UTC timestamps; money as integer minor units.
* `Idempotency-Key` on request creation, with replay of the stored response.
* `Cache-Control: no-store` on every authenticated response.
* The OpenAPI contract served at `/v1/developer/openapi.json`, filtered to the
  calling key's scopes.
* These guides compiled into the binary, served at `/v1/developer/docs`.

### Deprecated

Nothing. `1.0.0` is the first release.

### Removed

Nothing.

### Known limitations

Stated so they are not discovered as bugs:

* No outbound webhooks — integrations poll.
* No payment creation, capture or refund; requests created through the API carry
  no payment record.
* No guest identity of any kind in any response.
* No DJ-issued keys; keys belong to the club.
* No OAuth2 client-credentials flow.
* The Test Mode stand may be reset at any time; its data is not durable.
* Desktop/mobile SDKs do not exist; the OpenAPI contract is the integration
  surface.

## Compatibility notes for 1.x

* New endpoints, optional request fields, new response fields and new enum values
  may appear in a minor release. Clients must tolerate unknown fields and
  unknown enum values.
* Existing required scopes never change in `1.x`; a call that succeeded does not
  start failing with `insufficient_scope`.
* Error `code` values are permanent. New codes may be added; a client must treat
  an unknown code as the generic 4xx/5xx of its status.
* Removal or rename of an operation requires `/v2` and six months of notice.

## Reporting a problem with these docs

If a guide and the served contract disagree, the contract wins. Report the
disagreement with the endpoint, the key prefix (never the secret) and the
`info.version` from `/v1/developer/openapi.json`; the documentation is generated
from the same repository as the contract, so the fix ships together.