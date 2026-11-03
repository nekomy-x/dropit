//! Публичный статус стенда: `GET /v1/status`.
//!
//! Один и тот же бандл панелей стоит на тестовом стенде и в проде, поэтому
//! «какой сейчас режим» (test / maintenance / normal) определяет сервер.
//! Нижний статус-бар панелей опрашивает этот роут один раз при загрузке
//! страницы; авторизация не нужна — это те же факты, что отдаёт `/readyz`.
//!
//! Источник правды — `platform_settings.runtime_mode` (миграция 0016),
//! которую владелец задаёт в админ-панели. Недоступная БД не валит роут:
//! режим деградирует до env-флага или адаптивных фактов стенда, ответ
//! всегда `200`.
//!
//! Чистое ядро [`resolve_mode`] отделено от окружения и БД — матрица
//! разрешения проверяется unit-тестами без сети и без Postgres.

use axum::{
    extract::State,
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use chrono::Utc;
use serde_json::{json, Value};
use std::time::Instant;

use crate::routes::{payments_mode, ApiEnv};

/// Допустимые значения `platform_settings.runtime_mode` (контракт
/// `PATCH /v1/admin/platform-settings` и `GET /v1/status`).
pub const RUNTIME_MODES: [&str; 4] = ["auto", "test", "maintenance", "normal"];

/// Таймаут одной сетевой проверки статуса (БД, Redis, почта): страницы
/// опрашивают `/v1/status` при каждой загрузке, зависший коннект не должен
/// вешать запрос.
///
/// Независимые проверки выполняются параллельно (`tokio::join!`), поэтому
/// худший случай ответа — один этот таймаут (~1.5 с), а не сумма по всем
/// проверкам (раньше шесть последовательных проверок складывались в ~9 с).
/// Так худший ответ укладывается в 10-секундный бюджет
/// `apps/web-status/status.js` (`AbortController`) даже при полностью мёртвой
/// инфраструктуре.
const DB_READ_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(1_500);

/// Валидно ли значение `runtime_mode` из тела PATCH (или из БД).
pub fn is_valid_runtime_mode(value: &str) -> bool {
    RUNTIME_MODES.contains(&value)
}

/// Результат разрешения режима стенда — тело `GET /v1/status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedMode {
    /// Эффективный режим для статус-бара: `test` | `maintenance` | `normal`.
    pub mode: &'static str,
    /// Откуда он взялся: `admin` | `env` | `adaptive`.
    pub source: &'static str,
    /// Сырое значение настройки: `auto` | `test` | `maintenance` | `normal`.
    pub runtime_mode: &'static str,
}

/// Чистое разрешение режима — без БД и env, поэтому вся матрица
/// проверяется unit-тестами.
///
/// Порядок (контракт `GET /v1/status`):
///
/// 1. `db_runtime_mode` прочитан и это не `auto` → берём его как есть,
///    `source = "admin"` (ручное решение владельца сильнее всего);
/// 2. `auto` или БД недоступна (`db_runtime_mode == None`) и
///    `DROPIT_MAINTENANCE` включён → `maintenance`, `source = "env"`;
/// 3. иначе адаптивно: открытая регистрация (`open_signup`) или
///    mock-платежи (`payments_mock`) → `test`, иначе `normal`,
///    `source = "adaptive"`.
///
/// `None` означает «БД не сконфигурирована/недоступна/строки нет»:
/// публичный статус обязан отвечать `200` даже без БД, поэтому None
/// трактуется как `auto` — тогда работают шаги 2 и 3.
pub fn resolve_mode(
    db_runtime_mode: Option<&str>,
    maintenance_env: bool,
    open_signup: bool,
    payments_mock: bool,
) -> ResolvedMode {
    // Неизвестное значение (в БД его не пустит CHECK, но защищаемся от
    // старых/битых строк) трактуем как `auto`, а не как режим.
    let runtime_mode = match db_runtime_mode {
        Some("test") => "test",
        Some("maintenance") => "maintenance",
        Some("normal") => "normal",
        _ => "auto",
    };
    if runtime_mode != "auto" {
        return ResolvedMode {
            mode: runtime_mode,
            source: "admin",
            runtime_mode,
        };
    }
    if maintenance_env {
        return ResolvedMode {
            mode: "maintenance",
            source: "env",
            runtime_mode,
        };
    }
    let mode = if open_signup || payments_mock {
        "test"
    } else {
        "normal"
    };
    ResolvedMode {
        mode,
        source: "adaptive",
        runtime_mode,
    }
}

