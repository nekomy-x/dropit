//! Админская аналитика: реальные агрегаты обзора платформы, фича-флаги и
//! выгрузки.
//!
//! Контракт и обоснование — `docs/audit/web-admin-gaps.md` §7.11 (overview,
//! сейчас «simulated»), §7.7 (feature flags), §7.10 (export).
//!
//! Правило честности (`docs/ui/PANEL-DESIGN.md`, AGENTS.md): каждое число на
//! экране оператора приходит из SQL по текущему состоянию БД; серии трендов
//! считаются по `date_trunc` за запрошенный период, а не из констант фронта.
//!
//! Границы модуля и принятые решения:
//!   * **Счётчики обзора — перепись текущего состояния**, а не выборка «за
//!     период»: экран спрашивает «сколько клубов активно сейчас», и
//!     `AdminOverviewPage.tsx` считает те же KPI по спискам, а не по датам
//!     создания. Период управляет только сериями (24/7/30 точек). Поэтому
//!     `sessions.created` — это сессии в статусе `created`, а не «созданные за
//!     сутки»; в ответе это зафиксировано формой, а здесь — комментарием.
//!   * **Серии** — бакеты `date_trunc` по UTC (`request.created_at`,
//!     `payment.captured_at`), пропущенные бакеты заполняются нулями в Rust:
//!     пустая неделя не должна выглядеть как отсутствие графика.
//!   * **Деньги — только minor units**, без float и деления на 100: валюта —
//!     свойство платежа (`payment.currency`), а не эндпоинта.
//!   * **Фича-флаги** — состояние платформы, а не localStorage админки. Набор
//!     ключей засеян миграцией `0020_feature_flags.sql`; PATCH меняет только
//!     известный ключ (неизвестный → 404), иначе «включить что угодно» стало бы
//!     способом обойти вайтлист UI.
//!   * **Экспорт** — до 5000 строк и RFC 4180; больше — это уже не «скачать из
//!     админки», а выгрузка БД, и честнее отказать, чем молча обрезать.

use axum::{
    extract::{
        rejection::{JsonRejection, QueryRejection},
        Path, Query, State,
    },
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, patch},
    Json, Router,
};
use chrono::{DateTime, Duration, Utc};
use dropit_common::{ApiError, ErrorEnvelope, Role};
use serde::Serialize;
use serde_json::{json, Value};
use sqlx::PgPool;
use std::collections::HashMap;
use uuid::Uuid;

use crate::rbac::{auth_error_response, require_admin_write, AuthContext};
use crate::routes::{api_error, ApiEnv};

/// Допустимые значения `period` (контракт `GET /v1/admin/overview`).
const OVERVIEW_PERIODS: [&str; 3] = ["24h", "7d", "30d"];
/// Окно «гость онлайн»: устройство, замеченное за последние сутки.
const GUEST_ONLINE_WINDOW_HOURS: i32 = 24;
/// Потолок выгрузки: выше — отказываем `export_too_large`, а не обрезаем.
const EXPORT_ROW_LIMIT: usize = 5_000;
const EXPORT_RESOURCES: [&str; 4] = ["users", "clubs", "djs", "payments"];
const EXPORT_FORMATS: [&str; 2] = ["csv", "json"];

/// Роутер админской аналитики и настроек платформы.
pub fn admin_insights_router() -> Router<ApiEnv> {
    Router::new()
        .route("/v1/admin/overview", get(admin_overview))
        .route("/v1/admin/feature-flags", get(admin_feature_flags))
        .route(
            "/v1/admin/feature-flags/:key",
            patch(admin_feature_flag_patch),
        )
        .route("/v1/admin/export", get(admin_export))
}

// ============================================================
// Общие помощники
// ============================================================

/// Проверка роли JWT: все маршруты модуля — только `Role::Admin`.
///
/// Так же, как `admin_users_routes.rs`: гейт `role_route` в `routes.rs`
/// применяется не ко всем админ-роутам, поэтому роль проверяется в хендлере.
fn admin_only(ctx: &AuthContext) -> Option<Response> {
    (ctx.role() != Role::Admin).then(|| auth_error_response(ApiError::Forbidden))
}

/// Пул БД или `503`-конверт (без БД читать нечего, но и падать некуда).
#[allow(clippy::result_large_err)]
fn pool_of(state: &ApiEnv) -> Result<PgPool, Response> {
    state.repo.as_ref().map(|r| r.pool()).ok_or_else(|| {
        api_error(
            ApiError::ExternalServiceUnavailable,
            "database not configured",
        )
    })
}

fn bad_request(detail: &str) -> Response {
    api_error(ApiError::BadRequest, detail)
}

/// Значение query-параметра вне списка допустимых → `400 validation_error`.
///
/// Не 422: это не валидация тела, а неизвестный вариант запроса; клиенту
/// важно увидеть список `allowed`, по которому он ошибся.
fn invalid_parameter(parameter: &str, value: &str, allowed: &[&'static str]) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(ErrorEnvelope {
            code: "validation_error".to_string(),
            message: "The request failed validation.".to_string(),
            details: Some(json!({
                "parameter": parameter,
                "value": value,
                "allowed": allowed,
            })),
        }),
    )
        .into_response()
}

