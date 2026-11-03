---
id: catalog
title: Catalog
summary: Search the track catalog, understand the hit shape and the canonical key, and cache results without going stale.
requires: ["catalog:read"]
order: 120
---

# Catalog

The catalog endpoint is how an integration finds the `track_id` that
`POST /sessions/{id}/requests` requires. It searches the club's track cache,
falling back to the external provider and caching what it finds.

## GET /v1/developer/catalog/search

Requires `catalog:read`.

```
GET /v1/developer/catalog/search?q=daft+punk&limit=10
```

| Query | Required | Rules | Default |
| --- | --- | --- | --- |
| `q` | yes | 2–80 characters after trimming. Shorter or longer answers `422 validation_error` with `details.field = "q"`. | — |
| `limit` | no | `1..25`; outside the range answers `422 validation_error` with `details.field = "limit"`. | `10` |

There is no `style` filter on the public catalog search: style is a property of
a request, not of the catalog. Send it on request creation instead.

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

## Response shape

| Field | Type | Notes |
| --- | --- | --- |
| `items` | array | Up to `limit` hits, best match first. |
| `items[].id` | UUID | **The `track_id` to use on request creation.** This is the platform's track-cache id, not the provider's id. |
| `items[].title` | string | Track title as the provider spells it. |
| `items[].artist` | string | Artist name. |
| `items[].album` | string | Album, may be empty. |
| `items[].duration_ms` | integer | Duration in milliseconds. |
| `items[].isrc` | string | Recording code, when the provider supplies it. |
| `items[].artwork_url` | string | Cover art URL. |
| `items[].preview_url` | string | Short audio preview, when available. |
| `items[].explicit` | boolean | Explicit-content flag from the provider. |
| `cached` | boolean | `true` when this answer came from the cache rather than a live provider call. |

Provider identity (`source`, `source_id`, `source_url`) is internal: it is used
for deduplication and the "same song" count, but the public API exposes neither
the provider nor its ids. Search returns the platform's track `id`, and that is
what a request takes.

`cached: false` is not an error — it means the provider was consulted and the
result is now in the cache for the next query.

## Track identity

Use `items[].id` from this response as the `track_id` for
`POST /sessions/{id}/requests`. It is stable for the cached track, so you can
store it and reuse it.

The "same song" logic behind `sort=hot` and `same_track_count` groups tracks by
an internal canonical key derived from the provider entry. Two search hits with
different `id`s are different songs even if the title matches; the same `id` is
the same song. Because the canonical key is internal and absent from responses:

* store the returned `id` and the title/artist for display;
* resolve request `track_id`s only from search hits — never reconstruct them
  from titles;
* ISRC is useful for your own deduplication, but the queue's grouping does not
  follow it, so do not expect the two counts to agree.

## Caching

The endpoint caches in two layers: query results in the club's cache and hits in
the shared track cache. Practical rules:

* **Cache hits are cheap** (`cached: true`) but still count against your rate
  limit. Do not poll the catalog on a timer; search when a human is choosing a
  track.
* **Query strings are part of the cache key.** `daft punk` and `Daft Punk` may
  produce two entries. Normalise to lowercase, trim, and collapse whitespace in
  your client to keep the hit rate high.
* **Cached hits can outlive a provider delisting.** A track that exists in the
  cache can still be placed as a request; the DJ decides whether to play it.
* **Artwork and preview URLs are provider-hosted.** They may expire or change
  host; treat the URL as an opaque string with a lifetime, and do not rewrite
  its query parameters.

```ts
const cache = new Map<string, { at: number; hits: unknown[] }>();
const TTL_MS = 5 * 60 * 1000;

async function searchTrack(query: string, apiKey: string) {
  const key = query.trim().toLowerCase().replace(/\s+/g, " ");
  const hit = cache.get(key);
  if (hit && Date.now() - hit.at < TTL_MS) return hit.hits;

  const url = new URL(`${process.env.DROPIT_API_BASE}/catalog/search`);
  url.searchParams.set("q", key);
  url.searchParams.set("limit", "5");
  const response = await fetch(url, { headers: { Authorization: `Bearer ${apiKey}` } });
  if (!response.ok) throw new Error(`${response.status} ${await response.text()}`);
  const body = (await response.json()) as { items: unknown[] };
  cache.set(key, { at: Date.now(), hits: body.items });
  return body.items;
}
```

A five-minute TTL is enough for a staff tool: catalog results for a given query
rarely change, and a stale list only means a slightly older ordering.

The `cached` flag describes the server side, not your cache: `false` means the
provider was called and the result was written to the platform cache, `true`
means it was served from it. Both are successful searches.

## Errors

| Status | `code` | Cause |
| --- | --- | --- |
| 400 | `bad_request` | `q` is missing entirely (the query string cannot be parsed). |
| 422 | `validation_error` | `q` shorter than 2 or longer than 80, or `limit` outside `1..25`; `details.field` names the parameter. |
| 403 | `insufficient_scope` | The key has no `catalog:read`. |
| 503 | `catalog_unavailable` | The external provider could not be reached and no cache entry exists for the query. |
| 503 | `external_unavailable` | The platform's own dependency is down. |

`catalog_unavailable` is the only endpoint-level outage you should expect, and it
is transient: retry with backoff, and prefer a cached query while it lasts. It
does not affect sessions, requests or stats — a catalog outage does not stop the
night.