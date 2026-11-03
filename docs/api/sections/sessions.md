---
id: sessions
title: Sessions
summary: Create, inspect and close a night, issue the guest QR, and stay on the right side of the session state machine.
requires: ["sessions:read"]
order: 100
---

# Sessions

A session is one club night: the thing guests join by scanning a QR code, the
thing the DJ opens, and the thing requests belong to. The API lets an
integration create a session, watch it, close it, and issues the QR token that
lets guests in.

## State machine

```
created ──DJ opens──► active ──DJ pauses──► paused_acceptance
                        │                        │
                        └──── DJ resumes ────────┘
                        │
                     close (API or panel)
                        ▼
                     closed
```

| State | Meaning | Requests accepted? |
| --- | --- | --- |
| `created` | The night exists but the room is not live yet. Created by the club panel or `POST /sessions`. | no |
| `active` | The DJ opened the session. Guests can join and request. | yes |
| `paused_acceptance` | The DJ paused acceptance (crowded queue, set change). Guests stay in the room. | no — `409 session_not_active` |
| `closed` | The night is over. Pending requests expire. | no — `409 session_closed` |

Two transitions are deliberately **not** available through the API:

* `created → active` is a DJ action in the DJ console. `sessions:write` lets you
  create and close a night; it does not let an integration start accepting
  requests on the DJ's behalf.
* `active → paused_acceptance` is also the DJ's. The API observes the state, it
  does not steer the booth.

## Session object

The list, the detail read and the create call all return a session object with
these fields; the detail read may additionally carry counters, which are
additive and safe to ignore:

```json
{
  "id": "3f2b1c4e-8a6d-4c1f-9e2a-5b7d0c9f1a2e",
  "status": "active",
  "timezone": "Europe/Madrid",
  "dj_id": "9a7b6c5d-4e3f-4a2b-8c1d-0e9f8a7b6c5d",
  "pricing_policy_id": "e4c1a9b0-2f3d-4e5c-8a7b-6d5e4f3a2b1c",
  "opened_at": "2026-10-28T22:04:11Z",
  "closed_at": null,
  "created_at": "2026-10-28T20:30:00Z"
}
```

| Field | Type | Notes |
| --- | --- | --- |
| `id` | UUID | Session id; use it everywhere else. |
| `status` | enum | `created`, `active`, `paused_acceptance`, `closed`. |
| `timezone` | IANA string | The zone the session was opened in; drives the planned auto-close (next 07:00 in that zone). |
| `dj_id` | UUID or `null` | The DJ assigned to the night, when one is assigned. |
| `pricing_policy_id` | UUID or `null` | The price list used for the night, when one was chosen at creation. |
| `opened_at` | timestamp or `null` | When the DJ opened the room; `null` while `created`. |
| `closed_at` | timestamp or `null` | When the session closed; `null` while it is live. |
| `created_at` | timestamp | When the night was created. |

`GET /sessions/{id}` adds a `counts` object for the night:

```json
{
  "counts": { "total": 62, "pending": 4, "accepted": 3, "rejected": 6, "played": 49, "queue": 7 }
}
```

`queue` is the number of requests waiting for action (`pending` + `accepted`);
`total` counts every request of the session, including `expired`.

Treat unknown `status` values as "not active" and stop acting until the value is
one you recognise; the enum is allowed to grow inside `v1`.

## List sessions

Requires `sessions:read`.

```
GET /v1/developer/sessions?status=active&limit=20&cursor=…
```

| Query | Values | Notes |
| --- | --- | --- |
| `status` | `created`, `active`, `paused_acceptance`, `closed` | Optional filter. |
| `limit` | `1..100` | Cursor page size. |
| `cursor` | opaque | From `next_cursor`. |

```sh
curl -s "$DROPIT_API_BASE/sessions?status=active&limit=20" \
  -H "Authorization: Bearer $DROPIT_API_KEY"
```

```json
{
  "items": [
    {
      "id": "3f2b1c4e-8a6d-4c1f-9e2a-5b7d0c9f1a2e",
      "status": "active",
      "timezone": "Europe/Madrid",
      "dj_id": "9a7b6c5d-4e3f-4a2b-8c1d-0e9f8a7b6c5d",
      "pricing_policy_id": "e4c1a9b0-2f3d-4e5c-8a7b-6d5e4f3a2b1c",
      "opened_at": "2026-10-28T22:04:11Z",
      "closed_at": null,
      "created_at": "2026-10-28T20:30:00Z"
    }
  ],
  "next_cursor": null
}
```

Ordering is newest first (`created_at` descending). A typical automation finds
"the current session" by filtering `status=active` and taking the first item; if
the list is empty, there is no live room to act on.

## Get one session

Requires `sessions:read`.

```
GET /v1/developer/sessions/{id}
```

```sh
curl -s "$DROPIT_API_BASE/sessions/3f2b1c4e-8a6d-4c1f-9e2a-5b7d0c9f1a2e" \
  -H "Authorization: Bearer $DROPIT_API_KEY"
```

