---
id: recipes
title: Recipes
summary: Three automations you can ship — queue triage with a nightly close, POS-placed requests, and a nightly revenue reconciliation.
requires: []
order: 150
---

# Recipes

Three complete automations, each with runnable code. They are written against
the endpoints in this documentation and nothing else; copy them, replace the
environment variables, run them.

Every recipe assumes:

```sh
export DROPIT_API_BASE="https://<your-api-host>/v1/developer"
export DROPIT_API_KEY="dit_live_…"
```

and a key with the scopes listed at the top of the recipe. Create separate keys
per automation — that is what makes revoking one harmless.

## Recipe A — queue triage and a nightly close

**Scopes:** `sessions:read`, `sessions:write`, `requests:read`,
`requests:write`, (`stats:read` optional).

Two jobs in one script: accept pending requests whose style is on the venue's
allowlist, and close the session after last call. There is no price on a request
in v1, so "cheap to accept" is expressed as a **style allowlist** — the closest
honest rule the API supports.

```ts
// triage.ts — run every 20 s while a session is live, and once after close time.
const BASE = process.env.DROPIT_API_BASE!;
const KEY = process.env.DROPIT_API_KEY!;
const ACCEPT_STYLES = new Set(["house", "disco", "funk"]);
const CLOSE_AFTER_HOUR = 4; // local hour after which the session is closed
const PENDING_ONLY = "pending";

type Session = { id: string; status: string; timezone: string };
type QueueItem = {
  id: string;
  status: string;
  style: string | null;
  track: { title: string; artist: string };
};

async function api<T>(path: string, init: RequestInit = {}): Promise<T | null> {
  const response = await fetch(`${BASE}${path}`, {
    ...init,
    headers: {
      Authorization: `Bearer ${KEY}`,
      "Content-Type": "application/json",
      ...init.headers,
    },
  });
  if (response.status === 404) return null; // foreign or gone — skip, never retry
  if (response.status === 409) {
    console.warn(`skip ${path}: ${(await response.json()).code}`);
    return null; // state conflict: the next tick reconciles
  }
  if (!response.ok) throw new Error(`${response.status} ${await response.text()}`);
  return (await response.json()) as T;
}

async function liveSession(): Promise<Session | null> {
  // Filter on the server: a `created` or `paused_acceptance` night is not live.
  const page = await api<{ items: Session[] }>("/sessions?status=active&limit=1");
  return page?.items[0] ?? null;
}

function localHour(session: Session): number {
  return Number(
    new Intl.DateTimeFormat("en-GB", {
      hour: "2-digit",
      hour12: false,
      timeZone: session.timezone,
    }).format(new Date()),
  );
}

async function triage(session: Session) {
  const queue = await api<{ items: QueueItem[] }>(`/sessions/${session.id}/queue?sort=new`);
  for (const item of queue?.items ?? []) {
    if (item.status !== PENDING_ONLY) continue;
    const style = (item.style ?? "").toLowerCase();
    if (!ACCEPT_STYLES.has(style)) continue;
    await api(`/requests/${item.id}/accept`, { method: "POST" });
    console.log(`accepted ${item.track.artist} — ${item.track.title} (${style})`);
  }
}

async function maybeClose(session: Session) {
  if (localHour(session) < CLOSE_AFTER_HOUR) return;
  await api(`/sessions/${session.id}/close`, { method: "POST" });
  console.log(`closed session ${session.id} after last call`);
}

const session = await liveSession();
if (!session) {
  console.log("no active session — nothing to do");
} else {
  await triage(session);
  await maybeClose(session);
}
```

A `systemd` timer makes this a real worker:

```ini
# /etc/systemd/system/dropit-triage.service
[Unit]
Description=DROP IT queue triage
After=network-online.target

[Service]
Type=oneshot
EnvironmentFile=/etc/dropit/triage.env
ExecStart=/usr/bin/node /opt/dropit/triage.js
```

```ini
# /etc/systemd/system/dropit-triage.timer
[Unit]
Description=Run DROP IT triage every 20 seconds

[Timer]
OnBootSec=30s
OnUnitActiveSec=20s

[Install]
WantedBy=timers.target
```

Notes learned the hard way:

* **Stop polling when there is no active session.** The first call of each tick
  is the session list; if it is empty, the whole run is one request.
* **`409` is the normal answer while the room is not live.** Request creation
  and moderation answer `session_not_active` whenever the DJ has paused or not
  yet opened the session; do not treat it as a bug.