/// Включающие значения `DROPIT_MAINTENANCE` (без учёта регистра, после
/// `trim`): `1`, `true`, `yes`, `on`. Всё остальное — выключено, включая
/// отсутствие переменной (безопасный дефолт: прод не уходит в maintenance
/// из-за опечатки в значении).
pub fn env_truthy(raw: Option<&str>) -> bool {
    raw.map(|v| v.trim().to_ascii_lowercase())
        .is_some_and(|v| matches!(v.as_str(), "1" | "true" | "yes" | "on"))
}

/// `DROPIT_MAINTENANCE` из окружения процесса (читается на каждый запрос —
/// переключение флага на живом процессе не требует рестарта).
fn maintenance_from_env() -> bool {
    env_truthy(std::env::var("DROPIT_MAINTENANCE").ok().as_deref())
}

/// Сырое `platform_settings.runtime_mode` или `None`, если БД не
/// сконфигурирована/недоступна/строки нет. Ошибки и таймаут не
/// пробрасываются: None для [`resolve_mode`] = `auto` (шаги 2/3).
async fn read_db_runtime_mode(state: &ApiEnv) -> Option<String> {
    let pool = state.repo.as_ref()?.pool();
    let query =
        sqlx::query_scalar::<_, String>("SELECT runtime_mode FROM platform_settings WHERE id = 1")
            .fetch_optional(&pool);
    match tokio::time::timeout(DB_READ_TIMEOUT, query).await {
        Ok(Ok(mode)) => mode,
        Ok(Err(e)) => {
            tracing::warn!(error = %e, "status: runtime_mode read failed; falling back to auto");
            None
        }
        Err(_) => {
            tracing::warn!("status: runtime_mode read timed out; falling back to auto");
            None
        }
    }
}

/// `GET /v1/status` — публичный, без авторизации.
async fn runtime_status(State(state): State<ApiEnv>) -> impl IntoResponse {
    let payments = payments_mode();
    let db_mode = read_db_runtime_mode(&state).await;
    let resolved = resolve_mode(
        db_mode.as_deref(),
        maintenance_from_env(),
        state.open_signup,
        payments == "mock",
    );
    (
        StatusCode::OK,
        Json(json!({
            "mode": resolved.mode,
            "source": resolved.source,
            "runtime_mode": resolved.runtime_mode,
            "open_signup": state.open_signup,
            "payments": payments,
        })),
    )
}

// ============================================================
// Публичная статус-страница сервисов (GET /v1/status/services)
// ============================================================

/// Один чек статус-страницы. `state`: `ok` | `degraded` | `down` | `unknown`.
struct Check {
    id: &'static str,
    label: &'static str,
    state: &'static str,
    detail: String,
    latency_ms: Option<u128>,
}

impl Check {
    fn new(
        id: &'static str,
        label: &'static str,
        state: &'static str,
        detail: impl Into<String>,
        latency_ms: Option<u128>,
    ) -> Self {
        Self {
            id,
            label,
            state,
            detail: detail.into(),
            latency_ms,
        }
    }

    fn to_json(&self) -> Value {
        json!({
            "id": self.id,
            "label": self.label,
            "state": self.state,
            "detail": self.detail,
            "latency_ms": self.latency_ms,
        })
    }
}

/// Читает значение с таймаутом: зависший запрос не должен вешать страницу.
async fn timed<T>(
    timeout: std::time::Duration,
    future: impl std::future::Future<Output = Result<T, sqlx::Error>>,
) -> Result<T, ()> {
    match tokio::time::timeout(timeout, future).await {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(err)) => {
            tracing::warn!(error = %err, "status services: query failed");
            Err(())
        }
        Err(_) => {
            tracing::warn!("status services: query timed out");
            Err(())
        }
    }
}