/// Ошибка БД: наружу — конверт `internal`, в лог — причина.
fn db_failure(err: sqlx::Error) -> Response {
    tracing::error!(error = %err, "admin insights: database query failed");
    api_error(ApiError::Internal, "database error")
}

/// Пишет запись аудита в переданной транзакции (конвенция
/// `admin_users_routes.rs`: `entity` — стабильный текстовый id сущности).
async fn audit_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    actor_id: Uuid,
    action: &str,
    entity: &str,
    diff: Value,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO audit_log (actor_type, actor_id, action, entity, diff) \
         VALUES ('admin', $1, $2, $3, $4)",
    )
    .bind(actor_id)
    .bind(action)
    .bind(entity)
    .bind(diff)
    .execute(&mut **tx)
    .await
    .map(|_| ())
}

// ============================================================
// GET /v1/admin/overview
// ============================================================

/// Период обзора: длина и шаг серии фиксированы контрактом.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OverviewPeriod {
    Hours24,
    Days7,
    Days30,
}

impl OverviewPeriod {
    /// Разбор `period` (дефолт — `24h`, мусор — `400`).
    #[allow(clippy::result_large_err)]
    fn parse(raw: Option<&str>) -> Result<Self, Response> {
        match raw.map(str::trim).filter(|value| !value.is_empty()) {
            None | Some("24h") => Ok(Self::Hours24),
            Some("7d") => Ok(Self::Days7),
            Some("30d") => Ok(Self::Days30),
            Some(other) => Err(invalid_parameter("period", other, &OVERVIEW_PERIODS)),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Hours24 => "24h",
            Self::Days7 => "7d",
            Self::Days30 => "30d",
        }
    }

    /// Число точек серии: ровно столько, сколько периодов в контракте.
    fn points(self) -> i32 {
        match self {
            Self::Hours24 => 24,
            Self::Days7 => 7,
            Self::Days30 => 30,
        }
    }

    /// Шаг одного бакета.
    fn step(self) -> Duration {
        match self {
            Self::Hours24 => Duration::hours(1),
            Self::Days7 | Self::Days30 => Duration::days(1),
        }
    }

    /// Единица `date_trunc` (константа из enum — в SQL подставляется только она).
    fn trunc(self) -> &'static str {
        match self {
            Self::Hours24 => "hour",
            Self::Days7 | Self::Days30 => "day",
        }
    }
}

/// Начало текущего бакета по UTC.
///
/// `AT TIME ZONE 'UTC'` в обе стороны: `date_trunc('day', now())` зависит от
/// часового пояса сессии БД, а бакеты обязаны быть одинаковыми у всех
/// читателей — иначе «сегодня» на сервере в Europe/Madrid и в UTC разъедется.
async fn series_anchor(pool: &PgPool, trunc: &str) -> Result<DateTime<Utc>, sqlx::Error> {
    let sql = format!("SELECT date_trunc('{trunc}', now() AT TIME ZONE 'UTC') AT TIME ZONE 'UTC'");
    sqlx::query_scalar::<_, DateTime<Utc>>(&sql)
        .fetch_one(pool)
        .await
}

/// Клубы: всего / active / pending_verification / suspended.
type ClubsCensus = (i64, i64, i64, i64);
/// DJ: всего / active / invited / suspended.
type DjsCensus = (i64, i64, i64, i64);
/// Сессии: активные («открытые» вечера) / created / paused_acceptance.
type SessionsCensus = (i64, i64, i64);
/// Запросы: всего / pending / accepted / played / rejected.
type RequestsCensus = (i64, i64, i64, i64, i64);
/// Платежи: captured_count / captured_minor / refunded_minor / failed_count.
type PaymentsCensus = (i64, i64, i64, i64);
/// Гости: аккаунты / устройства, замеченные за сутки.
type GuestsCensus = (i64, i64);

async fn clubs_census(pool: &PgPool) -> Result<ClubsCensus, sqlx::Error> {
    sqlx::query_as::<_, ClubsCensus>(
        "SELECT count(*)::bigint, \
                count(*) FILTER (WHERE status = 'active')::bigint, \
                count(*) FILTER (WHERE status = 'pending_verification')::bigint, \
                count(*) FILTER (WHERE status = 'suspended')::bigint \
         FROM club",
    )
    .fetch_one(pool)
    .await
}

async fn djs_census(pool: &PgPool) -> Result<DjsCensus, sqlx::Error> {
    sqlx::query_as::<_, DjsCensus>(
        "SELECT count(*)::bigint, \
                count(*) FILTER (WHERE status = 'active')::bigint, \
                count(*) FILTER (WHERE status = 'invited')::bigint, \
                count(*) FILTER (WHERE status = 'suspended')::bigint \
         FROM dj",
    )
    .fetch_one(pool)
    .await
}

