# Mail infrastructure contract (Resend)

**Status:** authoritative for implementation. All agents build against this file.
**Scope:** transactional e-mail sending, delivery tracking, suppressions, admin oversight.
**Provider:** Resend HTTP API only (`POST /emails`), Svix-signed webhooks.
**Non-goals:** inbound mail, marketing broadcasts, SMTP, templates edited in DB.

## 1. Architecture (fits the existing shape)

```
pg trigger sites (dj invite, guest verify, payout)
   └─ MailRepo::enqueue  ──────────────► mail_message (status=queued)
                                          │
dropit-workers "mail" loop                │  claim_batch (SKIP LOCKED)
   └─ MailRepo::claim_batch ──────────────┘
   └─ render(template)                  (already rendered at enqueue — see §5)
   └─ MailSender::send (ResendSender) ──► api.resend.com/emails
   └─ mark_sent(provider_id) | mark_failed(backoff) | mark_suppressed
                                          │
Resend webhooks ─► POST /webhooks/resend ─┴─► mail_event + status transition + mail_suppression
```

Rules carried over from the project (do not reinvent):

- **Degradation first.** No `RESEND_API_KEY` → `NoopMail` (returns a synthetic
  id, logs, never touches the network). No `RESEND_WEBHOOK_SECRET` → the webhook
  route answers `503 mail_webhook_disabled` instead of trusting unsigned bodies.
- **No new third-party crates.** Everything is built on deps already in
  `Cargo.toml`: `reqwest`, `hmac`, `sha2`, `base64`, `hex`, `chrono`, `serde`,
  `sqlx`, `uuid`, `thiserror`, `tracing`, `async-trait`. `cargo deny` must stay
  clean; no `lettre`, no `handlebars`.
- **Secrets never logged.** API key, signing secret and provider bodies are
  truncated/sanitised exactly like `infra-push::transport::sanitize_body`.
- **Idempotency at every hop.** `mail_message.idempotency_key` is UNIQUE and is
  the `Idempotency-Key` header value sent to Resend. Webhook events are deduped
  by `svix-id` (`mail_event.svix_id` UNIQUE).
- **Money/delivery semantics.** Enqueue happens in the same transaction as the
  business change where one exists; delivery is at-least-once, consumers are
  idempotent.

## 2. Database (migration `0013_mail.sql`)

Additive only. No changes to existing tables, no FK to `club`/`dj`/`guest_device`
(soft references like `outbox`/`audit_log`), no `TRUNCATE`-order impact.

```sql
CREATE TABLE mail_message (
    id               UUID PRIMARY KEY,
    idempotency_key  TEXT NOT NULL UNIQUE,
    template         TEXT NOT NULL,
    to_email         CITEXT NOT NULL,
    to_name          TEXT,
    from_email       TEXT NOT NULL,
    reply_to         TEXT,
    subject          TEXT NOT NULL,
    html             TEXT NOT NULL,
    text             TEXT NOT NULL,
    vars             JSONB NOT NULL DEFAULT '{}'::jsonb,
    status           TEXT NOT NULL DEFAULT 'queued',
    provider         TEXT NOT NULL DEFAULT 'resend',
    provider_id      TEXT,
    attempts         INTEGER NOT NULL DEFAULT 0,
    max_attempts     INTEGER NOT NULL DEFAULT 5,
    next_attempt_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_error       TEXT,
    aggregate_type   TEXT,
    aggregate_id     UUID,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    sent_at          TIMESTAMPTZ,
    delivered_at     TIMESTAMPTZ
);
```

`status` vocabulary (only these values; `CHECK` constraint):

`queued | sending | sent | delivered | delivery_delayed | bounced | complained | failed | suppressed`

Indexes:

