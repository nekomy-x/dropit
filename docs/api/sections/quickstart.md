---
id: quickstart
title: Quickstart
summary: Create a key in the panel, make your first calls with curl, TypeScript or Python, and place a request without duplicates.
requires: []
order: 20
---

# Quickstart

This page takes you from "no credentials" to a real request in the queue. It
assumes the club has an active session, or that you can create one.

## 1. Create an API key in the panel

Keys are not created through the API — you create them as a club owner or
manager in the club panel:

1. Open the **Club panel** and sign in.
2. Go to **Settings** in the sidebar, then **Developer access**.
3. Choose **Create key**, pick a preset, and give the key a name that says where
   it lives (`pos-integration`, `nightly-report`).
4. Copy the secret from the one-time reveal dialog. It is shown once and never
   again; only its SHA-256 hash is stored.

For the first run choose the preset **Read-only integration**. It includes
`docs:read`, `profile:read`, `sessions:read`, `requests:read`, `catalog:read`
and `stats:read`, but cannot change anything. You will add write scopes by
creating a second key later — scopes are never widened on an existing key.

Store the secret in an environment variable and never in code:

```sh
export DROPIT_API_KEY="dit_live_k4m2p9x7qa_9f8e…"
export DROPIT_API_BASE="https://<your-api-host>/v1/developer"
# Test Mode stand: export DROPIT_API_BASE="http://150.241.68.57/v1/developer"
```

## 2. Identify the key

`GET /me` needs no scope. Use it in health checks and to confirm which key a
deployment actually has.

```sh
curl -s "$DROPIT_API_BASE/me" \
  -H "Authorization: Bearer $DROPIT_API_KEY"
```

```json
{
  "key_id": "a1c2d3e4-5f60-4718-9a2b-3c4d5e6f7081",
  "prefix": "k4m2p9x7qa",
  "environment": "live",
  "scopes": [
    "docs:read",
    "profile:read",
    "sessions:read",
    "requests:read",
    "catalog:read",
    "stats:read"
  ],
  "rate_limit_per_min": 120,
  "club_id": "7d3e9b21-4c5a-4f8e-9b1d-2a6c8e0f3b47",
  "api_version": "1.0.0",
  "docs_url": "/v1/developer/docs",
  "openapi_url": "/v1/developer/openapi.json"
}
```

If this answers `401 invalid_api_key`, the header or the secret is wrong — see
**Authentication**. If it answers `403 test_key_not_allowed`, you are holding a
`test` key against a live deployment.

## 3. Read the club profile

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
    "address": "Calle Mayor 14",
    "country": "ES",
    "email": "ops@clubnoche.example",
    "phone": "+34 600 000 000",
    "logo_url": "https://cdn.example/club-noche.png",
    "timezone": "Europe/Madrid",
    "status": "active"
  }
}
```

The profile is read-only in v1. It is the stable way to map a DROP IT club to a
venue record in your own system: use `club.id` as the key and `club.slug` as the
friendly name.

## 4. Find the current session

```sh
curl -s "$DROPIT_API_BASE/sessions?status=active&limit=10" \
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

If there is no active session, create one (requires `sessions:write`):

```sh
curl -s -X POST "$DROPIT_API_BASE/sessions" \
  -H "Authorization: Bearer $DROPIT_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"timezone":"Europe/Madrid"}'
```

A created session is in `created` state. Opening the room for requests is a DJ
action in the DJ console, not an API action — the API creates and closes
sessions, the DJ decides when the queue accepts requests.

## 5. Find a track and place a request

Search the catalog first (requires `catalog:read`):

```sh
curl -s "$DROPIT_API_BASE/catalog/search?q=daft%20punk&limit=3" \
  -H "Authorization: Bearer $DROPIT_API_KEY"
```

```json
{
  "items": [
    {
      "id": "0d9e8f7a-6b5c-4d3e-8f2a-1b0c9d8e7f60",
      "title": "One More Time",
      "artist": "Daft Punk",
      "album": "Discovery",
      "duration_ms": 320357,
      "isrc": "GBDUW0000059",
      "artwork_url": "https://is1-ssl.mzstatic.com/…/100x100bb.jpg",
      "preview_url": "https://audio-ssl.itunes.apple.com/…/preview.m4a",
      "explicit": false
    }
  ],
  "cached": true
}
```