async fn sessions_census(pool: &PgPool) -> Result<SessionsCensus, sqlx::Error> {
    // «Открытая» сессия — это `active`: гости могут заказывать. `created` —
    // вечер создан, но ещё не открыт; `paused_acceptance` — приём приостановлен.
    sqlx::query_as::<_, SessionsCensus>(
        "SELECT count(*) FILTER (WHERE status = 'active')::bigint, \
                count(*) FILTER (WHERE status = 'created')::bigint, \
                count(*) FILTER (WHERE status = 'paused_acceptance')::bigint \
         FROM session",
    )
    .fetch_one(pool)
    .await
}

async fn requests_census(pool: &PgPool) -> Result<RequestsCensus, sqlx::Error> {
    sqlx::query_as::<_, RequestsCensus>(
        "SELECT count(*)::bigint, \
                count(*) FILTER (WHERE status = 'pending')::bigint, \
                count(*) FILTER (WHERE status = 'accepted')::bigint, \
                count(*) FILTER (WHERE status = 'played')::bigint, \
                count(*) FILTER (WHERE status = 'rejected')::bigint \
         FROM request",
    )
    .fetch_one(pool)
    .await
}

async fn payments_census(pool: &PgPool) -> Result<PaymentsCensus, sqlx::Error> {
    // Деньги считаются по фактически закрытым платежам: `captured_minor` —
    // сумма снимков цены, `refunded_minor` — возвраты по ним. Чистая выручка —
    // их разность (та же формула, что в серии `revenue_minor`).
    sqlx::query_as::<_, PaymentsCensus>(
        "SELECT \
            count(*) FILTER ( \
                WHERE status IN ('captured', 'partially_refunded', 'refunded') \
            )::bigint, \
            COALESCE(sum(amount_minor) FILTER ( \
                WHERE status IN ('captured', 'partially_refunded', 'refunded') \
            ), 0)::bigint, \
            COALESCE(sum(refunded_minor) FILTER ( \
                WHERE status IN ('captured', 'partially_refunded', 'refunded') \
            ), 0)::bigint, \
            count(*) FILTER (WHERE status = 'failed')::bigint \
         FROM payment",
    )
    .fetch_one(pool)
    .await
}

async fn guests_census(pool: &PgPool) -> Result<GuestsCensus, sqlx::Error> {
    // `total` — зарегистрированные гости (`guest_account`), как в каталоге
    // §7.1. `online_24h` — устройства: анонимный гость тоже находится в клубе,
    // даже если не апгрейдил аккаунт.
    sqlx::query_as::<_, GuestsCensus>(
        "SELECT \
            (SELECT count(*)::bigint FROM guest_account), \
            (SELECT count(*)::bigint FROM guest_device \
              WHERE last_seen >= now() - ($1::int * interval '1 hour'))",
    )
    .bind(GUEST_ONLINE_WINDOW_HOURS)
    .fetch_one(pool)
    .await
}

/// Бакеты `request.created_at` за период (только непустые — нули добивает Rust).
async fn requests_series(
    pool: &PgPool,
    period: OverviewPeriod,
    start: DateTime<Utc>,
) -> Result<Vec<(DateTime<Utc>, i64)>, sqlx::Error> {
    let sql = format!(
        "SELECT date_trunc('{trunc}', created_at AT TIME ZONE 'UTC') AT TIME ZONE 'UTC' AS bucket, \
                count(*)::bigint AS value \
         FROM request WHERE created_at >= $1 GROUP BY 1 ORDER BY 1",
        trunc = period.trunc(),
    );
    sqlx::query_as::<_, (DateTime<Utc>, i64)>(&sql)
        .bind(start)
        .fetch_all(pool)
        .await
}

/// Бакеты чистой выручки: `sum(amount_minor - refunded_minor)` по
/// `captured_at` только для фактически закрытых платежей.
async fn revenue_series(
    pool: &PgPool,
    period: OverviewPeriod,
    start: DateTime<Utc>,
) -> Result<Vec<(DateTime<Utc>, i64)>, sqlx::Error> {
    let sql = format!(
        "SELECT date_trunc('{trunc}', captured_at AT TIME ZONE 'UTC') AT TIME ZONE 'UTC' AS bucket, \
                COALESCE(sum(amount_minor - refunded_minor), 0)::bigint AS value \
         FROM payment \
         WHERE captured_at >= $1 \
           AND status IN ('captured', 'partially_refunded', 'refunded') \
         GROUP BY 1 ORDER BY 1",
        trunc = period.trunc(),
    );
    sqlx::query_as::<_, (DateTime<Utc>, i64)>(&sql)
        .bind(start)
        .fetch_all(pool)
        .await
}

