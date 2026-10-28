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
    /// `Idempotency-Key` уже привязан к ДРУГОМУ `request_id` — выдача
    /// чужого `payment_id`/суммы недопустима (→ 409 `idempotency_conflict`).
    #[error("idempotency key belongs to another request")]
    IdempotencyConflict,
}

/// Список колонок `payment` для `SELECT … FROM payment` — единый
/// источник правды, чтобы `query_as::<PaymentRow>` и `RETURNING` не
/// рассинхронизировались. Включает все поля `PaymentRow` в порядке
/// декларации структуры.
const PAYMENT_COLS: &str = "id, request_id, stripe_payment_intent_id, idempotency_key, \
     amount_minor, currency, status, refunded_minor, failure_reason, \
     stripe_error_code, split_snapshot, created_at, captured_at";

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

/// Исход атомарного создания платежа для запроса
/// ([`BillingRepo::create_payment_for_request`]).
#[derive(Debug)]
pub enum CreatePaymentOutcome {
    /// Создана новая строка payment(intent).
    Created(PaymentRow),
    /// Найден существующий «активный» платёж — replay/гонка: новый
    /// intent создаваться не должен, возвращается эта строка.
    Existing(PaymentRow),
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

    /// Вычисляет снимок правил разреза для платежа. Неизменяем после
    /// captured — аудит «почему разделили именно так». Ошибка сериализации
    /// (практически невозможна для примитивных полей) пробрасывается как
    /// `BillingError::Db(Decode)` — без паник в рабочем коде.
    fn build_snapshot(
        amount_minor: i64,
        commission: &CommissionParams,
    ) -> Result<SplitSnapshot, BillingError> {
        let split = compute_split(
            amount_minor,
            commission.commission_bp,
            commission.dj_share_bp,
        )
        .map_err(|_| BillingError::AmountMismatch)?;
        Ok(SplitSnapshot {
            commission_bp: commission.commission_bp,
            dj_share_bp: commission.dj_share_bp,
            amount_minor,
            currency: commission.currency.clone(),
            platform_minor: split.platform_minor,
            club_minor: split.club_minor,
            dj_minor: split.dj_minor,
        })
    }

