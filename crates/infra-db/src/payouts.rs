//! Репозиторий выплат и сверки (F6, Test Mode).
//!
//! Правила (техкарта Часть V, шаги 7–9):
//!   * НЕТТО по payee = Σ split(pending) − Σ некомпенсированных
//!     reversed — НЕ глобальная сумма;
//!   * порог 10 € (Q4): ниже — перенос (scheduled на след. неделю);
//!     финальная выплата без порога при блокировке/уходе;
//!   * transfer-вебхуки идемпотентны по stripe_transfer_id;
//!   * часовой poller незакрытых in_flight: воркер, упавший после
//!     вызова провайдера, не оставит payout висеть;
//!   * reconciliation: ledger vs события, расхождение → incident;
//!   * ИНВАРИАНТ ПО КАЖДОМУ payee:
//!     Σ ledger(payable:X) == Σ split(X,pending)
//!       + Σ split(X, в выплатах in_flight|paid) − Σ reversed.

use chrono::{DateTime, Utc};
use dropit_domain::{
    decide_payout, PayoutAction, PayoutDecision, PayoutStatus, DEFAULT_PAYOUT_THRESHOLD_MINOR,
    MAX_PAYOUT_ATTEMPTS,
};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum PayoutError {
    #[error("database error")]
    Db(#[from] sqlx::Error),
    #[error("payout not found")]
    NotFound,
    #[error("invalid transition")]
    InvalidTransition,
}

#[derive(Debug, sqlx::FromRow)]
pub struct PayoutRow {
    pub id: Uuid,
    pub payee_type: String,
    pub payee_id: Uuid,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub amount_minor: i64,
    pub min_threshold_minor: i64,
    pub status: String,
    pub stripe_transfer_id: Option<String>,
    pub fail_reason: Option<String>,
    pub attempts: i32,
    #[sqlx(rename = "final")]
    pub is_final: bool,
    pub created_at: DateTime<Utc>,
    pub paid_at: Option<DateTime<Utc>>,
}

impl PayoutRow {
    pub fn payout_status(&self) -> Option<PayoutStatus> {
        match self.status.as_str() {
            "scheduled" => Some(PayoutStatus::Scheduled),
            "in_flight" => Some(PayoutStatus::InFlight),
            "paid" => Some(PayoutStatus::Paid),
            "failed" => Some(PayoutStatus::Failed),
            "on_hold" => Some(PayoutStatus::OnHold),
            _ => None,
        }
    }
}

/// Payee для выплат: клуб или диджей.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Payee {
    pub payee_type: &'static str, // 'club' | 'dj'
    pub payee_id: Uuid,
}

/// Нетто payee за всё время (минорные единицы).
#[derive(Debug, sqlx::FromRow)]
struct NetRow {
    payee_type: String,
    payee_id: Uuid,
    net_minor: i64,
}

#[derive(Clone)]
pub struct PayoutRepo {
    pool: PgPool,
}

