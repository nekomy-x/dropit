# DROP IT Developer API v1

The public HTTP/JSON API for clubs and their developers. It automates the
operational loop around a night — sessions, guest entry QR, the music request
queue, catalog search and the nightly numbers — over the same backend that serves
the club panel.

* **Base URL:** `https://<your-api-host>/v1/developer`
* **Test Mode stand:** `http://150.241.68.57/v1/developer`
* **Contract:** served as OpenAPI 3.1 JSON at `/v1/developer/openapi.json`,
  filtered to the scopes of the calling key
* **Design authority:** [`ADR-010`](../adr/ADR-010-developer-api-v1.md)
* **Version:** `1.0.0`, first published 2026-10-28

## Authentication in one paragraph

Every call needs an API key created in the club panel (**Club panel → Settings →
Developer access**). Send it as `Authorization: Bearer dit_live_…` or as
`X-API-Key: dit_live_…`; both are equivalent. The secret is shown once, only a
SHA-256 hash is stored, and a key that leaks can be revoked immediately from the
same panel. There is no OAuth and no anonymous access.

## Scope model

A key carries an explicit set of scopes; there is no wildcard and no implicit
admin scope. The nine scopes are `docs:read`, `profile:read`, `sessions:read`,
`sessions:write`, `qr:write`, `requests:read`, `requests:write`, `catalog:read`
and `stats:read`. A key can be narrowed but never widened: widening means
creating a new key. Every response is scoped to the key's club; a foreign id
answers `404`.

## Guides

| Guide | What it covers | Required scope |
| --- | --- | --- |
| [Overview](sections/overview.md) | What you can automate, conventions, environments, out-of-scope list | — |
| [Quickstart](sections/quickstart.md) | Key creation in the panel, first calls in curl/TypeScript/Python, placing a request | — |
| [Authentication](sections/authentication.md) | Key format, both headers, 401 codes, rotation, revocation | — |
| [Scopes](sections/scopes.md) | The nine scopes, presets, least privilege, the endpoint matrix | — |
| [Errors](sections/errors.md) | The error envelope, every code, retry decisions | — |
| [Rate limits](sections/rate-limits.md) | 60-second window per key, headers, backoff | — |
| [Pagination](sections/pagination.md) | Cursors, ordering, walking a list | — |
| [Versioning](sections/versioning.md) | The `/v1` promise, additive changes, six-month deprecation | — |
| [Profile](sections/profile.md) | `GET /v1/developer/profile` fields | `profile:read` |
| [Sessions](sections/sessions.md) | States, create/close, guest QR, safety notes | `sessions:read` |
| [Requests](sections/requests.md) | Queue, idempotent placement, moderation, privacy | `requests:read` |
| [Catalog](sections/catalog.md) | Search parameters, hit shape, canonical key, caching | `catalog:read` |
| [Stats](sections/stats.md) | Period overview, revenue in minor units, top tracks | `stats:read` |
| [Security](sections/security.md) | Hashing, one-time reveal, IP allowlist, incident response | — |
| [Recipes](sections/recipes.md) | Queue triage, POS integration, nightly reconciliation | — |
| [Changelog](sections/changelog.md) | Release history and compatibility notes | — |

The guides with a required scope are only visible to a key that holds it: the
manifest at `GET /v1/developer/docs` is filtered, and a direct request answers
`403 insufficient_scope` with the missing scope in `details`.

## Where the docs live

The same content is available three ways, all built from this repository:

1. **In the binary** — these markdown files are compiled into the API process and
   served by `GET /v1/developer/docs` and `GET /v1/developer/docs/{section}`.
2. **In the panel** — the club panel's Developers page renders the guides next to
   the keys, so documentation is reachable where the key is created.
3. **In the repository** — `docs/api/sections/*.md`, with the contract in
   `docs/api/openapi-developer.yaml`.

Because the docs are embedded at build time, the deployed documentation is the
documentation the binary was built from. `node scripts/api-docs-check.mjs` is the
gate that keeps guides, contract and generated Rust in sync.

## Support and versioning policy

* **Versioning:** `v1` is additive. New endpoints, optional fields and new enum
  values can appear at any time; breaking changes require `/v2`. Deprecations get
  at least six months of notice in the [changelog](sections/changelog.md).
* **Support:** there is no public support portal in v1. Contract questions and
  bugs go through the platform team that operates your deployment; include the
  key **prefix** and the `info.version` from `/v1/developer/openapi.json`, never
  the secret.
* **Status:** the API reports liveness at `/healthz` and readiness at `/readyz`
  on the same host. Both are unauthenticated.
* **Rate limits:** per key, 60-second window, 30–600 rpm configurable in the
  panel. A `429` includes `Retry-After`.

## Sandbox / Test Mode

* Keys carry their environment: `dit_test_…` for test, `dit_live_…` for live.
* A `test` key is **refused** on a deployment in live payment mode
  (`403 test_key_not_allowed`) — no silent mixing of test traffic and live data.
* On the Test Mode stand (`http://150.241.68.57`) both kinds work, and its data
  may be reset at any time. Do not build durable state against it.
* The docs and the contract are identical on both environments; only the data
  and the payment mode differ.

## Repository layout

```
docs/api/
  README.md                  # this page
  openapi-developer.yaml     # hand-written OpenAPI 3.1 contract
  sections/*.md              # guides, with YAML front matter (id, title, summary, requires, order)
scripts/
  build-api-docs.mjs         # markdown -> crates/gateway-api/src/developer_docs_content.rs
  api-docs-check.mjs         # the gate: front matter, scopes, path coverage, generated output
```

To change a guide, edit the markdown, run `node scripts/build-api-docs.mjs`, and
commit both the guide and the regenerated Rust file. Never edit
`developer_docs_content.rs` by hand: it is overwritten.