    /// Создаёт payment(intent) со снимком цены и правил разреза.
    /// Снимок неизменяем после captured — аудит «почему разделили».
    ///
    /// Идемпотентна по `idempotency_key` (UNIQUE в схеме): сетевой ретрай
    /// с тем же ключом НЕ создаёт второй строки и НЕ даёт 500 —
    /// возвращается уже сохранённая запись **того же** `request_id`.
    ///
    /// Защита от выдачи чужого платежа (W7-A, ДЕФЕКТ 2): используется
    /// `ON CONFLICT (idempotency_key) DO NOTHING` + проверка
    /// `rows_affected`. При конфликте строка перечитывается по ключу и
    /// её `request_id` ЯВНО сверяется с ожидаемым:
    /// * совпадает → replay, возвращается та же строка;
    /// * чужой `request_id` → `BillingError::IdempotencyConflict`
    ///   (никакой выдачи чужого `payment_id`/`amount_minor`).
    ///
    /// Строки split создаются при захвате (payment без денег не плодит
    /// будущие payable).
    pub async fn create_payment(
        &self,
        request_id: Uuid,
        amount_minor: i64,
        idempotency_key: &str,
        intent_id: &str,
        commission: &CommissionParams,
    ) -> Result<PaymentRow, BillingError> {
        let snapshot = Self::build_snapshot(amount_minor, commission)?;
        let snapshot_json = serde_json::to_value(&snapshot)
            .map_err(|_| BillingError::Db(sqlx::Error::Decode("snapshot serialize".into())))?;
        let id = Uuid::now_v7();
        // DO NOTHING: при конфликте по ключу INSERT ничего не меняет и
        // не возвращает чужую строку (в отличие от DO UPDATE RETURNING).
        let res = sqlx::query(
            "INSERT INTO payment (id, request_id, stripe_payment_intent_id, idempotency_key, \
             amount_minor, currency, status, split_snapshot) \
             VALUES ($1, $2, $3, $4, $5, $6, 'intent', $7) \
             ON CONFLICT (idempotency_key) DO NOTHING",
        )
        .bind(id)
        .bind(request_id)
        .bind(intent_id)
        .bind(idempotency_key)
        .bind(amount_minor)
        .bind(&commission.currency)
        .bind(snapshot_json)
        .execute(&self.pool)
        .await?;
        if res.rows_affected() == 1 {
            // Новая строка — читаем её по сгенерированному id.
            let row = sqlx::query_as::<_, PaymentRow>(&format!(
                "SELECT {PAYMENT_COLS} FROM payment WHERE id = $1"
            ))
            .bind(id)
            .fetch_one(&self.pool)
            .await?;
            return Ok(row);
        }
        // Конфликт по idempotency_key: перечитываем строку-владельца ключа
        // и ЯВНО сверяем request_id. Чужой → IdempotencyConflict.
        let existing = sqlx::query_as::<_, PaymentRow>(&format!(
            "SELECT {PAYMENT_COLS} FROM payment WHERE idempotency_key = $1"
        ))
        .bind(idempotency_key)
        .fetch_one(&self.pool)
        .await?;
        if existing.request_id != request_id {
            return Err(BillingError::IdempotencyConflict);
        }
        Ok(existing)
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

    /// Находит «активный» платёж запроса — то есть НЕ в терминально
    /// провальном/отменённом состоянии (`status NOT IN
    /// ('cancelled','refunded','failed')`). Если такой платёж есть,
    /// повторное создание платежа для этого request обязано вернуть его
    /// (replay), а не создавать новый intent (анти-двойное списание).
    /// Берётся последняя по `created_at` строка — на случай, когда после
    /// cancelled/failed создан новый.
    pub async fn find_active_payment_by_request(
        &self,
        request_id: Uuid,
    ) -> Result<Option<PaymentRow>, BillingError> {
        let row = sqlx::query_as::<_, PaymentRow>(
            "SELECT id, request_id, stripe_payment_intent_id, idempotency_key, amount_minor, \
             currency, status, refunded_minor, failure_reason, stripe_error_code, \
             split_snapshot, created_at, captured_at FROM payment \
             WHERE request_id = $1 AND status NOT IN ('cancelled','refunded','failed') \
             ORDER BY created_at DESC LIMIT 1",
        )
        .bind(request_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    /// Атомарно создаёт платёж для запроса либо возвращает существующий
    /// «активный» платёж. Гарантирует инвариант «один request → один
    /// активный платёж» без миграции схемы:
    ///
    /// * `SELECT … FOR UPDATE` по строке `request` сериализует параллельные
    ///   создания платежа для одного request — два одновременных запроса
    ///   с разными `Idempotency-Key` не создадут двух строк;
    /// * повторная проверка существующего активного платежа ВНУТРИ блокировки
    ///   ловит гонку, когда предыдущий вызов уже успел создать платёж;
    /// * вставка идемпотентна по `idempotency_key` (`ON CONFLICT … DO
    ///   NOTHING` + проверка `rows_affected`) — сетевой ретрай тем же
    ///   ключом не даёт 500 и не плодит дубль; при конфликте строка
    ///   перечитывается и её `request_id` сверяется с ожидаемым
    ///   (чужой → `IdempotencyConflict`, никакой выдачи чужого платежа);
    /// * гейт: `request.status == 'pending'`, иначе `RequestNotPending`.
    ///
    /// Вызывающий слой ДО этого вызова обязан сделать быструю проверку
    /// `find_active_payment_by_request` (без блокировки), чтобы не
    /// вызывать провайдера на replay-дубле.
    pub async fn create_payment_for_request(
        &self,
        request_id: Uuid,
        amount_minor: i64,
        idempotency_key: &str,
        intent_id: &str,
        commission: &CommissionParams,
    ) -> Result<CreatePaymentOutcome, BillingError> {
        let mut tx = self.pool.begin().await?;
        // Блокируем строку request — сериализация параллельных созданий
        // платежа для одного request (достаточно без частичного индекса).
        let (req_status,): (String,) =
            sqlx::query_as("SELECT status FROM request WHERE id = $1 FOR UPDATE")
                .bind(request_id)
                .fetch_one(&mut *tx)
                .await?;
        if req_status != "pending" {
            return Err(BillingError::RequestNotPending);
        }
        // Повторная проверка существующего активного платежа ВНУТРИ
        // блокировки request — ловит гонку с параллельным создателем.
        let existing = sqlx::query_as::<_, PaymentRow>(
            "SELECT id, request_id, stripe_payment_intent_id, idempotency_key, amount_minor, \
             currency, status, refunded_minor, failure_reason, stripe_error_code, \
             split_snapshot, created_at, captured_at FROM payment \
             WHERE request_id = $1 AND status NOT IN ('cancelled','refunded','failed') \
             ORDER BY created_at DESC LIMIT 1",
        )
        .bind(request_id)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(p) = existing {
            tx.commit().await?;
            return Ok(CreatePaymentOutcome::Existing(p));
        }
        // Новая строка — идемпотентная по idempotency_key.
        // Защита от выдачи чужого платежа (W7-A, ДЕФЕКТ 2): DO NOTHING +
        // проверка rows_affected; при конфликте строка перечитывается и
        // её request_id ЯВНО сверяется с ожидаемым. Чужой request_id →
        // IdempotencyConflict (409), а не чужой payment_id.
        let snapshot = Self::build_snapshot(amount_minor, commission)?;
        let snapshot_json = serde_json::to_value(&snapshot)
            .map_err(|_| BillingError::Db(sqlx::Error::Decode("snapshot serialize".into())))?;
        let id = Uuid::now_v7();
        let res = sqlx::query(
            "INSERT INTO payment (id, request_id, stripe_payment_intent_id, idempotency_key, \
             amount_minor, currency, status, split_snapshot) \
             VALUES ($1, $2, $3, $4, $5, $6, 'intent', $7) \
             ON CONFLICT (idempotency_key) DO NOTHING",
        )
        .bind(id)
        .bind(request_id)
        .bind(intent_id)
        .bind(idempotency_key)
        .bind(amount_minor)
        .bind(&commission.currency)
        .bind(snapshot_json)
        .execute(&mut *tx)
        .await?;
        if res.rows_affected() == 1 {
            let row = sqlx::query_as::<_, PaymentRow>(&format!(
                "SELECT {PAYMENT_COLS} FROM payment WHERE id = $1"
            ))
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
            tx.commit().await?;
            return Ok(CreatePaymentOutcome::Created(row));
        }
        // Конфликт по ключу — перечитываем владельца и сверяем request_id.
        let existing = sqlx::query_as::<_, PaymentRow>(&format!(
            "SELECT {PAYMENT_COLS} FROM payment WHERE idempotency_key = $1"
        ))
        .bind(idempotency_key)
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        if existing.request_id != request_id {
            // Чужой ключ: 409, никакой выдачи чужого payment_id.
            return Err(BillingError::IdempotencyConflict);
        }
        // Тот же request, тот же ключ — replay существующей строки.
        Ok(CreatePaymentOutcome::Existing(existing))
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
    ///
    /// Условный переход (W7-A, ДЕФЕКТ 1): `UPDATE … WHERE status IN
    /// ('intent','processing')` — только эти два состояния стейт-машина
    /// разрешает перевести в `cancelling` (`(Intent,BeginCancel)` и
    /// `(Processing,BeginCancel)`). Никакого «слепого» UPDATE по id:
    ///
    /// * `rows_affected == 1` — переход применён, возвращается строка
    ///   со статусом `cancelling`;
    /// * `rows_affected == 0` — статус не подходит (captured/cancelled/
    ///   refunded/failed) либо успел измениться в гонке: строка
    ///   перечитывается и возвращается её **актуальное** состояние, а не
    ///   рапортуется об отмене. Так replay-вызов `confirm` (платёж уже
    ///   `processing` после параллельного подтверждения) НЕ отменяет
    ///   легитимно подтверждённый платёж.
    ///
    /// Разрешено `processing → cancelling` осознанно: TTL-воркер может
    /// начать отмену уже подтверждённого, но ещё не захваченного платежа
    /// (гонка cancel↔capture закрывается вебхуком: `cancelling → captured`).
    pub async fn begin_cancel(&self, payment_id: Uuid) -> Result<PaymentRow, BillingError> {
        let mut tx = self.pool.begin().await?;
        // Условный UPDATE c RETURNING: WHERE-условие — единственный
        // авторитет перехода (стейт-машина encoded в IN
        // ('intent','processing')); никакой слепой перезаписи
        // подтверждённого/терминального платежа. `fetch_optional` возвращает
        // None ровно когда rows_affected == 0 (статус не подошёл).
        let updated = sqlx::query_as::<_, PaymentRow>(&format!(
            "UPDATE payment SET status = 'cancelling' \
             WHERE id = $1 AND status IN ('intent','processing') \
             RETURNING {PAYMENT_COLS}"
        ))
        .bind(payment_id)
        .fetch_optional(&mut *tx)
        .await?;
        let row = match updated {
            // Переход применился → статус теперь cancelling.
            Some(r) => r,
            // 0 строк: статус не подошёл (captured/cancelled/refunded/
            // failed) либо успел измениться в гонке. Перечитываем
            // АКТУАЛЬНОЕ состояние и возвращаем его — не рапортуем об
            // отмене и не возвращаем ошибку. Это защита подтверждённого
            // платежа от параллельного replay confirm.
            None => {
                sqlx::query_as::<_, PaymentRow>(&format!(
                    "SELECT {PAYMENT_COLS} FROM payment WHERE id = $1"
                ))
                .bind(payment_id)
                .fetch_one(&mut *tx)
                .await?
            }
        };
        tx.commit().await?;
        Ok(row)
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
