//! Задачи воркера: отчёт, одноразовые прогоны и супервизор.
//!
//! Каждая задача возвращает [`JobReport`] и печатает одну JSON-строку итога.
//! Код возврата: `0` для статусов `ok`/`skipped`, `1` для `error`.
//!
//! # Идемпотентность
//!
//! Все `--once`-прогоны безопасны для повторного запуска: идемпотентность
//! обеспечивается существующими статусами/ключами в БД (`published_at`,
//! `status`, `revoked_at`, идемпотентность Transfer-вебхуков), а не флагами
//! в памяти.

use std::sync::Arc;
use std::time::Duration;

use dropit_infra_db::{ClosedSession, JobsRepo, PayoutRepo};
use dropit_infra_nats::{MessageBus, OutboxRelay, OutboxStore, RelayReport};
use serde::Serialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::mail::{run_mail, DbMailStore};
use crate::push::{run_push, DbTokenSource};
use crate::runtime::WorkerEnv;

/// Отчёт одного прогона задачи. Сериализуется в одну JSON-строку.
#[derive(Debug, Clone, Serialize)]
pub struct JobReport {
    /// Имя задачи (`outbox`, `payouts`, ...).
    pub job: &'static str,
    /// Исход: `ok` | `skipped` | `error`.
    pub status: &'static str,
    /// Счётчики/детали (плоские поля, мерджатся в вывод).
    pub detail: Value,
}

impl JobReport {
    /// Успешный прогон.
    pub fn ok(job: &'static str, detail: Value) -> Self {
        Self {
            job,
            status: "ok",
            detail,
        }
    }

    /// Прогон пропущен (деградация: нет БД/NATS).
    pub fn skipped(job: &'static str, reason: &str) -> Self {
        Self {
            job,
            status: "skipped",
            detail: json!({ "reason": reason }),
        }
    }

    /// Ошибка прогона (транспорт/БД).
    pub fn error(job: &'static str, message: String) -> Self {
        Self {
            job,
            status: "error",
            detail: json!({ "error": message }),
        }
    }

    /// Код возврата процесса: `0` для ok/skipped, `1` для error.
    #[must_use]
    pub fn exit_code(&self) -> i32 {
        if self.status == "error" {
            1
        } else {
            0
        }
    }

    /// Одна JSON-строка итога (job, status, поля detail на верхнем уровне).
    #[must_use]
    pub fn to_line(&self) -> String {
        let mut obj = serde_json::Map::new();
        obj.insert("job".to_string(), Value::String(self.job.to_string()));
        obj.insert("status".to_string(), Value::String(self.status.to_string()));
        if let Value::Object(detail) = &self.detail {
            for (k, v) in detail {
                obj.insert(k.clone(), v.clone());
            }
        } else {
            obj.insert("detail".to_string(), self.detail.clone());
        }
        serde_json::to_string(&Value::Object(obj)).unwrap_or_else(|_| "{}".to_string())
    }
}

/// Деталь отчёта outbox-реля из [`RelayReport`].
fn relay_detail(r: RelayReport) -> Value {
    json!({
        "claimed": r.claimed,
        "published": r.published,
        "failed": r.failed,
        "skipped": r.skipped,
    })
}

// ============================================================
// Outbox-реле (ADR-002): публикация накопленных событий в NATS.
// ============================================================

/// Один прогон outbox-реля.
///
/// При `nats_connected == false` (NoopBus) реле пропускается: публикация в
/// no-op без доставки потеряла бы события (они помечались бы `published_at`
/// без реальной отправки). События остаются в очереди до поднятия NATS;
/// push-диспетчер при этом продолжает обрабатывать свои типы.
pub async fn run_outbox_once(
    store: Arc<dyn OutboxStore>,
    bus: Arc<dyn MessageBus>,
    batch_size: i64,
    max_attempts: i32,
    nats_connected: bool,
) -> JobReport {
    if !nats_connected {
        tracing::warn!(
            "outbox: NATS недоступен (NoopBus) — реле пропущен, события останутся в очереди"
        );
        return JobReport::skipped("outbox", "nats_unavailable");
    }
    let relay = OutboxRelay::new(store, bus, batch_size, max_attempts);
    match relay.run_once().await {
        Ok(r) => {
            tracing::info!(
                claimed = r.claimed,
                published = r.published,
                "outbox relay done"
            );
            JobReport::ok("outbox", relay_detail(r))
        }
        Err(e) => JobReport::error("outbox", e.to_string()),
    }
}