```sql
CREATE INDEX idx_mail_message_pending ON mail_message (next_attempt_at)
    WHERE status IN ('queued', 'failed');
CREATE INDEX idx_mail_message_created ON mail_message (created_at DESC, id DESC);
CREATE INDEX idx_mail_message_provider ON mail_message (provider_id)
    WHERE provider_id IS NOT NULL;
CREATE INDEX idx_mail_message_aggregate ON mail_message (aggregate_type, aggregate_id)
    WHERE aggregate_id IS NOT NULL;
```

```sql
CREATE TABLE mail_event (
    id          UUID PRIMARY KEY,
    svix_id     TEXT NOT NULL UNIQUE,
    event_type  TEXT NOT NULL,
    provider_id TEXT,
    payload     JSONB NOT NULL,
    received_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_mail_event_provider ON mail_event (provider_id, received_at DESC);

CREATE TABLE mail_suppression (
    email      CITEXT PRIMARY KEY,
    reason     TEXT NOT NULL,          -- 'bounce' | 'complaint' | 'manual'
    detail     TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

`test_repo()` in `crates/gateway-api/src/e2e.rs` does **not** know these tables —
each mail e2e test truncates `mail_message, mail_event, mail_suppression` itself
(same pattern as `guest_account`).

## 3. Crate `crates/infra-mail` (`dropit-infra-mail`)

Module map: `lib.rs` (re-exports, `#![deny(missing_docs)]`), `error.rs`,
`transport.rs` (internal `HttpTransport` + `ReqwestTransport`, copied shape from
`infra-push`), `resend.rs`, `sender.rs`, `webhook.rs`, `event.rs`, `template.rs`,
`error.rs`, `tests.rs`.

```rust
pub struct MailAddress { pub email: String, pub name: Option<String> }
impl MailAddress { pub fn parse(raw: &str) -> Result<Self, MailError>; } // "Name <a@b>" | "a@b"
impl std::fmt::Display for MailAddress {}                                // round-trips parse()

pub struct OutgoingMail {
    pub id: uuid::Uuid,
    pub idempotency_key: String,
    pub from: MailAddress,
    pub to: MailAddress,
    pub reply_to: Option<MailAddress>,
    pub subject: String,
    pub html: String,
    pub text: String,
}

pub struct SendReceipt { pub provider_id: String, pub provider: &'static str }

#[async_trait::async_trait]
pub trait MailSender: Send + Sync {
    async fn send(&self, mail: &OutgoingMail) -> Result<SendReceipt, MailError>;
    /// `true` when the transport really leaves the process (NoopMail → false).
    fn is_live(&self) -> bool { true }
}

pub struct NoopMail;                       // id = "noop-<uuid>", is_live() == false

pub enum MailError {
    Unavailable(String),                   // network / 5xx / exhausted retries
    Unauthorized(String),                  // 401/403, bad api key
    RateLimited { retry_after_secs: Option<i64> },  // 429 (+ Retry-After)
    Rejected(String),                      // 4xx validation (bad recipient, unverified domain)
    Decode(String),                        // unparsable provider response
    Config(String),                        // missing/invalid configuration
    Timeout,
    SignatureInvalid(String),              // webhook: bad/missing signature
    SignatureExpired,                      // webhook: timestamp outside tolerance
}

pub struct ResendConfig {
    pub api_key: String,
    pub api_base: String,                  // default https://api.resend.com
    pub from: MailAddress,
    pub reply_to: Option<MailAddress>,
}
pub struct ResendSender { /* … */ }
impl ResendSender {
    pub fn new(config: ResendConfig) -> Result<Self, MailError>;
    pub fn config(&self) -> &ResendConfig;
    #[cfg(test)] pub(crate) fn with_transport(config: ResendConfig, t: SharedTransport) -> Self;
}

pub struct MailConfig { pub resend: ResendConfig, pub webhook_secret: Option<String> }
pub fn from_env() -> Option<MailConfig>;            // env names in §6
pub(crate) fn from_env_via<F: Fn(&str) -> Option<String>>(get: F) -> Option<MailConfig>;
pub fn webhook_verifier_from_env() -> Result<WebhookVerifier, MailError>;
```