* **Do not auto-close by wall clock alone** unless the venue wants a hard stop.
  The platform's planned close (next 07:00 in the session zone) is the safety
  net; `CLOSE_AFTER_HOUR` is the business decision.
* **`409` is not an error** in a polling loop — the DJ paused, the session
  closed between your list and your write. Log and continue.
* **Accepting is visible to the DJ instantly.** If the set is an art, restrict
  the allowlist, or let the automation only *suggest* by keeping requests
  pending and reading the queue in the booth UI.

## Recipe B — POS/PMS places a request

**Scopes:** `catalog:read`, `sessions:read`, `requests:write`.

A table orders a bottle and picks two songs; the POS calls DROP IT. The important
part is not the HTTP call — it is the idempotency key, which must be derived from
the business event so a retry after a timeout never duplicates the request.

```python
# pos_bridge.py — called from the POS with (ticket_id, line_no, track_query)
import os
import re
import sys
import requests

BASE = os.environ["DROPIT_API_BASE"]
KEY = os.environ["DROPIT_API_KEY"]

session = requests.Session()
session.headers.update({"Authorization": f"Bearer {KEY}", "Content-Type": "application/json"})


def _normalise(text: str) -> str:
    return re.sub(r"\s+", " ", text.strip().lower())


def search_track(query: str) -> dict:
    response = session.get(
        f"{BASE}/catalog/search", params={"q": _normalise(query), "limit": 5}, timeout=10
    )
    response.raise_for_status()
    items = response.json()["items"]
    if not items:
        raise LookupError(f"no catalog match for {query!r}")
    return items[0]


def active_session_id() -> str:
    # Only `active` accepts requests; `created` and `paused_acceptance` do not.
    response = session.get(f"{BASE}/sessions", params={"status": "active", "limit": 1}, timeout=10)
    response.raise_for_status()
    items = response.json()["items"]
    if not items:
        raise LookupError("no active session: the DJ has not opened the room")
    return items[0]["id"]


def place_request(ticket_id: str, line_no: int, track_query: str, style: str | None = None) -> dict:
    track = search_track(track_query)
    session_id = active_session_id()
    idem = f"pos:{ticket_id}:{line_no}"

    response = session.post(
        f"{BASE}/sessions/{session_id}/requests",
        headers={"Idempotency-Key": idem},
        json={"track_id": track["id"], "style": style},
        timeout=10,
    )
    if response.status_code == 409:
        body = response.json()
        if body["code"] == "idempotency_conflict":
            raise RuntimeError("this ticket line already has a different track")
        if body["code"] == "session_not_active":
            raise RuntimeError("the queue is not accepting requests right now")
        raise
    if response.status_code == 422:
        body = response.json()
        field = body.get("details", {}).get("field")
        if field == "track_id":
            raise LookupError("track is not in the cache any more; search again")
        raise RuntimeError(f"invalid request: {body['message']}")
    if response.status_code == 404:
        raise LookupError("session not found; refresh the POS cache")
    response.raise_for_status()
    return response.json()
```

```python
if __name__ == "__main__":
    ticket, line, query = sys.argv[1], int(sys.argv[2]), " ".join(sys.argv[3:])
    created = place_request(ticket, line, query, style="house")
    print(f"request {created['id']} is {created['status']}")
```

`track_id` is the `id` returned by the catalog search — the platform's cached
track id, not the music provider's own id. Cache the search result per track so a
busy night does not burn the rate limit on the same query.

Operational rules for a POS bridge:

* one idempotency key per **ticket line**, generated when the order is taken, not
  when the HTTP call is made;
* persist the returned request `id` against the ticket line, so the receipt can
  show "in the queue" and support can find it later;
* if `status` is already `accepted`, print "accepted" rather than "waiting" —
  the DJ may have accepted it between your call and the print;
* `409 session_not_active` means the room stopped accepting requests between the
  list and the write: tell the waiter, do not retry into a closed night.
* `404` on the session means the id is gone (or never belonged to your club):
  re-read the active session and try once more at most.

## Recipe C — nightly pull and reconciliation

**Scopes:** `stats:read`, `sessions:read`, `requests:read`.

After the session closes, pull the night, write it to your warehouse, and compare
the platform's captured revenue with the till. Keep money as integer minor units
all the way into the database; format only at render time.

