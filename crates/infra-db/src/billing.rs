//! Репозиторий биллинга (F5): платежи, сплиты, ledger, вебхуки.
//!
//! ПЛАТЕЖНЫЙ ЦИКЛ (техкарта Часть V):
//!   1. request → payment(intent) со снимком цены и split_snapshot;
//!   2. confirm → GUARD (session active + request pending) → processing;
//!   3. вебхук захвата → идемпотентно по (aggregate_id, event_type):
//!      TX = payment→captured + ledger двойная проводка + split-строки;
//!   4. дубль вебхука (другой event_id) → NO-OP (не вторая проводка!).
//!
//! Инварианты:
//!   * Σ split(payment) == amount_minor;
//!   * Σ ledger_entry(txn_id) == 0 (микро-инвариант каждой проводки);
//!   * ledger — append-only, ошибки — сторно, не UPDATE.

use chrono::{DateTime, Utc};
use dropit_domain::{compute_split, PaymentAction, PaymentStatus};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum BillingError {
    #[error("database error")]
    Db(#[from] sqlx::Error),
    #[error("payment not found")]
    NotFound,
    #[error("request not pending")]
    RequestNotPending,
    #[error("session not active")]
    SessionNotActive,
    #[error("invalid transition")]
    InvalidTransition,
    #[error("webhook event already processed")]
    AlreadyProcessed,
    #[error("amount mismatch")]
    AmountMismatch,
}

#[derive(Debug, sqlx::FromRow)]
pub struct PaymentRow {
    pub id: Uuid,
    pub request_id: Uuid,
    pub stripe_payment_intent_id: Option<String>,
    pub idempotency_key: String,
    pub amount_minor: i64,
    pub currency: String,
    pub status: String,
    pub refunded_minor: i64,
    pub failure_reason: Option<String>,
    pub stripe_error_code: Option<String>,
    pub split_snapshot: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub captured_at: Option<DateTime<Utc>>,
}

impl PaymentRow {
    pub fn payment_status(&self) -> Option<PaymentStatus> {
        match self.status.as_str() {
            "intent" => Some(PaymentStatus::Intent),
            "processing" => Some(PaymentStatus::Processing),
            "captured" => Some(PaymentStatus::Captured),
            "partially_refunded" => Some(PaymentStatus::PartiallyRefunded),
            "refunded" => Some(PaymentStatus::Refunded),
            "failed" => Some(PaymentStatus::Failed),
            "cancelling" => Some(PaymentStatus::Cancelling),
            "cancelled" => Some(PaymentStatus::Cancelled),
            _ => None,
        }
    }
}

/// Правила разреза на момент создания (снимок в payment).
#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub struct SplitSnapshot {
    pub commission_bp: i32,
    pub dj_share_bp: i32,
    pub amount_minor: i64,
    pub currency: String,
    pub platform_minor: i64,
    pub club_minor: i64,
    pub dj_minor: i64,
}

/// Параметры комиссий для расчёта (значения — конфиг, владелец).
pub struct CommissionParams {
    pub commission_bp: i32,
    pub dj_share_bp: i32,
    pub currency: String,
}

#[derive(Clone)]
pub struct BillingRepo {
    pool: PgPool,
}

