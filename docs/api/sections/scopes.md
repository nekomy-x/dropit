---
id: scopes
title: Scopes
summary: The nine permissions a key can carry, the four presets, and how to keep a key at the minimum it needs.
requires: []
order: 40
---

# Scopes

A key carries an explicit, ordered set of scopes. There is no wildcard, no
implicit admin scope and no "read implies write" rule: anything not listed is
forbidden. Unknown scopes are rejected when the key is created, not when a call
is made.

## The nine scopes

| Scope | Risk | Grants | Endpoints |
| --- | --- | --- | --- |
| `docs:read` | read | Read this documentation and the contract. | `GET /docs`, `GET /docs/{section}`, `GET /openapi.json` |
| `profile:read` | read | Club name, address, contact, timezone, status. | `GET /profile` |
| `sessions:read` | read | List and inspect sessions. | `GET /sessions`, `GET /sessions/{id}` |
| `sessions:write` | write | Create a session, close a session. | `POST /sessions`, `POST /sessions/{id}/close` |
| `qr:write` | sensitive | Issue or rotate the guest entry QR token. It opens the room. | `POST /sessions/{id}/qr` |
| `requests:read` | read | Read the queue, a request, the club-wide request list. | `GET /sessions/{id}/queue`, `GET /requests`, `GET /requests/{id}` |
| `requests:write` | write | Place a request; accept, reject or mark it played. | `POST /sessions/{id}/requests`, `POST /requests/{id}/accept`, `POST /requests/{id}/reject`, `POST /requests/{id}/played` |
| `catalog:read` | read | Search the track catalog. | `GET /catalog/search` |
| `stats:read` | read | Aggregated requests, revenue and top tracks. | `GET /stats/overview` |

`GET /me` requires no scope: a key must be able to identify itself even if its
only purpose is to be monitored.

Notes that matter in practice:

* `sessions:write` lets you create and close a session, **not** open it for
  requests. Opening the queue is a DJ action in the DJ console — the API cannot
  start accepting requests on the DJ's behalf.
* `qr:write` is marked sensitive because the token it returns is a guest
  credential. Treat it like a secret (see the **Sessions** guide).
* `requests:write` is required for moderation actions too: accepting a request
  is a write, even though it returns an object similar to a read.
* `stats:read` exposes money (revenue in minor units). Do not hand it to a
  display integration that only needs the queue.

## Presets

The panel offers four presets. A preset is only a shortcut for checking scopes;
the key stores the explicit list.

| Preset | Scopes | Typical use |
| --- | --- | --- |
| Documentation only | `docs:read` | Reading the contract from a build script or an internal portal. |
| Read-only integration | `docs:read`, `profile:read`, `sessions:read`, `requests:read`, `catalog:read`, `stats:read` | BI dashboards, read-only PMS sync, monitoring. |
| Queue automation | read-only + `requests:write` | Auto-accept rules, POS-placed requests, moderation bots. |
| Full automation | all nine scopes | A trusted back office that runs the whole night. |

Presets are a starting point, not a review. A "full automation" key is a
browser-equivalent credential for the club; most integrations need much less.

## Least privilege

Choose the smallest set that completes the job, then add a second key if the job
grows:

| Job | Minimum scopes |
| --- | --- |
| Show tonight's queue on a staff tablet | `requests:read`, `sessions:read` |
| Close the session automatically after last call | `sessions:read`, `sessions:write` |
| Place a request when a table orders a bottle | `catalog:read`, `requests:write`, `sessions:read` |
| Accept every pending request automatically | `requests:read`, `requests:write` |
| Nightly revenue reconciliation | `stats:read`, `sessions:read` |
| Rotate the guest QR from a door terminal | `qr:write`, `sessions:read` |
| Publish a status page with the club name | `profile:read` |

A useful review question: *if this key leaked, what is the worst sentence that
could be written about the incident?* If the answer is "someone could read
tonight's queue", the scope set is right. If it is "someone could open the room
to strangers at 02:00", remove `qr:write` from that key.

## Narrowing: allowed. Widening: not.

* **Narrowing** — removing scopes from an existing key — is allowed at any time
  from the panel. It takes effect on the next request.
* **Widening** — adding a scope — is refused (`422 scope_widening_forbidden`).
  Create a new key with the wider set, deploy it, then revoke the narrow one.

This asymmetry is deliberate. Widening a key is a permission change that needs a
human decision and an audit entry; creating a new key makes that decision
explicit and reviewable.

## What a missing scope looks like

The check happens before the handler touches data, so a denied request has no
side effects. The response is `403` with enough detail to fix the key without
guessing:

```json
{
  "code": "insufficient_scope",
  "message": "The API key does not grant the scope required by this endpoint.",
  "details": {
    "required_scope": "stats:read",
    "granted_scopes": ["docs:read", "profile:read", "sessions:read"]
  }
}
```

The docs endpoints share the same rule: the guide manifest returned by
`GET /docs` only contains sections your key can read. A docs-only key never
learns what the revenue section says. Requesting a section directly without the
scope answers `403 insufficient_scope` with the section's first missing scope.

## Scope ↔ endpoint matrix

Every Developer API path and the single scope it requires:

| Path | Method | Scope |
| --- | --- | --- |
| `/v1/developer/me` | GET | none |
| `/v1/developer/docs` | GET | `docs:read` |
| `/v1/developer/docs/{section}` | GET | `docs:read` |
| `/v1/developer/openapi.json` | GET | `docs:read` |
| `/v1/developer/profile` | GET | `profile:read` |
| `/v1/developer/sessions` | GET | `sessions:read` |
| `/v1/developer/sessions` | POST | `sessions:write` |
| `/v1/developer/sessions/{id}` | GET | `sessions:read` |
| `/v1/developer/sessions/{id}/close` | POST | `sessions:write` |
| `/v1/developer/sessions/{id}/qr` | POST | `qr:write` |
| `/v1/developer/sessions/{id}/queue` | GET | `requests:read` |
| `/v1/developer/sessions/{id}/requests` | POST | `requests:write` |
| `/v1/developer/requests` | GET | `requests:read` |
| `/v1/developer/requests/{id}` | GET | `requests:read` |
| `/v1/developer/requests/{id}/accept` | POST | `requests:write` |
| `/v1/developer/requests/{id}/reject` | POST | `requests:write` |
| `/v1/developer/requests/{id}/played` | POST | `requests:write` |
| `/v1/developer/catalog/search` | GET | `catalog:read` |
| `/v1/developer/stats/overview` | GET | `stats:read` |

The same mapping is machine-readable: fetch `GET /openapi.json` and read
`x-required-scope` on each operation. The response is filtered to your key's
scopes, which makes it a reliable answer to "what can this key actually do?".