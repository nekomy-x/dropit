---
id: profile
title: Profile
summary: GET /v1/developer/profile — the club fields you can rely on to map a DROP IT club to a venue record.
requires: ["profile:read"]
order: 90
---

# Profile

The profile is the club's identity as DROP IT knows it: the fields the club
owner edits in the panel. It is read-only in v1 — the API does not let an
integration rename a club or change its contact details.

## GET /v1/developer/profile

Requires `profile:read`.

```sh
curl -s "$DROPIT_API_BASE/profile" \
  -H "Authorization: Bearer $DROPIT_API_KEY"
```

```json
{
  "club": {
    "id": "7d3e9b21-4c5a-4f8e-9b1d-2a6c8e0f3b47",
    "name": "Club Noche",
    "slug": "club-noche",
    "address": "Calle Mayor 14, 28013 Madrid",
    "country": "ES",
    "email": "ops@clubnoche.example",
    "phone": "+34 600 000 000",
    "logo_url": "https://cdn.example/club-noche.png",
    "timezone": "Europe/Madrid",
    "status": "active"
  }
}
```

The club object is nested under `club`; a flat top-level object is not the
response shape.

## Fields

| Field | Type | Nullable | Notes |
| --- | --- | --- | --- |
| `id` | UUID string | no | Stable primary key. Use this as the foreign key in your system. |
| `name` | string | no | Display name as the club entered it. Free text, may change. |
| `slug` | string | no | URL-safe identifier, unique. Useful for public references; may change if the club renames. |
| `address` | string | yes | Postal address as one line. Not structured; do not parse it. |
| `country` | string | yes | ISO 3166-1 alpha-2, uppercase (`ES`, `DE`, `PT`). |
| `email` | string | yes | Operational contact of the club, not a guest address. |
| `phone` | string | yes | Free-form, may include a country code. |
| `logo_url` | string | yes | Absolute URL of the club logo. May be served from a CDN; do not assume a stable path. |
| `timezone` | string | no | IANA zone the club operates in (`Europe/Madrid`). The same value sessions are opened with. |
| `status` | string | no | `active`, `suspended` or another lifecycle value. See below. |

An absent or unset value is `null` (or the field is absent) — the API does not
send empty strings to mean "not set".

## `status`

`status` is the club's lifecycle state on the platform:

| Value | Meaning for an integration |
| --- | --- |
| `active` | Normal. Every scoped call works. |
| `suspended` | The club is not operating. **All** calls with its keys answer `403 club_suspended`, including this endpoint — so in practice you see this state as a `403`, and the value here is what the panel shows. |

Because a suspended club's keys are refused at authentication, a successful
`GET /profile` already implies `status` was `active` at request time. If you
need the exact value for another club (for an operator console), that is an
admin-panel surface, not this API.

## Using the profile correctly

* **Key your records by `club.id`.** `slug` and `name` are human-facing and may
  change; `id` does not.
* **Cache the profile for minutes, not seconds.** It changes a handful of
  times a year. One fetch per hour per club is generous; one per request is
  waste.
* **Do not infer capabilities from the profile.** Whether a club has an active
  DJ, a running session or a catalog provider is visible on the sessions and
  catalog endpoints, not here.
* **Treat `email` and `phone` as club contact data, not guest data.** They are
  business contact details the club published to its own integration. Do not
  expose them on a public page without the club's consent.

## Errors

| Status | `code` | Cause |
| --- | --- | --- |
| 401 | `invalid_api_key` / `api_key_expired` / `api_key_revoked` | Credential problem. See **Authentication**. |
| 403 | `insufficient_scope` | The key has no `profile:read`; `details.required_scope` confirms it. |
| 404 | `not_found` | The key's club row is missing (broken data, not a client error). |
| 403 | `club_suspended` / `ip_not_allowed` / `test_key_not_allowed` | Access rule failed. |
| 503 | `external_unavailable` | The database is not reachable; retry with backoff. |

There is no `404` on this endpoint: a valid key always has exactly one club, so
the profile exists whenever authentication succeeded.