async fn database_check(state: &ApiEnv) -> Check {
    let Some(repo) = state.repo.as_ref() else {
        return Check::new(
            "database",
            "Database",
            "down",
            "Database is not configured for this process",
            None,
        );
    };
    let started = Instant::now();
    let pool = repo.pool();
    let query = sqlx::query_scalar::<_, i32>("SELECT 1").fetch_one(&pool);
    match timed(DB_READ_TIMEOUT, query).await {
        Ok(_) => Check::new(
            "database",
            "Database",
            "ok",
            "PostgreSQL reachable",
            Some(started.elapsed().as_millis()),
        ),
        Err(()) => Check::new(
            "database",
            "Database",
            "down",
            "PostgreSQL is not reachable",
            Some(started.elapsed().as_millis()),
        ),
    }
}

async fn schema_check(state: &ApiEnv) -> Check {
    let Some(repo) = state.repo.as_ref() else {
        return Check::new(
            "schema",
            "Database schema",
            "unknown",
            "Cannot verify without a database",
            None,
        );
    };
    let pool = repo.pool();
    let query = sqlx::query_scalar::<_, bool>(
        "SELECT to_regclass('public.api_key') IS NOT NULL \
                AND to_regclass('public.worker_heartbeat') IS NOT NULL",
    )
    .fetch_one(&pool);
    match timed(DB_READ_TIMEOUT, query).await {
        Ok(true) => Check::new(
            "schema",
            "Database schema",
            "ok",
            "Current release schema present",
            None,
        ),
        Ok(false) => Check::new(
            "schema",
            "Database schema",
            "degraded",
            "Schema is older than this release",
            None,
        ),
        Err(()) => Check::new(
            "schema",
            "Database schema",
            "unknown",
            "Schema could not be verified",
            None,
        ),
    }
}

async fn redis_check(state: &ApiEnv) -> Check {
    let Some(store) = state.tickets.as_ref() else {
        return Check::new(
            "redis",
            "Redis",
            "unknown",
            "No ticket store configured",
            None,
        );
    };
    let backend = store.backend();
    let started = Instant::now();
    // Ping Redis тоже под таймаутом. Это единственная проверка, которая
    // раньше могла висеть бесконечно: TCP-соединение к зависшему Redis не
    // отваливается само, и страница (у которой свой 10-секундный
    // `AbortController`) оставалась пустой. Таймаут — честное `unknown`:
    // «не проверили», а не «сломано».
    match tokio::time::timeout(DB_READ_TIMEOUT, store.ping()).await {
        Ok(Ok(())) => Check::new(
            "redis",
            "Redis",
            "ok",
            if backend == "redis" {
                "Redis reachable"
            } else {
                "In-memory store (single node)"
            },
            Some(started.elapsed().as_millis()),
        ),
        Ok(Err(err)) => {
            tracing::warn!(error = %err, "status services: redis ping failed");
            Check::new(
                "redis",
                "Redis",
                "down",
                "Redis is not reachable",
                Some(started.elapsed().as_millis()),
            )
        }
        Err(_) => {
            tracing::warn!("status services: redis ping timed out");
            Check::new(
                "redis",
                "Redis",
                "unknown",
                "Redis check timed out",
                Some(started.elapsed().as_millis()),
            )
        }
    }
}

async fn workers_check(state: &ApiEnv) -> Check {
    let Some(repo) = state.repo.as_ref() else {
        return Check::new(
            "workers",
            "Workers",
            "unknown",
            "Cannot verify without a database",
            None,
        );
    };
    let pool = repo.pool();
    let query = sqlx::query_scalar::<_, Option<i64>>(
        "SELECT EXTRACT(EPOCH FROM (now() - last_seen_at))::bigint \
         FROM worker_heartbeat WHERE worker = 'dropit-workers'",
    )
    .fetch_optional(&pool);
    match timed(DB_READ_TIMEOUT, query).await {
        Ok(Some(Some(age))) if age <= 45 => Check::new(
            "workers",
            "Workers",
            "ok",
            format!("Last heartbeat {age}s ago"),
            None,
        ),
        // До 30 минут воркеры считаются отстающими, а не мёртвыми: один
        // пропущенный тик не должен красить весь стенд в down.
        Ok(Some(Some(age))) if age <= 1800 => Check::new(
            "workers",
            "Workers",
            "degraded",
            format!("Last heartbeat {age}s ago"),
            None,
        ),
        Ok(Some(Some(age))) => Check::new(
            "workers",
            "Workers",
            "down",
            format!("Last heartbeat {age}s ago"),
            None,
        ),
        Ok(_) => Check::new("workers", "Workers", "down", "No heartbeat recorded", None),
        Err(()) => Check::new(
            "workers",
            "Workers",
            "unknown",
            "Heartbeat could not be read",
            None,
        ),
    }
}

