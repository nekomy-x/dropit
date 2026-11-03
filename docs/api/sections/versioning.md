---
id: versioning
title: Versioning
summary: What /v1 promises, which changes are additive, and the six-month deprecation window.
requires: []
order: 80
---

# Versioning

The public version is `1.0.0`, and the version lives in the path:
`/v1/developer/...`. The path version is the compatibility promise; the
`info.version` of the OpenAPI document is informational and increments on any
release, including compatible ones.

## The /v1 contract

While an endpoint is under `/v1/developer`, the following never changes without
a new major version:

* **paths and methods** — an operation is not renamed, moved or removed;
* **required scopes** — a call that succeeded does not start returning
  `403 insufficient_scope`;
* **response field names and types** — `status` stays a string, `created_at`
  stays an RFC 3339 timestamp, money stays an integer count of minor units;
* **error `code` values** — `rate_limited` stays `rate_limited`;
* **authentication transport** — both `Authorization: Bearer` and `X-API-Key`
  keep working;
* **timestamp and money conventions** — ISO-8601 UTC, integer minor units.

If a breaking change is unavoidable, it ships as `/v2` and `/v1` keeps working
for the deprecation window below.

## What counts as compatible

These changes can land at any time and do not require a version bump:

| Change | Example |
| --- | --- |
| New endpoint | a per-session stats endpoint under the same base |
| New optional request field | `POST /sessions` gains `note?` |
| New response field | a session gains `created_at` |
| New enum value | `status` gains a state; treat unknown enum values as possible |
| New error `code` for a new failure mode | `session_archived` |
| Looser validation | accepting a longer `style` string |
| New optional header | `X-Request-Id` |

Because of this, your client must:

1. **ignore unknown response fields** — do not deserialize into a strict schema
   that rejects extra keys;
2. **tolerate unknown enum values** — fall back to a neutral rendering rather
   than throwing;
3. **not assume an error code list is closed** — handle unknown `code` as a
   generic 4xx/5xx of that status.

## Deprecation

* Minimum notice: **6 months** between the announcement and the removal of an
  operation.
* Announcement channel: the changelog section of these docs
  (`GET /v1/developer/docs/changelog`), and the panel's developer page.
  There is no e-mail list in v1.
* During the window the operation keeps working and answers a
  `Deprecation` header with the planned removal date, plus a `Link` header to
  the replacement when one exists.
* Removal only happens in a major version. A deprecated `/v1` operation is
  never deleted while `/v1` exists.

```http
HTTP/1.1 200 OK
Deprecation: true
Sunset: Wed, 28 Apr 2027 00:00:00 GMT
Link: </v1/developer/sessions/{id}/queue>; rel="successor-version"
```

Treat the presence of `Deprecation: true` as an action item, not an error.

## Version discovery

```sh
curl -s "$DROPIT_API_BASE/openapi.json" \
  -H "Authorization: Bearer $DROPIT_API_KEY" \
  | head -c 400
```

```json
{
  "openapi": "3.1.0",
  "info": { "title": "DROP IT Developer API", "version": "1.0.0" },
  "servers": [{ "url": "/v1/developer" }]
}
```

The document you receive is filtered to your key's scopes: only operations the
key can call appear. That makes `GET /openapi.json` both a version probe and an
honest capability report.

Call it from CI and compare `info.version` against the version you tested
against. A minor bump means "compatible additions"; read the changelog before
deploying anyway.

## Changelog

Every release that touches the public surface gets an entry in the **Changelog**
guide: version, date, what was added, what was deprecated, what was removed.
The current entry is `1.0.0`, dated 2026-10-28.

## Not covered by the promise

These are deliberately outside the compatibility guarantee:

* **ordering of items inside a page beyond the documented sort key** — the
  documented order is stable; ties are broken by id;
* **the internal format of opaque cursors** — they may change, but a cursor
  returned by the running deployment always works against it;
* **human-readable `message` strings** — they may be reworded at any time;
* **rate-limit header values** — the window is fixed at 60 s, the numbers move;
* **retention periods of usage records** — currently 30 days, may be extended;
* **the Test Mode stand's uptime and data** — it is a test deployment, its data
  may be reset.

## SDKs and generated clients

There are no official SDKs in v1. The contract at `/v1/developer/openapi.json`
is a standard OpenAPI 3.1 document, so generators work:

```sh
# Generate a typed client from the live, scope-filtered contract.
curl -s "$DROPIT_API_BASE/openapi.json" \
  -H "Authorization: Bearer $DROPIT_API_KEY" > dropit.openapi.json
npx openapi-typescript dropit.openapi.json -o src/dropit-types.ts
```

Regenerate when the changelog announces an addition you want to use. Treat
generated types as a convenience: the compatibility rules above are what your
integration may rely on, not the exact generated shapes of a given generator.