// ============================================================
// DB-задачи (выплаты, сверка, retention, автозакрытие).
// Переиспользуют методы jobs.rs/payouts.rs — формулы и статусы не меняются.
// ============================================================

/// Прогон выплат за закрытые периоды.
///
/// Сначала освобождает зависшие интенты (`expire_stale_intents`: intent →
/// cancelling после 15 мин TTL — техкарта V), затем выполняет цикл выплат
/// (`run_payout_cycle`). Финальные payees (блокировка/уход) здесь не
/// передаются — регулярный цикл.
#[cfg(test)]
pub async fn run_payouts_once(jobs: Option<&JobsRepo>, payouts: Option<&PayoutRepo>) -> JobReport {
    run_payouts_with_mail(jobs, payouts, None).await
}

/// Прогон выплат с уведомлениями: та же логика, плюс письма `payout_paid`.
///
/// Письмо ставится в очередь ПОСЛЕ успешного создания выплаты: если выплата
/// не создалась, получатель не должен узнать о деньгах, которых нет.
/// Ошибка почты не отменяет выплату — деньги важнее уведомления.
pub async fn run_payouts_with_mail(
    jobs: Option<&JobsRepo>,
    payouts: Option<&PayoutRepo>,
    mail: Option<&dropit_infra_db::mail::MailRepo>,
) -> JobReport {
    let Some(payouts) = payouts else {
        return JobReport::skipped("payouts", "no_database");
    };

    let expired = match jobs {
        Some(j) => match j.expire_stale_intents().await {
            Ok(v) => v.len(),
            Err(e) => return JobReport::error("payouts", format!("expire_stale_intents: {e}")),
        },
        None => 0,
    };

    match payouts.run_payout_cycle(&[]).await {
        Ok(rows) => {
            let total: i64 = rows.iter().map(|r| r.amount_minor).sum();
            let mailed = match mail {
                Some(repo) => enqueue_payout_mails(repo, &rows).await,
                None => 0,
            };
            tracing::info!(
                created = rows.len(),
                expired_intents = expired,
                total_minor = total,
                "payout cycle done"
            );
            JobReport::ok(
                "payouts",
                json!({
                    "payouts_created": rows.len(),
                    "total_minor": total,
                    "expired_intents": expired,
                    "mails_queued": mailed,
                }),
            )
        }
        Err(e) => JobReport::error("payouts", format!("run_payout_cycle: {e}")),
    }
}

/// Прогон сверки: ledger vs провайдер + инвариант по payee.
pub async fn run_reconcile_once(payouts: Option<&PayoutRepo>) -> JobReport {
    let Some(payouts) = payouts else {
        return JobReport::skipped("reconcile", "no_database");
    };
    let run_id = match payouts.run_reconciliation().await {
        Ok(id) => id,
        Err(e) => return JobReport::error("reconcile", format!("run_reconciliation: {e}")),
    };
    let status = payouts
        .run_status(run_id)
        .await
        .unwrap_or_else(|_| "unknown".to_string());
    let violations = payouts
        .payee_invariant_violations()
        .await
        .unwrap_or_default();
    tracing::info!(run_id = %run_id, status = %status, violations = violations.len(), "reconciliation done");
    JobReport::ok(
        "reconcile",
        json!({
            "run_id": run_id,
            "status": status,
            "payee_invariant_violations": violations.len(),
        }),
    )
}