/// Разворачивает непустые бакеты в полную серию из `period.points()` точек.
///
/// Пропущенный час/день — это ноль запросов, а не отсутствие данных: график
/// оператора должен быть непрерывным.
fn fill_series(
    anchor: DateTime<Utc>,
    period: OverviewPeriod,
    rows: &[(DateTime<Utc>, i64)],
) -> Vec<Value> {
    let step = period.step();
    let first = anchor - step * (period.points() - 1);
    (0..period.points())
        .map(|index| {
            let bucket = first + step * index;
            let value = rows
                .iter()
                .find(|(at, _)| *at == bucket)
                .map(|(_, value)| *value)
                .unwrap_or(0);
            json!({ "bucket": bucket.to_rfc3339(), "value": value })
        })
        .collect()
}

/// `GET /v1/admin/overview?period=24h|7d|30d` — реальный обзор платформы.
async fn admin_overview(
    ctx: AuthContext,
    State(state): State<ApiEnv>,
    query: Result<Query<HashMap<String, String>>, QueryRejection>,
) -> Response {
    if let Some(resp) = admin_only(&ctx) {
        return resp;
    }
    let pool = match pool_of(&state) {
        Ok(pool) => pool,
        Err(resp) => return resp,
    };
    let Query(params) = match query {
        Ok(query) => query,
        Err(_) => return bad_request("malformed query string"),
    };
    let period = match OverviewPeriod::parse(params.get("period").map(String::as_str)) {
        Ok(period) => period,
        Err(resp) => return resp,
    };

    // Якорь берём у БД: рядом с ним лежат фикстуры и `created_at`/`captured_at`,
    // поэтому часы процесса и сервера не могут разъехаться с бакетами.
    let anchor = match series_anchor(&pool, period.trunc()).await {
        Ok(anchor) => anchor,
        Err(err) => return db_failure(err),
    };
    let start = anchor - period.step() * (period.points() - 1);

    // Независимые агрегаты — параллельно: восемь запросов последовательно
    // складывались бы в заметную для оператора задержку.
    let (clubs, djs, sessions, requests, payments, guests, request_rows, revenue_rows) = tokio::join!(
        clubs_census(&pool),
        djs_census(&pool),
        sessions_census(&pool),
        requests_census(&pool),
        payments_census(&pool),
        guests_census(&pool),
        requests_series(&pool, period, start),
        revenue_series(&pool, period, start),
    );
    let clubs = match clubs {
        Ok(value) => value,
        Err(err) => return db_failure(err),
    };
    let djs = match djs {
        Ok(value) => value,
        Err(err) => return db_failure(err),
    };
    let sessions = match sessions {
        Ok(value) => value,
        Err(err) => return db_failure(err),
    };
    let requests = match requests {
        Ok(value) => value,
        Err(err) => return db_failure(err),
    };
    let payments = match payments {
        Ok(value) => value,
        Err(err) => return db_failure(err),
    };
    let guests = match guests {
        Ok(value) => value,
        Err(err) => return db_failure(err),
    };
    let request_rows = match request_rows {
        Ok(value) => value,
        Err(err) => return db_failure(err),
    };
    let revenue_rows = match revenue_rows {
        Ok(value) => value,
        Err(err) => return db_failure(err),
    };

    let (clubs_total, clubs_active, clubs_pending, clubs_suspended) = clubs;
    let (djs_total, djs_active, djs_invited, djs_suspended) = djs;
    let (sessions_open, sessions_created, sessions_paused) = sessions;
    let (requests_total, requests_pending, requests_accepted, requests_played, requests_rejected) =
        requests;
    let (captured_count, captured_minor, refunded_minor, failed_count) = payments;
    let (guests_total, guests_online) = guests;

    (
        StatusCode::OK,
        Json(json!({
            "period": period.as_str(),
            "generated_at": Utc::now().to_rfc3339(),
            "clubs": {
                "total": clubs_total,
                "active": clubs_active,
                "pending_verification": clubs_pending,
                "suspended": clubs_suspended,
            },
            "djs": {
                "total": djs_total,
                "active": djs_active,
                "invited": djs_invited,
                "suspended": djs_suspended,
            },
            "sessions": {
                "open": sessions_open,
                "created": sessions_created,
                "paused_acceptance": sessions_paused,
            },
            "requests": {
                "total": requests_total,
                "pending": requests_pending,
                "accepted": requests_accepted,
                "played": requests_played,
                "rejected": requests_rejected,
            },
            "payments": {
                "captured_count": captured_count,
                "captured_minor": captured_minor,
                "refunded_minor": refunded_minor,
                "failed_count": failed_count,
            },
            "guests": {
                "total": guests_total,
                "online_24h": guests_online,
            },
            "series": {
                "requests": fill_series(anchor, period, &request_rows),
                "revenue_minor": fill_series(anchor, period, &revenue_rows),
            },
        })),
    )
        .into_response()
}

// ============================================================
// GET/PATCH /v1/admin/feature-flags
// ============================================================