/// Настроен ли relay событий (NATS). В Test Mode `NATS_URL` пуст, и
/// воркер штатно не публикует outbox — это конфигурация, а не сбой.
fn relay_configured() -> bool {
    std::env::var("NATS_URL")
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
}

async fn outbox_check(state: &ApiEnv) -> Check {
    let Some(repo) = state.repo.as_ref() else {
        return Check::new(
            "outbox",
            "Event outbox",
            "unknown",
            "Cannot verify without a database",
            None,
        );
    };
    let pool = repo.pool();
    let query = sqlx::query_as::<_, (i64, i64)>(
        "SELECT count(*)::bigint, \
                COALESCE(EXTRACT(EPOCH FROM (now() - min(created_at)))::bigint, 0) \
         FROM outbox WHERE published_at IS NULL",
    )
    .fetch_one(&pool);
    let relay = relay_configured();
    match timed(DB_READ_TIMEOUT, query).await {
        Ok((0, _)) if relay => Check::new("outbox", "Event outbox", "ok", "Empty", None),
        Ok((0, _)) => Check::new(
            "outbox",
            "Event outbox",
            "ok",
            "Relay not configured (NATS is off)",
            None,
        ),
        // События ждут, но relay выключен конфигурацией: это честный
        // Test Mode, а не зависший сервис. Показываем `ok` с деталями.
        Ok((count, _)) if !relay => Check::new(
            "outbox",
            "Event outbox",
            "ok",
            format!("{count} queued — relay not configured (NATS is off)"),
            None,
        ),
        Ok((count, age)) if age < 120 => Check::new(
            "outbox",
            "Event outbox",
            "ok",
            format!("{count} pending, newest {age}s old"),
            None,
        ),
        Ok((count, age)) if count <= 1000 => Check::new(
            "outbox",
            "Event outbox",
            "degraded",
            format!("{count} pending, oldest {age}s old — relay is stalled"),
            None,
        ),
        Ok((count, age)) => Check::new(
            "outbox",
            "Event outbox",
            "down",
            format!("{count} pending, oldest {age}s old — relay is stalled"),
            None,
        ),
        Err(()) => Check::new(
            "outbox",
            "Event outbox",
            "unknown",
            "Outbox could not be read",
            None,
        ),
    }
}

async fn mail_check(state: &ApiEnv) -> Check {
    let Some(mail) = state.mail.as_ref() else {
        return Check::new(
            "mail",
            "Mail queue",
            "ok",
            "Not configured (delivery disabled)",
            None,
        );
    };
    match tokio::time::timeout(DB_READ_TIMEOUT, mail.stats(24)).await {
        Ok(Ok(stats)) => {
            let pending = stats.queued + stats.sending;
            let state = if stats.failed > 0 || pending > 50 {
                "degraded"
            } else {
                "ok"
            };
            Check::new(
                "mail",
                "Mail queue",
                state,
                format!(
                    "{} queued · {} failed · {} sent (24h)",
                    pending, stats.failed, stats.sent
                ),
                None,
            )
        }
        Ok(Err(err)) => {
            tracing::warn!(error = %err, "status services: mail stats failed");
            Check::new(
                "mail",
                "Mail queue",
                "unknown",
                "Mail queue could not be read",
                None,
            )
        }
        Err(_) => Check::new(
            "mail",
            "Mail queue",
            "unknown",
            "Mail queue read timed out",
            None,
        ),
    }
}

/// Платёжная подсистема в публичном смысле: «принимает ли стенд деньги».
///
/// `ok` возможен только в mock-режиме: заглушка живёт в процессе, её работа
/// проверяема by design. В live-режиме реального проба провайдера (баланс,
/// webhook-endpoint) нет, поэтому честное состояние — `unknown` («Provider not
/// probed»), а не `ok`: непроверенное нельзя выдавать за работающее.
/// Недоступная БД — `degraded`: записи о платежах не читаются, даже если сам
/// провайдер жив.
fn payments_check(db_down: bool) -> Check {
    let payments = payments_mode();
    let (state, detail) = if db_down {
        ("degraded", "Payment records unavailable")
    } else if payments == "live" {
        ("unknown", "Provider not probed")
    } else {
        ("ok", "Mock provider (test mode)")
    };
    Check::new("payments", "Payments", state, detail, None)
}