/// Прогон GDPR/retention: псевдонимизация неактивных ≥ 180 дней устройств.
pub async fn run_retention_once(jobs: Option<&JobsRepo>) -> JobReport {
    let Some(jobs) = jobs else {
        return JobReport::skipped("retention", "no_database");
    };
    match jobs.retention_sweep().await {
        Ok(ids) => {
            tracing::info!(anonymized = ids.len(), "retention sweep done");
            JobReport::ok("retention", json!({ "anonymized_devices": ids.len() }))
        }
        Err(e) => JobReport::error("retention", format!("retention_sweep: {e}")),
    }
}

/// Прогон автозакрытия сессий по расписанию клуба (07:00 IANA + неактивность).
pub async fn run_close_sessions_once(jobs: Option<&JobsRepo>) -> JobReport {
    let Some(jobs) = jobs else {
        return JobReport::skipped("close-sessions", "no_database");
    };
    match jobs.auto_close_sessions().await {
        Ok(closed) => {
            let ids: Vec<Uuid> = closed
                .iter()
                .map(|c: &ClosedSession| c.session_id)
                .collect();
            tracing::info!(closed = ids.len(), "auto-close sessions done");
            JobReport::ok(
                "close-sessions",
                json!({ "sessions_closed": ids.len(), "session_ids": ids }),
            )
        }
        Err(e) => JobReport::error("close-sessions", format!("auto_close_sessions: {e}")),
    }
}

/// Прогон диспетчера push над DB-backed стором.
pub async fn run_push_once(env: &WorkerEnv) -> JobReport {
    let Some(store) = env.outbox_store.as_ref() else {
        return JobReport::skipped("push", "no_database");
    };
    let Some(jobs) = env.jobs.as_ref() else {
        return JobReport::skipped("push", "no_database");
    };
    let dyn_store: Arc<dyn OutboxStore> = store.clone();
    let token_source = DbTokenSource::new(jobs.clone());
    run_push(
        dyn_store.as_ref(),
        env.push_sender.as_ref(),
        &token_source,
        env.config.push_batch_size,
    )
    .await
}

/// Прогон диспетчера писем над DB-backed очередью.
///
/// Без БД — `skipped` (очередь читать неоткуда). Без ключей провайдера
/// диспетчер всё равно работает: `NoopMail` помечает письма отправленными
/// без сети, поэтому локальный стенд и e2e видят тот же жизненный цикл.
pub async fn run_mail_once(env: &WorkerEnv) -> JobReport {
    let Some(repo) = env.mail.as_ref() else {
        return JobReport::skipped("mail", "no_database");
    };
    let store = DbMailStore::new(repo.clone());
    run_mail(&store, env.mail_sender.as_ref(), env.config.mail_batch_size).await
}

/// Прогон outbox над DB-backed стором.
pub async fn run_outbox_env(env: &WorkerEnv) -> JobReport {
    let Some(store) = env.outbox_store.as_ref() else {
        return JobReport::skipped("outbox", "no_database");
    };
    let dyn_store: Arc<dyn OutboxStore> = store.clone();
    run_outbox_once(
        dyn_store,
        env.bus.clone(),
        env.config.outbox_batch_size,
        env.config.outbox_max_attempts,
        env.nats_connected,
    )
    .await
}