/// Строка `feature_flag` в форме, которую читает админка.
#[derive(Debug, Serialize, sqlx::FromRow)]
struct FeatureFlag {
    key: String,
    enabled: bool,
    description: String,
    updated_at: DateTime<Utc>,
    updated_by: Option<Uuid>,
}

/// `GET /v1/admin/feature-flags` — все флаги платформы.
async fn admin_feature_flags(ctx: AuthContext, State(state): State<ApiEnv>) -> Response {
    if let Some(resp) = admin_only(&ctx) {
        return resp;
    }
    let pool = match pool_of(&state) {
        Ok(pool) => pool,
        Err(resp) => return resp,
    };
    let rows = sqlx::query_as::<_, FeatureFlag>(
        "SELECT key, enabled, description, updated_at, updated_by \
         FROM feature_flag ORDER BY key",
    )
    .fetch_all(&pool)
    .await;
    match rows {
        Ok(items) => (StatusCode::OK, Json(json!({ "items": items }))).into_response(),
        Err(err) => db_failure(err),
    }
}

/// `PATCH /v1/admin/feature-flags/:key` — включить/выключить флаг.
///
/// Только существующий ключ: набор флагов — контракт (миграция 0020), а не
/// произвольный key-value store. Аудит пишется в той же транзакции, что и
/// `UPDATE`, поэтому «переключили, но не записали» невозможно.
async fn admin_feature_flag_patch(
    ctx: AuthContext,
    State(state): State<ApiEnv>,
    Path(raw_key): Path<String>,
    body: Result<Json<Value>, JsonRejection>,
) -> Response {
    if let Some(resp) = admin_only(&ctx) {
        return resp;
    }
    if let Err(resp) = require_admin_write(&ctx, &state).await {
        return resp;
    }
    let pool = match pool_of(&state) {
        Ok(pool) => pool,
        Err(resp) => return resp,
    };
    let key = raw_key.trim();
    if key.is_empty() {
        return bad_request("feature flag key is required");
    }
    let Json(value) = match body {
        Ok(body) => body,
        Err(_) => return bad_request("malformed json body"),
    };
    let Some(enabled) = value.get("enabled").and_then(Value::as_bool) else {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ErrorEnvelope {
                code: "validation_error".to_string(),
                message: "The request failed validation.".to_string(),
                details: Some(json!({
                    "field": "enabled",
                    "detail": "enabled must be a boolean",
                })),
            }),
        )
            .into_response();
    };

    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(err) => return db_failure(err),
    };
    let before =
        sqlx::query_scalar::<_, bool>("SELECT enabled FROM feature_flag WHERE key = $1 FOR UPDATE")
            .bind(key)
            .fetch_optional(&mut *tx)
            .await;
    let before = match before {
        Ok(Some(before)) => before,
        Ok(None) => return api_error(ApiError::NotFound, "feature flag not found"),
        Err(err) => return db_failure(err),
    };
    let updated = sqlx::query_as::<_, FeatureFlag>(
        "UPDATE feature_flag \
         SET enabled = $2, updated_at = now(), updated_by = $3 \
         WHERE key = $1 \
         RETURNING key, enabled, description, updated_at, updated_by",
    )
    .bind(key)
    .bind(enabled)
    .bind(ctx.subject_id())
    .fetch_one(&mut *tx)
    .await;
    let updated = match updated {
        Ok(updated) => updated,
        Err(err) => return db_failure(err),
    };
    let diff = json!({ "key": key, "before": before, "after": enabled });
    if let Err(err) = audit_in_tx(
        &mut tx,
        ctx.subject_id(),
        "admin.feature_flag.update",
        key,
        diff,
    )
    .await
    {
        tracing::error!(error = %err, "feature flag audit failed");
        return api_error(ApiError::Internal, "audit write failed");
    }
    if let Err(err) = tx.commit().await {
        return db_failure(err);
    }
    (StatusCode::OK, Json(updated)).into_response()
}

// ============================================================
// GET /v1/admin/export
// ============================================================

/// Строка выгрузки пользователей: `admin_user` ∪ `club` ∪ `dj` ∪ `guest_account`.
type UserExportRow = (
    Uuid,
    String,
    String,
    String,
    String,
    Option<Uuid>,
    DateTime<Utc>,
    Option<DateTime<Utc>>,
);
type ClubExportRow = (Uuid, String, String, String, String, DateTime<Utc>);
type DjExportRow = (Uuid, String, String, Option<Uuid>, String, DateTime<Utc>);
type PaymentExportRow = (
    Uuid,
    Uuid,
    String,
    i64,
    String,
    i64,
    Option<DateTime<Utc>>,
    DateTime<Utc>,
);

