---
id: requests
title: Requests
summary: The live queue, placing requests idempotently, moderating them, and the privacy line the API never crosses.
requires: ["requests:read"]
order: 110
---

# Requests

A request is a guest asking for a track in the current session. The API can read
the queue, place a request on a guest's behalf (a POS or a staff tool does this),
and accept, reject or mark requests played. Everything is scoped to your club.

## Request states

```
pending ──accept──► accepted ──played──► played
   │                    │
   ├──reject──► rejected │
   └──session closes──► expired
```

| State | Meaning |
| --- | --- |
| `pending` | Waiting for the DJ. |
| `accepted` | The DJ will play it. |
| `played` | It was played. |
| `rejected` | The DJ declined it. |
| `expired` | The session closed with the request still pending. |

Only the documented transitions exist. `rejected`, `played` and `expired` are
terminal; a request never moves back to `pending`. Repeating the same action on
a request already in that state is a successful no-op, not an error.

## The queue

Requires `requests:read`.

```
GET /v1/developer/sessions/{id}/queue?sort=new
```

| Query | Values | Default |
| --- | --- | --- |
| `sort` | `new` — arrival order; `hot` — most-requested track first, then arrival | `new` |

```sh
curl -s "$DROPIT_API_BASE/sessions/3f2b1c4e-8a6d-4c1f-9e2a-5b7d0c9f1a2e/queue?sort=hot" \
  -H "Authorization: Bearer $DROPIT_API_KEY"
```

```json
{
  "items": [
    {
      "id": "b71d5e02-9c34-4a6f-8d1b-0e2f4a6c8d10",
      "session_id": "3f2b1c4e-8a6d-4c1f-9e2a-5b7d0c9f1a2e",
      "status": "pending",
      "style": "house",
      "created_at": "2026-10-28T22:14:05Z",
      "same_track_count": 3,
      "track": {
        "title": "One More Time",
        "artist": "Daft Punk",
        "artwork_url": "https://is1-ssl.mzstatic.com/…/100x100bb.jpg",
        "preview_url": "https://audio-ssl.itunes.apple.com/…/preview.m4a",
        "isrc": "GBDUW0000059"
      }
    }
  ]
}
```

| Field | Type | Notes |
| --- | --- | --- |
| `id` | UUID | Request id. |
| `session_id` | UUID | The night it belongs to. |
| `status` | enum | `pending` or `accepted` in the queue; other states appear in `GET /requests`. |
| `style` | string or `null` | Free-form style attached by whoever placed the request. |
| `created_at` | timestamp | When the request entered the queue. |
| `same_track_count` | integer | How many times **the same song** was requested in this session. |
| `track.title`, `track.artist` | string | Track metadata as stored in the club's cache. |
| `track.artwork_url`, `track.preview_url`, `track.isrc` | string or `null` | Optional provider metadata. |

The queue is not paginated: it returns the active requests of one session
(`pending` + `accepted`) in the requested order. For history, use
`GET /requests` with a `status` filter.

`sort=hot` orders by `same_track_count` descending, then by `created_at`. The
"same song" grouping uses the catalog's internal canonical key, not the display
title, so remasters and duplicate catalog entries do not split the count. The
canonical key itself is not exposed.

## Place a request

Requires `requests:write`.

```
POST /v1/developer/sessions/{id}/requests
Idempotency-Key: <opaque string, max 200 characters>
```

```sh
curl -s -X POST "$DROPIT_API_BASE/sessions/3f2b1c4e-8a6d-4c1f-9e2a-5b7d0c9f1a2e/requests" \
  -H "Authorization: Bearer $DROPIT_API_KEY" \
  -H "Content-Type: application/json" \
  -H "Idempotency-Key: order-2026-10-28-0042" \
  -d '{"track_id":"0d9e8f7a-6b5c-4d3e-8f2a-1b0c9d8e7f60","style":"house"}'
```