```ts
// nightly.ts
const BASE = process.env.DROPIT_API_BASE!;
const KEY = process.env.DROPIT_API_KEY!;

type Overview = {
  period: "day" | "week" | "month";
  from: string;
  to: string;
  sessions: number;
  active_session: { id: string; status: string } | null;
  requests: { total: number; pending: number; accepted: number; rejected: number; played: number };
  revenue: { currency: string; captured_minor: number };
  top_tracks: Array<{ title: string; artist: string; count: number }>;
};

async function get<T>(path: string): Promise<T> {
  const response = await fetch(`${BASE}${path}`, { headers: { Authorization: `Bearer ${KEY}` } });
  if (!response.ok) throw new Error(`${path}: ${response.status} ${await response.text()}`);
  return (await response.json()) as T;
}

const overview = await get<Overview>("/stats/overview?period=day");

// The club-wide request list is the audit trail behind the aggregate.
type Request = { id: string; status: string; created_at: string };
const requests: Request[] = [];
let cursor: string | null = null;
do {
  const query = new URLSearchParams({ limit: "100", ...(cursor ? { cursor } : {}) });
  const page = await get<{ items: Request[]; next_cursor: string | null }>(
    `/requests?${query.toString()}`,
  );
  requests.push(...page.items);
  cursor = page.next_cursor;
} while (cursor);

// Invariant check: the aggregate must agree with the list it summarises.
const played = requests.filter((request) => request.status === "played").length;
if (played !== overview.requests.played) {
  console.warn(
    `mismatch: list says ${played} played, overview says ${overview.requests.played}` +
      ` (window ${overview.from} → ${overview.to})`,
  );
}

// Till reconciliation: one currency, integer minor units, no float anywhere.
const tillRows: Array<{ currency: string; amount_minor: number }> = await readTill(overview.from, overview.to);
const till = tillRows
  .filter((row) => row.currency === overview.revenue.currency)
  .reduce((sum, row) => sum + row.amount_minor, 0);
const delta = overview.revenue.captured_minor - till;

console.log(
  `${overview.revenue.currency}: platform ${overview.revenue.captured_minor}, ` +
    `till ${till}, delta ${delta}`,
);
if (delta !== 0) {
  await flagForReview({ window: { from: overview.from, to: overview.to }, delta });
}

declare function readTill(from: string, to: string): Promise<Array<{ currency: string; amount_minor: number }>>;
declare function flagForReview(payload: unknown): Promise<void>;
```

```python
import os
import requests

BASE = os.environ["DROPIT_API_BASE"]
KEY = os.environ["DROPIT_API_KEY"]
headers = {"Authorization": f"Bearer {KEY}"}

overview = requests.get(f"{BASE}/stats/overview", params={"period": "day"}, headers=headers, timeout=15)
overview.raise_for_status()
data = overview.json()

currency = data["revenue"]["currency"]
captured = data["revenue"]["captured_minor"]  # integer minor units

print(f"window {data['from']} → {data['to']}")
print(f"sessions={data['sessions']} requests={data['requests']['total']}")
print(f"revenue={captured} {currency} minor units")
for track in data["top_tracks"]:
    print(f"  {track['count']:>3}x {track['artist']} — {track['title']}")
```

Reconciliation rules that prevent arguments:

1. **Use the window the API reports.** `from`/`to` are the server's boundaries;
   re-deriving them client-side invites off-by-minutes disputes.
2. **Compare only like with like.** Platform revenue is what the platform
   captured in its own currency; a till that mixes cash and card does not sum to
   it without a cash/card breakdown on your side.
3. **Never convert to float.** Store `captured_minor` as an integer with the
   currency beside it. If you must convert, do it in the reporting layer.
4. **Record the delta, not just the alarm.** A nightly row of
   `(from, to, currency, platform_minor, till_minor, delta)` answers "how long
   has this been drifting" months later.
5. **A mismatch is usually a window or a currency**, not a theft. Check those
   before escalating.

## Common patterns across all three

| Concern | Pattern |
| --- | --- |
| Retries | Exponential backoff with jitter on `429` / `5xx`, honour `Retry-After` exactly. |
| Duplicate writes | Idempotency key derived from the business event, persisted before the first attempt. |
| Missing resource | `404` means "not in your club" — skip, do not retry. |
| State changes under you | `409` means "re-read and reconcile", not "retry immediately". |
| Secrets | One key per automation, minimum scopes, environment variable or secret manager. |
| Logging | Log `code`, path and status; never the key, never the QR token. |
| Stopping | Poll only while a session is `active`; stop after close. |

Every recipe is a small, single-purpose process behind a timer. If one of them
fails, the night continues: that is the design constraint this API was shaped
around.