A session that does not exist returns `404 not_found`; a session that belongs to
another club returns the same `404`, by design.

## Create a session

Requires `sessions:write`.

```
POST /v1/developer/sessions
```

```json
{
  "dj_id": "9a7b6c5d-4e3f-4a2b-8c1d-0e9f8a7b6c5d",
  "pricing_policy_id": "e4c1a9b0-2f3d-4e5c-8a7b-6d5e4f3a2b1c",
  "timezone": "Europe/Madrid"
}
```

| Field | Required | Notes |
| --- | --- | --- |
| `timezone` | yes | IANA zone (`Europe/Madrid`, `Europe/Lisbon`). DST-safe; the session's planned close is computed from it. Anything else answers `422 validation_error` with `details.field = "timezone"`. |
| `dj_id` | no | Must be a DJ of **this** club. A foreign or unknown DJ answers `404 not_found` — the API does not confirm another club's DJ id. |
| `pricing_policy_id` | no | Must belong to this club; otherwise `404 not_found`. |

The response is `201` with the session object in `created` state. Idempotency is
not automatic here: creating twice creates two sessions. If your scheduler may
run twice, list `status=created` sessions first and reuse one.

Creating a session does not open it. Until the DJ opens the session, guest
requests answer `409 session_closed` and the queue is empty.

## Close a session

Requires `sessions:write`.

```
POST /v1/developer/sessions/{id}/close
```

```sh
curl -s -X POST "$DROPIT_API_BASE/sessions/3f2b1c4e-8a6d-4c1f-9e2a-5b7d0c9f1a2e/close" \
  -H "Authorization: Bearer $DROPIT_API_KEY"
```

Closing is the end of the night:

* the session moves to `closed` and stays closed — re-opening is not part of v1;
* pending requests expire (they stop occupying the queue and count as not
  played);
* the QR token for the session stops letting guests in;
* further request creation or moderation answers `409 session_closed`.

The call returns the session object in its new state. Closing an already closed
session answers `409 session_closed`; treat that as success if your goal was
"make sure it is closed".

Close after last call, not at a fixed clock time, unless you deliberately want
a hard stop. The platform's planned close (next 07:00 in the session timezone)
is a safety net, not a schedule to rely on.

## Issue the guest QR

Requires `qr:write` — a sensitive scope: the token it returns lets guests into
the room.

```
POST /v1/developer/sessions/{id}/qr
```

```sh
curl -s -X POST "$DROPIT_API_BASE/sessions/3f2b1c4e-8a6d-4c1f-9e2a-5b7d0c9f1a2e/qr" \
  -H "Authorization: Bearer $DROPIT_API_KEY"
```

```json
{
  "session_id": "3f2b1c4e-8a6d-4c1f-9e2a-5b7d0c9f1a2e",
  "qr_token": "9c1f4a…",
  "expires_at": "2026-10-29T07:00:00Z",
  "join_url": "https://<guest-app-host>/join?token=9c1f4a…"
}
```

| Field | Meaning |
| --- | --- |
| `session_id` | The night the token belongs to. |
| `qr_token` | The plain token to encode in the QR image. Returned **once**; only its SHA-256 hash is stored. |
| `expires_at` | When the token stops working — the session's planned close. |
| `join_url` | **Optional.** Present only when the deployment configures a guest join page; the absolute URL to encode (or an NFC tag). When absent, build the QR from `qr_token` using the guest app's deeplink. Never assume the field exists. |

Behaviour that matters:

* **Issuing rotates.** Each call invalidates the previous token for the session.
  Guests who already joined keep their grant until the session closes; guests
  who scan an old QR get `404`.
* **The token is a credential.** Do not log it, do not put it in analytics, do
  not print it on a screen you would not leave unattended. If it leaks, rotate
  immediately — that is one extra call.
* **It does not open the room.** A QR on a `created` session does not accept
  guests until the DJ opens the session; while `created`, joining is refused.
* **Closed sessions refuse rotation** with `409 session_closed`: the token would
  be useless anyway.

A door terminal that needs a QR rotation per shift should hold `qr:write` and
`requests:read` only — not `stats:read`, not `requests:write`.

## Errors

| Status | `code` | Cause |
| --- | --- | --- |
| 403 | `insufficient_scope` | Missing `sessions:read` / `sessions:write` / `qr:write`. |
| 404 | `not_found` | Unknown session id, or a session of another club; also an unknown `dj_id` / `pricing_policy_id` on create (`details.resource` names it). |
| 409 | `session_closed` | Closing an already closed session, or issuing a QR for one. |
| 422 | `validation_error` | Bad timezone, bad `status`/`limit`/`cursor` query value, malformed body. |
| 503 | `external_unavailable` | Database unavailable. |

## Safety checklist

Before you give an integration `sessions:write` or `qr:write`, answer these:

1. Who can trigger a close, and is there a confirmation in front of it?
2. Is the QR token stored anywhere it could leak (logs, screenshots, chat)?
3. Does the automation refuse to act when `status` is not the expected one?
4. Is there exactly one scheduler creating sessions, or can a retry create a
   second empty night?