/// Колонки выгрузки — контракт (§7.10). Порядок фиксирован: CSV-заголовок и
/// JSON-ключи обязаны совпадать, иначе одна и та же выгрузка читается по-разному.
const USER_HEADERS: [&str; 8] = [
    "id",
    "role",
    "status",
    "display_name",
    "email",
    "club_id",
    "created_at",
    "last_seen_at",
];
const CLUB_HEADERS: [&str; 6] = ["id", "name", "slug", "email", "status", "created_at"];
const DJ_HEADERS: [&str; 6] = [
    "id",
    "display_name",
    "email",
    "club_id",
    "status",
    "created_at",
];
const PAYMENT_HEADERS: [&str; 8] = [
    "id",
    "request_id",
    "status",
    "amount_minor",
    "currency",
    "refunded_minor",
    "captured_at",
    "created_at",
];

/// Пользователь в выгрузке. `status` нормализован до каталогового словаря
/// (`active|invited|suspended|blocked`) — того же, что фильтры §7.1.
#[derive(Debug, Serialize)]
struct UserExport {
    id: Uuid,
    role: String,
    status: String,
    display_name: String,
    email: String,
    club_id: Option<Uuid>,
    created_at: DateTime<Utc>,
    last_seen_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
struct ClubExport {
    id: Uuid,
    name: String,
    slug: String,
    email: String,
    status: String,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
struct DjExport {
    id: Uuid,
    display_name: String,
    email: String,
    club_id: Option<Uuid>,
    status: String,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
struct PaymentExport {
    id: Uuid,
    request_id: Uuid,
    status: String,
    amount_minor: i64,
    currency: String,
    refunded_minor: i64,
    captured_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
}

/// Готовые к отдаче строки одного ресурса.
enum ExportData {
    Users(Vec<UserExport>),
    Clubs(Vec<ClubExport>),
    Djs(Vec<DjExport>),
    Payments(Vec<PaymentExport>),
}

impl ExportData {
    /// Строки для JSON-выгрузки (типы сохраняются: числа — числами, NULL — null).
    fn into_items(self) -> Result<Value, serde_json::Error> {
        match self {
            ExportData::Users(rows) => serde_json::to_value(rows),
            ExportData::Clubs(rows) => serde_json::to_value(rows),
            ExportData::Djs(rows) => serde_json::to_value(rows),
            ExportData::Payments(rows) => serde_json::to_value(rows),
        }
    }

