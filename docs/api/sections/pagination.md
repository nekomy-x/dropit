---
id: pagination
title: Pagination
summary: Opaque cursors, stable descending order, 1–100 items per page, and a loop that never duplicates or skips.
requires: []
order: 70
---

# Pagination

Every list endpoint returns the same shape. There is no page number and no
offset — only a cursor:

```json
{
  "items": [ { "id": "…" } ],
  "next_cursor": "MTc2MTY4MzI0NTAwMDAwMHwzZjJiMWM0ZS04YTZkLTRjMWYtOWUyYS01Yjd6MGM5ZjFhMmU"
}
```

When `next_cursor` is `null`, the list is exhausted. Do not treat a page with
fewer than `limit` items as the end: only `next_cursor` decides.

## Parameters

| Parameter | Applies to | Rules |
| --- | --- | --- |
| `limit` | all lists | Integer `1..100`, default `50`. Outside the range answers `422 validation_error` with `details.field = "limit"`. |
| `cursor` | all lists | Opaque string from the previous response's `next_cursor`. Do not decode, modify or construct one; a malformed value answers `422 validation_error` with `details.field = "cursor"`. |
| `status` | `GET /sessions`, `GET /requests` | Filter by state; unknown values answer `422 validation_error`. |
| `session_id` | `GET /requests` | Restrict the club-wide list to one session. |
| `sort` | `GET /sessions/{id}/queue` | `new` (arrival order) or `hot` (same-track count, then arrival). |

If you care about a deterministic page size, send `limit` explicitly. The
default is `50`; it is documented here so a page never surprises you, and it may
grow inside `v1` only in a way that cannot break a correct client.

## Cursor semantics

The cursor encodes the position in a **stable total order**: `created_at`
descending, then `id` descending. It is an opaque base64url token; the fact
that it looks decodable is an implementation detail, not a contract.

* **Stable:** an item that exists for the whole walk appears exactly once.
* **Descending:** newest first. A new item created during the walk sorts ahead
  of the first page, so it will not appear in a walk that started earlier —
  that is expected, not a missed item. Fetch the first page again if you need
  "since now".
* **Filter-bound:** a cursor belongs to the filter set it was issued under.
  Reusing a cursor with different `status`/`session_id`/`sort` is not supported;
  start from `null` when you change the filters.
* **Opaque:** a cursor from another key, another club or another endpoint is
  invalid (`422 validation_error`) — it cannot be used to read across tenants.

## Walking a list

```ts
async function* pages<T>(path: string, apiKey: string): AsyncGenerator<T[]> {
  const base = process.env.DROPIT_API_BASE!;
  let cursor: string | null = null;
  do {
    const url = new URL(`${base}${path}`);
    url.searchParams.set("limit", "100");
    if (cursor) url.searchParams.set("cursor", cursor);
    const response = await fetch(url, { headers: { Authorization: `Bearer ${apiKey}` } });
    if (!response.ok) throw new Error(`${response.status} ${await response.text()}`);
    const page = (await response.json()) as { items: T[]; next_cursor: string | null };
    yield page.items;
    cursor = page.next_cursor;
  } while (cursor);
}

for await (const requests of pages<{ id: string; status: string }>(
  "/requests?status=played",
  process.env.DROPIT_API_KEY!,
)) {
  for (const request of requests) console.log(request.id);
}
```

```python
import os
import requests

def walk(path, api_key):
    base = os.environ["DROPIT_API_BASE"]
    cursor = None
    while True:
        params = {"limit": 100}
        if cursor:
            params["cursor"] = cursor
        response = requests.get(
            f"{base}{path}",
            headers={"Authorization": f"Bearer {api_key}"},
            params=params,
            timeout=10,
        )
        response.raise_for_status()
        page = response.json()
        yield from page["items"]
        cursor = page["next_cursor"]
        if not cursor:
            break

for request in walk("/requests?status=played", os.environ["DROPIT_API_KEY"]):
    print(request["id"])
```

## Cursor lifetime

* Cursors are valid as long as the referenced row exists in the ordering. They
  are not signed with an expiry, but a cursor that points at a deleted item is
  still resolvable because the cursor carries ordering values, not a row id.
* With 30-day usage/event retention and nights being ephemeral, a cursor older
  than a few weeks is not useful. For a full historical export, page by
  `session_id` and time window instead of walking a huge cursor chain.
* Do not persist cursors as application state. Persist what you have already
  processed (ids or timestamps), and start a new walk when you resume.

## Common mistakes

| Mistake | Consequence |
| --- | --- |
| Looping while `items.length === limit` | Stops early on a short last page or loops forever when the server caps `limit` below what you asked. |
| Building a cursor manually | Invalid cursor; `422`, or worse, a silently wrong page. |
| Reusing a cursor after changing `status` | Undefined ordering for the new filter; results may repeat or skip. |
| Requesting `limit=500` | Clamped to `100`; the response does not error, so read `items.length`, not your requested value. |
| Treating `next_cursor` as an id | It is positional, not an identifier; it cannot fetch a single item. |

## Worked example

`GET /requests?status=pending&limit=2` returns:

```json
{
  "items": [
    { "id": "b71d5e02-9c34-4a6f-8d1b-0e2f4a6c8d10", "status": "pending" },
    { "id": "c82e6f13-0d45-4b70-9e2c-1f3a5b7d9e21", "status": "pending" }
  ],
  "next_cursor": "MTc2MTY4MzI0NTAwMDAwMHxjODJlNmYxMy0wZDQ1LTRiNzAtOWUyYy0xZjNhNWI3ZDllMjE"
}
```

`GET /requests?status=pending&limit=2&cursor=MTc2MTY4MzI0NTAwMDAwMHxjODJlNmYxMy0wZDQ1LTRiNzAtOWUyYy0xZjNhNWI3ZDllMjE`
returns the next two, and `next_cursor` becomes `null` after the last one.
The cursor is opaque: if you paste it into a decoder you will get bytes, but the
only supported operation is passing it back unchanged.