### Resend request

`POST {api_base}/emails`, headers `Authorization: Bearer <key>`,
`Content-Type: application/json`, `User-Agent: dropit-infra-mail`,
`Idempotency-Key: <mail.idempotency_key>` (1..=256 chars).

```json
{"from":"DROP IT <no-reply@dropit.club>","to":["a@b.c"],"subject":"…",
 "html":"…","text":"…","reply_to":["r@b.c"]}
```

`reply_to` is omitted when absent. Response must be `200` with
`{"id":"<uuid>"}`; any other 2xx is a `Decode` error (contract, not a guess).

Error mapping: `401|403` → `Unauthorized`; `429` → `RateLimited{retry_after_secs}`
from the `Retry-After` header (seconds or HTTP-date, best effort);
`400|404|409|422` → `Rejected(sanitized body)`; `5xx` → `Unavailable`;
reqwest timeout → `Timeout`; connect/request → `Unavailable`.

### Webhooks (Svix)

Headers: `svix-id`, `svix-timestamp`, `svix-signature`
(`v1,<base64>` entries, space-separated). Secret `whsec_<base64>`; strip the
`whsec_` label and base64-decode the rest. Signed content is the **raw bytes**
`"{svix_id}.{svix_timestamp}.{raw_body}"`, HMAC-SHA256, constant-time compare
(`hmac::Mac::verify_slice`). Tolerance 300 s (`WEBHOOK_TOLERANCE_SECS`), drift
is a separate error (`SignatureExpired`), empty secret → `Config`.

```rust
pub struct WebhookHeaders { pub id: String, pub timestamp: String, pub signature: String }
impl WebhookHeaders { pub fn from_pairs<'a>(i: impl Iterator<Item = (&'a str, &'a str)>) -> Option<Self>; }
pub struct WebhookVerifier { /* secret, tolerance */ }
impl WebhookVerifier {
    pub fn from_whsec(secret: &str) -> Result<Self, MailError>;
    pub fn verify(&self, raw_body: &[u8], h: &WebhookHeaders, now_unix: i64) -> Result<(), MailError>;
}

pub enum MailEventKind {
    Sent, Delivered, DeliveryDelayed, Bounced, Complained, Opened, Clicked, Failed, Suppressed, Other,
}
impl MailEventKind { pub fn as_str(&self) -> &'static str; }   // "email.sent" …
pub struct MailWebhookEvent {
    pub kind: MailEventKind,
    pub provider_id: Option<String>,     // data.email_id
    pub email: Option<String>,           // data.to[0]
    pub bounce_kind: Option<String>,     // "hard" | "soft" | "undetermined"
    pub reason: Option<String>,          // data.bounce.message | data.failed.reason
    pub raw: serde_json::Value,
}
pub fn parse_event(raw: &[u8]) -> Result<MailWebhookEvent, MailError>;
```

Unknown `type` values must parse as `Other` (forward compatibility), never fail.
A payload without `type` is `Decode`.

### Templates

No template engine. `{{var}}` placeholders, HTML-escaped in `html`,
verbatim in `text`. Missing variable → `Config("missing template var …")`
(a broken template is a bug, not a silent blank line).

```rust
pub enum TemplateKey { DjInvite, GuestVerify, PayoutPaid, MailTest }
impl TemplateKey {
    pub fn as_str(&self) -> &'static str;                    // "dj_invite" …
    pub fn from_str(s: &str) -> Option<Self>;
}
pub struct RenderedMail { pub subject: String, pub html: String, pub text: String }
pub fn render(key: TemplateKey, vars: &serde_json::Value) -> Result<RenderedMail, MailError>;
pub fn escape_html(s: &str) -> String;
```

Template subject text (RU, matches the product voice):

