---
id: errors
title: Errors
summary: The single error envelope, every code the API can return, and how to retry safely.
requires: []
order: 50
---

# Errors

Every failure — authentication, permission, validation, state conflict, rate
limit, upstream outage — uses one envelope. Branch on `code`, never on the
human-readable `message`: the message may be reworded, the code will not be.

```json
{
  "code": "insufficient_scope",
  "message": "The API key does not grant the scope required by this endpoint.",
  "details": {
    "required_scope": "stats:read",
    "granted_scopes": ["docs:read", "profile:read"]
  }
}
```

| Field | Type | Present | Meaning |
| --- | --- | --- | --- |
| `code` | string | always | Stable machine-readable identifier, `snake_case`. Safe to switch on. |
| `message` | string | always | English, human-readable, may change. Log it, do not parse it. |
| `details` | object | usually | Structured context: which scope, which field, which limit. Shape depends on `code`. |

`details` is absent or `{}` when there is nothing useful to add. It never
contains secret material, and never reveals another club's data — a foreign id
is simply `404 not_found`.

## Codes by status

### 400 — malformed request

| `code` | When |
| --- | --- |
| `bad_request` | The body is not valid JSON, or a required part of the request could not be read at all. `details.field` names the part (`body`, `header`). |

### 401 — not authenticated

| `code` | When | Fix |
| --- | --- | --- |
| `invalid_api_key` | Missing header, a header that is not a `dit_…` key, unknown prefix, or wrong secret. | Send `Authorization: Bearer dit_…` or `X-API-Key`. The API deliberately does not say which part was wrong. |
| `api_key_expired` | The key passed its expiry date. | Create a new key in the panel. |
| `api_key_revoked` | The key was revoked. | Create a new key; if you rotated, deploy the new secret. |

### 403 — authenticated, not allowed

| `code` | When | `details` |
| --- | --- | --- |
| `insufficient_scope` | The key lacks the scope the endpoint requires. Checked before any data is touched. | `{ required_scope, granted_scopes }` |
| `ip_not_allowed` | The caller's IP is outside the key's allowlist. Fails closed: an unparsable allowlist rule denies too. | `{}` |
| `club_suspended` | The owning club is not `active`. | `{}` |
| `test_key_not_allowed` | A `dit_test_…` key was used against a deployment in live payment mode. | `{}` |

### 404 — not found in your club

| `code` | When |
| --- | --- |
| `not_found` | The id does not exist **within your club**: sessions, requests, DJs and pricing policies all answer this single code. `details.resource` names what was looked up (`Session`, `Request`, `DJ`, …). A resource that belongs to another club answers exactly this — not `403` — so ids cannot be probed for existence across tenants. |

There are no per-resource 404 codes in the Developer API: `session_not_found`
and `request_not_found` belong to the older guest/panel surface, not to `/v1/developer`.

### 409 — the state does not allow it

| `code` | When | What to do |
| --- | --- | --- |
| `session_closed` | The session is already `closed`: closing it again, or issuing a QR token for it. | Treat "already closed" as success and stop acting on the session. |
| `session_not_active` | Request creation on a session that is not `active` — still `created`, `paused_acceptance`, or `closed`. | Read the session and surface its real `status`; retry only while it is (or becomes) `active`. |
| `invalid_request_transition` | Moderation is not allowed from the request's current state (e.g. `accept` after `played`). | Re-read the request; do not retry the same action. |
| `idempotency_conflict` | The same `Idempotency-Key` was used with a different body. | Use a new key for the new payload; a retry must send the identical body. |
| `conflict` | A generic state conflict not covered by a more specific code. | Re-read the resource and reconcile before retrying. |

### 422 — validation failed

| `code` | When | `details` |
| --- | --- | --- |
| `validation_error` | A query parameter, header or body field is missing, malformed or out of range: bad `limit`/`status`/`cursor`/`period`, an unknown `track_id`, an invalid `timezone`, a missing `Idempotency-Key`. | `{ field }` naming the offending parameter |
| `scope_widening_forbidden` | A panel key update tried to add a scope. Key management lives in the club panel, not in the Developer API, but the code is documented here because it can appear in a copy-pasted example. | `{ added }` — create a new key instead |

### 429 — rate limited

| `code` | When | Headers |
| --- | --- | --- |
| `rate_limited` | The key exceeded its per-minute calls. `details` is `{ limit, window_seconds }`. | `Retry-After`, `X-RateLimit-Limit`, `X-RateLimit-Remaining`, `X-RateLimit-Reset` |

See **Rate limits** for the window and the backoff recipe.

