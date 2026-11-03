---
id: authentication
title: Authentication
summary: Key format, both supported headers, the 401 codes, rotation, revocation, and why a key never belongs in a browser.
requires: []
order: 30
---

# Authentication

Every Developer API call needs an API key. There are no anonymous endpoints, and
no OAuth: one header, one credential.

## Key format

```
dit_<environment>_<prefix>_<secret>
dit_live_k4m2p9x7qa_9f8e1d…            # 64 hex characters of secret
```

| Part | Meaning | Safe to log? |
| --- | --- | --- |
| `dit` | Fixed scheme prefix. | yes |
| `live` \| `test` | Environment the key belongs to. | yes |
| 10 hex characters | Public lookup id, unique per key. Shown in the panel and in support tickets. | yes |
| 64 hex characters | 256 bits of secret entropy, generated once. | **never** |

The secret is returned exactly once, in the response that creates or rotates the
key, with `Cache-Control: no-store`. The server stores only
`sha256(plaintext)` plus the prefix; there is no way to recover a lost secret.
If you lose it, rotate the key.

## Sending the key

Two transports, identical behaviour. Use exactly one per request.

```sh
# Bearer, the default
curl -s "$DROPIT_API_BASE/me" -H "Authorization: Bearer $DROPIT_API_KEY"

# Equivalent header form
curl -s "$DROPIT_API_BASE/me" -H "X-API-Key: $DROPIT_API_KEY"
```

Precedence: if `Authorization: Bearer dit_…` is present, it wins; `X-API-Key` is
read only when no usable Bearer value exists. A header that does not start with
`dit_` is ignored, and the request is answered `401 invalid_api_key`.

Never send the key in a query string. It ends up in proxy logs, browser history
and referrer headers.

## 401 responses

| Status | `code` | Meaning |
| --- | --- | --- |
| 401 | `invalid_api_key` | No key, a malformed key, an unknown prefix, or a wrong secret. Deliberately indistinguishable — the API does not confirm that a prefix exists. |
| 401 | `api_key_expired` | The key passed its `expires_at`. Create a new key in the panel. |
| 401 | `api_key_revoked` | The key was revoked. Rotate or create a new one. |
| 403 | `insufficient_scope` | The key is valid but lacks the scope; `details.required_scope` names it and `details.granted_scopes` lists what you have. |
| 403 | `ip_not_allowed` | The request came from an address outside the key's allowlist. |
| 403 | `club_suspended` | The owning club is not `active`. |
| 403 | `test_key_not_allowed` | A `test` key was used on a live deployment. |

Authentication is checked before scopes, scopes before the handler, so a
revoked key never reaches club data and a data endpoint never runs a partial
query.

## Rotation

Rotation issues a fresh secret for the same logical key:

1. In the panel, choose **Rotate** on the key and pick a grace window
   (`0`, `24`, `48` or `72` hours).
2. The response contains the new plaintext once. Deploy it to your systems.
3. The **old** secret keeps working until the grace window ends, then stops.

Grace is bounded: old secrets die at the end of the window whether or not you
have deployed the new one. If `grace_hours` is `0`, the old secret stops working
on the next request — use that in an incident, not in a routine rotation.

Rotation does not change the scopes, the IP allowlist or the rate limit of the
key; it only replaces the secret and records `rotated_from`.

## Revocation

Revocation is immediate and irreversible: every call with the revoked key fails
with `401 api_key_revoked` from the next request on. Revoke rather than delete
when a secret may have leaked — the audit trail keeps the history, and the panel
keeps showing the key as revoked with its last-used timestamp.

A club can hold at most 25 active keys. Revoke the ones you have replaced.

## Rotation versus new key

| You need to | Do |
| --- | --- |
| Replace a leaked secret, keep the same permissions | Rotate with a grace window, then deploy the new secret |
| Add a permission the key does not have | Create a **new** key with the wider scope set, then revoke the old one |
| Remove a permission | Narrow the scopes on the existing key (narrowing is allowed) |
| Give a contractor one-off access | Create a short-lived key (`expires_in_days`) with the minimum scopes, revoke it after the job |

Scopes can never be widened on an existing key. This is intentional: a stale
deployment cannot silently gain permissions it was never reviewed for.

## Never ship a key to a browser

An API key is a server credential equivalent to a password for the club's data.
It must not appear in:

* front-end JavaScript bundles, mobile apps or Electron apps shipped to users;
* HTML, cookies, `localStorage` / `sessionStorage`;
* source control, even in a private repository;
* CI logs, crash reports, or analytics payloads;
* screenshots and support tickets (use the `prefix` instead).

The club panel itself does not use API keys — it authenticates staff with a JWT
session. If an integration needs to run in a browser, put a thin server-side
proxy in front of it: the browser talks to your backend, your backend holds the
key.

## Storing and loading keys

Use the secret manager of whatever platform you deploy on, and read the key at
process start:

```ts
// Node: fail fast at boot, never at request time.
const apiKey = process.env.DROPIT_API_KEY;
if (!apiKey?.startsWith("dit_")) {
  throw new Error("DROPIT_API_KEY is missing or malformed");
}
```

```python
import os

api_key = os.environ.get("DROPIT_API_KEY", "")
if not api_key.startswith("dit_"):
    raise SystemExit("DROPIT_API_KEY is missing or malformed")
```

```sh
# Kubernetes: a Secret mounted as an env var
kubectl create secret generic dropit-api --from-literal=DROPIT_API_KEY='dit_live_…'
```

Rules that keep you out of trouble:

* one key per integration and environment — never share a key between staging
  and production;
* `test` keys for test deployments, `live` keys for live money;
* restrict the key to the egress IPs of your infrastructure (panel → key →
  IP allowlist, exact IP or CIDR, IPv4 and IPv6);
* set an expiry for keys that serve a project rather than a permanent
  integration;
* write down which human owns each key; the panel records `created_by`.

## Auditing what a key did

The panel shows, per key: last used, calls in the last 24 hours, errors in the
last 24 hours, and the most recent calls (`method`, `path`, `status`, `at`).
The raw key is never logged — only the prefix. Retention of call records is 30
days.

Every key management action (create, rotate, revoke, narrow) is written to the
platform audit log without secret material, so "who widened this key and when"
is always answerable.