The `id` is the value to send as `track_id`. There is no free-text title path:
search first, then request by id.

Place the request (requires `requests:write`). The `Idempotency-Key` header is
**required**: it is what makes retries safe.

```sh
curl -s -X POST "$DROPIT_API_BASE/sessions/3f2b1c4e-8a6d-4c1f-9e2a-5b7d0c9f1a2e/requests" \
  -H "Authorization: Bearer $DROPIT_API_KEY" \
  -H "Content-Type: application/json" \
  -H "Idempotency-Key: 6f0c1b2a-7d3e-4a5b-9c8d-1e2f3a4b5c6d" \
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

Sending the same `Idempotency-Key` again returns the stored response instead of
creating a second request. Reuse the key on every retry of the same business
action; use a new key for a genuinely new request.

## 6. The same flow in TypeScript

```ts
const BASE = process.env.DROPIT_API_BASE!; // https://<host>/v1/developer
const KEY = process.env.DROPIT_API_KEY!;

type ApiError = { code: string; message: string; details?: unknown };

async function call<T>(path: string, init: RequestInit = {}): Promise<T> {
  const response = await fetch(`${BASE}${path}`, {
    ...init,
    headers: {
      Authorization: `Bearer ${KEY}`,
      "Content-Type": "application/json",
      ...init.headers,
    },
  });
  if (!response.ok) {
    const error = (await response.json()) as ApiError;
    throw new Error(`${response.status} ${error.code}: ${error.message}`);
  }
  return (await response.json()) as T;
}

const me = await call<{ prefix: string; scopes: string[] }>("/me");

const sessions = await call<{ items: Array<{ id: string; status: string }> }>(
  "/sessions?status=active&limit=10",
);

const queue = await call<{ items: Array<{ id: string; title: string }> }>(
  `/sessions/${sessions.items[0].id}/queue?sort=new`,
);

const created = await call<{ id: string; status: string }>(
  `/sessions/${sessions.items[0].id}/requests`,
  {
    method: "POST",
    headers: { "Idempotency-Key": crypto.randomUUID() },
    body: JSON.stringify({ track_id: "0d9e8f7a-6b5c-4d3e-8f2a-1b0c9d8e7f60", style: "house" }),
  },
);
console.log(me.prefix, queue.items.length, created.id, created.status);
```

## 7. The same flow in Python

```python
import os
import requests

BASE = os.environ["DROPIT_API_BASE"]  # https://<host>/v1/developer
KEY = os.environ["DROPIT_API_KEY"]

session = requests.Session()
session.headers.update({"Authorization": f"Bearer {KEY}"})

def call(method: str, path: str, **kwargs):
    response = session.request(method, f"{BASE}{path}", timeout=10, **kwargs)
    if not response.ok:
        error = response.json()
        raise RuntimeError(f"{response.status_code} {error['code']}: {error['message']}")
    return response.json()

me = call("GET", "/me")
sessions = call("GET", "/sessions", params={"status": "active", "limit": 10})
session_id = sessions["items"][0]["id"]

created = call(
    "POST",
    f"/sessions/{session_id}/requests",
    headers={"Idempotency-Key": "6f0c1b2a-7d3e-4a5b-9c8d-1e2f3a4b5c6d"},
    json={"track_id": "0d9e8f7a-6b5c-4d3e-8f2a-1b0c9d8e7f60", "style": "house"},
)
print(me["prefix"], created["id"], created["status"])
```

Set a timeout on every call. The API is synchronous and normally answers in
tens of milliseconds; a hung connection is a network problem, not a reason to
retry blind.

## 8. What to do when a call fails

* `401` / `403` — credentials or permissions. Read `code`, not the message:
  `invalid_api_key`, `insufficient_scope`, `ip_not_allowed`,
  `test_key_not_allowed`, `club_suspended`.
* `404` — the id does not exist **in your club**. It also means "another club's
  id": the API never confirms foreign resources.
* `409` — the state does not allow the action: `session_not_active`,
  `session_closed`, `invalid_request_transition`, `idempotency_conflict`.
* `422` — validation failed; `details` names the field.
* `429` — rate limit; wait for `Retry-After` seconds.
* `5xx` — retry with exponential backoff; the operation may or may not have
  been applied, so re-read the resource before retrying a write.

The full list with examples is in the **Errors** guide.