    /// Тело CSV: заголовок + строки, RFC 4180 (CRLF, кавычки удваиваются).
    fn to_csv(&self) -> String {
        match self {
            ExportData::Users(rows) => csv_table(
                &USER_HEADERS,
                rows.iter().map(UserExport::csv_fields).collect(),
            ),
            ExportData::Clubs(rows) => csv_table(
                &CLUB_HEADERS,
                rows.iter().map(ClubExport::csv_fields).collect(),
            ),
            ExportData::Djs(rows) => {
                csv_table(&DJ_HEADERS, rows.iter().map(DjExport::csv_fields).collect())
            }
            ExportData::Payments(rows) => csv_table(
                &PAYMENT_HEADERS,
                rows.iter().map(PaymentExport::csv_fields).collect(),
            ),
        }
    }
}

impl UserExport {
    fn csv_fields(&self) -> Vec<String> {
        vec![
            self.id.to_string(),
            self.role.clone(),
            self.status.clone(),
            self.display_name.clone(),
            self.email.clone(),
            optional_uuid(self.club_id),
            self.created_at.to_rfc3339(),
            optional_time(self.last_seen_at),
        ]
    }
}

impl ClubExport {
    fn csv_fields(&self) -> Vec<String> {
        vec![
            self.id.to_string(),
            self.name.clone(),
            self.slug.clone(),
            self.email.clone(),
            self.status.clone(),
            self.created_at.to_rfc3339(),
        ]
    }
}

impl DjExport {
    fn csv_fields(&self) -> Vec<String> {
        vec![
            self.id.to_string(),
            self.display_name.clone(),
            self.email.clone(),
            optional_uuid(self.club_id),
            self.status.clone(),
            self.created_at.to_rfc3339(),
        ]
    }
}

impl PaymentExport {
    fn csv_fields(&self) -> Vec<String> {
        vec![
            self.id.to_string(),
            self.request_id.to_string(),
            self.status.clone(),
            self.amount_minor.to_string(),
            self.currency.clone(),
            self.refunded_minor.to_string(),
            optional_time(self.captured_at),
            self.created_at.to_rfc3339(),
        ]
    }
}

fn optional_uuid(value: Option<Uuid>) -> String {
    value.map(|value| value.to_string()).unwrap_or_default()
}

fn optional_time(value: Option<DateTime<Utc>>) -> String {
    value.map(|value| value.to_rfc3339()).unwrap_or_default()
}

/// Собирает CSV из заголовка и строк. Каждое поле проходит RFC 4180-экранирование:
/// запятая/кавычка/перевод строки внутри значения не должны ломать таблицу.
fn csv_table(headers: &[&str], rows: Vec<Vec<String>>) -> String {
    let mut out = String::new();
    push_csv_row(
        &mut out,
        headers.iter().map(|header| (*header).to_string()).collect(),
    );
    for row in rows {
        push_csv_row(&mut out, row);
    }
    out
}

fn push_csv_row(out: &mut String, fields: Vec<String>) {
    for (index, field) in fields.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str(&csv_field(field));
    }
    // RFC 4180: строки разделяются CRLF (а не LF), иначе часть таблиц
    // (Excel, старые CSV-парсеры) читает файл как одну строку.
    out.push_str("\r\n");
}

/// Одно поле CSV по RFC 4180.
fn csv_field(value: &str) -> String {
    let needs_quotes =
        value.contains(',') || value.contains('"') || value.contains('\n') || value.contains('\r');
    if needs_quotes {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

/// Ошибка выборки выгрузки: слишком много строк или сбой БД.
enum ExportFailure {
    TooLarge,
    Database(sqlx::Error),
}

/// Простые `SELECT` по тем же таблицам, что каталог §7.1 / финансы.
///
/// Тяжёлую логику (фильтры, keyset, нормализацию статусов) не дублируем:
/// выгрузка — снимок «как есть» до 5000 строк.
async fn fetch_export(pool: &PgPool, resource: &str) -> Result<ExportData, ExportFailure> {
    let limit = (EXPORT_ROW_LIMIT + 1) as i64;
    match resource {
        "users" => {
            let rows = sqlx::query_as::<_, UserExportRow>(EXPORT_USERS_SQL)
                .bind(limit)
                .fetch_all(pool)
                .await
                .map_err(ExportFailure::Database)?;
            if rows.len() > EXPORT_ROW_LIMIT {
                return Err(ExportFailure::TooLarge);
            }
            Ok(ExportData::Users(
                rows.into_iter().map(UserExport::from).collect(),
            ))
        }
        "clubs" => {
            let rows = sqlx::query_as::<_, ClubExportRow>(
                "SELECT id, name, slug, email::text, status, created_at \
                 FROM club ORDER BY created_at DESC, id DESC LIMIT $1",
            )
            .bind(limit)
            .fetch_all(pool)
            .await
            .map_err(ExportFailure::Database)?;
            if rows.len() > EXPORT_ROW_LIMIT {
                return Err(ExportFailure::TooLarge);
            }
            Ok(ExportData::Clubs(
                rows.into_iter().map(ClubExport::from).collect(),
            ))
        }
        "djs" => {
            let rows = sqlx::query_as::<_, DjExportRow>(
                "SELECT id, display_name, email::text, club_id, status, created_at \
                 FROM dj ORDER BY created_at DESC, id DESC LIMIT $1",
            )
            .bind(limit)
            .fetch_all(pool)
            .await
            .map_err(ExportFailure::Database)?;
            if rows.len() > EXPORT_ROW_LIMIT {
                return Err(ExportFailure::TooLarge);
            }
            Ok(ExportData::Djs(
                rows.into_iter().map(DjExport::from).collect(),
            ))
        }
        _ => {
            let rows = sqlx::query_as::<_, PaymentExportRow>(
                "SELECT id, request_id, status, amount_minor, currency, refunded_minor, \
                        captured_at, created_at \
                 FROM payment ORDER BY created_at DESC, id DESC LIMIT $1",
            )
            .bind(limit)
            .fetch_all(pool)
            .await
            .map_err(ExportFailure::Database)?;
            if rows.len() > EXPORT_ROW_LIMIT {
                return Err(ExportFailure::TooLarge);
            }
            Ok(ExportData::Payments(
                rows.into_iter().map(PaymentExport::from).collect(),
            ))
        }
    }
}

/// Каталог пользователей в выгрузке: тот же UNION, что `DIRECTORY_SQL`, но без
/// фильтров/пагинации и с фиксированным лимитом. Статусы нормализованы CASE'ами
/// (маленькие маппинги, а не копия всей логики каталога).
const EXPORT_USERS_SQL: &str = r#"
SELECT id, role, status, display_name, email, club_id, created_at, last_seen_at
FROM (
    SELECT a.id,
           'admin'::text AS role,
           CASE a.status
               WHEN 'suspended' THEN 'suspended'
               WHEN 'blocked'   THEN 'blocked'
               WHEN 'invited'   THEN 'invited'
               WHEN 'pending_verification' THEN 'invited'
               ELSE 'active'
           END AS status,
           COALESCE(NULLIF(btrim(a.display_name), ''), split_part(a.email::text, '@', 1)) AS display_name,
           a.email::text AS email,
           NULL::uuid AS club_id,
           a.created_at,
           NULL::timestamptz AS last_seen_at
    FROM admin_user a
    UNION ALL
    SELECT c.id,
           'club_staff'::text,
           CASE c.status
               WHEN 'active'    THEN 'active'
               WHEN 'suspended' THEN 'suspended'
               WHEN 'blocked'   THEN 'blocked'
               ELSE 'invited'
           END,
           c.name,
           c.email::text,
           c.id,
           c.created_at,
           NULL::timestamptz
    FROM club c
    UNION ALL
    SELECT d.id,
           'dj'::text,
           CASE d.status
               WHEN 'invited'   THEN 'invited'
               WHEN 'suspended' THEN 'suspended'
               WHEN 'blocked'   THEN 'blocked'
               ELSE 'active'
           END,
           d.display_name,
           d.email::text,
           d.club_id,
           d.created_at,
           d.last_seen_at
    FROM dj d
    UNION ALL
    SELECT g.id,
           'guest'::text,
           CASE
               WHEN EXISTS (
                   SELECT 1 FROM guest_device gd
                   WHERE gd.account_id = g.id AND gd.banned
               ) THEN 'blocked'
               WHEN g.verified THEN 'active'
               ELSE 'invited'
           END,
           g.email_display,
           g.email_display,
           NULL::uuid,
           g.created_at,
           (SELECT max(gd.last_seen) FROM guest_device gd WHERE gd.account_id = g.id)
    FROM guest_account g
) AS directory
ORDER BY created_at DESC, id DESC
LIMIT $1
"#;

impl From<UserExportRow> for UserExport {
    fn from(row: UserExportRow) -> Self {
        let (id, role, status, display_name, email, club_id, created_at, last_seen_at) = row;
        Self {
            id,
            role,
            status,
            display_name,
            email,
            club_id,
            created_at,
            last_seen_at,
        }
    }
}

impl From<ClubExportRow> for ClubExport {
    fn from(row: ClubExportRow) -> Self {
        let (id, name, slug, email, status, created_at) = row;
        Self {
            id,
            name,
            slug,
            email,
            status,
            created_at,
        }
    }
}

impl From<DjExportRow> for DjExport {
    fn from(row: DjExportRow) -> Self {
        let (id, display_name, email, club_id, status, created_at) = row;
        Self {
            id,
            display_name,
            email,
            club_id,
            status,
            created_at,
        }
    }
}

impl From<PaymentExportRow> for PaymentExport {
    fn from(row: PaymentExportRow) -> Self {
        let (
            id,
            request_id,
            status,
            amount_minor,
            currency,
            refunded_minor,
            captured_at,
            created_at,
        ) = row;
        Self {
            id,
            request_id,
            status,
            amount_minor,
            currency,
            refunded_minor,
            captured_at,
            created_at,
        }
    }
}

/// `GET /v1/admin/export?resource=users|clubs|djs|payments&format=csv|json`
async fn admin_export(
    ctx: AuthContext,
    State(state): State<ApiEnv>,
    query: Result<Query<HashMap<String, String>>, QueryRejection>,
) -> Response {
    if let Some(resp) = admin_only(&ctx) {
        return resp;
    }
    let pool = match pool_of(&state) {
        Ok(pool) => pool,
        Err(resp) => return resp,
    };
    let Query(params) = match query {
        Ok(query) => query,
        Err(_) => return bad_request("malformed query string"),
    };
    let resource = match params
        .get("resource")
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        Some(value) if EXPORT_RESOURCES.contains(&value) => value.to_string(),
        Some(other) => return invalid_parameter("resource", other, &EXPORT_RESOURCES),
        None => return invalid_parameter("resource", "", &EXPORT_RESOURCES),
    };
    let format = match params
        .get("format")
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        None => "csv",
        Some(value) if EXPORT_FORMATS.contains(&value) => value,
        Some(other) => return invalid_parameter("format", other, &EXPORT_FORMATS),
    };

    let data = match fetch_export(&pool, &resource).await {
        Ok(data) => data,
        Err(ExportFailure::TooLarge) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorEnvelope {
                    code: "export_too_large".to_string(),
                    message: format!(
                        "The export exceeds {EXPORT_ROW_LIMIT} rows; narrow it down first."
                    ),
                    details: Some(json!({
                        "resource": resource,
                        "limit": EXPORT_ROW_LIMIT,
                    })),
                }),
            )
                .into_response();
        }
        Err(ExportFailure::Database(err)) => return db_failure(err),
    };

    if format == "json" {
        let items = match data.into_items() {
            Ok(items) => items,
            Err(err) => {
                tracing::error!(error = %err, "admin export: json serialization failed");
                return api_error(ApiError::Internal, "serialization error");
            }
        };
        return (
            StatusCode::OK,
            Json(json!({
                "resource": resource,
                "generated_at": Utc::now().to_rfc3339(),
                "items": items,
            })),
        )
            .into_response();
    }

    let body = data.to_csv();
    let filename = format!("dropit-{resource}-{}.csv", Utc::now().format("%Y-%m-%d"));
    let disposition = match HeaderValue::from_str(&format!("attachment; filename=\"{filename}\"")) {
        Ok(value) => value,
        Err(err) => {
            tracing::error!(error = %err, "admin export: content-disposition is invalid");
            return api_error(ApiError::Internal, "invalid filename");
        }
    };
    let mut response = body.into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/csv; charset=utf-8"),
    );
    response
        .headers_mut()
        .insert(header::CONTENT_DISPOSITION, disposition);
    response
}