/// Поставить в очередь письма об отправленных выплатах.
///
/// Возвращает число писем, реально вставленных в очередь (дубликаты по
/// `idempotency_key` не считаются). Ключ — `payout_paid:<payout_id>`, поэтому
/// повторный прогон цикла (или рестарт воркера) не рассылает письма заново.
async fn enqueue_payout_mails(
    mail: &dropit_infra_db::mail::MailRepo,
    rows: &[dropit_infra_db::PayoutRow],
) -> usize {
    let mut queued = 0usize;
    let pool = mail.pool();
    for row in rows {
        let Some(email) = payee_email(&pool, &row.payee_type, row.payee_id).await else {
            tracing::debug!(
                payee_type = %row.payee_type,
                payee_id = %row.payee_id,
                "mail: у получателя выплаты нет e-mail, письмо пропущено"
            );
            continue;
        };
        let to = match dropit_infra_mail::MailAddress::parse(&email) {
            Ok(addr) => addr,
            Err(e) => {
                tracing::warn!(error = %e, "mail: адрес получателя выплаты не разобран");
                continue;
            }
        };
        let vars = serde_json::json!({
            "payee_name": payee_name(&pool, &row.payee_type, row.payee_id).await
                .unwrap_or_else(|| "Здравствуйте".to_string()),
            "amount": format_minor_eur(row.amount_minor),
            "period": format_period(row.period_start, row.period_end),
        });
        let rendered =
            match dropit_infra_mail::render(dropit_infra_mail::TemplateKey::PayoutPaid, &vars) {
                Ok(r) => r,
                Err(e) => {
                    tracing::warn!(error = %e, "mail: шаблон выплаты не отрисован");
                    continue;
                }
            };
        let new_mail = dropit_infra_db::mail::NewMail {
            idempotency_key: format!("payout_paid:{}", row.id),
            template: dropit_infra_mail::TemplateKey::PayoutPaid
                .as_str()
                .to_string(),
            to_email: to.email.clone(),
            to_name: to.name.clone(),
            from_email: mail_from(),
            reply_to: mail_reply_to(),
            subject: rendered.subject,
            html: rendered.html,
            text: rendered.text,
            vars,
            aggregate_type: Some("payout".to_string()),
            aggregate_id: Some(row.id),
        };
        match mail.enqueue(new_mail).await {
            Ok((_, duplicate)) => {
                if !duplicate {
                    queued += 1;
                }
            }
            Err(e) => tracing::warn!(error = %e, "mail: не удалось поставить письмо о выплате"),
        }
    }
    queued
}

/// Адрес получателя выплаты: `club.email` или `dj.email`.
async fn payee_email(pool: &sqlx::PgPool, payee_type: &str, payee_id: Uuid) -> Option<String> {
    let query = match payee_type {
        "club" => "SELECT email::text FROM club WHERE id = $1",
        "dj" => "SELECT email::text FROM dj WHERE id = $1",
        _ => return None,
    };
    let row: Option<(String,)> = sqlx::query_as(query)
        .bind(payee_id)
        .fetch_optional(pool)
        .await
        .ok()?;
    row.map(|(e,)| e).filter(|e| e.contains('@'))
}

/// Имя получателя выплаты для приветствия в письме.
async fn payee_name(pool: &sqlx::PgPool, payee_type: &str, payee_id: Uuid) -> Option<String> {
    let query = match payee_type {
        "club" => "SELECT name FROM club WHERE id = $1",
        "dj" => "SELECT display_name FROM dj WHERE id = $1",
        _ => return None,
    };
    let row: Option<(String,)> = sqlx::query_as(query)
        .bind(payee_id)
        .fetch_optional(pool)
        .await
        .ok()?;
    row.map(|(n,)| n).filter(|n| !n.trim().is_empty())
}

/// Сумма в минорных единицах → `"12.34 EUR"`.
///
/// Своя функция, а не `format!` на месте: в `dropit_common::money` нет
/// форматтера, а деньги в письме не имеют права показываться «как есть»
/// (`1234` вместо `12.34` читается как ошибка в 100 раз).
fn format_minor_eur(amount_minor: i64) -> String {
    let sign = if amount_minor < 0 { "-" } else { "" };
    let abs = amount_minor.unsigned_abs();
    format!("{sign}{}.{:02} EUR", abs / 100, abs % 100)
}

/// Период выплаты → `"2026-10-01 — 2026-10-08"`.
fn format_period(
    start: chrono::DateTime<chrono::Utc>,
    end: chrono::DateTime<chrono::Utc>,
) -> String {
    format!("{} — {}", start.format("%Y-%m-%d"), end.format("%Y-%m-%d"))
}