fn runtime_check(mode: &str) -> Check {
    let (state, detail) = match mode {
        "maintenance" => ("degraded", "Maintenance mode"),
        "test" => ("ok", "Test mode"),
        _ => ("ok", "Normal operation"),
    };
    Check::new("runtime", "Runtime mode", state, detail, None)
}

/// Проверки, из которых строится публичный список. Только те подсистемы,
/// чьё состояние человек видит на странице: `schema`, `mail` и `runtime` в
/// публичный ответ не попадают by design, поэтому и не запрашиваются — лишний
/// поход в БД за `platform_settings` на каждом опросе страницы не нужен.
///
/// Независимые проверки идут параллельно: последовательный обход складывал
/// их таймауты (6 × 1.5 с ≈ 9 с) и выводил ответ за клиентский бюджет.
async fn collect_public_checks(state: &ApiEnv) -> Vec<Check> {
    let (database, redis, workers, outbox) = tokio::join!(
        database_check(state),
        redis_check(state),
        workers_check(state),
        outbox_check(state),
    );
    let mut checks = vec![
        Check::new("api", "API", "ok", "Responding", Some(0)),
        database,
        redis,
        workers,
        outbox,
    ];
    // Платежи зависят от результата проверки БД, поэтому добавляются после
    // `join!`, а не внутри него.
    checks.push(payments_check(state_of(&checks, "database") == "down"));
    checks
}

/// Собирает все технические проверки (для админского detail-эндпоинта).
///
/// Возвращает и разрешённый режим стенда: админский роут показывает его в
/// `environment.runtime_mode`, и второй такой же запрос к
/// `platform_settings` был бы лишним чтением.
async fn collect_checks(state: &ApiEnv) -> (String, Vec<Check>) {
    let (database, schema, redis, workers, outbox, mail, mode) = tokio::join!(
        database_check(state),
        schema_check(state),
        redis_check(state),
        workers_check(state),
        outbox_check(state),
        mail_check(state),
        resolved_mode(state),
    );
    let mut checks = vec![
        Check::new("api", "API", "ok", "Responding", Some(0)),
        database,
        schema,
        redis,
        workers,
        outbox,
        mail,
    ];
    checks.push(payments_check(state_of(&checks, "database") == "down"));
    checks.push(runtime_check(&mode));
    (mode, checks)
}

/// Эффективный режим стенда — только для админского detail-эндпоинта:
/// публичная страница сервисов его не читает и не показывает.
async fn resolved_mode(state: &ApiEnv) -> String {
    let db_mode = read_db_runtime_mode(state).await;
    resolve_mode(
        db_mode.as_deref(),
        maintenance_from_env(),
        state.open_signup,
        payments_mode() == "mock",
    )
    .mode
    .to_string()
}

fn state_of(checks: &[Check], id: &str) -> &'static str {
    checks
        .iter()
        .find(|check| check.id == id)
        .map(|check| check.state)
        .unwrap_or("unknown")
}

fn overall_state(checks: &[Check]) -> &'static str {
    if checks.iter().any(|check| check.state == "down") {
        "down"
    } else if checks.iter().any(|check| check.state == "degraded") {
        "degraded"
    } else {
        "ok"
    }
}

/// Человеческая формулировка состояния для публичной страницы.
fn public_note(state: &str) -> &'static str {
    match state {
        "ok" => "Working normally",
        "degraded" => "Working with delays",
        "down" => "Not available right now",
        _ => "Status unavailable",
    }
}

fn public_service(id: &str, name: &str, state: &str, note: &str) -> Value {
    json!({ "id": id, "name": name, "state": state, "note": note })
}

/// Публичная формулировка для платежей. Единственное состояние, в котором
/// строка несёт полезную человеку информацию, — `ok` mock-режима: деньги не
/// двигаются, и это надо сказать прямо. В остальных случаях — общие слова
/// страницы, чтобы не обещать работающий провайдер.
fn public_payments_note(state: &str) -> &'static str {
    if state == "ok" {
        "Test mode — no real money moves"
    } else {
        public_note(state)
    }
}

