---
id: security
title: Security
summary: How keys are stored and verified, what each control buys you, and what to do when one leaks.
requires: []
order: 140
---

# Security

This page describes the security model you are building against, so you can
decide how much to trust a key and what to do when something goes wrong.

## Key storage

* The secret is generated once from 256 bits of entropy and returned once.
* The platform stores `sha256(secret)` (hex) and the 10-character public prefix.
  There is no plaintext and no reversible form of the secret at rest or in
  backups.
* Verification is `prefix` lookup followed by a constant-time comparison of the
  digest, so a timing side channel does not reveal how much of a guess was
  right.
* A 256-bit random secret does not need a password KDF; brute force is out of
  reach by order of magnitude. A leaked database dump yields no usable key.
* The raw key is never written to logs. The usage journal records the key id,
  method, path, status and time — never the credential.

## One-time reveal

The plaintext exists in exactly two responses: creation and rotation, both with
`Cache-Control: no-store`. If the response is lost, the key cannot be recovered.
Rotate it.

Consequences for your process:

* create the key when a human is present to copy the secret into a secret
  manager — not from a scheduled job that discards the body;
* never log an HTTP response body that contains `key`;
* if you must move a secret between systems, move it through a secret manager,
  not through chat or a ticket.

## Deny by default

* Every endpoint requires a scope except `GET /me`.
* There is no wildcard and no implicit admin scope. A key that can read the docs
  cannot read revenue.
* A scope is checked **before** the handler touches data: a denied request has no
  side effects and returns no partial data.
* Scopes can be narrowed on an existing key but never widened. Widening means a
  new key, a new review and a new audit entry.
* A key is bound to one club. Every query is filtered by that club; a foreign id
  answers `404`, so ids cannot be enumerated across tenants.
* The OpenAPI document served to a key is filtered by that key's scopes: an
  integration cannot even discover operations it may not call.

**Docs access is a real scope.** `docs:read` gates the guides:
`GET /docs` returns a manifest containing only the sections the key can read.
Sensitive operational detail (revenue fields, QR semantics) stays behind the
scope that also gates the endpoint.

## IP allowlist

A key can be restricted to exact IPs or CIDR ranges, IPv4 and IPv6. The check
fails closed: an unparsable rule denies access rather than allowing it, and a
request whose client address cannot be determined is treated as not allowed.

* Set the allowlist to your egress addresses (NAT gateway, static egress IPs).
* Remember that serverless platforms and CI runners often have dynamic egress —
  either pin them or accept a wider range.
* The client address is taken from the connection, with the platform's trusted
  proxy handling; do not expect a spoofed `X-Forwarded-For` to pass the check.
* A failed allowlist check is `403 ip_not_allowed` and is recorded in the usage
  journal. A burst of them is worth investigating.

## Environment separation

Keys carry `live` or `test` in their string, and the environment is enforced
server-side:

* on a deployment in live payment mode, a `test` key is refused with
  `403 test_key_not_allowed` — test traffic never writes into live data;
* on the Test Mode stand, both kinds work and may share the same database; do
  not assume production-grade retention there;
* a key from one environment cannot be "promoted" to the other. Issue a new key
  in the target environment.

Do not put a live key in a staging environment "just to test the real thing".
Use a `test` key with the same scopes; the endpoint behaviour is identical.

## Audit

Every authenticated call is recorded: key id, method, path, status, time.
Retention is 30 days, after which the retention job purges the rows.

Every key lifecycle action — create, rotate, revoke, narrow — writes an audit
log entry with the acting principal (club owner or platform admin), the key id
and a diff that never contains secret material.

What this gives you:

| Question | Where the answer lives |
| --- | --- |
| Is this key used at all? | Panel → Developer access → key → last used |
| What did it call in the last 24 h? | Panel usage panel; `calls_24h`, `errors_24h` |
| Who created or widened it? | Platform audit log |
| Did it fail a lot last night? | Usage panel, error count and recent calls |

A key that shows no calls for weeks is a candidate for revocation. A key whose
scope set is wider than its call log needs is a candidate for narrowing.

## Data minimisation

The Developer API deliberately returns less than the panel:

* no guest identity, e-mail, phone, device id or IP in any response;
* no payment records, Stripe ids, or per-request amounts in v1 — only the
  aggregated `revenue.captured_minor`;
* no internal notes, no other clubs' data, no staff accounts;
* request detail exposes the track and the state, not the person.

If your integration needs guest data, it does not exist to be requested. Build
around pseudonymous aggregates.

## Incident response

When a secret may have leaked (pasted into a ticket, committed, logged by a
proxy), the order of operations matters:

1. **Revoke immediately** in the panel. Every call with the key fails on the next
   request; there is no propagation delay to wait for.
2. **Create a replacement key** with the same scopes and a fresh expiry. Deploy
   it to your systems.
3. **Read the usage journal** for the window between the leak and the revoke:
   unexpected paths, error bursts, calls from unknown IPs.
4. **Narrow the blast radius retrospectively.** If the leaked key had
   `qr:write` or `sessions:write`, check whether any session was closed or QR
   rotated without a human action.
5. **Rotate, do not delete.** Rotation keeps the key's identity and history;
   deletion is not offered, and revocation already makes the secret useless.

For a planned rotation (a leaver, a new deployment), use rotation with a grace
window instead:

| Situation | Action |
| --- | --- |
| Secret leaked or suspected | Revoke now, create a new key |
| Routine replacement, zero downtime | Rotate with `grace_hours: 24`, deploy the new secret, let the old one die |
| Contractor finished | Revoke |
| Key compromised and audit needed | Rotate with `grace_hours: 0` to kill the old secret instantly while keeping the key record |

## Practical hardening checklist

- [ ] One key per integration, per environment.
- [ ] Secret in a secret manager, injected at boot, never in the image.
- [ ] Minimum scopes; no `qr:write` on anything that does not issue QR codes.
- [ ] IP allowlist set to static egress.
- [ ] Expiry set for project keys; calendar reminder before it hits.
- [ ] `Retry-After` respected; backoff with jitter on `5xx`.
- [ ] Errors logged with `code`, never with the key.
- [ ] Usage panel reviewed after every rotation and every incident.
- [ ] A named owner per key, written down outside the panel.