```json
{
  "id": "b71d5e02-9c34-4a6f-8d1b-0e2f4a6c8d10",
  "session_id": "3f2b1c4e-8a6d-4c1f-9e2a-5b7d0c9f1a2e",
  "status": "pending",
  "style": "house",
  "price_minor": 0,
  "decline_reason": null,
  "played_at": null,
  "created_at": "2026-10-28T22:14:05Z",
  "track": {
    "title": "One More Time",
    "artist": "Daft Punk",
    "artwork_url": "https://is1-ssl.mzstatic.com/…/100x100bb.jpg",
    "preview_url": "https://audio-ssl.itunes.apple.com/…/preview.m4a",
    "isrc": "GBDUW0000059"
  }
}
```

| Body field | Required | Notes |
| --- | --- | --- |
| `track_id` | yes | UUID of a track the server knows (search the catalog first). An unknown id answers `422 validation_error` with `details.field = "track_id"`. |
| `style` | no | Free-form label, at most 40 characters after trimming. |

The response is `201` with the request card — the same shape as a request
detail. There is no `queue_position` field: if you need a position, read the
queue and find the id.

### Idempotency-Key is required

The header is mandatory on this endpoint — omitting it answers
`422 validation_error` with `details.field = "Idempotency-Key"`; it does not
create a request.

* the first call with a key creates the request and stores the response;
* every later call with **the same** key and the same body returns that stored
  response as `201`, with the same request `id`, and creates nothing;
* reusing a key for a **different** body answers `409 idempotency_conflict` — a
  key identifies one business event, so this is a bug in the caller, not a retry.

Pick the key from your own business event, not from a counter that resets:
`order-2026-10-28-0042`, a payment id, a POS ticket number. Keys are scoped to
the API key that sent them, so two integrations cannot collide.

### Which session accepts requests

The session must be `active`; the DJ opens it in the DJ console. If it is not
active — still `created`, paused, or closed — request creation answers
`409 session_not_active`. That single code covers every non-active state: read
the session first (`GET /sessions/{id}`) and surface the actual `status` instead
of guessing from the error.

### What the guest sees

Requests created through the API are attributed to an internal synthetic guest
device, one per API key, which the platform creates on first use. In the queue
and the DJ console they behave exactly like guest requests — same states, same
ordering, same moderation. That identity never leaves the platform.

`price_minor` is `0` for every API-created request: **payment is out of scope for
v1**. The endpoint accepts no amount and creates no payment record.

## Club-wide requests

Requires `requests:read`.

```
GET /v1/developer/requests?status=played&session_id=…&limit=50
```

| Query | Values |
| --- | --- |
| `status` | `pending`, `accepted`, `rejected`, `played`, `expired` |
| `session_id` | Restrict to one session; an unknown or foreign session answers `404 not_found` |
| `limit` | `1..100`, default `50` |
| `cursor` | Opaque cursor from `next_cursor` |

This is the bulk read: use it for reports, reconciliation and dashboards instead
of walking every session queue. Newest first, cursor-paginated (see
**Pagination**). Items use the same card shape as the detail read.

## Request detail

Requires `requests:read`.

```
GET /v1/developer/requests/{id}
```

```json
{
  "id": "b71d5e02-9c34-4a6f-8d1b-0e2f4a6c8d10",
  "session_id": "3f2b1c4e-8a6d-4c1f-9e2a-5b7d0c9f1a2e",
  "status": "accepted",
  "style": "house",
  "price_minor": 0,
  "decline_reason": null,
  "played_at": null,
  "created_at": "2026-10-28T22:14:05Z",
  "track": {
    "title": "One More Time",
    "artist": "Daft Punk",
    "artwork_url": "https://is1-ssl.mzstatic.com/…/100x100bb.jpg",
    "preview_url": "https://audio-ssl.itunes.apple.com/…/preview.m4a",
    "isrc": "GBDUW0000059"
  }
}
```