impl PayoutRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> PgPool {
        self.pool.clone()
    }

    // ---------------------------------------------------------
    // Цикл выплат (еженедельно, пн 10:00 UTC — триггер извне)
    // ---------------------------------------------------------

    /// НЕТТО каждого payee: pending-сплиты минус некомпенсированные
    /// реверсы. Сплиты, уже входящие в НE ЗАКРЫТЫЕ выплаты
    /// (in_flight|paid), исключаются — двойной выплаты нет.
    /// ПО КАЖДОМУ payee, не глобально (правило техкарты).
    pub async fn net_by_payee(&self) -> Result<Vec<(Payee, i64)>, PayoutError> {
        let rows: Vec<NetRow> = sqlx::query_as(
            "SELECT s.payee_type, s.payee_id, \
             COALESCE(sum(s.amount_minor - s.reversed_minor), 0)::bigint AS net_minor \
             FROM split s \
             WHERE s.status = 'pending' \
             GROUP BY s.payee_type, s.payee_id",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .filter_map(|r| {
                let payee_type = match r.payee_type.as_str() {
                    "club" => Some("club"),
                    "dj" => Some("dj"),
                    _ => None,
                }?;
                Some((
                    Payee {
                        payee_type,
                        payee_id: r.payee_id,
                    },
                    r.net_minor,
                ))
            })
            .collect())
    }

    /// Один цикл выплат: для каждого payee считает нетто, решает
    /// (порог/финальная), создаёт payout и «отправляет» Transfer
    /// (мок: transfer_id присваивается сразу, статус in_flight).
    ///
    /// Возвращает список созданных payouts (для ответа админу).
    /// Ниже порога → payout НЕ создаётся (перос на след. цикл —
    /// данные никуда не деваются: split остаётся pending).
    pub async fn run_payout_cycle(
        &self,
        final_payees: &[Payee],
    ) -> Result<Vec<PayoutRow>, PayoutError> {
        let mut created = Vec::new();
        for (payee, net) in self.net_by_payee().await? {
            let is_final = final_payees.contains(&payee);
            match decide_payout(net, DEFAULT_PAYOUT_THRESHOLD_MINOR, is_final) {
                PayoutDecision::Send(amount) => {
                    let row = self.create_payout(payee, amount, is_final).await?;
                    created.push(row);
                }
                // Ниже порога — перенос: split остаётся pending,
                // следующий цикл увидит то же нетто.
                PayoutDecision::BelowThreshold { .. } | PayoutDecision::Nothing => {}
            }
        }
        Ok(created)
    }

    /// Создаёт payout и «отправляет» Transfer (МОК: in_flight,
    /// transfer_id сразу; реальный вызов — провайдер после ADR-004).
    async fn create_payout(
        &self,
        payee: Payee,
        amount_minor: i64,
        is_final: bool,
    ) -> Result<PayoutRow, PayoutError> {
        let transfer_id = format!("tr_mock_{}", Uuid::now_v7().simple());
        let row = sqlx::query_as::<_, PayoutRow>(
            "INSERT INTO payout (id, payee_type, payee_id, period_start, period_end, \
             amount_minor, min_threshold_minor, status, stripe_transfer_id, attempts, final) \
             VALUES ($1, $2, $3, now() - interval '7 days', now(), $4, $5, 'in_flight', $6, 1, $7) \
             RETURNING id, payee_type, payee_id, period_start, period_end, amount_minor, \
             min_threshold_minor, status, stripe_transfer_id, fail_reason, attempts, \
             final AS \"final\", created_at, paid_at",
        )
        .bind(Uuid::now_v7())
        .bind(payee.payee_type)
        .bind(payee.payee_id)
        .bind(amount_minor)
        .bind(DEFAULT_PAYOUT_THRESHOLD_MINOR)
        .bind(&transfer_id)
        .bind(is_final)
        .fetch_one(&self.pool)
        .await?;
        // Сплиты payee закрепляются за выплатой: status → paid_out
        // (в in_flight-выплате деньги уже «в пути», повторный цикл их
        // не увидит — защита от двойной выплаты).
        sqlx::query(
            "UPDATE split SET status = 'paid_out' \
             WHERE payee_type = $1 AND payee_id = $2 AND status = 'pending'",
        )
        .bind(payee.payee_type)
        .bind(payee.payee_id)
        .execute(&self.pool)
        .await?;
        Ok(row)
    }

    // ---------------------------------------------------------
    // Transfer-вебхуки (идемпотентно по stripe_transfer_id)
    // ---------------------------------------------------------

    /// transfer.paid: payout → paid. Идемпотентно: повторный вебхук
    /// (тот же transfer_id) — no-op Ok(false).
    pub async fn handle_transfer_paid(
        &self,
        stripe_transfer_id: &str,
    ) -> Result<bool, PayoutError> {
        let row = self
            .find_by_transfer(stripe_transfer_id)
            .await?
            .ok_or(PayoutError::NotFound)?;
        if row.status == "paid" {
            return Ok(false); // дубль вебхука
        }
        let updated = sqlx::query(
            "UPDATE payout SET status = 'paid', paid_at = now() \
             WHERE stripe_transfer_id = $1 AND status = 'in_flight'",
        )
        .bind(stripe_transfer_id)
        .execute(&self.pool)
        .await?;
        Ok(updated.rows_affected() == 1)
    }

    /// transfer.failed: payout → failed; после MAX_PAYOUT_ATTEMPTS
    /// ретраев — on_hold + инцидент админу.
    pub async fn handle_transfer_failed(
        &self,
        stripe_transfer_id: &str,
        reason: &str,
    ) -> Result<PayoutRow, PayoutError> {
        let row = self
            .find_by_transfer(stripe_transfer_id)
            .await?
            .ok_or(PayoutError::NotFound)?;
        let current = row
            .payout_status()
            .ok_or(PayoutError::Db(sqlx::Error::Decode(
                "unknown payout status".into(),
            )))?;
        let next = current
            .apply(PayoutAction::TransferFailed)
            .map_err(|_| PayoutError::InvalidTransition)?;
        let attempts = row.attempts + 1;
        let (status, hold) = if attempts >= MAX_PAYOUT_ATTEMPTS {
            ("on_hold", true)
        } else {
            (status_str(next), false)
        };
        let updated = sqlx::query_as::<_, PayoutRow>(
            "UPDATE payout SET status = $2, fail_reason = $3, attempts = $4 \
             WHERE stripe_transfer_id = $1 \
             RETURNING id, payee_type, payee_id, period_start, period_end, amount_minor, \
             min_threshold_minor, status, stripe_transfer_id, fail_reason, attempts, \
             final AS \"final\", created_at, paid_at",
        )
        .bind(stripe_transfer_id)
        .bind(status)
        .bind(reason)
        .bind(attempts)
        .fetch_one(&self.pool)
        .await?;
        if hold {
            // Инцидент: ручной разбор админом (payout_failed).
            sqlx::query(
                "INSERT INTO incident (id, kind, message, ref) \
                 VALUES ($1, 'payout_failed', $2, $3)",
            )
            .bind(Uuid::now_v7())
            .bind(format!(
                "payout to {} {} failed {} times: {reason}",
                row.payee_type, row.payee_id, MAX_PAYOUT_ATTEMPTS
            ))
            .bind(row.id.to_string())
            .execute(&self.pool)
            .await?;
            // Сплиты возвращаются в pending: деньги payee не
            // пропали, следующий разбор/цикл их увидит.
            sqlx::query(
                "UPDATE split SET status = 'pending' \
                 WHERE payee_type = $1 AND payee_id = $2 AND status = 'paid_out'",
            )
            .bind(row.payee_type)
            .bind(row.payee_id)
            .execute(&self.pool)
            .await?;
        }
        Ok(updated)
    }

    /// Retry: failed → in_flight (новый transfer_id — идемпотентность
    /// по нему), attempts+1. Вызывается воркером по backoff.
    pub async fn retry_payout(&self, payout_id: Uuid) -> Result<PayoutRow, PayoutError> {
        let row = self
            .find_payout(payout_id)
            .await?
            .ok_or(PayoutError::NotFound)?;
        if row.status != "failed" {
            return Err(PayoutError::InvalidTransition);
        }
        let transfer_id = format!("tr_mock_{}", Uuid::now_v7().simple());
        let updated = sqlx::query_as::<_, PayoutRow>(
            "UPDATE payout SET status = 'in_flight', stripe_transfer_id = $2, attempts = attempts + 1 \
             WHERE id = $1 \
             RETURNING id, payee_type, payee_id, period_start, period_end, amount_minor, \
             min_threshold_minor, status, stripe_transfer_id, fail_reason, attempts, \
             final AS \"final\", created_at, paid_at",
        )
        .bind(payout_id)
        .bind(&transfer_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(updated)
    }

    /// Незакрытые in_flight — для часового poller (вебхук потерялся).
    pub async fn in_flight_payouts(&self) -> Result<Vec<PayoutRow>, PayoutError> {
        let rows = sqlx::query_as::<_, PayoutRow>(
            "SELECT id, payee_type, payee_id, period_start, period_end, amount_minor, \
             min_threshold_minor, status, stripe_transfer_id, fail_reason, attempts, \
             final AS \"final\", created_at, paid_at FROM payout WHERE status = 'in_flight'",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn find_payout(&self, id: Uuid) -> Result<Option<PayoutRow>, PayoutError> {
        let row = sqlx::query_as::<_, PayoutRow>(
            "SELECT id, payee_type, payee_id, period_start, period_end, amount_minor, \
             min_threshold_minor, status, stripe_transfer_id, fail_reason, attempts, \
             final AS \"final\", created_at, paid_at FROM payout WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    /// Выплаты за последнюю неделю (админ-список).
    pub async fn recent_payouts(&self) -> Result<Vec<PayoutRow>, PayoutError> {
        let rows = sqlx::query_as::<_, PayoutRow>(
            "SELECT id, payee_type, payee_id, period_start, period_end, amount_minor, \
             min_threshold_minor, status, stripe_transfer_id, fail_reason, attempts, \
             final AS \"final\", created_at, paid_at FROM payout \
             WHERE created_at > now() - interval '7 days' ORDER BY created_at DESC",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Статус reconciliation-прогона.
    pub async fn run_status(&self, run_id: Uuid) -> Result<String, PayoutError> {
        let (status,): (String,) =
            sqlx::query_as("SELECT status FROM reconciliation_run WHERE id = $1")
                .bind(run_id)
                .fetch_one(&self.pool)
                .await?;
        Ok(status)
    }

    async fn find_by_transfer(&self, tid: &str) -> Result<Option<PayoutRow>, PayoutError> {
        let row = sqlx::query_as::<_, PayoutRow>(
            "SELECT id, payee_type, payee_id, period_start, period_end, amount_minor, \
             min_threshold_minor, status, stripe_transfer_id, fail_reason, attempts, \
             final AS \"final\", created_at, paid_at FROM payout WHERE stripe_transfer_id = $1",
        )
        .bind(tid)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    // ---------------------------------------------------------
    // ИНВАРИАНТ ПО КАЖДОМУ PAYEE (ночной чекер, техкарта V шаг 9)
    // ---------------------------------------------------------

    /// Для каждого payee: Σ ledger(payable:X) должна равняться
    /// Σ нетто-сплитов X (pending + в выплатах). Нарушение →
    /// стоп выплат + инцидент. Возвращает список сломанных payee.
    pub async fn payee_invariant_violations(&self) -> Result<Vec<Payee>, PayoutError> {
        // ledger-сторона: сумма по payable:X (у нас ledger ведёт
        // payable:club / payable:dj без id в account — сопоставление
        // через ref payment'ов со сплитами этого payee).
        let rows: Vec<(String, Uuid, i64, i64)> = sqlx::query_as(
            "SELECT s.payee_type, s.payee_id, \
             COALESCE(sum(s.amount_minor - s.reversed_minor), 0)::bigint AS splits_net, \
             (SELECT COALESCE(sum(l.amount_minor), 0)::bigint FROM ledger_entry l \
              JOIN payment p ON p.id::text = l.ref \
              JOIN split s2 ON s2.payment_id = p.id AND s2.payee_type = s.payee_type AND s2.payee_id = s.payee_id \
              WHERE l.account = 'payable:' || s.payee_type) AS ledger_sum \
             FROM split s GROUP BY s.payee_type, s.payee_id",
        )
        .fetch_all(&self.pool)
        .await?;
        let mut violations = Vec::new();
        for (payee_type, payee_id, splits_net, ledger_sum) in rows {
            if splits_net != ledger_sum {
                violations.push(Payee {
                    payee_type: match payee_type.as_str() {
                        "club" => "club",
                        "dj" => "dj",
                        _ => continue,
                    },
                    payee_id,
                });
            }
        }
        Ok(violations)
    }

    // ---------------------------------------------------------
    // Reconciliation (ежедневная сверка, техкарта V шаг 8)
    // ---------------------------------------------------------

    /// Сверка: захваченные платежи (ledger) vs captured-статусы.
    /// Расхождение → incident + статус discrepancies.
    pub async fn run_reconciliation(&self) -> Result<Uuid, PayoutError> {
        let run_id = Uuid::now_v7();
        sqlx::query(
            "INSERT INTO reconciliation_run (id, period_start, period_end, status) \
             VALUES ($1, now() - interval '24 hours', now(), 'running')",
        )
        .bind(run_id)
        .execute(&self.pool)
        .await?;

        // Ожидаемо: Σ ledger по payable+revenue (сумма захваченного).
        let (expected,): (i64,) = sqlx::query_as(
            "SELECT COALESCE(sum(amount_minor), 0)::bigint FROM ledger_entry \
             WHERE entry_type = 'capture'",
        )
        .fetch_one(&self.pool)
        .await?;
        // Фактически: Σ captured-платежей.
        let (actual,): (i64,) = sqlx::query_as(
            "SELECT COALESCE(sum(amount_minor - refunded_minor), 0)::bigint FROM payment \
             WHERE status IN ('captured', 'partially_refunded', 'refunded')",
        )
        .fetch_one(&self.pool)
        .await?;

        let discrepancies: Vec<serde_json::Value> = if expected == actual {
            vec![]
        } else {
            vec![serde_json::json!({
                "kind": "capture_sum_mismatch",
                "expected_minor": expected,
                "actual_minor": actual,
            })]
        };

        let status = if discrepancies.is_empty() {
            "ok"
        } else {
            "discrepancies"
        };
        sqlx::query(
            "UPDATE reconciliation_run SET expected_minor = $2, actual_minor = $3, \
             discrepancies = $4, status = $5 WHERE id = $1",
        )
        .bind(run_id)
        .bind(expected)
        .bind(actual)
        .bind(serde_json::to_value(&discrepancies).expect("serializes"))
        .bind(status)
        .execute(&self.pool)
        .await?;

        if !discrepancies.is_empty() {
            sqlx::query(
                "INSERT INTO incident (id, kind, message, ref) \
                 VALUES ($1, 'reconciliation', $2, $3)",
            )
            .bind(Uuid::now_v7())
            .bind(format!(
                "reconciliation: expected {expected} vs actual {actual} (minor units)"
            ))
            .bind(run_id.to_string())
            .execute(&self.pool)
            .await?;
        }
        Ok(run_id)
    }

    /// Открытые инциденты (для админ-панели).
    pub async fn open_incidents(&self) -> Result<Vec<(Uuid, String, String)>, PayoutError> {
        let rows: Vec<(Uuid, String, String)> = sqlx::query_as(
            "SELECT id, kind, message FROM incident WHERE resolved_at IS NULL ORDER BY created_at DESC",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Разбор инцидента админом.
    pub async fn resolve_incident(
        &self,
        incident_id: Uuid,
        admin_id: Uuid,
    ) -> Result<bool, PayoutError> {
        let res = sqlx::query(
            "UPDATE incident SET resolved_by = $2, resolved_at = now() \
             WHERE id = $1 AND resolved_at IS NULL",
        )
        .bind(incident_id)
        .bind(admin_id)
        .execute(&self.pool)
        .await?;
        Ok(res.rows_affected() == 1)
    }
}

fn status_str(s: PayoutStatus) -> &'static str {
    match s {
        PayoutStatus::Scheduled => "scheduled",
        PayoutStatus::InFlight => "in_flight",
        PayoutStatus::Paid => "paid",
        PayoutStatus::Failed => "failed",
        PayoutStatus::OnHold => "on_hold",
    }
}