/// Адрес отправителя писем (`MAIL_FROM`).
fn mail_from() -> String {
    std::env::var("MAIL_FROM")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "DROP IT <no-reply@dropit.local>".to_string())
}

/// Адрес для ответа (`MAIL_REPLY_TO`).
fn mail_reply_to() -> Option<String> {
    std::env::var("MAIL_REPLY_TO")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

// ============================================================
// Супервизор: все задачи по интервалам из env.
// ============================================================

/// Запустить супервизор: по задаче на интервал, до Ctrl-C.
///
/// Каждая задача печатает одну JSON-строку итога за прогон. Завершается
/// кодом 0 по сигналу прерывания.
pub async fn run_supervisor(env: Arc<WorkerEnv>) -> i32 {
    let mut handles = Vec::new();

    handles.push(spawn_interval(
        env.clone(),
        env.config.outbox_interval,
        "outbox",
        |e| Box::pin(async move { run_outbox_env(&e).await }),
    ));
    handles.push(spawn_interval(
        env.clone(),
        env.config.push_interval,
        "push",
        |e| Box::pin(async move { run_push_once(&e).await }),
    ));
    handles.push(spawn_interval(
        env.clone(),
        env.config.mail_interval,
        "mail",
        |e| Box::pin(async move { run_mail_once(&e).await }),
    ));
    handles.push(spawn_interval(
        env.clone(),
        env.config.payout_interval,
        "payouts",
        |e| {
            Box::pin(async move {
                run_payouts_with_mail(e.jobs.as_ref(), e.payouts.as_ref(), e.mail.as_ref()).await
            })
        },
    ));
    handles.push(spawn_interval(
        env.clone(),
        env.config.reconcile_interval,
        "reconcile",
        |e| Box::pin(async move { run_reconcile_once(e.payouts.as_ref()).await }),
    ));
    handles.push(spawn_interval(
        env.clone(),
        env.config.retention_interval,
        "retention",
        |e| Box::pin(async move { run_retention_once(e.jobs.as_ref()).await }),
    ));
    handles.push(spawn_interval(
        env.clone(),
        env.config.close_sessions_interval,
        "close-sessions",
        |e| Box::pin(async move { run_close_sessions_once(e.jobs.as_ref()).await }),
    ));

    tracing::info!(
        "supervisor: запущено {} задач, ожидание Ctrl-C",
        handles.len()
    );
    // Ждём сигнал прерывания; задачи крутятся до отмены.
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("supervisor: получен Ctrl-C, завершаем задачи");
        }
    }
    for h in handles {
        h.abort();
    }
    0
}

/// Тип функции-задачи для супервизора: принимает окружение, возвращает
/// `Pin<Box<Future>>` (асинхронный прогон).
type JobFn =
    fn(Arc<WorkerEnv>) -> std::pin::Pin<Box<dyn std::future::Future<Output = JobReport> + Send>>;

