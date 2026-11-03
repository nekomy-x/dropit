---
id: stats
title: Stats
summary: One aggregated overview per period — sessions, requests, captured revenue in minor units, top tracks.
requires: ["stats:read"]
order: 130
---

# Stats

`GET /v1/developer/stats/overview` is the one read an integration needs to put
numbers on a wall, reconcile a night, or feed a warehouse. It aggregates; it
never returns individual guests or payment records.

## GET /v1/developer/stats/overview

Requires `stats:read`.

```
GET /v1/developer/stats/overview?period=day
```

| Query | Values | Default | Window |
| --- | --- | --- | --- |
| `period` | `day`, `week`, `month` | `day` | Rolling: the last 24 h, 7 days, 30 days, ending now. |

```sh
curl -s "$DROPIT_API_BASE/stats/overview?period=week" \
  -H "Authorization: Bearer $DROPIT_API_KEY"
```

```json
{
  "period": "week",
  "from": "2026-10-21T22:14:05Z",
  "to": "2026-10-28T22:14:05Z",
  "sessions": 5,
  "active_session": {
    "id": "3f2b1c4e-8a6d-4c1f-9e2a-5b7d0c9f1a2e",
    "status": "active",
    "opened_at": "2026-10-28T22:04:11Z"
  },
  "requests": {
    "total": 412,
    "pending": 9,
    "accepted": 14,
    "rejected": 37,
    "played": 352
  },
  "revenue": {
    "currency": "EUR",
    "captured_minor": 129500
  },
  "top_tracks": [
    { "title": "One More Time", "artist": "Daft Punk", "count": 11 },
    { "title": "Fever", "artist": "Elley Duhé", "count": 8 }
  ]
}
```

## Fields

| Field | Type | Notes |
| --- | --- | --- |
| `period` | enum | Echoes the request. |
| `from`, `to` | timestamps | The exact window used, UTC. Use these in your report header instead of computing your own window — the boundary is decided server-side. |
| `sessions` | integer | Sessions **started** inside the window (any state). |
| `active_session` | object or `null` | The club's current non-closed session, newest first: `{ id, status, opened_at }` with `status` in `created`, `active`, `paused_acceptance`. `null` when every session has closed. Always read `status` — a `created` or `paused_acceptance` session is not accepting requests. |
| `requests.total` | integer | All requests created in the window, every state. |
| `requests.pending` / `accepted` / `rejected` / `played` | integer | Count per current state. They sum to `total` only if no request is `expired`; expired requests are counted in `total` and not in any of the four named buckets. |
| `revenue.currency` | string | ISO-4217 code. |
| `revenue.captured_minor` | integer | **Money in minor units.** `129500` in `EUR` is €1295.00. Integer, never a float. |
| `top_tracks` | array | The five most-requested tracks in the window, ordered by `count` descending. Object per: `title`, `artist`, `count`. |
| `top_tracks[].count` | integer | How many requests (any state except rejected) named that canonical track in the window. |

`active_session` carries only an id, a status and `opened_at` — fetch
`GET /sessions/{id}` if you need the DJ, the timezone or request counts.

## Money

Money is always integer minor units plus a currency:

| Value | Currency | Human amount |
| --- | --- | --- |
| `129500` | `EUR` | €1 295.00 |
| `950` | `EUR` | €9.50 |
| `12000` | `GBP` | £120.00 |

There are no floats, no decimal strings and no per-request amounts in v1. The
`captured_minor` figure is revenue the platform captured in the window, not
what the club or DJ ultimately receive — fees and splits are a settlement
concern, not an API one. Do not divide by 100 and store the result as a float in
a ledger; keep the integer, format only for display.

```ts
const { revenue } = await call<{ revenue: { currency: string; captured_minor: number } }>(
  "/stats/overview?period=day",
);
const formatted = new Intl.NumberFormat("es-ES", {
  style: "currency",
  currency: revenue.currency,
}).format(revenue.captured_minor / 100);
console.log(formatted); // "1.295,00 €"
```

## What "period" does not mean

* It is a **rolling window**, not a calendar day/week/month. At 02:00 on
  Saturday, `period=day` covers Friday 02:00 → Saturday 02:00, which is usually
  what an operator means by "tonight". If you need a calendar bucket, align your
  own calls to midnight and store `from`/`to` from the response.
* `sessions` counts sessions created inside the window. A night that started
  before the window and is still running is not counted in `sessions` but does
  appear in `active_session`.
* `top_tracks` is over requests, not plays: a rejected request is not counted,
  an accepted one is.
* Rejected and expired requests are separate states; do not compute rejection
  rate from `total - played`.

## Reconciliation recipe

For a nightly close, compare the platform number with your own till/POS total:

```ts
type Overview = {
  period: "day" | "week" | "month";
  from: string;
  to: string;
  revenue: { currency: string; captured_minor: number };
  requests: { total: number; played: number };
};

const day = await call<Overview>("/stats/overview?period=day");
console.log(
  `window ${day.from} → ${day.to}: ${day.requests.played}/${day.requests.total} played, ` +
    `${day.revenue.captured_minor} ${day.revenue.currency} minor units captured`,
);
```

Persist `from`, `to` and the raw integer. When the numbers disagree with the
till, the first question is whether both sides used the same window — the
response's `from`/`to` is the authoritative one.

## Errors

| Status | `code` | Cause |
| --- | --- | --- |
| 403 | `insufficient_scope` | The key has no `stats:read`. |
| 422 | `validation_error` | `period` is not one of `day`, `week`, `month` (`details.field = "period"`). |
| 503 | `external_unavailable` | Database unavailable; retry with backoff. |

## Caching and polling

One call per minute is more than enough. The overview is an aggregate over a
window that moves in minutes, not seconds.

* Dashboard: poll every 60 s while a session is `active`, then stop.
* Nightly close: call once after the session is `closed`.
* Warehouse: one call per hour with all three periods is three requests — cheap
  and complete enough for a daily load; for anything finer, read the club-wide
  request list (`GET /requests`) and aggregate yourself.