/// Публичный список сервисов: только то, что нужно человеку, — «работает /
/// с задержками / недоступно». Никаких внутренних имён, счётчиков, задержек
/// в миллисекундах и деталей инфраструктуры.
fn public_services(checks: &[Check]) -> Vec<Value> {
    let api = state_of(checks, "api");
    let db = state_of(checks, "database");
    let redis = state_of(checks, "redis");
    let outbox = state_of(checks, "outbox");
    let workers = state_of(checks, "workers");
    let payments = state_of(checks, "payments");

    let panels = if api == "down" || db == "down" {
        "down"
    } else if redis != "ok" {
        // Непроверенный Redis (таймаут) красит панели в degraded: без него
        // не выдаются ws-тикеты, а «Working normally» был бы догадкой.
        "degraded"
    } else {
        "ok"
    };
    let requests = if db == "down" {
        "down"
    } else if workers != "ok" || outbox != "ok" {
        "degraded"
    } else {
        "ok"
    };
    let guest_entry = if db == "down" { "down" } else { "ok" };
    let api_state = if api == "down" {
        "down"
    } else if db == "down" {
        "degraded"
    } else {
        "ok"
    };

    vec![
        public_service("panel", "Club panel", panels, public_note(panels)),
        public_service("dj", "DJ console", panels, public_note(panels)),
        public_service("api", "API", api_state, public_note(api_state)),
        public_service(
            "requests",
            "Music requests",
            requests,
            if requests == "ok" {
                "Accepting requests"
            } else {
                public_note(requests)
            },
        ),
        public_service(
            "guest-entry",
            "Guest entry (QR)",
            guest_entry,
            public_note(guest_entry),
        ),
        public_service(
            "payments",
            "Payments",
            payments,
            public_payments_note(payments),
        ),
    ]
}