| key | subject | required vars |
| --- | --- | --- |
| `dj_invite` | `Приглашение в {club_name}` | `club_name`, `dj_name`, `invite_url`, `expires_at` |
| `guest_verify` | `Подтвердите e-mail` | `code`, `expires_at` |
| `payout_paid` | `Выплата отправлена` | `payee_name`, `amount`, `period` |
| `mail_test` | `Тестовое письмо DROP IT` | `sent_by` |

Links come from `MAIL_APP_BASE_URL` and are passed in `vars` by the caller, so
the renderer never reads env.

## 4. `dropit_infra_db::mail`

```rust
pub struct NewMail {
    pub idempotency_key: String, pub template: String,
    pub to_email: String, pub to_name: Option<String>,
    pub from_email: String, pub reply_to: Option<String>,
    pub subject: String, pub html: String, pub text: String,
    pub vars: serde_json::Value,
    pub aggregate_type: Option<String>, pub aggregate_id: Option<Uuid>,
}
pub struct MailRow { id, idempotency_key, template, to_email, to_name, subject,
                     status, attempts, max_attempts, provider_id, last_error,
                     created_at, updated_at, sent_at, delivered_at, vars }
pub struct MailStats { pub window_hours: i64, pub queued: i64, pub sending: i64,
                       pub sent: i64, pub delivered: i64, pub bounced: i64,
                       pub complained: i64, pub failed: i64, pub suppressed: i64,
                       pub total: i64 }
pub struct MailFilter { pub status: Option<String>, pub query: Option<String>,
                        pub limit: i64, pub cursor: Option<String> }  // keyset (created_at,id)
pub struct MailDetail { pub mail: MailRow, pub events: Vec<MailEventRow> }

impl MailRepo {
    pub fn new(pool: sqlx::PgPool) -> Self;
    /// INSERT … ON CONFLICT (idempotency_key) DO NOTHING → (id, duplicate).
    pub async fn enqueue(&self, mail: NewMail) -> Result<(Uuid, bool), MailDbError>;
    /// SKIP LOCKED claim of due `queued|failed` rows; sets status='sending'.
    /// `MailRow` — полная строка: диспетчер собирает из неё `OutgoingMail`
    /// без второго запроса.
    pub async fn claim_batch(&self, limit: i64) -> Result<Vec<MailRow>, MailDbError>;
    pub async fn mark_sent(&self, id: Uuid, provider_id: &str) -> Result<(), MailDbError>;
    /// attempts+1; attempts >= max_attempts or retry_after None → status='failed',
    /// otherwise status='queued' with next_attempt_at = now() + retry_after.
    pub async fn mark_failed(&self, id: Uuid, error: &str, retry_after: Option<Duration>) -> Result<(), MailDbError>;
    pub async fn mark_suppressed(&self, id: Uuid, reason: &str) -> Result<(), MailDbError>;
    /// Dedup by svix_id. Returns `true` when the event was new and applied.
    pub async fn record_event(&self, svix_id: &str, event_type: &str,
                              provider_id: Option<&str>, payload: &serde_json::Value) -> Result<bool, MailDbError>;
    /// Applies the delivery-state transition for `provider_id`.
    pub async fn apply_event(&self, provider_id: &str, kind: &str) -> Result<bool, MailDbError>;
    /// Страница журнала: `MailListItem` — проекция без `html`/`text`/`vars`
    /// (тела писем бывают по несколько килобайт, а список их не показывает).
    pub async fn list(&self, filter: MailFilter) -> Result<(Vec<MailListItem>, Option<String>), MailDbError>;
    pub async fn get(&self, id: Uuid) -> Result<Option<MailDetail>, MailDbError>;
    /// Re-queues the same content under a fresh `<original_key>:r<n>` idempotency key.
    pub async fn requeue(&self, id: Uuid) -> Result<Uuid, MailDbError>;
    pub async fn stats(&self, window_hours: i64) -> Result<MailStats, MailDbError>;
    pub async fn is_suppressed(&self, email: &str) -> Result<bool, MailDbError>;
    pub async fn add_suppression(&self, email: &str, reason: &str, detail: Option<&str>) -> Result<(), MailDbError>;
    pub async fn list_suppressions(&self, limit: i64) -> Result<Vec<MailSuppressionRow>, MailDbError>;
    pub async fn remove_suppression(&self, email: &str) -> Result<bool, MailDbError>;
}
```

