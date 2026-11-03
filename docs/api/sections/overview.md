---
id: overview
title: Overview
summary: What the Developer API v1 can automate, who it is for, and the conventions every endpoint shares.
requires: []
order: 10
---

# Overview

The DROP IT Developer API v1 is an HTTP/JSON API for the operational loop around a
club night: the club profile, sessions, guest entry QR, the music request queue,
catalog search and the aggregated nightly numbers. It is the same backend that
serves the club panel, exposed under a separate namespace with its own
authentication, its own rate limit and its own versioned contract.

Two surfaces share one host:

| Surface | Base path | Authentication | Contract |
| --- | --- | --- | --- |
| Club panel / DJ console | `/v1/**` | JWT session (browser) | Product internal |
| **Developer API v1** | **`/v1/developer/**`** | **API key** | **This documentation** |

The base URL is `https://<your-api-host>/v1/developer`. On the public Test Mode
stand it is `http://150.241.68.57/v1/developer`. Every path in this documentation
is relative to that base, so `GET /profile` means
`GET https://<your-api-host>/v1/developer/profile`.

## Who this is for

* club operators and their in-house developers wiring DROP IT into a POS, PMS,
  booking or cash-register system;
* integrators building queue automation for a venue;
* analysts pulling nightly numbers into a warehouse.

Tokens are issued per club and scope every response to that club. A key created
for club A cannot see club B: foreign ids answer `404`, never `403`, so ids
cannot be enumerated across tenants.

## What you can automate in v1

| Capability | Endpoints | Scope |
| --- | --- | --- |
| Key identity and docs | `GET /me`, `GET /docs`, `GET /docs/{section}`, `GET /openapi.json` | none / `docs:read` |
| Club profile | `GET /profile` | `profile:read` |
| Sessions (the night) | `GET /sessions`, `POST /sessions`, `GET /sessions/{id}`, `POST /sessions/{id}/close` | `sessions:read`, `sessions:write` |
| Guest entry QR | `POST /sessions/{id}/qr` | `qr:write` |
| Live queue | `GET /sessions/{id}/queue` | `requests:read` |
| Place and moderate requests | `POST /sessions/{id}/requests`, `GET /requests`, `GET /requests/{id}`, `POST /requests/{id}/accept`, `POST /requests/{id}/reject`, `POST /requests/{id}/played` | `requests:read`, `requests:write` |
| Catalog search | `GET /catalog/search` | `catalog:read` |
| Nightly numbers | `GET /stats/overview` | `stats:read` |

## Conventions

These hold for every endpoint and are not repeated per section.

* **JSON only.** Request bodies and responses are `application/json; charset=utf-8`.
* **Timestamps** are ISO-8601 in UTC with an explicit offset, e.g.
  `2026-10-28T22:14:05Z`.
* **Money** is an integer count of minor units plus an ISO-4217 currency code.
  `12950` in `EUR` means €129.50. Never a float.
* **Ids** are UUID strings. A path parameter is always the canonical id, not a
  slug or a name.
* **Lists** are cursor-paginated: `{ "items": [...], "next_cursor": "..." }`,
  with `next_cursor` `null` on the last page. See the **Pagination** guide.
* **Errors** use one envelope, `{ "code", "message", "details" }`, with a stable
  machine-readable `code`. See the **Errors** guide.
* **Caching.** Authenticated Developer API responses carry
  `Cache-Control: no-store`, because they depend on the scopes of the key. Do
  not cache them in shared proxies.
* **Rate limit** is per key: a rolling 60-second window, `X-RateLimit-*` headers
  on every response a handler produces. See **Rate limits**.
* **Documentation is compiled into the binary.** The guides you are reading and
  the OpenAPI contract are embedded at build time, so the deployed docs and the
  deployed routing cannot drift apart. `GET /openapi.json` returns the contract
  filtered to the scopes of your key.

## Environments

Keys exist in one of two environments, encoded in the key string itself:
`dit_live_…` and `dit_test_…`.

* On a deployment running in **live** payment mode, a `test` key is refused with
  `403 test_key_not_allowed` — test data is never silently mixed into live data.
* On the **Test Mode** stand both kinds of key work, and the docs you are reading
  are the same on both.
* Keys of a club whose status is not `active` are refused with
  `403 club_suspended`.

## Getting a key

Keys are created in the club panel, not through the API: **Club panel → Settings
(sidebar) → Developer access → Create key**. The plaintext secret is shown once;
copy it into your secret manager immediately. The panel is also where you
narrow scopes, rotate, revoke, set an IP allowlist and inspect usage. See
**Authentication** and **Scopes**.

## Where to go next

| If you want to | Read |
| --- | --- |
| Make your first call in five minutes | **Quickstart** |
| Understand key formats and rotation | **Authentication** |
| Pick the smallest set of permissions | **Scopes** |
| Handle failures correctly | **Errors** |
| Page through long lists | **Pagination** |
| Know what changes are allowed | **Versioning** |
| Build a real integration | **Recipes** |

## Out of scope for v1

Stated here so you do not build against a promise we have not made:

* no outbound webhooks — poll the endpoints you need;
* no payment creation, capture or refunds — requests created through the API
  carry no payment record;
* no guest identities or PII, ever;
* no DJ accounts issuing keys; keys belong to the club;
* no OAuth2 client-credentials flow.

These are candidates for a later version. Anything added will be additive within
`v1`; see the **Versioning** guide.