| Field | Notes |
| --- | --- |
| `decline_reason` | Present on rejected requests, otherwise `null`. |
| `played_at` | Set when the request is marked played; otherwise `null`. |
| `price_minor` | Minor units captured for the request; `0` in v1 for every request. |

There is no `position` or `queue_size` field. Compute a position from the queue
if you need one; it is a presentation detail, not part of the contract.

## Moderate

All three actions require `requests:write`:

```
POST /v1/developer/requests/{id}/accept
POST /v1/developer/requests/{id}/reject
POST /v1/developer/requests/{id}/played
```

```sh
curl -s -X POST "$DROPIT_API_BASE/requests/b71d5e02-9c34-4a6f-8d1b-0e2f4a6c8d10/accept" \
  -H "Authorization: Bearer $DROPIT_API_KEY"
```

The response is `200` with the updated request card plus a `noop` flag:

```json
{
  "id": "b71d5e02-9c34-4a6f-8d1b-0e2f4a6c8d10",
  "session_id": "3f2b1c4e-8a6d-4c1f-9e2a-5b7d0c9f1a2e",
  "status": "accepted",
  "style": "house",
  "price_minor": 0,
  "decline_reason": null,
  "played_at": null,
  "created_at": "2026-10-28T22:14:05Z",
  "track": { "title": "One More Time", "artist": "Daft Punk" },
  "noop": false
}
```

`noop: true` means the action was already in that state and nothing changed —
retrying an accept is safe and does not produce an error or a duplicate event.
`reject` accepts an optional JSON body with a `reason`; `accept` and `played`
take no body.

| Action | Allowed from | Result |
| --- | --- | --- |
| `accept` | `pending` (or already `accepted` → `noop: true`) | `accepted` |
| `reject` | `pending` or `accepted` (the DJ changed their mind before playing); already `rejected` → `noop: true` | `rejected` |
| `played` | `pending` (the DJ played it without accepting) or `accepted`; already `played` → `noop: true` | `played` |

A transition the domain forbids (for example `accept` after `played`, or any
moderation of an `expired` request) answers `409 invalid_request_transition`
before anything is written. `rejected`, `played` and `expired` are terminal:
the only action they accept is their own repeat (a no-op) — `reject` on an
`expired` request is a `409`, not a no-op. A request that does not exist in your club answers
`404 not_found`.

## Errors

| Status | `code` | Cause |
| --- | --- | --- |
| 403 | `insufficient_scope` | Missing `requests:read` or `requests:write`. |
| 404 | `not_found` | Unknown session or request id, or one owned by another club. |
| 409 | `session_not_active` | The session is not `active`; requests are refused for `created`, `paused_acceptance` and `closed`. |
| 409 | `invalid_request_transition` | The action is not allowed from the request's current state. |
| 409 | `idempotency_conflict` | Same `Idempotency-Key`, different body. |
| 422 | `validation_error` | Missing `Idempotency-Key`, unknown `track_id`, `style` too long, bad `limit`/`status`/`cursor`. |

## Privacy

* No guest identity is exposed. Responses contain no device id, no e-mail, no
  token, no IP and no payment identifier.
* You cannot look up "who requested this" — and neither can support. The data
  does not reach you because it does not leave the platform.
* Requests created through the API are attributed to a synthetic device so the
  queue is operationally valid without inventing a guest.
* Do not try to re-identify a request by correlating timestamps with your own
  staff actions and then store it as guest data. The contract forbids it in
  spirit, and there is no field to hang it on.

## Practical notes

* **Search before creating.** `GET /catalog/search` returns the track `id`; a
  request needs that id, not a title.
* **One request per business event.** If a table orders three songs, send three
  requests with three idempotency keys; do not loop on one key.
* **Persist the key before the first attempt.** The idempotency key is the whole
  safety mechanism; generate it with the pending order, not with the HTTP call.
* **Do not auto-accept everything blindly.** Acceptance is a creative decision
  in most venues; if you automate it, restrict by style or by an allowlist of
  tracks. There is no price on a request in v1, so "cheap" cannot be the rule.