/// Запустить цикл задачи с интервалом.
fn spawn_interval(
    env: Arc<WorkerEnv>,
    period: Duration,
    name: &'static str,
    job: JobFn,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(period);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            ticker.tick().await;
            let report = job(env.clone()).await;
            println!("{}", report.to_line());
            if report.status == "error" {
                tracing::warn!(job = name, "task reported error");
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use dropit_infra_nats::InMemoryOutboxStore;
    use dropit_infra_push::NoopPush;
    use serde_json::json;
    use uuid::Uuid;

    fn row(n: u128, et: &str, payload: Value, attempts: i32) -> dropit_infra_nats::OutboxRow {
        dropit_infra_nats::OutboxRow {
            id: Uuid::from_u128(n),
            aggregate_id: Uuid::from_u128(n + 100),
            event_type: et.to_string(),
            payload,
            attempts,
        }
    }

    #[test]
    fn report_to_line_flattens_detail() {
        let r = JobReport::ok("outbox", json!({"claimed": 3, "published": 3}));
        let line = r.to_line();
        assert!(line.contains("\"job\":\"outbox\""));
        assert!(line.contains("\"status\":\"ok\""));
        assert!(line.contains("\"claimed\":3"));
        assert!(line.contains("\"published\":3"));
        assert_eq!(r.exit_code(), 0);
    }

    #[test]
    fn report_error_exit_code_one() {
        let r = JobReport::error("outbox", "boom".to_string());
        assert_eq!(r.exit_code(), 1);
        assert!(r.to_line().contains("\"status\":\"error\""));
        assert!(r.to_line().contains("\"error\":\"boom\""));
    }

    #[test]
    fn report_skipped_has_reason() {
        let r = JobReport::skipped("payouts", "no_database");
        assert_eq!(r.exit_code(), 0);
        assert!(r.to_line().contains("\"reason\":\"no_database\""));
    }

    #[tokio::test]
    async fn outbox_noop_bus_is_skipped() {
        let store: Arc<dyn OutboxStore> = Arc::new(InMemoryOutboxStore::new());
        let bus: Arc<dyn MessageBus> = Arc::new(dropit_infra_nats::NoopBus);
        let report = run_outbox_once(store, bus, 10, 5, false).await;
        assert_eq!(report.status, "skipped");
        assert_eq!(report.exit_code(), 0);
    }

    #[tokio::test]
    async fn outbox_empty_queue_returns_ok_zero() {
        let store: Arc<dyn OutboxStore> = Arc::new(InMemoryOutboxStore::new());
        let bus: Arc<dyn MessageBus> = Arc::new(dropit_infra_nats::NoopBus);
        // nats_connected=true с NoopBus (имитация: шина «настроена», но очередь пуста).
        let report = run_outbox_once(store, bus, 10, 5, true).await;
        assert_eq!(report.status, "ok");
        assert_eq!(report.detail["claimed"], 0);
        assert_eq!(report.detail["published"], 0);
        assert_eq!(report.exit_code(), 0);
    }

    #[tokio::test]
    async fn outbox_idempotent_second_run_publishes_nothing() {
        let store = Arc::new(InMemoryOutboxStore::new());
        store.push(row(1, "request.created", json!({}), 0));
        let bus: Arc<dyn MessageBus> = Arc::new(dropit_infra_nats::NoopBus);
        let dyn_store: Arc<dyn OutboxStore> = store.clone();

        let first = run_outbox_once(dyn_store.clone(), bus.clone(), 10, 5, true).await;
        assert_eq!(first.detail["published"], 1);
        let second = run_outbox_once(dyn_store, bus, 10, 5, true).await;
        assert_eq!(second.detail["claimed"], 0);
        assert_eq!(second.detail["published"], 0);
    }

    #[tokio::test]
    async fn db_jobs_without_repo_are_skipped() {
        assert_eq!(run_payouts_once(None, None).await.status, "skipped");
        assert_eq!(run_reconcile_once(None).await.status, "skipped");
        assert_eq!(run_retention_once(None).await.status, "skipped");
        assert_eq!(run_close_sessions_once(None).await.status, "skipped");
    }

    #[tokio::test]
    async fn push_job_through_env_without_db_is_skipped() {
        // Минимальный WorkerEnv без БД: push должен сообщить skipped.
        let env = WorkerEnv {
            config: Config::default(),
            nats_connected: false,
            bus: Arc::new(dropit_infra_nats::NoopBus),
            outbox_store: None,
            push_sender: Arc::new(NoopPush),
            jobs: None,
            payouts: None,
            mail_sender: Arc::new(dropit_infra_mail::NoopMail),
            mail: None,
        };
        let report = run_push_once(&env).await;
        assert_eq!(report.status, "skipped");
        assert_eq!(report.exit_code(), 0);
        // Диспетчер писем без БД тоже деградирует, а не падает.
        let mail_report = crate::jobs::run_mail_once(&env).await;
        assert_eq!(mail_report.status, "skipped");
        assert_eq!(mail_report.detail["reason"], "no_database");
    }

    use crate::config::Config;
}