### 5xx — retryable

| Status | `code` | When |
| --- | --- | --- |
| 500 | `internal` | An unexpected server error. The request may or may not have been applied. |
| 503 | `external_unavailable` | A dependency (database, catalog provider) is down or not configured. |
| 503 | `catalog_unavailable` | The external track catalog could not be reached. Search is temporarily unavailable; the rest of the API works. |

Retry `5xx` with exponential backoff and jitter. For a `POST`, re-read the
resource before retrying so you do not duplicate a side effect that actually
succeeded. If the write supports idempotency (request creation does), reuse the
same `Idempotency-Key` — that is the safe retry.

## Retry decision table

| Response | Retry? | How |
| --- | --- | --- |
| 200/201/204 | — | Success. |
| 400, 422 | No | Fix the payload. Retrying unchanged fails identically. |
| 401 | No | Fix credentials. A key that is expired or revoked never recovers. |
| 403 | No | Fix scopes, IP or environment. |
| 404 | No | Fix the id. It is absent in your club. |
| 409 `session_not_active` | Yes, later | Back off (30–120 s) while the session is paused, and stop once it is `closed`. |
| 409 others | No | Re-read the resource and reconcile. |
| 429 | Yes | Sleep `Retry-After` seconds, then retry. |
| 5xx | Yes | Exponential backoff, capped; reuse the idempotency key on writes. |

## A retry helper that respects the contract

```ts
type ApiErrorBody = { code: string; message: string; details?: Record<string, unknown> };

const RETRYABLE = new Set(["rate_limited", "internal", "external_unavailable", "catalog_unavailable"]);

export async function callWithRetry<T>(
  url: string,
  init: RequestInit,
  apiKey: string,
  attempts = 5,
): Promise<T> {
  for (let attempt = 0; ; attempt += 1) {
    const response = await fetch(url, {
      ...init,
      headers: { Authorization: `Bearer ${apiKey}`, "Content-Type": "application/json", ...init.headers },
    });
    if (response.ok) return (await response.json()) as T;

    const body = (await response.json().catch(() => null)) as ApiErrorBody | null;
    const retryable = response.status === 429 || response.status >= 500;
    if (!retryable || !body || !RETRYABLE.has(body.code) || attempt >= attempts) {
      throw new Error(`${response.status} ${body?.code ?? "unknown"}: ${body?.message ?? ""}`);
    }
    const retryAfter = Number(response.headers.get("Retry-After") ?? "0");
    const backoff = retryAfter > 0 ? retryAfter * 1000 : Math.min(2 ** attempt * 500, 8000);
    await new Promise((resolve) => setTimeout(resolve, backoff + Math.random() * 250));
  }
}
```

```python
import time
import random
import requests

RETRYABLE = {"rate_limited", "internal", "external_unavailable", "catalog_unavailable"}

def call_with_retry(method, url, api_key, attempts=5, **kwargs):
    for attempt in range(attempts + 1):
        response = requests.request(
            method, url,
            headers={"Authorization": f"Bearer {api_key}"},
            timeout=10, **kwargs,
        )
        if response.ok:
            return response.json()
        body = response.json() if response.content else {}
        if (
            attempt == attempts
            or body.get("code") not in RETRYABLE
            or not (response.status_code == 429 or response.status_code >= 500)
        ):
            raise RuntimeError(f"{response.status_code} {body.get('code')}: {body.get('message')}")
        retry_after = int(response.headers.get("Retry-After", "0"))
        delay = retry_after if retry_after else min(2 ** attempt * 0.5, 8.0)
        time.sleep(delay + random.random() * 0.25)
```

## Reading `details`

| Code | Typical `details` |
| --- | --- |
| `insufficient_scope` | `{ "required_scope": "qr:write", "granted_scopes": ["sessions:read"] }` |
| `rate_limited` | `{ "limit": 120, "window_seconds": 60 }` |
| `validation_error` | `{ "field": "limit" }` — the constraint is in `message` |
| `not_found` | `{ "resource": "Session" }` |
| `scope_widening_forbidden` | `{ "added": ["stats:read"] }` |

Use `details` to build actionable operator messages ("this key cannot issue the
QR; ask the owner to create a key with `qr:write`") rather than dumping the raw
English text.

## What errors never contain

* the API key, any prefix of the secret, or its hash;
* another club's name, ids or data;
* stack traces, SQL, or internal hostnames;
* guest identity of any kind.

If you see any of the above in an error body, treat it as a security report and
send it to the DROP IT maintainers with the prefix of the key (never the
secret).