State transitions (guarded in SQL, monotonic — a late `sent` cannot regress a
`delivered` row):

| event | from | to |
| --- | --- | --- |
| `sent` | `sending`,`queued` | `sent` (+`sent_at`) |
| `delivered` | `sent`,`delivery_delayed` | `delivered` (+`delivered_at`) |
| `delivery_delayed` | `sent` | `delivery_delayed` |
| `bounced` | `sent`,`delivered`,`delivery_delayed` | `bounced` |
| `complained` | `sent`,`delivered` | `complained` |
| `failed` | `queued`,`sending` | `failed` (+`last_error`) |
| `suppressed` | `queued`,`sending` | `suppressed` |

`bounced`/`complained` also upsert `mail_suppression` (reason `bounce`/`complaint`).
`opened`/`clicked` update nothing (they are recorded in `mail_event` only).

## 5. Worker `dropit-workers`

`crates/workers/src/mail.rs`:

```rust
pub async fn run_mail(store: &dyn MailStore, sender: &dyn MailSender, batch: i64) -> JobReport;
```

`MailStore` is a thin trait over the four calls the dispatcher needs
(`claim_batch`, `mark_sent`, `mark_failed`, `mark_suppressed`) so the dispatcher
is unit-testable with an in-memory double and no DB — same pattern as
`push.rs::PushTokenSource`.

Behaviour: claim → skip suppressed recipients (`MailSender` is not called, row →
`suppressed`) → rebuild `OutgoingMail` from the row → send → `mark_sent` or
`mark_failed` with `retry_after = min(2^attempts * 5s, 1h)` (`RateLimited` honours
the provider hint). One failed row never aborts the batch. Report keys:
`claimed, skipped_suppressed, sent, failed, suppressed, noop`.

Wire-up: `Command::Once("mail")`, supervisor interval `MAIL_INTERVAL_SECS`
(default 2), `MAIL_BATCH_SIZE` (default 50). Runtime builds `ResendSender` from
env, else `NoopMail` + one `warn!`.

## 6. Environment variables

| var | default | meaning |
| --- | --- | --- |
| `RESEND_API_KEY` | — | `re_…`; empty → `NoopMail` |
| `RESEND_WEBHOOK_SECRET` | — | `whsec_…`; empty → webhook 503 |
| `RESEND_API_URL` | `https://api.resend.com` | sandbox/stub base |
| `MAIL_FROM` | — | `DROP IT <no-reply@…>`; required when key is set |
| `MAIL_REPLY_TO` | — | optional |
| `MAIL_APP_BASE_URL` | `http://localhost:5173` | link prefix for templates |
| `MAIL_INTERVAL_SECS` | `2` | dispatcher period |
| `MAIL_BATCH_SIZE` | `50` | rows per run |
| `MAIL_MAX_ATTEMPTS` | `5` | per-row attempt cap |

Partial configuration (key without `MAIL_FROM`, etc.) → `warn!` + degrade to
`NoopMail`; never panic at boot.

## 7. HTTP contract

All admin routes are admin-only (`Role::Admin`), envelope errors as usual.