fn no_store(mut response: Response) -> Response {
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

/// `GET /v1/status/services` — публичная страница для людей.
///
/// Только список сервисов и их состояние. Технические детали (Redis,
/// outbox, heartbeat, схема, задержки) остаются на админском эндпоинте:
/// публичный ответ не раскрывает внутреннее устройство. Режим стенда
/// (`runtime_mode`) публичной странице не нужен — читать его здесь незачем.
async fn services_status(State(state): State<ApiEnv>) -> Response {
    let checks = collect_public_checks(&state).await;
    let services = public_services(&checks);
    let overall = if services.iter().any(|item| item["state"] == "down") {
        "down"
    } else if services.iter().any(|item| item["state"] == "degraded") {
        "degraded"
    } else {
        "ok"
    };
    no_store(
        (
            StatusCode::OK,
            Json(json!({
                "status": overall,
                "checked_at": Utc::now().to_rfc3339(),
                "services": services,
            })),
        )
            .into_response(),
    )
}

/// `GET /v1/admin/status/services` — полный технический статус для админов.
///
/// Здесь есть всё, что не должно быть публичным: имена подсистем, возраст
/// heartbeat, размер outbox, состояние схемы, режим платежей.
async fn admin_services_status(
    ctx: crate::rbac::AuthContext,
    State(state): State<ApiEnv>,
) -> Response {
    if ctx.role() != dropit_common::Role::Admin {
        return crate::rbac::auth_error_response(dropit_common::ApiError::Forbidden);
    }
    let (runtime_mode, checks) = collect_checks(&state).await;
    let overall = overall_state(&checks);
    no_store(
        (
            StatusCode::OK,
            Json(json!({
                "status": overall,
                "checked_at": Utc::now().to_rfc3339(),
                "version": env!("CARGO_PKG_VERSION"),
                "environment": {
                    "payments": payments_mode(),
                    "open_signup": state.open_signup,
                    "runtime_mode": runtime_mode,
                },
                "checks": checks.iter().map(Check::to_json).collect::<Vec<_>>(),
            })),
        )
            .into_response(),
    )
}

/// Публичный роутер статуса: монтируется в общий публичный роутер.
pub fn status_router() -> Router<ApiEnv> {
    Router::new()
        .route("/v1/status", get(runtime_status))
        .route("/v1/status/services", get(services_status))
        .route("/v1/admin/status/services", get(admin_services_status))
}

#[cfg(test)]
mod tests {
    use super::{env_truthy, is_valid_runtime_mode, resolve_mode};

    /// Админ задал режим вручную — он побеждает env и адаптивные факты,
    /// включая включённый `DROPIT_MAINTENANCE`.
    #[test]
    fn admin_mode_wins_over_env_and_adaptive() {
        for mode in ["test", "maintenance", "normal"] {
            let r = resolve_mode(Some(mode), true, true, true);
            assert_eq!(r.mode, mode, "admin mode {mode} must win");
            assert_eq!(r.source, "admin");
            assert_eq!(r.runtime_mode, mode);
        }
    }

    /// `auto` + включённый env — maintenance из env (не адаптивный test).
    #[test]
    fn auto_with_env_is_maintenance() {
        let r = resolve_mode(Some("auto"), true, true, true);
        assert_eq!(r.mode, "maintenance");
        assert_eq!(r.source, "env");
        assert_eq!(r.runtime_mode, "auto");
    }

    /// `auto`, env выключен, открытая регистрация → адаптивный test.
    #[test]
    fn auto_open_signup_is_adaptive_test() {
        let r = resolve_mode(Some("auto"), false, true, false);
        assert_eq!(r.mode, "test");
        assert_eq!(r.source, "adaptive");
        assert_eq!(r.runtime_mode, "auto");
    }

    /// `auto`, env выключен, закрытая регистрация, mock-платежи → test.
    #[test]
    fn auto_mock_payments_is_adaptive_test() {
        let r = resolve_mode(Some("auto"), false, false, true);
        assert_eq!(r.mode, "test");
        assert_eq!(r.source, "adaptive");
    }

    /// `auto`, env выключен, живой стенд (регистрация закрыта, платежи
    /// live) → normal.
    #[test]
    fn auto_live_closed_signup_is_normal() {
        let r = resolve_mode(Some("auto"), false, false, false);
        assert_eq!(r.mode, "normal");
        assert_eq!(r.source, "adaptive");
        assert_eq!(r.runtime_mode, "auto");
    }

    /// БД недоступна (None): роут деградирует, а не падает. Env включён →
    /// maintenance/env.
    #[test]
    fn db_unavailable_with_env_is_maintenance() {
        let r = resolve_mode(None, true, false, false);
        assert_eq!(r.mode, "maintenance");
        assert_eq!(r.source, "env");
        assert_eq!(r.runtime_mode, "auto", "no DB row → auto");
    }

    /// БД недоступна и env выключен → адаптивные факты стенда.
    #[test]
    fn db_unavailable_without_env_is_adaptive() {
        let test = resolve_mode(None, false, true, false);
        assert_eq!(test.mode, "test");
        assert_eq!(test.source, "adaptive");

        let normal = resolve_mode(None, false, false, false);
        assert_eq!(normal.mode, "normal");
        assert_eq!(normal.source, "adaptive");
    }

    /// Неизвестное/битое значение в БД не ломает разрешение: трактуется как
    /// `auto` (защита от старых строк в обход CHECK).
    #[test]
    fn unknown_db_value_falls_back_to_auto() {
        let r = resolve_mode(Some("production"), false, false, false);
        assert_eq!(r.runtime_mode, "auto");
        assert_eq!(r.mode, "normal");
        assert_eq!(r.source, "adaptive");
    }

    /// Разбор `DROPIT_MAINTENANCE`: включающие значения — 1/true/yes/on
    /// без учёта регистра и пробелов; остальное (включая отсутствие) —
    /// выключено.
    #[test]
    fn env_truthy_matrix() {
        for raw in ["1", "true", "TRUE", " yes ", "On", " on\n"] {
            assert!(env_truthy(Some(raw)), "{raw:?} must enable maintenance");
        }
        for raw in ["0", "false", "no", "off", "", "  ", "enable", "2"] {
            assert!(!env_truthy(Some(raw)), "{raw:?} must not enable");
        }
        assert!(!env_truthy(None));
    }

    /// Enum runtime_mode: ровно четыре значения контракта.
    #[test]
    fn runtime_mode_enum() {
        for mode in ["auto", "test", "maintenance", "normal"] {
            assert!(is_valid_runtime_mode(mode), "{mode} must be valid");
        }
        for mode in ["production", "", "Auto", "maintenance "] {
            assert!(!is_valid_runtime_mode(mode), "{mode:?} must be invalid");
        }
    }
}
