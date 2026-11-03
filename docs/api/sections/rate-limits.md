---
id: rate-limits
title: Rate limits
summary: One rolling 60-second window per key, the headers that report it, and a backoff that behaves.
requires: []
order: 60
---

# Rate limits

The limit is **per API key**, not per club, not per IP. Two integrations using
two keys do not compete for the same budget; a single runaway script cannot
starve the club panel, because the panel uses JWT and a different limiter.

## The window

* Window: **rolling 60 seconds** — the counter is the number of recorded calls
  in the last minute, not a bucket that resets on the clock.
* Default limit: **120 requests per minute**.
* Configurable per key in the panel: **30–600 requests per minute**.
* Exceeding the limit answers `429 rate_limited` with `Retry-After: 60`.

The counter covers every authenticated Developer API call, including `401`/`403`
failures that reached the gate. Plan capacity for retries too, not just for
successes.

## Headers

Every response produced by a handler — `2xx`, the `403 insufficient_scope` of a
missing scope, `404`, `409`, `422` — and the `429` itself carry the current state
of the window. Responses the authentication gate returns before a handler runs
(`401 invalid_api_key`, `api_key_expired`, `api_key_revoked`, `403
ip_not_allowed`, `club_suspended`, `test_key_not_allowed`, `503
external_unavailable`) may omit them: a key that was not accepted has no counted
window. Absence of headers is never "unlimited".

| Header | Meaning |
| --- | --- |
| `X-RateLimit-Limit` | The key's limit per window, e.g. `120`. |
| `X-RateLimit-Remaining` | Calls left in the current window, e.g. `87`. Never negative. |
| `X-RateLimit-Reset` | Upper bound in seconds until budget frees up. The API reports `60`, the full window; with a rolling window an old call drops out every second, so the real wait can be shorter. |
| `Retry-After` | Present on `429` only: seconds to wait before retrying. |

```http
HTTP/1.1 200 OK
Content-Type: application/json
Cache-Control: no-store
X-RateLimit-Limit: 120
X-RateLimit-Remaining: 87
X-RateLimit-Reset: 60
```

```http
HTTP/1.1 429 Too Many Requests
Content-Type: application/json
Retry-After: 60
X-RateLimit-Limit: 120
X-RateLimit-Remaining: 0
X-RateLimit-Reset: 60

{
  "code": "rate_limited",
  "message": "Rate limit exceeded for this API key. Retry after the window resets.",
  "details": { "limit": 120, "window_seconds": 60 }
}
```

Treat `X-RateLimit-Remaining` as the budget for the *rest of the minute* and
slow down before it reaches zero: a `429` wastes a round trip and, for writes,
risks confusion about whether the operation was applied.

## Choosing a limit

| Workload | Reasonable limit |
| --- | --- |
| Nightly report pull (a few dozen calls) | `30` |
| Queue display refreshing every 5 s | `60` |
| POS placing requests as they happen, with retries | `120` |
| Bulk reconciliation with pagination | `300` |
| Emergency / back-office automation | `600` |

Raising the limit does not raise the ceiling of good behaviour: if you find
yourself needing more than 300 rpm for reads, poll less and use `since`/cursor
parameters instead.

## A polite client

Three rules make a client well-behaved under this limiter:

1. **Pace writes.** Do not burst 200 request creations in one second; the queue
   itself serialises them and the burst only burns budget.
2. **Read the headers** and sleep when `X-RateLimit-Remaining` is low.
3. **Honour `Retry-After`** exactly; do not retry earlier and do not cap it at
   a smaller value.

```ts
async function limitedFetch(url: string, init: RequestInit, apiKey: string): Promise<Response> {
  for (let attempt = 0; attempt < 4; attempt += 1) {
    const response = await fetch(url, {
      ...init,
      headers: { Authorization: `Bearer ${apiKey}`, ...init.headers },
    });
    if (response.status !== 429) return response;
    const retryAfter = Number(response.headers.get("Retry-After") ?? "60");
    await new Promise((resolve) => setTimeout(resolve, retryAfter * 1000));
  }
  throw new Error("rate limited four times in a row");
}
```

```python
import time
import requests

def get_with_budget(url, api_key):
    response = requests.get(url, headers={"Authorization": f"Bearer {api_key}"}, timeout=10)
    remaining = int(response.headers.get("X-RateLimit-Remaining", "999"))
    if response.status_code == 429:
        time.sleep(int(response.headers.get("Retry-After", "60")))
        response = requests.get(url, headers={"Authorization": f"Bearer {api_key}"}, timeout=10)
    elif remaining < 5:
        # Be kind to the next call in this minute.
        time.sleep(2)
    return response
```

## Distributing work

* **Pagination is not free.** Each page is a call. A 30-day stats pull is one
  call, not one per day: `GET /stats/overview?period=month`.
* **Prefer one club-wide list** over N per-session calls:
  `GET /requests?session_id=…` returns the same data as a queue read but with
  cursor pagination designed for bulk reads.
* **Use conditional work.** Fetch the session list, and only act on sessions
  whose `status` you care about; do not poll every endpoint in a loop.
* **Spread cron jobs.** If several automations run at midnight, offset them by a
  few minutes. They share nothing except the wall clock, but they compete for
  the same connection pool on your side.

## When a limit is the wrong fix

If an integration hits the ceiling every night at the same time, the usual cause
is a polling loop where an event would do. The API has no webhooks in v1, so
acceptable substitutes are:

* poll the queue at a sensible interval (5–15 s) and only while a session is
  `active`;
* close the session after last call so the poller can stop;
* batch the nightly numbers into one `GET /stats/overview?period=day` call.

If that is still not enough, the honest answer is a higher per-key limit (up to
`600`) plus a narrower scope — not a second key used in parallel to multiply the
budget. Keys are limited per key on purpose; working around it is visible in the
usage log and defeats the audit trail.