```
POST /webhooks/resend                       (public, Svix-signed raw body)
  headers: svix-id, svix-timestamp, svix-signature
  200 {"received":true,"duplicate":false}
  200 {"received":true,"duplicate":true}    (replay of a known svix-id)
  400 mail_webhook_invalid                  (bad/expired signature, bad payload)
  503 mail_webhook_disabled                 (no RESEND_WEBHOOK_SECRET)
  413                                     (body > 1 MiB)

GET    /v1/admin/mail?status=&q=&limit=&cursor=
  → {"items":[MailItem],"next_cursor":null|string}
GET    /v1/admin/mail/stats
  → {"window_hours":24,"queued":0,"sending":0,"sent":0,"delivered":0,
     "bounced":0,"complained":0,"failed":0,"suppressed":0,"total":0}
GET    /v1/admin/mail/suppressions?limit=
  → {"items":[{"email","reason","detail","created_at"}]}
DELETE /v1/admin/mail/suppressions/{email}
  → {"removed":true|false}
GET    /v1/admin/mail/{id}
  → MailItem + {"html","text","vars":{…},"events":[{"id","type","received_at"}]}
POST   /v1/admin/mail/{id}/resend
  → {"id":"<new id>","status":"queued"}   404 mail_not_found
POST   /v1/admin/mail/test   body {"to":"a@b.c"}
  → {"id":"<id>","status":"queued"}       422 mail_validation_failed
```

`MailItem` (camel-free snake_case, matching the existing admin DTOs):

```json
{"id":"…","to":"a@b.c","to_name":null,"subject":"…","template":"dj_invite",
 "status":"delivered","attempts":1,"max_attempts":5,"provider_id":"…",
 "last_error":null,"created_at":"2026-10-27T18:00:00Z",
 "sent_at":"…","delivered_at":"…","aggregate_type":"dj","aggregate_id":"…"}
```

Route order matters: `/v1/admin/mail/stats` and `/v1/admin/mail/suppressions`
must be registered before `/v1/admin/mail/:id`.

Business triggers (fire-and-forget, never fail the business response):

| site | template | idempotency key | aggregate |
| --- | --- | --- | --- |
| `POST /v1/club/djs/invite` | `dj_invite` | `dj_invite:<dj_id>` | `dj`/`dj_id` |
| `POST /v1/guest/upgrade` | `guest_verify` | `guest_verify:<account_id>` | `guest_account`/id |
| worker payout cycle (per created payout) | `payout_paid` | `payout_paid:<payout_id>` | `payout`/id |

## 8. Admin UI (`apps/web-admin`, route `/mail`)

- One `PageHeading` + a compact KPI row (queued / delivered / bounced / failed,
  24 h), a filter bar (status select, search), a table (`PowerTable`-style rows,
  no nested cards), a detail drawer with the rendered HTML in a sandboxed iframe
  and the event timeline, and a suppressions section below.
- Tokens only (`var(--c-*)`, `panel-*` classes). No raw colours. Both themes.
- Resend action per row (`POST .../resend`), disabled while pending; success
  reloads the list.
- Add the route to `App.tsx`, a nav item to `Layout.tsx`, and
  `/admin/mail` to `scripts/panel-audit.mjs`.
- Empty/error states are real: no backend → `ErrorState`, zero rows →
  `panel-empty` with the next action.

## 9. Definition of done

1. `cargo test -p dropit-infra-mail` green (transport, idempotency header, error
   mapping, Svix verify incl. tampering/expiry, event parsing, templates).
2. `scripts/db-test.sh cargo test -p dropit-infra-db mail` green against the local
   Postgres (claim/backoff/suppression/keyset/transitions).
3. `cargo test -p dropit-workers` green (dispatcher with fakes).
4. `scripts/db-test.sh cargo test -p dropit-gateway-api e2e_mail -- --test-threads=1`
   green (webhook signed/unsigned/duplicate, admin list/detail/resend/stats,
   RBAC 403 for club, trigger enqueues on DJ invite).
5. `cargo check --workspace` and `cargo clippy --workspace -- -D warnings` clean.
6. `node scripts/panel-audit.mjs --app admin --route /admin/mail` green in both
   themes; screenshot at 1440 and 390 widths.
7. `docs/openapi.yaml`, `deploy/.env.example`, `deploy/docker-compose.yml`,
   `README.md`, `docs/adr/ADR-008-email-resend.md` updated.