impl BillingRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> PgPool {
        self.pool.clone()
    }

    // ---------------------------------------------------------
    // Шаг 1: создание платежа (intent) для запроса
    // ---------------------------------------------------------

    /// Создаёт payment(intent) со снимком цены и правил разреза.
    /// Снимок неизменяем после captured — аудит «почему разделили».
    pub async fn create_payment(
        &self,
        request_id: Uuid,
        amount_minor: i64,
        idempotency_key: &str,
        intent_id: &str,
        commission: &CommissionParams,
    ) -> Result<PaymentRow, BillingError> {
        let split = compute_split(
            amount_minor,
            commission.commission_bp,
            commission.dj_share_bp,
        )
        .map_err(|_| BillingError::AmountMismatch)?;
        let snapshot = SplitSnapshot {
            commission_bp: commission.commission_bp,
            dj_share_bp: commission.dj_share_bp,
            amount_minor,
            currency: commission.currency.clone(),
            platform_minor: split.platform_minor,
            club_minor: split.club_minor,
            dj_minor: split.dj_minor,
        };
        // Строки split создаются при захвате (payment без денег
        // не плодит будущие payable).
        let row = sqlx::query_as::<_, PaymentRow>(
            "INSERT INTO payment (id, request_id, stripe_payment_intent_id, idempotency_key, \
             amount_minor, currency, status, split_snapshot) \
             VALUES ($1, $2, $3, $4, $5, $6, 'intent', $7) \
             RETURNING id, request_id, stripe_payment_intent_id, idempotency_key, \
             amount_minor, currency, status, refunded_minor, failure_reason, \
             stripe_error_code, split_snapshot, created_at, captured_at",
        )
        .bind(Uuid::now_v7())
        .bind(request_id)
        .bind(intent_id)
        .bind(idempotency_key)
        .bind(amount_minor)
        .bind(&commission.currency)
        .bind(serde_json::to_value(&snapshot).expect("snapshot serializes"))
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn find_payment(&self, id: Uuid) -> Result<Option<PaymentRow>, BillingError> {
        let row = sqlx::query_as::<_, PaymentRow>(
            "SELECT id, request_id, stripe_payment_intent_id, idempotency_key, amount_minor, \
             currency, status, refunded_minor, failure_reason, stripe_error_code, \
             split_snapshot, created_at, captured_at FROM payment WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    /// Участники запроса (клуб, DJ) — для сплит-строк при захвате.
    pub async fn request_participants(
        &self,
        request_id: Uuid,
    ) -> Result<(Uuid, Option<Uuid>), BillingError> {
        let row: (Uuid, Option<Uuid>) = sqlx::query_as(
            "SELECT s.club_id, s.dj_id FROM request r JOIN session s ON s.id = r.session_id \
             WHERE r.id = $1",
        )
        .bind(request_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn find_payment_by_request(
        &self,
        request_id: Uuid,
    ) -> Result<Option<PaymentRow>, BillingError> {
        let row = sqlx::query_as::<_, PaymentRow>(
            "SELECT id, request_id, stripe_payment_intent_id, idempotency_key, amount_minor, \
             currency, status, refunded_minor, failure_reason, stripe_error_code, \
             split_snapshot, created_at, captured_at FROM payment WHERE request_id = $1",
        )
        .bind(request_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    // ---------------------------------------------------------
    // Шаг 2: confirm (GUARD) → processing
    // ---------------------------------------------------------

    /// Подтверждение оплаты гостем. GUARD (техкарта Часть IV):
    /// session == active AND request == pending, иначе ошибка —
    /// вызывающий обязан отменить интент (иначе «заплачено за
    /// expired»).
    pub async fn confirm_payment(&self, payment_id: Uuid) -> Result<PaymentRow, BillingError> {
        let mut tx = self.pool.begin().await?;
        let payment = sqlx::query_as::<_, PaymentRow>(
            "SELECT id, request_id, stripe_payment_intent_id, idempotency_key, amount_minor, \
             currency, status, refunded_minor, failure_reason, stripe_error_code, \
             split_snapshot, created_at, captured_at FROM payment WHERE id = $1 FOR UPDATE",
        )
        .bind(payment_id)
        .fetch_one(&mut *tx)
        .await?;
        if payment.status != "intent" {
            return Err(BillingError::InvalidTransition);
        }
        // GUARD: request pending.
        let (req_status, session_id): (String, Uuid) =
            sqlx::query_as("SELECT status, session_id FROM request WHERE id = $1")
                .bind(payment.request_id)
                .fetch_one(&mut *tx)
                .await?;
        if req_status != "pending" {
            return Err(BillingError::RequestNotPending);
        }
        // GUARD: session active.
        let (session_status,): (String,) =
            sqlx::query_as("SELECT status FROM session WHERE id = $1 FOR SHARE")
                .bind(session_id)
                .fetch_one(&mut *tx)
                .await?;
        if session_status != "active" {
            return Err(BillingError::SessionNotActive);
        }
        let row = sqlx::query_as::<_, PaymentRow>(
            "UPDATE payment SET status = 'processing' WHERE id = $1 \
             RETURNING id, request_id, stripe_payment_intent_id, idempotency_key, amount_minor, \
             currency, status, refunded_minor, failure_reason, stripe_error_code, \
             split_snapshot, created_at, captured_at",
        )
        .bind(payment_id)
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(row)
    }

    // ---------------------------------------------------------
    // Шаг 3: вебхук захвата → captured + ledger + split
    // ---------------------------------------------------------

    /// Обработка вебхука «платёж захвачен». Идемпотентность ПО
    /// (aggregate_id, event_type): delivery-дубль (тот же event_id)
    /// ловит UNIQUE у webhook_event; semantic-дубль (другой event_id,
    /// тот же переход) — стейт-машина: captured + Capture = NO-OP.
    ///
    /// Одна TX: payment→captured + split-строки + ledger двойная
    /// проводка. Возврат Ok(true) — проводка создана;
    /// Ok(false) — no-op (дубль).
    pub async fn handle_capture_webhook(
        &self,
        event_id: &str,
        event_type: &str,
        payment_id: Uuid,
        amount_minor: i64,
        club_id: Uuid,
        dj_id: Option<Uuid>,
    ) -> Result<bool, BillingError> {
        let mut tx = self.pool.begin().await?;

        // Delivery-дедуп: тот же event_id уже принят?
        let inserted = sqlx::query(
            "INSERT INTO webhook_event (id, source, event_id, event_type, aggregate_id, payload) \
             VALUES ($1, 'stripe', $2, $3, $4, $5) ON CONFLICT (event_id) DO NOTHING",
        )
        .bind(Uuid::now_v7())
        .bind(event_id)
        .bind(event_type)
        .bind(payment_id)
        .bind(serde_json::json!({ "event_id": event_id, "amount_minor": amount_minor }))
        .execute(&mut *tx)
        .await?;
        if inserted.rows_affected() == 0 {
            // Тот же event_id повторно доставлен: уже обработан.
            tx.rollback().await?;
            return Ok(false);
        }

        // Semantic-дедуп: (aggregate_id, event_type) уже обработан?
        let (processed,): (i64,) = sqlx::query_as(
            "SELECT count(*) FROM webhook_event \
             WHERE aggregate_id = $1 AND event_type = $2 AND processed_at IS NOT NULL",
        )
        .bind(payment_id)
        .bind(event_type)
        .fetch_one(&mut *tx)
        .await?;
        if processed > 0 {
            tx.rollback().await?;
            return Ok(false);
        }

        // Платёж.
        let payment = sqlx::query_as::<_, PaymentRow>(
            "SELECT id, request_id, stripe_payment_intent_id, idempotency_key, amount_minor, \
             currency, status, refunded_minor, failure_reason, stripe_error_code, \
             split_snapshot, created_at, captured_at FROM payment WHERE id = $1 FOR UPDATE",
        )
        .bind(payment_id)
        .fetch_one(&mut *tx)
        .await?;

        // Сумма вебхука должна совпасть с интентом (анти-подмена).
        if payment.amount_minor != amount_minor {
            tx.rollback().await?;
            return Err(BillingError::AmountMismatch);
        }

        let current = payment
            .payment_status()
            .ok_or(BillingError::Db(sqlx::Error::Decode(
                "unknown payment status".into(),
            )))?;
        let next = current
            .apply(PaymentAction::Capture)
            .map_err(|_| BillingError::InvalidTransition)?;

        if next == PaymentStatus::Captured && current != PaymentStatus::Captured {
            // Собственно захват: split-строки + ledger двойная проводка.
            let snapshot: SplitSnapshot =
                serde_json::from_value(payment.split_snapshot.clone().unwrap_or_default())
                    .map_err(|_| BillingError::Db(sqlx::Error::Decode("bad snapshot".into())))?;

            // Split-строки (материализация снапшота для выплат F6).
            for (payee_type, payee_id, amount) in [
                ("platform", None, snapshot.platform_minor),
                ("club", Some(club_id), snapshot.club_minor),
                ("dj", dj_id, snapshot.dj_minor),
            ] {
                if amount > 0 {
                    sqlx::query(
                        "INSERT INTO split (id, payment_id, payee_type, payee_id, amount_minor) \
                         VALUES ($1, $2, $3, $4, $5)",
                    )
                    .bind(Uuid::now_v7())
                    .bind(payment_id)
                    .bind(payee_type)
                    .bind(payee_id)
                    .bind(amount)
                    .execute(&mut *tx)
                    .await?;
                }
            }

            // Ledger: двойная запись. Дебет-сторона: распределение
            // денег получателям; кредит-сторона: выручка.
            // Σ строк == 0 по определению: +amount у получателей,
            // −amount у источника (revenue:platform).
            let txn = Uuid::now_v7();
            let currency = snapshot.currency.as_str();
            for (account, amount) in [
                ("revenue:platform", -snapshot.amount_minor),
                ("payable:club", snapshot.club_minor),
                ("payable:dj", snapshot.dj_minor),
                // Комиссия платформы — её доход (внутри revenue), в
                // payable не попадает: платформа получает сплит
                // через expense-проводки при выплатах (F6).
                // Микро-инвариант: Σ = −P + club + dj + platform_доход = 0.
                ("revenue:platform_commission", snapshot.platform_minor),
            ] {
                sqlx::query(
                    "INSERT INTO ledger_entry (id, txn_id, account, currency, amount_minor, \
                     entry_type, ref) VALUES ($1, $2, $3, $4, $5, 'capture', $6)",
                )
                .bind(Uuid::now_v7())
                .bind(txn)
                .bind(account)
                .bind(currency)
                .bind(amount)
                .bind(payment_id.to_string())
                .execute(&mut *tx)
                .await?;
            }

            // Пометка события обработанным — ВНУТРИ той же TX.
            sqlx::query("UPDATE webhook_event SET processed_at = now() WHERE event_id = $1")
                .bind(event_id)
                .execute(&mut *tx)
                .await?;

            sqlx::query(
                "UPDATE payment SET status = 'captured', captured_at = now() WHERE id = $1",
            )
            .bind(payment_id)
            .execute(&mut *tx)
            .await?;

            // Outbox-событие для WS-уведомлений (ADR-002/005).
            sqlx::query(
                "INSERT INTO outbox (id, aggregate_type, aggregate_id, event_type, payload) \
                 VALUES ($1, 'payment', $2, 'payment.captured', $3)",
            )
            .bind(Uuid::now_v7())
            .bind(payment_id)
            .bind(serde_json::json!({
                "payment_id": payment_id,
                "request_id": payment.request_id,
                "amount_minor": amount_minor,
            }))
            .execute(&mut *tx)
            .await?;

            tx.commit().await?;
            Ok(true)
        } else {
            // captured → captured: NO-OP, событие помечаем обработанным.
            sqlx::query("UPDATE webhook_event SET processed_at = now() WHERE event_id = $1")
                .bind(event_id)
                .execute(&mut *tx)
                .await?;
            tx.commit().await?;
            Ok(false)
        }
    }

    // ---------------------------------------------------------
    // Отмена (TTL-воркер, пауза сессии)
    // ---------------------------------------------------------

    /// Начинает отмену интента (локально cancelling; отмена У
    /// ПРОВАЙДЕРА — вызывающий слой, воркер).
    pub async fn begin_cancel(&self, payment_id: Uuid) -> Result<PaymentRow, BillingError> {
        let row = self
            .find_payment(payment_id)
            .await?
            .ok_or(BillingError::NotFound)?;
        let current = row
            .payment_status()
            .ok_or(BillingError::Db(sqlx::Error::Decode(
                "unknown payment status".into(),
            )))?;
        let _ = current
            .apply(PaymentAction::BeginCancel)
            .map_err(|_| BillingError::InvalidTransition)?;
        let updated = sqlx::query_as::<_, PaymentRow>(
            "UPDATE payment SET status = 'cancelling' WHERE id = $1 \
             RETURNING id, request_id, stripe_payment_intent_id, idempotency_key, amount_minor, \
             currency, status, refunded_minor, failure_reason, stripe_error_code, \
             split_snapshot, created_at, captured_at",
        )
        .bind(payment_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(updated)
    }

    /// Инвариант-чекер (для тестов и ночной джобы F6):
    /// Σ ledger_entry(txn) == 0 для каждой проводки.
    pub async fn ledger_txn_invariant(&self) -> Result<Vec<Uuid>, BillingError> {
        let rows: Vec<(Uuid,)> = sqlx::query_as(
            "SELECT txn_id FROM ledger_entry \
             GROUP BY txn_id HAVING sum(amount_minor) <> 0",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|(u,)| u).collect())
    }

    /// Инвариант: Σ split(payment) == amount_minor.
    pub async fn split_invariant(&self) -> Result<Vec<Uuid>, BillingError> {
        let rows: Vec<(Uuid,)> = sqlx::query_as(
            "SELECT s.payment_id FROM split s \
             JOIN payment p ON p.id = s.payment_id \
             GROUP BY s.payment_id, p.amount_minor \
             HAVING sum(s.amount_minor) <> p.amount_minor",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|(u,)| u).collect())
    }
}
