//! Роуты панелей (F7-бэкенд): клуб, админ, DJ-статистика.
//!
//! Контракт (техкарта Часть IV): dashboard клуба/админа, профиль,
//! тарифы (Q6: новая строка, valid_to у старой), доходы, platform-
//! settings (NULL-семантика: значения задаёт владелец), блокировки,
//! платежи со сплитами, audit-log, DJ-смена и статистика.

use crate::auth;
use crate::rbac::{auth_error_response, AuthContext};
use crate::routes::{api_error, ApiEnv};
use crate::status_routes::is_valid_runtime_mode;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, patch, post},
    Json, Router,
};
use chrono::{DateTime, Duration, Utc};
use dropit_common::{ApiError, ErrorEnvelope, Role};
use dropit_infra_stripe::{ConnectAccountReq, StripeError};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::PgPool;
use std::collections::BTreeMap;
use uuid::Uuid;

/// TTL одноразового invite-токена DJ (7 дней).
const DJ_INVITE_TTL_DAYS: i64 = 7;

pub fn panel_router() -> Router<ApiEnv> {
    Router::new()
        // Клуб.
        .route("/v1/club/dashboard", get(club_dashboard))
        .route(
            "/v1/club/profile",
            get(club_profile).patch(club_profile_patch),
        )
        .route(
            "/v1/club/pricing",
            get(club_pricing_list).post(club_pricing_create),
        )
        .route("/v1/club/earnings", get(club_earnings))
        .route("/v1/club/djs", get(club_djs))
        .route("/v1/club/djs/invite", post(club_djs_invite))
        .route(
            "/v1/club/djs/:id",
            patch(club_dj_patch).delete(club_dj_remove),
        )
        .route("/v1/club/djs/:id/restore", post(club_dj_restore))
        .route("/v1/dj/invite/accept", post(dj_invite_accept))
        .route("/v1/dj/presence", post(dj_presence))
        .route("/v1/club/payouts", get(club_payouts))
        .route(
            "/v1/club/stripe/onboarding-status",
            get(club_stripe_onboarding_status),
        )
        .route(
            "/v1/club/stripe/onboarding-link",
            post(club_stripe_onboarding_link),
        )
        // Админ.
        .route("/v1/admin/dashboard/now", get(admin_dashboard))
        .route("/v1/admin/clubs", get(admin_clubs).post(admin_clubs_create))
        .route("/v1/admin/clubs/:id", patch(admin_club_patch))
        .route("/v1/admin/guests", get(admin_guests))
        .route("/v1/admin/payments", get(admin_payments))
        .route("/v1/admin/payments/:id/splits", get(admin_payment_splits))
        .route("/v1/admin/audit-log", get(admin_audit_log))
        .route(
            "/v1/admin/platform-settings",
            get(admin_platform_settings).patch(admin_platform_settings_patch),
        )
        .route("/v1/admin/djs", get(admin_djs))
        .route(
            "/v1/admin/reconciliation-runs",
            get(admin_reconciliation_runs),
        )
        // DJ.
        .route("/v1/dj/shift", get(dj_shift))
        .route("/v1/dj/stats/today", get(dj_stats_today))
}

/// Конверт ошибки с произвольным кодом/статусом (для бизнес-кодов, которых
/// нет в [`ApiError`]: `dj_already_invited`, `club_not_found`,
/// `invalid_timezone`, `stripe_unavailable`). Использует существующий
/// тип [`ErrorEnvelope`], новый тип не вводится.
#[allow(clippy::result_large_err)]
fn api_error_custom(status: StatusCode, code: &str, message: &str, details: Value) -> Response {
    (
        status,
        Json(ErrorEnvelope {
            code: code.to_string(),
            message: message.to_string(),
            details: Some(details),
        }),
    )
        .into_response()
}

/// Детали ошибки Stripe для конверта (`{code, message, status?}`).
fn stripe_error_details(err: &StripeError) -> Value {
    match err {
        StripeError::Api {
            status,
            code,
            message,
        } => json!({
            "code": code,
            "message": message,
            "status": status,
        }),
        other => json!({
            "code": "stripe_error",
            "message": other.to_string(),
        }),
    }
}

/// Нормализация slug: lowercase, не-alnum → '-', без двойных и краёв.
fn slugify(input: &str) -> String {
    let s: String = input
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    s.trim_matches('-')
        .split('-')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

#[allow(clippy::result_large_err)]
fn pool_of(state: &ApiEnv) -> Result<PgPool, Response> {
    state.repo.as_ref().map(|r| r.pool()).ok_or_else(|| {
        api_error(
            ApiError::ExternalServiceUnavailable,
            "database not configured",
        )
    })
}

// ============================================================
// Клуб
// ============================================================

#[derive(Deserialize)]
pub struct ProfilePatch {
    pub name: Option<String>,
    pub address: Option<String>,
    pub phone: Option<String>,
    pub country: Option<String>,
    #[serde(default, deserialize_with = "deserialize_nullable_field")]
    pub logo_url: Option<Option<String>>,
}

fn deserialize_nullable_field<'de, D>(deserializer: D) -> Result<Option<Option<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<String>::deserialize(deserializer).map(Some)
}

#[derive(Deserialize)]
pub struct PricingCreate {
    pub price_minor: i64,
    pub dj_share_bp: Option<i32>,
}

/// Дашборд клуба: активная сессия, запросы за вечер, доход.
async fn club_dashboard(ctx: AuthContext, State(state): State<ApiEnv>) -> Response {
    if ctx.role() != Role::Club {
        return auth_error_response(ApiError::Forbidden);
    }
    let Ok(pool) = pool_of(&state) else {
        return pool_of(&state).unwrap_err();
    };
    let row = sqlx::query_as::<_, (Option<Uuid>, Option<String>, i64, i64)>(
        "WITH s AS (SELECT id, status FROM session WHERE club_id = $1 AND status <> 'closed' \
               ORDER BY created_at DESC LIMIT 1) \
         SELECT (SELECT id FROM s), (SELECT status FROM s), \
         (SELECT count(*) FROM request r JOIN session ss ON ss.id = r.session_id \
          WHERE ss.club_id = $1 AND r.created_at > now() - interval '24 hours'), \
         (SELECT COALESCE(sum(sp.amount_minor - sp.reversed_minor), 0)::bigint FROM split sp \
          WHERE sp.payee_type = 'club' AND sp.payee_id = $1)",
    )
    .bind(ctx.subject_id())
    .fetch_one(&pool)
    .await;
    let Ok((session_id, session_status, requests_today, earned_total)) = row else {
        return api_error(ApiError::Internal, "db error");
    };
    (
        StatusCode::OK,
        Json(json!({
            "session": session_id.map(|id| json!({ "id": id, "status": session_status })),
            "requests_today": requests_today,
            "earned_total_minor": earned_total,
        })),
    )
        .into_response()
}

/// Профиль клуба.
async fn club_profile(ctx: AuthContext, State(state): State<ApiEnv>) -> Response {
    if ctx.role() != Role::Club {
        return auth_error_response(ApiError::Forbidden);
    }
    let Ok(pool) = pool_of(&state) else {
        return pool_of(&state).unwrap_err();
    };
    match sqlx::query_as::<_, (String, String, Option<String>, Option<String>, String, Option<String>, Option<String>, String, String)>(
        "SELECT name, slug, address, country, email, phone, logo_url, stripe_onboarding_status, status \
         FROM club WHERE id = $1",
    )
    .bind(ctx.subject_id())
    .fetch_one(&pool)
    .await
    {
        Ok((name, slug, address, country, email, phone, logo_url, stripe_status, status)) => (
            StatusCode::OK,
            Json(json!({
                "id": ctx.subject_id(),
                "name": name, "slug": slug, "address": address, "country": country,
                "email": email, "phone": phone, "logo_url": logo_url,
                "stripe_onboarding_status": stripe_status, "status": status,
            })),
        )
            .into_response(),
        Err(_) => api_error(ApiError::Internal, "db error"),
    }
}

async fn club_profile_patch(
    ctx: AuthContext,
    State(state): State<ApiEnv>,
    Json(req): Json<ProfilePatch>,
) -> Response {
    if ctx.role() != Role::Club {
        return auth_error_response(ApiError::Forbidden);
    }
    let Ok(pool) = pool_of(&state) else {
        return pool_of(&state).unwrap_err();
    };
    match sqlx::query(
        "UPDATE club SET \
         name = COALESCE($2, name), address = COALESCE($3, address), \
         phone = COALESCE($4, phone), country = COALESCE($5, country), \
         logo_url = CASE WHEN $6 THEN $7 ELSE logo_url END \
         WHERE id = $1",
    )
    .bind(ctx.subject_id())
    .bind(&req.name)
    .bind(&req.address)
    .bind(&req.phone)
    .bind(&req.country)
    .bind(req.logo_url.is_some())
    .bind(req.logo_url.flatten())
    .execute(&pool)
    .await
    {
        Ok(_) => club_profile(ctx, State(state)).await,
        Err(_) => api_error(ApiError::Internal, "db error"),
    }
}

/// Тарифы клуба: история (Q6: изменение = новая строка).
async fn club_pricing_list(ctx: AuthContext, State(state): State<ApiEnv>) -> Response {
    if ctx.role() != Role::Club {
        return auth_error_response(ApiError::Forbidden);
    }
    let Ok(pool) = pool_of(&state) else {
        return pool_of(&state).unwrap_err();
    };
    let rows = sqlx::query_as::<
        _,
        (
            Uuid,
            i64,
            Option<i32>,
            chrono::DateTime<chrono::Utc>,
            Option<chrono::DateTime<chrono::Utc>>,
        ),
    >(
        "SELECT id, price_minor, dj_share_bp, valid_from, valid_to \
         FROM pricing_policy WHERE club_id = $1 ORDER BY valid_from DESC LIMIT 50",
    )
    .bind(ctx.subject_id())
    .fetch_all(&pool)
    .await;
    let Ok(rows) = rows else {
        return api_error(ApiError::Internal, "db error");
    };
    (
        StatusCode::OK,
        Json(json!({
            "items": rows.into_iter().map(|(id, price, dj_share, from, to)| json!({
                "id": id, "price_minor": price, "dj_share_bp": dj_share,
                "valid_from": from, "valid_to": to,
            })).collect::<Vec<_>>()
        })),
    )
        .into_response()
}

/// Новая цена: старая строка закрывается (valid_to), новая действует.
/// Дейстует на НОВЫЕ запросы (Q6); оплаченные не трогаем.
async fn club_pricing_create(
    ctx: AuthContext,
    State(state): State<ApiEnv>,
    Json(req): Json<PricingCreate>,
) -> Response {
    if ctx.role() != Role::Club {
        return auth_error_response(ApiError::Forbidden);
    }
    let Ok(pool) = pool_of(&state) else {
        return pool_of(&state).unwrap_err();
    };
    // Валидация: цена > 0; dj_share_bp 0..10000 если задан.
    if req.price_minor <= 0 {
        return api_error(ApiError::ValidationError, "price must be positive");
    }
    if let Some(bp) = req.dj_share_bp {
        if !(0..=10_000).contains(&bp) {
            return api_error(ApiError::ValidationError, "dj_share_bp 0..10000");
        }
    }
    let mut tx = match pool.begin().await {
        Ok(t) => t,
        Err(_) => return api_error(ApiError::Internal, "db error"),
    };
    // Закрыть действующую политику (Q6: изменение = новая строка).
    if sqlx::query(
        "UPDATE pricing_policy SET valid_to = now() \
         WHERE club_id = $1 AND valid_to IS NULL",
    )
    .bind(ctx.subject_id())
    .execute(&mut *tx)
    .await
    .is_err()
    {
        return api_error(ApiError::Internal, "db error");
    }
    let row = sqlx::query_as::<_, (Uuid,)>(
        "INSERT INTO pricing_policy (id, club_id, price_minor, dj_share_bp, created_by) \
         VALUES ($1, $2, $3, $4, $2) RETURNING id",
    )
    .bind(Uuid::now_v7())
    .bind(ctx.subject_id())
    .bind(req.price_minor)
    .bind(req.dj_share_bp)
    .fetch_one(&mut *tx)
    .await;
    let Ok((id,)) = row else {
        return api_error(ApiError::Internal, "db error");
    };
    if tx.commit().await.is_err() {
        return api_error(ApiError::Internal, "db error");
    }
    (
        StatusCode::CREATED,
        Json(json!({ "id": id, "price_minor": req.price_minor, "dj_share_bp": req.dj_share_bp })),
    )
        .into_response()
}

#[derive(Deserialize)]
pub struct EarningsParams {
    pub period: Option<String>, // today | week | month
}

/// Доходы клуба за период (нетто-сплиты).
async fn club_earnings(
    ctx: AuthContext,
    State(state): State<ApiEnv>,
    Query(params): Query<EarningsParams>,
) -> Response {
    if ctx.role() != Role::Club {
        return auth_error_response(ApiError::Forbidden);
    }
    let Ok(pool) = pool_of(&state) else {
        return pool_of(&state).unwrap_err();
    };
    let interval = match params.period.as_deref() {
        Some("week") => "7 days",
        Some("month") => "30 days",
        _ => "1 day",
    };
    let row = sqlx::query_as::<_, (i64, i64, i64)>(&format!(
        "SELECT \
         (SELECT COALESCE(sum(sp.amount_minor - sp.reversed_minor), 0)::bigint FROM split sp \
          JOIN payment p ON p.id = sp.payment_id \
          WHERE sp.payee_type = 'club' AND sp.payee_id = $1 \
          AND p.captured_at > now() - interval '{interval}'), \
         (SELECT count(*) FROM payment p JOIN request r ON r.id = p.request_id \
          JOIN session s ON s.id = r.session_id WHERE s.club_id = $1 AND p.status = 'captured' \
          AND p.captured_at > now() - interval '{interval}'), \
         (SELECT COALESCE(sum(p.amount_minor), 0)::bigint FROM payment p JOIN request r ON r.id = p.request_id \
          JOIN session s ON s.id = r.session_id WHERE s.club_id = $1 AND p.status = 'captured' \
          AND p.captured_at > now() - interval '{interval}')"
    ))
    .bind(ctx.subject_id())
    .fetch_one(&pool)
    .await
    .map_err(|_| ());
    let Ok((net, payments, gross)) = row else {
        return api_error(ApiError::Internal, "db error");
    };
    (
        StatusCode::OK,
        Json(json!({
            "period": params.period.unwrap_or_else(|| "today".into()),
            "net_minor": net,
            "payments": payments,
            "gross_minor": gross,
        })),
    )
        .into_response()
}

// ------------------------------------------------------------
// Клуб: состав DJ — список, статус, заметка, удаление, восстановление
// ------------------------------------------------------------
//
// Модель, которую видит клуб на `/djs`:
//
//   * **состав** — строки `dj` этого клуба, у которых `removed_at IS NULL`;
//     снятые с состава показываются отдельно (`?include_removed=1`) с
//     возможностью вернуть;
//   * **доступ** (`status`) — active / suspended / invited / blocked;
//     меняется ОТДЕЛЬНО от состава;
//   * **присутствие** — `last_seen_at` от heartbeat'а DJ-консоли
//     (`POST /v1/dj/presence`, раз в 30 с), поэтому «в сети» — это факт
//     из БД, а не догадка панели;
//   * **смена** — открытая сессия, назначенная этому DJ;
//   * **заметка** — рабочая пометка клуба (≤500 символов), видна только
//     клубу.
//
// Удаление — мягкое (`removed_at`): на `dj` ссылаются `session` и `split`,
// и стирать историю смен и выплат нельзя (техкарта). Исключение — DJ,
// который так и не принял приглашение (`password_hash = '!'`) и не имеет
// смен: это чистая строка-приглашение, её удаляем физически, иначе
// `UNIQUE(email)` навсегда блокирует повторный инвайт на тот же адрес.

/// Запись считается «в сети», если heartbeat моложе этого окна.
const DJ_PRESENCE_TTL_SECS: i64 = 90;
/// Предел длины рабочей заметки клуба о DJ.
const DJ_NOTE_MAX_CHARS: usize = 500;

#[derive(Deserialize)]
struct ClubDjListQuery {
    /// `1`/`true` — включить снятых с состава.
    include_removed: Option<String>,
}

impl ClubDjListQuery {
    fn wants_removed(&self) -> bool {
        matches!(
            self.include_removed.as_deref(),
            Some("1") | Some("true") | Some("yes")
        )
    }
}

/// Строка списка DJ: одна выборка, без N+1 по сменам.
#[derive(sqlx::FromRow)]
struct ClubDjRow {
    id: Uuid,
    display_name: String,
    email: String,
    status: String,
    note: Option<String>,
    last_seen_at: Option<DateTime<Utc>>,
    presence_age_seconds: Option<i64>,
    removed_at: Option<DateTime<Utc>>,
    invite_expires_at: Option<DateTime<Utc>>,
    has_password: bool,
    open_session_id: Option<Uuid>,
    created_at: DateTime<Utc>,
}

/// Диджеи клуба: состав, доступ, присутствие и открытая смена.
async fn club_djs(
    ctx: AuthContext,
    Query(query): Query<ClubDjListQuery>,
    State(state): State<ApiEnv>,
) -> Response {
    if ctx.role() != Role::Club {
        return auth_error_response(ApiError::Forbidden);
    }
    let Ok(pool) = pool_of(&state) else {
        return pool_of(&state).unwrap_err();
    };
    let rows = sqlx::query_as::<_, ClubDjRow>(
        "SELECT d.id, d.display_name, d.email, d.status, d.note, d.last_seen_at, \
         CASE WHEN d.last_seen_at IS NULL THEN NULL \
              ELSE GREATEST(0, EXTRACT(EPOCH FROM (now() - d.last_seen_at))::bigint) END \
              AS presence_age_seconds, \
         d.removed_at, d.invite_expires_at, (d.password_hash <> '!') AS has_password, \
         (SELECT s.id FROM session s \
           WHERE s.dj_id = d.id AND s.status <> 'closed' \
           ORDER BY s.created_at DESC LIMIT 1) AS open_session_id, \
         d.created_at \
         FROM dj d \
         WHERE d.club_id = $1 AND ($2 OR d.removed_at IS NULL) \
         ORDER BY d.removed_at NULLS FIRST, d.display_name",
    )
    .bind(ctx.subject_id())
    .bind(query.wants_removed())
    .fetch_all(&pool)
    .await
    .map_err(|_| ());
    let Ok(rows) = rows else {
        return api_error(ApiError::Internal, "db error");
    };
    (
        StatusCode::OK,
        Json(json!({
            "server_time": Utc::now(),
            "presence_ttl_seconds": DJ_PRESENCE_TTL_SECS,
            "items": rows.into_iter().map(|row| json!({
                "id": row.id,
                "display_name": row.display_name,
                "email": row.email,
                "status": row.status,
                "note": row.note,
                "last_seen_at": row.last_seen_at,
                "presence_age_seconds": row.presence_age_seconds,
                "online": row
                    .presence_age_seconds
                    .is_some_and(|age| age <= DJ_PRESENCE_TTL_SECS),
                "on_shift": row.open_session_id.is_some(),
                "open_session_id": row.open_session_id,
                "removed": row.removed_at.is_some(),
                "removed_at": row.removed_at,
                "invite_expires_at": row.invite_expires_at,
                "has_password": row.has_password,
                "created_at": row.created_at,
            })).collect::<Vec<_>>()
        })),
    )
        .into_response()
}

/// Изменение доступа и/или заметки DJ. Клуб всегда берётся из токена.
///
/// Поля независимы: `{status}` — только доступ (так работает старая
/// панель), `{note}` — только заметка, `{note: null}` — очистить заметку.
///
/// Тело разбирается в `Value`, а не в структуру с `Option<Value>`: serde
/// отображает **и** отсутствующее поле, **и** явный `null` в `None`, так
/// что «не трогать заметку» и «очистить заметку» стали бы неразличимы.
/// С `Value` ключ виден напрямую (`obj.contains_key`).
#[derive(Deserialize)]
#[serde(transparent)]
struct ClubDjPatch(Value);

/// Что делать с заметкой после PATCH.
enum NotePatch {
    Keep,
    Clear,
    Set(String),
}

/// Клуб меняет доступ и заметку принадлежащего ему DJ: `club_id` в теле
/// отсутствует, он берётся из токена (изоляция клубов).
async fn club_dj_patch(
    ctx: AuthContext,
    Path(dj_id): Path<Uuid>,
    State(state): State<ApiEnv>,
    Json(body): Json<ClubDjPatch>,
) -> Response {
    if ctx.role() != Role::Club {
        return auth_error_response(ApiError::Forbidden);
    }
    let Some(obj) = body.0.as_object() else {
        return api_error(ApiError::BadRequest, "body must be a JSON object");
    };

    // `status`: отсутствует → не трогаем; строка → валидируем; null/иное → 400.
    let next_status: Option<String> = match obj.get("status") {
        None => None,
        Some(Value::String(raw)) => {
            if !matches!(raw.as_str(), "active" | "suspended") {
                return api_error(ApiError::BadRequest, "status must be active or suspended");
            }
            Some(raw.clone())
        }
        Some(_) => return api_error(ApiError::BadRequest, "status must be a string"),
    };

    // `note`: ключа нет → не трогаем; null → очистить; строка → сохранить.
    let note = match obj.get("note") {
        None => NotePatch::Keep,
        Some(Value::Null) => NotePatch::Clear,
        Some(Value::String(raw)) => {
            let trimmed = raw.trim();
            if trimmed.chars().count() > DJ_NOTE_MAX_CHARS {
                return api_error(ApiError::BadRequest, "note must be at most 500 characters");
            }
            if trimmed.is_empty() {
                NotePatch::Clear
            } else {
                NotePatch::Set(trimmed.to_string())
            }
        }
        Some(_) => return api_error(ApiError::BadRequest, "note must be a string or null"),
    };

    if next_status.is_none() && matches!(note, NotePatch::Keep) {
        return api_error(
            ApiError::BadRequest,
            "nothing to update: send status and/or note",
        );
    }

    let Ok(pool) = pool_of(&state) else {
        return pool_of(&state).unwrap_err();
    };

    // 404 только если DJ не этого клуба или снят с состава.
    let current: Option<(String, Option<String>)> = sqlx::query_as(
        "SELECT status, note FROM dj WHERE id = $1 AND club_id = $2 AND removed_at IS NULL",
    )
    .bind(dj_id)
    .bind(ctx.subject_id())
    .fetch_optional(&pool)
    .await
    .unwrap_or(None);
    let Some((current_status, current_note)) = current else {
        return api_error_custom(
            StatusCode::NOT_FOUND,
            "dj_not_found",
            "DJ not found in this club",
            json!({}),
        );
    };

    // Приглашённый DJ ещё не имеет доступа: приостанавливать нечего.
    if next_status.is_some() && !matches!(current_status.as_str(), "active" | "suspended") {
        return api_error_custom(
            StatusCode::CONFLICT,
            "dj_status_locked",
            "This DJ has not accepted the invitation yet; access can be changed after activation",
            json!({ "status": current_status }),
        );
    }

    let (note_provided, note_value) = match note {
        NotePatch::Keep => (false, None),
        NotePatch::Clear => (true, None),
        NotePatch::Set(text) => (true, Some(text)),
    };

    #[allow(clippy::type_complexity)]
    let updated: Result<Option<(Uuid, String, String, Option<String>)>, sqlx::Error> =
        sqlx::query_as(
            "UPDATE dj SET \
             status = COALESCE($3::text, status), \
             note = CASE WHEN $4 THEN $5::text ELSE note END \
             WHERE id = $1 AND club_id = $2 AND removed_at IS NULL \
             RETURNING id, display_name, status, note",
        )
        .bind(dj_id)
        .bind(ctx.subject_id())
        .bind(next_status.as_deref())
        .bind(note_provided)
        .bind(note_value.as_deref())
        .fetch_optional(&pool)
        .await;

    match updated {
        Ok(Some((id, display_name, status, note))) => {
            let status_changed = status != current_status;
            let note_changed = note != current_note;
            if status_changed {
                let _ = sqlx::query(
                    "INSERT INTO audit_log (actor_type, actor_id, action, entity, diff) \
                     VALUES ('club', $1, 'dj.status_change', $2, $3)",
                )
                .bind(ctx.subject_id())
                .bind(id.to_string())
                .bind(json!({ "from": current_status, "status": status }))
                .execute(&pool)
                .await;
            }
            if note_changed {
                let _ = sqlx::query(
                    "INSERT INTO audit_log (actor_type, actor_id, action, entity, diff) \
                     VALUES ('club', $1, 'dj.note_update', $2, $3)",
                )
                .bind(ctx.subject_id())
                .bind(id.to_string())
                .bind(json!({ "note": note }))
                .execute(&pool)
                .await;
            }
            (
                StatusCode::OK,
                Json(json!({
                    "id": id,
                    "display_name": display_name,
                    "status": status,
                    "note": note,
                })),
            )
                .into_response()
        }
        Ok(None) => api_error_custom(
            StatusCode::NOT_FOUND,
            "dj_not_found",
            "DJ not found in this club",
            json!({}),
        ),
        Err(_) => api_error(ApiError::Internal, "db error"),
    }
}

/// Снять DJ с состава.
///
/// Строку нельзя удалить, если у DJ есть история: `session.dj_id` и
/// `split.payee_id` ссылаются на него, а история смен и выплат
/// неприкосновенна (техкарта). Поэтому:
///
///   * DJ с паролем или со сменами → мягкое удаление (`removed_at`),
///     доступ блокируется (`suspended`), приглашение аннулируется;
///   * DJ, так и не принявший приглашение → физическое удаление чистой
///     строки-приглашения (иначе `UNIQUE(email)` не даст пригласить его
///     снова на тот же адрес).
///
/// Снять с состава DJ, у которого открыта смена, нельзя: комната осталась
/// бы без диджея — сначала закрывается смена.
async fn club_dj_remove(
    ctx: AuthContext,
    Path(dj_id): Path<Uuid>,
    State(state): State<ApiEnv>,
) -> Response {
    if ctx.role() != Role::Club {
        return auth_error_response(ApiError::Forbidden);
    }
    let Ok(pool) = pool_of(&state) else {
        return pool_of(&state).unwrap_err();
    };
    let club_id = ctx.subject_id();

    let current: Option<(String, bool, Option<Uuid>)> = sqlx::query_as(
        "SELECT d.status, (d.password_hash <> '!') AS has_password, \
         (SELECT s.id FROM session s \
           WHERE s.dj_id = d.id AND s.status <> 'closed' \
           ORDER BY s.created_at DESC LIMIT 1) AS open_session_id \
         FROM dj d WHERE d.id = $1 AND d.club_id = $2 AND d.removed_at IS NULL",
    )
    .bind(dj_id)
    .bind(club_id)
    .fetch_optional(&pool)
    .await
    .unwrap_or(None);
    let Some((status, has_password, open_session_id)) = current else {
        return api_error_custom(
            StatusCode::NOT_FOUND,
            "dj_not_found",
            "DJ not found in this club",
            json!({}),
        );
    };

    if let Some(session_id) = open_session_id {
        return api_error_custom(
            StatusCode::CONFLICT,
            "dj_on_shift",
            "This DJ has an open shift; close the shift before removing them from the roster",
            json!({ "session_id": session_id }),
        );
    }

    if !has_password {
        // Приглашение не принято — истории нет, строка удаляется целиком.
        // Защита от гонки: `password_hash = '!'` повторяется в WHERE.
        let deleted = sqlx::query(
            "DELETE FROM dj WHERE id = $1 AND club_id = $2 AND removed_at IS NULL \
             AND password_hash = '!' \
             AND NOT EXISTS (SELECT 1 FROM session s WHERE s.dj_id = dj.id)",
        )
        .bind(dj_id)
        .bind(club_id)
        .execute(&pool)
        .await;
        match deleted {
            Ok(result) if result.rows_affected() == 1 => {
                let _ = sqlx::query(
                    "INSERT INTO audit_log (actor_type, actor_id, action, entity, diff) \
                     VALUES ('club', $1, 'dj.remove', $2, $3)",
                )
                .bind(club_id)
                .bind(dj_id.to_string())
                .bind(json!({ "deleted": "hard", "status": status }))
                .execute(&pool)
                .await;
                return (
                    StatusCode::OK,
                    Json(json!({ "id": dj_id, "deleted": "hard", "status": status })),
                )
                    .into_response();
            }
            Ok(_) => {} // не совпало условие — уходим в мягкое удаление ниже
            Err(_) => return api_error(ApiError::Internal, "db error"),
        }
    }

    let removed: Result<Option<(Uuid, String)>, sqlx::Error> = sqlx::query_as(
        "UPDATE dj SET removed_at = now(), \
         status = CASE WHEN status = 'active' THEN 'suspended' ELSE status END, \
         invite_token_hash = NULL, invite_expires_at = NULL \
         WHERE id = $1 AND club_id = $2 AND removed_at IS NULL \
         RETURNING id, display_name",
    )
    .bind(dj_id)
    .bind(club_id)
    .fetch_optional(&pool)
    .await;
    match removed {
        Ok(Some((id, display_name))) => {
            // Снятый с состава DJ не должен продлевать сессию: access-JWT
            // живёт свои 15 минут, а refresh — 30 дней, и без отзыва он
            // молча выписывал бы новые токены уже после исключения.
            let _ = sqlx::query(
                "UPDATE refresh_token SET revoked_at = now() \
                 WHERE subject_type = 'dj' AND subject_id = $1 AND revoked_at IS NULL",
            )
            .bind(id)
            .execute(&pool)
            .await;
            let _ = sqlx::query(
                "INSERT INTO audit_log (actor_type, actor_id, action, entity, diff) \
                 VALUES ('club', $1, 'dj.remove', $2, $3)",
            )
            .bind(club_id)
            .bind(id.to_string())
            .bind(json!({ "deleted": "soft", "status": status }))
            .execute(&pool)
            .await;
            (
                StatusCode::OK,
                Json(json!({
                    "id": id,
                    "display_name": display_name,
                    "deleted": "soft",
                    "status": "suspended",
                })),
            )
                .into_response()
        }
        Ok(None) => api_error_custom(
            StatusCode::NOT_FOUND,
            "dj_not_found",
            "DJ not found in this club",
            json!({}),
        ),
        Err(_) => api_error(ApiError::Internal, "db error"),
    }
}

/// Вернуть снятого DJ в состав. Идемпотентно: повторный вызов на уже
/// восстановленном DJ отвечает 200 с текущим состоянием.
///
/// Доступ восстанавливается вместе с составом (`active`): DJ возвращается
/// по решению клуба, а отдельный шаг «ещё и разблокируй» только плодил бы
/// забытые полусостояния. Что именно произошло — видно в аудите.
async fn club_dj_restore(
    ctx: AuthContext,
    Path(dj_id): Path<Uuid>,
    State(state): State<ApiEnv>,
) -> Response {
    if ctx.role() != Role::Club {
        return auth_error_response(ApiError::Forbidden);
    }
    let Ok(pool) = pool_of(&state) else {
        return pool_of(&state).unwrap_err();
    };
    let club_id = ctx.subject_id();

    let restored: Result<Option<(Uuid, String, String)>, sqlx::Error> = sqlx::query_as(
        "UPDATE dj SET removed_at = NULL, status = 'active' \
         WHERE id = $1 AND club_id = $2 AND removed_at IS NOT NULL \
         RETURNING id, display_name, status",
    )
    .bind(dj_id)
    .bind(club_id)
    .fetch_optional(&pool)
    .await;

    let (id, display_name, status) = match restored {
        Ok(Some(row)) => row,
        Ok(None) => {
            // Либо уже в составе, либо чужой DJ.
            let existing: Option<(Uuid, String, String)> = sqlx::query_as(
                "SELECT id, display_name, status FROM dj WHERE id = $1 AND club_id = $2",
            )
            .bind(dj_id)
            .bind(club_id)
            .fetch_optional(&pool)
            .await
            .unwrap_or(None);
            match existing {
                Some((id, display_name, status)) => (id, display_name, status),
                None => {
                    return api_error_custom(
                        StatusCode::NOT_FOUND,
                        "dj_not_found",
                        "DJ not found in this club",
                        json!({}),
                    )
                }
            }
        }
        Err(_) => return api_error(ApiError::Internal, "db error"),
    };

    let _ = sqlx::query(
        "INSERT INTO audit_log (actor_type, actor_id, action, entity, diff) \
         VALUES ('club', $1, 'dj.restore', $2, $3)",
    )
    .bind(club_id)
    .bind(id.to_string())
    .bind(json!({ "status": status }))
    .execute(&pool)
    .await;

    (
        StatusCode::OK,
        Json(json!({ "id": id, "display_name": display_name, "status": status, "removed": false })),
    )
        .into_response()
}

/// `POST /v1/dj/presence` — heartbeat DJ-консоли.
///
/// Консоль зовёт его раз в 30 с, пока вкладка видима; клуб читает
/// `last_seen_at` в списке состава и показывает «в сети», пока heartbeat
/// моложе [`DJ_PRESENCE_TTL_SECS`]. Ничего не кешируется: одна запись в
/// строку, которую читает только клуб этого DJ.
async fn dj_presence(ctx: AuthContext, State(state): State<ApiEnv>) -> Response {
    if ctx.role() != Role::Dj {
        return auth_error_response(ApiError::Forbidden);
    }
    let Ok(pool) = pool_of(&state) else {
        return pool_of(&state).unwrap_err();
    };
    let updated: Result<Option<DateTime<Utc>>, sqlx::Error> = sqlx::query_scalar(
        "UPDATE dj SET last_seen_at = now() WHERE id = $1 RETURNING last_seen_at",
    )
    .bind(ctx.subject_id())
    .fetch_optional(&pool)
    .await;
    match updated {
        Ok(Some(last_seen_at)) => (
            StatusCode::OK,
            Json(json!({ "last_seen_at": last_seen_at, "online": true })),
        )
            .into_response(),
        Ok(None) => api_error_custom(
            StatusCode::NOT_FOUND,
            "dj_not_found",
            "DJ account not found",
            json!({}),
        ),
        Err(_) => api_error(ApiError::Internal, "db error"),
    }
}

/// Выплаты клуба.
async fn club_payouts(ctx: AuthContext, State(state): State<ApiEnv>) -> Response {
    if ctx.role() != Role::Club {
        return auth_error_response(ApiError::Forbidden);
    }
    let Ok(pool) = pool_of(&state) else {
        return pool_of(&state).unwrap_err();
    };
    let rows = sqlx::query_as::<_, (Uuid, i64, String, Option<chrono::DateTime<chrono::Utc>>)>(
        "SELECT id, amount_minor, status, paid_at FROM payout \
         WHERE payee_type = 'club' AND payee_id = $1 ORDER BY created_at DESC LIMIT 50",
    )
    .bind(ctx.subject_id())
    .fetch_all(&pool)
    .await
    .map_err(|_| ());
    let Ok(rows) = rows else {
        return api_error(ApiError::Internal, "db error");
    };
    (
        StatusCode::OK,
        Json(json!({
            "items": rows.into_iter().map(|(id, amount, status, paid_at)| json!({
                "id": id, "amount_minor": amount, "status": status, "paid_at": paid_at,
            })).collect::<Vec<_>>()
        })),
    )
        .into_response()
}

// ------------------------------------------------------------
// Клуб: инвайт DJ + Stripe-онбординг (W2-C)
// ------------------------------------------------------------

/// Тело `POST /v1/club/djs/invite`. `club_id` в теле намеренно отсутствует:
/// клуб всегда берётся из токена (изоляция клубов), переданный в теле
/// `club_id` игнорируется (serde по умолчанию пропускает лишние поля).
#[derive(Deserialize)]
pub struct DjInviteRequest {
    pub email: String,
    pub display_name: String,
}

#[derive(Deserialize)]
pub struct DjInviteAcceptRequest {
    pub invite_token: String,
    pub password: String,
}

/// Одноразовое принятие приглашения: назначить пароль и активировать аккаунт.
async fn dj_invite_accept(
    State(state): State<ApiEnv>,
    Json(req): Json<DjInviteAcceptRequest>,
) -> Response {
    let Ok(pool) = pool_of(&state) else {
        return pool_of(&state).unwrap_err();
    };
    if req.password.len() < 10 || req.password.len() > 128 {
        return api_error(ApiError::ValidationError, "password length 10..128");
    }
    if req.invite_token.len() != 64 || !req.invite_token.bytes().all(|b| b.is_ascii_hexdigit()) {
        return api_error(ApiError::Unauthorized, "invalid or expired invitation");
    }
    let token_hash = auth::hash_refresh_token(&req.invite_token);
    let password_hash = match auth::hash_password(&req.password) {
        Ok(hash) => hash,
        Err(_) => return api_error(ApiError::Internal, "password hashing failed"),
    };
    let updated = sqlx::query(
        "UPDATE dj SET password_hash = $2, status = 'active', invite_token_hash = NULL, invite_expires_at = NULL \
         WHERE invite_token_hash = $1 AND invite_expires_at > now() AND status = 'invited' \
         RETURNING id",
    )
    .bind(token_hash)
    .bind(password_hash)
    .fetch_optional(&pool)
    .await;
    match updated {
        Ok(Some(_)) => (StatusCode::OK, Json(json!({ "accepted": true }))).into_response(),
        Ok(None) => api_error(ApiError::Unauthorized, "invalid or expired invitation"),
        Err(_) => api_error(ApiError::Internal, "db error"),
    }
}

/// Приглашение DJ в клуб.
///
/// DJ создаётся в статусе `invited` (без пароля — логин невозможен до
/// завершения инвайт-флоу). Одноразовый `invite_token` (128 бит, hex)
/// хранится только sha256-хешем — переиспользуется подход хеширования
/// токенов из `auth/identity` ([`auth::hash_refresh_token`]), отдельный
/// столбец, не пароль. TTL — 7 дней. Повторный инвайт того же e-mail в
/// этот клуб → 409 `dj_already_invited`.
async fn club_djs_invite(
    ctx: AuthContext,
    State(state): State<ApiEnv>,
    Json(req): Json<DjInviteRequest>,
) -> Response {
    if ctx.role() != Role::Club {
        return auth_error_response(ApiError::Forbidden);
    }
    let Ok(pool) = pool_of(&state) else {
        return pool_of(&state).unwrap_err();
    };

    let email = req.email.trim();
    let display = req.display_name.trim();
    if !(3..=320).contains(&email.len()) {
        return api_error(ApiError::ValidationError, "email length 3..320");
    }
    if !email.contains('@') || !email.contains('.') {
        return api_error(ApiError::ValidationError, "invalid email");
    }
    if display.is_empty() || display.len() > 120 {
        return api_error(ApiError::ValidationError, "display_name 1..120");
    }

    let club_id = ctx.subject_id();

    // Тот же e-mail уже приглашён/создан в ЭТОТ клуб → 409.
    let existing: Option<(Uuid,)> =
        sqlx::query_as("SELECT id FROM dj WHERE email = $1 AND club_id = $2")
            .bind(email)
            .bind(club_id)
            .fetch_optional(&pool)
            .await
            .unwrap_or(None);
    if existing.is_some() {
        return api_error_custom(
            StatusCode::CONFLICT,
            "dj_already_invited",
            "DJ with this email already invited to this club",
            json!({}),
        );
    }

    let dj_id = Uuid::now_v7();
    let token = auth::generate_refresh_token();
    let token_hash = auth::hash_refresh_token(&token);
    let expires_at = Utc::now() + Duration::days(DJ_INVITE_TTL_DAYS);

    // password_hash = '!' — заведомо не Argon2: invited-DJ не может
    // войти по паролю, пока инвайт не завершён (verify_password вернёт
    // Err → «invalid credentials»). Секрет входа — одноразовый
    // invite_token, не пароль.
    let inserted = sqlx::query(
        "INSERT INTO dj \
         (id, club_id, email, password_hash, display_name, status, \
          invite_token_hash, invite_expires_at) \
         VALUES ($1, $2, $3, '!', $4, 'invited', $5, $6)",
    )
    .bind(dj_id)
    .bind(club_id)
    .bind(email)
    .bind(display)
    .bind(&token_hash)
    .bind(expires_at)
    .execute(&pool)
    .await;

    if let Err(e) = inserted {
        // UNIQUE(email) глобально: e-mail уже числится DJ в любом клубе.
        if e.as_database_error()
            .and_then(|db| db.code())
            .is_some_and(|c| c == "23505")
        {
            return api_error_custom(
                StatusCode::CONFLICT,
                "dj_already_invited",
                "DJ with this email already exists",
                json!({}),
            );
        }
        return api_error(ApiError::Internal, "db error");
    }

    // Аудит: инвайт — кадровое действие клуба.
    let _ = sqlx::query(
        "INSERT INTO audit_log (actor_type, actor_id, action, entity, diff) \
         VALUES ('club', $1, 'dj.invite', $2, $3)",
    )
    .bind(club_id)
    .bind(dj_id.to_string())
    .bind(json!({ "email": email, "display_name": display }))
    .execute(&pool)
    .await;

    // Письмо-приглашение (ADR-008): best-effort, инвайт уже создан.
    // Токен уходит только в письме и в ответе — в БД лежит хеш.
    enqueue_dj_invite_mail(
        &state,
        &pool,
        DjInviteMail {
            club_id,
            dj_id,
            email,
            display,
            invite_token: &token,
            expires_at,
        },
    )
    .await;

    (
        StatusCode::CREATED,
        Json(json!({
            "dj_id": dj_id,
            "invite_token": token,
            "invite_expires_at": expires_at,
        })),
    )
        .into_response()
}

/// Данные письма-приглашения DJ.
///
/// Структура вместо шести позиционных аргументов: `email` и `display` —
/// обе строки, перепутать их местами было бы нечем.
struct DjInviteMail<'a> {
    club_id: Uuid,
    dj_id: Uuid,
    email: &'a str,
    display: &'a str,
    invite_token: &'a str,
    expires_at: chrono::DateTime<Utc>,
}

/// Поставить в очередь письмо-приглашение DJ.
///
/// Ошибка почты не валит инвайт: клуб уже получил `invite_token` в ответе и
/// может передать его вручную — это ровно то поведение, которое было до
/// появления почты. Поэтому все ошибки здесь только логируются.
async fn enqueue_dj_invite_mail(state: &ApiEnv, pool: &sqlx::PgPool, invite: DjInviteMail<'_>) {
    let DjInviteMail {
        club_id,
        dj_id,
        email,
        display,
        invite_token,
        expires_at,
    } = invite;
    let Some(mail_repo) = state.mail.as_ref() else {
        return;
    };
    let club_name: Option<(String,)> = sqlx::query_as("SELECT name FROM club WHERE id = $1")
        .bind(club_id)
        .fetch_optional(pool)
        .await
        .unwrap_or(None);
    let club_name = club_name
        .map(|(n,)| n)
        .unwrap_or_else(|| "DROP IT".to_string());

    let vars = json!({
        "club_name": club_name,
        "dj_name": display,
        "invite_url": format!(
            "{}/dj/invite?token={}",
            crate::infra::mail_app_base_url(),
            invite_token
        ),
        "expires_at": expires_at.to_rfc3339(),
    });

    let to = match dropit_infra_mail::MailAddress::parse(email) {
        Ok(addr) => addr,
        Err(e) => {
            tracing::warn!(error = %e, "mail: адрес приглашения не разобран");
            return;
        }
    };

    crate::mail_routes::enqueue_best_effort(
        mail_repo.as_ref(),
        crate::mail_routes::MailIntent {
            template: dropit_infra_mail::TemplateKey::DjInvite,
            to: &to,
            vars,
            idempotency_key: format!("dj_invite:{dj_id}"),
            aggregate_type: Some("dj"),
            aggregate_id: Some(dj_id),
        },
    )
    .await;
}

/// Статус Stripe-онбординга клуба с живой синхронизацией у провайдера.
///
/// Если у клуба есть `stripe_account_id` — состояние обновляется через
/// `StripeApi::retrieve_account()` и сохраняется в БД (`charges_enabled`
/// не хранится построчно — источник истиныStripe;persist-только статус-
/// метка). Ошибка провайдера → деградация: возвращается последнее
/// сохранённое состояние, в лог `warn`, не 500.
async fn club_stripe_onboarding_status(ctx: AuthContext, State(state): State<ApiEnv>) -> Response {
    if ctx.role() != Role::Club {
        return auth_error_response(ApiError::Forbidden);
    }
    let Ok(pool) = pool_of(&state) else {
        return pool_of(&state).unwrap_err();
    };

    let row: Result<(Option<String>, String), sqlx::Error> = sqlx::query_as(
        "SELECT stripe_account_id, stripe_onboarding_status FROM club WHERE id = $1",
    )
    .bind(ctx.subject_id())
    .fetch_one(&pool)
    .await;
    let Ok((acct_opt, saved_status)) = row else {
        return api_error(ApiError::Internal, "db error");
    };

    let Some(acct_id) = acct_opt else {
        // Аккаунта нет — онбординг не стартовал.
        return (
            StatusCode::OK,
            Json(json!({
                "stripe_account_id": Value::Null,
                "status": "none",
                "charges_enabled": false,
                "payouts_enabled": false,
                "details_submitted": false,
            })),
        )
            .into_response();
    };

    // Живая синхронизация с провайдером, если он сконфигурирован.
    if let Some(stripe) = state.stripe.clone() {
        match stripe.retrieve_account(&acct_id).await {
            Ok(acc) => {
                let status = stripe_status_from_account(&acc);
                // Сохраняем метку в существующем словаре колонки
                // (verified/pending/restricted), чтобы не плодить
                // новые значения и не ломать фронтенд.
                let persist = match status {
                    "enabled" => "verified",
                    "restricted" => "restricted",
                    _ => "pending",
                };
                let _ = sqlx::query("UPDATE club SET stripe_onboarding_status = $2 WHERE id = $1")
                    .bind(ctx.subject_id())
                    .bind(persist)
                    .execute(&pool)
                    .await;
                return (
                    StatusCode::OK,
                    Json(json!({
                        "stripe_account_id": acct_id,
                        "status": status,
                        "charges_enabled": acc.charges_enabled,
                        "payouts_enabled": acc.payouts_enabled,
                        "details_submitted": acc.details_submitted,
                    })),
                )
                    .into_response();
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    account = %acct_id,
                    "stripe retrieve_account failed; возвращаем последнее сохранённое состояние"
                );
            }
        }
    }

    // Деградация: провайдер недоступен/не сконфигурирован — отдаём
    // последнее сохранённое состояние.
    let status = stripe_status_from_saved(&saved_status);
    (
        StatusCode::OK,
        Json(json!({
            "stripe_account_id": acct_id,
            "status": status,
            "charges_enabled": false,
            "payouts_enabled": false,
            "details_submitted": false,
        })),
    )
        .into_response()
}

/// Тело `POST /v1/club/stripe/onboarding-link`.
#[derive(Deserialize)]
pub struct OnboardingLinkRequest {
    pub refresh_url: String,
    pub return_url: String,
}

/// Создание/обновление Stripe Connect onboarding-link для клуба.
///
/// Если у клуба ещё нет `stripe_account_id` — создаётся Connect-аккаунт
/// (`create_connected_account`), id сохраняется. Затем запрашивается
/// `account_link` — URL Stripe-hosted формы. Повторный вызов НЕ создаёт
/// второй аккаунт (идемпотентность по сохранённому id). Ошибка провайдера
/// → 502 `stripe_unavailable` с code/message провайдера в `details`.
async fn club_stripe_onboarding_link(
    ctx: AuthContext,
    State(state): State<ApiEnv>,
    Json(req): Json<OnboardingLinkRequest>,
) -> Response {
    if ctx.role() != Role::Club {
        return auth_error_response(ApiError::Forbidden);
    }
    let Ok(pool) = pool_of(&state) else {
        return pool_of(&state).unwrap_err();
    };
    let Some(stripe) = state.stripe.clone() else {
        return api_error_custom(
            StatusCode::BAD_GATEWAY,
            "stripe_unavailable",
            "stripe provider not configured",
            json!({ "code": "not_configured" }),
        );
    };

    if !is_http_url(&req.refresh_url) || !is_http_url(&req.return_url) {
        return api_error(
            ApiError::ValidationError,
            "refresh_url/return_url must be http(s)",
        );
    }

    let club_id = ctx.subject_id();
    let row: Result<(Option<String>, String, Option<String>), sqlx::Error> =
        sqlx::query_as("SELECT stripe_account_id, email, country FROM club WHERE id = $1")
            .bind(club_id)
            .fetch_one(&pool)
            .await;
    let Ok((acct_opt, email, country)) = row else {
        return api_error(ApiError::Internal, "db error");
    };

    let acct_id = match acct_opt {
        Some(id) => id,
        None => {
            // Создание Connect-аккаунта у провайдера.
            let mut metadata = BTreeMap::new();
            metadata.insert("club_id".to_string(), club_id.to_string());
            let req = ConnectAccountReq {
                email,
                country: country.unwrap_or_else(|| "ES".to_string()),
                business_type: "individual".to_string(),
                metadata,
            };
            match stripe.create_connected_account(req).await {
                Ok(acc) => {
                    let _ = sqlx::query(
                        "UPDATE club SET stripe_account_id = $2, stripe_onboarding_status = 'pending' \
                         WHERE id = $1",
                    )
                    .bind(club_id)
                    .bind(&acc.id)
                    .execute(&pool)
                    .await;
                    acc.id
                }
                Err(e) => {
                    return api_error_custom(
                        StatusCode::BAD_GATEWAY,
                        "stripe_unavailable",
                        "stripe create_connected_account failed",
                        stripe_error_details(&e),
                    );
                }
            }
        }
    };

    match stripe
        .account_link(&acct_id, &req.refresh_url, &req.return_url)
        .await
    {
        Ok(url) => (
            StatusCode::CREATED,
            Json(json!({ "url": url, "expires_at": Value::Null })),
        )
            .into_response(),
        Err(e) => api_error_custom(
            StatusCode::BAD_GATEWAY,
            "stripe_unavailable",
            "stripe account_link failed",
            stripe_error_details(&e),
        ),
    }
}

/// `enabled` если charges+payouts разрешены; `pending` пока KYC не сдан;
/// иначе `restricted`.
fn stripe_status_from_account(acc: &dropit_infra_stripe::ConnectedAccount) -> &'static str {
    if acc.charges_enabled && acc.payouts_enabled {
        "enabled"
    } else if !acc.details_submitted {
        "pending"
    } else {
        "restricted"
    }
}

/// Маппинг сохранённой метки колонки (verified/pending/restricted/…)
/// в контрактный статус none|pending|restricted|enabled.
fn stripe_status_from_saved(saved: &str) -> &'static str {
    match saved {
        "verified" | "enabled" => "enabled",
        "restricted" => "restricted",
        _ => "pending",
    }
}

/// Простейшая проверка http(s) URL (схема + хост).
fn is_http_url(s: &str) -> bool {
    s.starts_with("http://") || s.starts_with("https://")
}

// ============================================================
// Админ
// ============================================================

/// Дашборд «сейчас»: сессии, запросы/мин, оборот за день.
async fn admin_dashboard(ctx: AuthContext, State(state): State<ApiEnv>) -> Response {
    if ctx.role() != Role::Admin {
        return auth_error_response(ApiError::Forbidden);
    }
    let Ok(pool) = pool_of(&state) else {
        return pool_of(&state).unwrap_err();
    };
    let row = sqlx::query_as::<_, (i64, i64, i64, i64, i64)>(
        "SELECT \
         (SELECT count(*) FROM session WHERE status <> 'closed'), \
         (SELECT count(*) FROM request WHERE created_at > now() - interval '24 hours'), \
         (SELECT count(*) FROM request WHERE created_at > now() - interval '1 minute'), \
         (SELECT COALESCE(sum(amount_minor), 0)::bigint FROM payment WHERE status IN ('captured', 'partially_refunded', 'refunded') \
          AND created_at > now() - interval '24 hours'), \
         (SELECT count(*) FROM incident WHERE resolved_at IS NULL)",
    )
    .fetch_one(&pool)
    .await
    .map_err(|_| ());
    let Ok((sessions, requests_day, req_per_min, turnover_day, open_incidents)) = row else {
        return api_error(ApiError::Internal, "db error");
    };
    (
        StatusCode::OK,
        Json(json!({
            "active_sessions": sessions,
            "requests_today": requests_day,
            "requests_per_minute": req_per_min,
            "turnover_today_minor": turnover_day,
            "open_incidents": open_incidents,
        })),
    )
        .into_response()
}

/// Список клубов (+ оборот) для админа.
async fn admin_clubs(ctx: AuthContext, State(state): State<ApiEnv>) -> Response {
    if ctx.role() != Role::Admin {
        return auth_error_response(ApiError::Forbidden);
    }
    let Ok(pool) = pool_of(&state) else {
        return pool_of(&state).unwrap_err();
    };
    let rows = sqlx::query_as::<_, (Uuid, String, Option<String>, String, String, i64)>(
        "SELECT c.id, c.name, c.city, c.email, c.status, \
         (SELECT COALESCE(sum(p.amount_minor), 0)::bigint FROM payment p \
          JOIN request r ON r.id = p.request_id JOIN session s ON s.id = r.session_id \
          WHERE s.club_id = c.id AND p.status IN ('captured', 'partially_refunded')) \
         FROM club c ORDER BY c.created_at DESC LIMIT 100",
    )
    .fetch_all(&pool)
    .await
    .map_err(|_| ());
    let Ok(rows) = rows else {
        return api_error(ApiError::Internal, "db error");
    };
    (
        StatusCode::OK,
        Json(json!({
            "items": rows.into_iter().map(|(id, name, city, email, status, turnover)| json!({
                "id": id, "name": name, "city": city, "email": email, "status": status, "turnover_minor": turnover,
            })).collect::<Vec<_>>()
        })),
    )
        .into_response()
}

#[derive(Deserialize)]
pub struct ClubPatch {
    /// active | suspended | blocked (pending_verification только
    /// админом при верификации).
    pub status: Option<String>,
}

/// Блокировка/пауза/разблокировка клуба. Статусные поля, НЕ delete
/// (финансовые записи). Действует на новые сессии.
async fn admin_club_patch(
    ctx: AuthContext,
    State(state): State<ApiEnv>,
    Path(club_id): Path<Uuid>,
    Json(req): Json<ClubPatch>,
) -> Response {
    if ctx.role() != Role::Admin {
        return auth_error_response(ApiError::Forbidden);
    }
    let Ok(pool) = pool_of(&state) else {
        return pool_of(&state).unwrap_err();
    };
    if let Some(status) = &req.status {
        if !matches!(
            status.as_str(),
            "active" | "suspended" | "blocked" | "pending_verification"
        ) {
            return api_error(ApiError::ValidationError, "bad status");
        }
    }
    let updated =
        sqlx::query("UPDATE club SET status = COALESCE($2, status) WHERE id = $1 RETURNING id")
            .bind(club_id)
            .bind(&req.status)
            .fetch_optional(&pool)
            .await
            .map_err(|_| ());
    match updated {
        Ok(Some(_)) => {
            // Аудит: блокировка — административное действие.
            let _ = sqlx::query(
                "INSERT INTO audit_log (actor_type, actor_id, action, entity, diff) \
                 VALUES ('admin', $1, 'club.status_change', $2, $3)",
            )
            .bind(ctx.subject_id())
            .bind(club_id.to_string())
            .bind(json!({ "status": req.status }))
            .execute(&pool)
            .await;
            (
                StatusCode::OK,
                Json(json!({ "id": club_id, "status": req.status })),
            )
                .into_response()
        }
        Ok(None) => api_error(ApiError::NotFound, "club not found"),
        Err(_) => api_error(ApiError::Internal, "db error"),
    }
}

/// Допустимые значения `payment.status` — зеркало перечисления
/// `PaymentStatus` (`infra-db::billing`). Валидация фильтра `status` в
/// [`admin_payments`]: значение вне списка → `400 invalid_status`, чтобы
/// мусор не доходил до SQL и не маскировал опечатку как «нет строк».
const PAYMENT_STATUSES: &[&str] = &[
    "intent",
    "processing",
    "captured",
    "partially_refunded",
    "refunded",
    "failed",
    "cancelling",
    "cancelled",
];

#[derive(Deserialize)]
pub struct PaymentsParams {
    /// Фильтр по статусу платежа; валидируется по [`PAYMENT_STATUSES`].
    pub status: Option<String>,
    /// Фильтр по клубу (через `payment → request → session → club`):
    /// у `payment` нет колонки `club_id`, поэтому изоляция клуба — JOIN.
    pub club_id: Option<Uuid>,
    pub limit: Option<i64>,
}

/// Платежи (фильтры: статус, клуб) для админа.
///
/// Без параметров — прежнее поведение: последние платежи (`ORDER BY
/// created_at DESC`, `LIMIT`, дефолт 100, кламп 1..500). `status` и
/// `club_id` добавляются как `WHERE`-предикаты к тому же `ORDER BY`/`LIMIT`
/// (keyset/лимит-пагинация сохранена). `club_id` фильтрует через JOIN
/// `request → session` (INNER JOIN безопасен: FK-инвариант гарантирует,
/// что у каждого payment есть request и session — строк не теряется).
async fn admin_payments(
    ctx: AuthContext,
    State(state): State<ApiEnv>,
    Query(params): Query<PaymentsParams>,
) -> Response {
    if ctx.role() != Role::Admin {
        return auth_error_response(ApiError::Forbidden);
    }
    let Ok(pool) = pool_of(&state) else {
        return pool_of(&state).unwrap_err();
    };
    // Валидация статуса: мусор → 400 (до SQL). None — фильтр выключен.
    if let Some(status) = &params.status {
        if !PAYMENT_STATUSES.contains(&status.as_str()) {
            return crate::routes::api_error_with_code(
                StatusCode::BAD_REQUEST,
                "invalid_status",
                "The request failed validation.",
                "unknown payment status",
            );
        }
    }
    let rows = sqlx::query_as::<_, (Uuid, Uuid, i64, String, Option<String>, chrono::DateTime<chrono::Utc>)>(
            "SELECT p.id, p.request_id, p.amount_minor, p.status, p.stripe_payment_intent_id, p.created_at \
             FROM payment p \
             JOIN request r ON r.id = p.request_id \
             JOIN session s ON s.id = r.session_id \
             WHERE ($2::text IS NULL OR p.status = $2::text) \
               AND ($3::uuid IS NULL OR s.club_id = $3::uuid) \
             ORDER BY p.created_at DESC LIMIT $1",
        )
        .bind(params.limit.unwrap_or(100).clamp(1, 500))
        .bind(params.status.as_deref())
        .bind(params.club_id)
        .fetch_all(&pool)
        .await
        .map_err(|_| ());
    let Ok(rows) = rows else {
        return api_error(ApiError::Internal, "db error");
    };
    (
        StatusCode::OK,
        Json(json!({
            "items": rows.into_iter().map(|(id, request_id, amount, status, intent, created)| json!({
                "id": id, "request_id": request_id, "amount_minor": amount,
                "status": status, "stripe_payment_intent_id": intent, "created_at": created,
            })).collect::<Vec<_>>()
        })),
    )
        .into_response()
}

/// Сплиты платежа (разрез для админа).
async fn admin_payment_splits(
    ctx: AuthContext,
    State(state): State<ApiEnv>,
    Path(payment_id): Path<Uuid>,
) -> Response {
    if ctx.role() != Role::Admin {
        return auth_error_response(ApiError::Forbidden);
    }
    let Ok(pool) = pool_of(&state) else {
        return pool_of(&state).unwrap_err();
    };
    let rows = sqlx::query_as::<_, (String, Option<Uuid>, i64, i64, String)>(
        "SELECT payee_type, payee_id, amount_minor, reversed_minor, status \
         FROM split WHERE payment_id = $1",
    )
    .bind(payment_id)
    .fetch_all(&pool)
    .await
    .map_err(|_| ());
    let Ok(rows) = rows else {
        return api_error(ApiError::Internal, "db error");
    };
    (
        StatusCode::OK,
        Json(json!({
            "payment_id": payment_id,
            "items": rows.into_iter().map(|(t, id, amount, reversed, status)| json!({
                "payee_type": t, "payee_id": id, "amount_minor": amount,
                "reversed_minor": reversed, "status": status,
            })).collect::<Vec<_>>()
        })),
    )
        .into_response()
}

/// Аудит-лог (append-only; последние 100).
async fn admin_audit_log(ctx: AuthContext, State(state): State<ApiEnv>) -> Response {
    if ctx.role() != Role::Admin {
        return auth_error_response(ApiError::Forbidden);
    }
    let Ok(pool) = pool_of(&state) else {
        return pool_of(&state).unwrap_err();
    };
    let rows = sqlx::query_as::<
        _,
        (
            Uuid,
            String,
            Option<Uuid>,
            String,
            String,
            chrono::DateTime<chrono::Utc>,
        ),
    >(
        "SELECT id, actor_type, actor_id, action, entity, at FROM audit_log \
             ORDER BY at DESC LIMIT 100",
    )
    .fetch_all(&pool)
    .await
    .map_err(|_| ());
    let Ok(rows) = rows else {
        return api_error(ApiError::Internal, "db error");
    };
    (
        StatusCode::OK,
        Json(json!({
            "items": rows.into_iter().map(|(id, actor_type, actor_id, action, entity, at)| json!({
                "id": id,
                "actor_type": actor_type, "actor_id": actor_id,
                "action": action, "entity": entity, "at": at,
            })).collect::<Vec<_>>()
        })),
    )
        .into_response()
}

#[derive(Serialize, sqlx::FromRow)]
pub struct PlatformSettings {
    pub commission_mode: String,
    pub commission_percent_bp: Option<i32>,
    pub commission_fixed_minor: Option<i64>,
    pub min_price_minor: Option<i64>,
    pub max_price_minor: Option<i64>,
    pub dj_share_of_remainder_bp: Option<i32>,
    pub currency: String,
    /// Режим эксплуатации стенда (миграция 0016): `auto` | `test` |
    /// `maintenance` | `normal`. `auto` — режим выводится из фактов стенда.
    pub runtime_mode: String,
}

/// Текущие настройки платформы. NULL = «не заданы владельцем».
async fn admin_platform_settings(ctx: AuthContext, State(state): State<ApiEnv>) -> Response {
    if ctx.role() != Role::Admin {
        return auth_error_response(ApiError::Forbidden);
    }
    let Ok(pool) = pool_of(&state) else {
        return pool_of(&state).unwrap_err();
    };
    match sqlx::query_as::<_, PlatformSettings>(
        "SELECT commission_mode, commission_percent_bp, commission_fixed_minor, \
         min_price_minor, max_price_minor, dj_share_of_remainder_bp, currency, \
         runtime_mode \
         FROM platform_settings WHERE id = 1",
    )
    .fetch_one(&pool)
    .await
    {
        Ok(s) => (
            StatusCode::OK,
            Json(json!({
                "commission_mode": s.commission_mode,
                "commission_percent_bp": s.commission_percent_bp,
                "commission_fixed_minor": s.commission_fixed_minor,
                "min_price_minor": s.min_price_minor,
                "max_price_minor": s.max_price_minor,
                "dj_share_of_remainder_bp": s.dj_share_of_remainder_bp,
                "currency": s.currency,
                "runtime_mode": s.runtime_mode,
            })),
        )
            .into_response(),
        Err(_) => api_error(ApiError::Internal, "db error"),
    }
}

#[derive(Deserialize)]
pub struct PlatformSettingsPatch {
    pub commission_mode: Option<String>,
    pub commission_percent_bp: Option<i32>,
    pub commission_fixed_minor: Option<i64>,
    pub min_price_minor: Option<i64>,
    pub max_price_minor: Option<i64>,
    pub dj_share_of_remainder_bp: Option<i32>,
    /// Режим эксплуатации стенда: `auto` (адаптивно) | `test` |
    /// `maintenance` | `normal`. Валидируется по enum; `auto` возвращает
    /// разрешение режима фактам стенда.
    pub runtime_mode: Option<String>,
    /// false → сбросить в NULL («не задано»); валидация значений —
    /// только диапазоны, бизнес-значения выбирает владелец (Q11).
    #[serde(default)]
    pub clear_commission_percent_bp: bool,
    #[serde(default)]
    pub clear_commission_fixed_minor: bool,
    #[serde(default)]
    pub clear_min_price_minor: bool,
    #[serde(default)]
    pub clear_max_price_minor: bool,
    #[serde(default)]
    pub clear_dj_share_of_remainder_bp: bool,
}

/// Изменение настроек платформы (владелец/админ). Все проверки —
/// диапазоны; значения НЕ навязываются кодом (README правило 2).
/// Аудит обязателен (деньги).
async fn admin_platform_settings_patch(
    ctx: AuthContext,
    State(state): State<ApiEnv>,
    Json(req): Json<PlatformSettingsPatch>,
) -> Response {
    if ctx.role() != Role::Admin {
        return auth_error_response(ApiError::Forbidden);
    }
    let Ok(pool) = pool_of(&state) else {
        return pool_of(&state).unwrap_err();
    };
    if let Some(mode) = &req.commission_mode {
        if !matches!(mode.as_str(), "percent" | "fixed" | "unset") {
            return api_error(
                ApiError::ValidationError,
                "commission_mode: percent|fixed|unset",
            );
        }
    }
    if let Some(bp) = req.commission_percent_bp {
        if !(0..=10_000).contains(&bp) {
            return api_error(ApiError::ValidationError, "commission_percent_bp 0..10000");
        }
    }
    if let Some(bp) = req.dj_share_of_remainder_bp {
        if !(0..=10_000).contains(&bp) {
            return api_error(ApiError::ValidationError, "dj_share_bp 0..10000");
        }
    }
    if let Some(mode) = &req.runtime_mode {
        if !is_valid_runtime_mode(mode) {
            return api_error(
                ApiError::ValidationError,
                "runtime_mode: auto|test|maintenance|normal",
            );
        }
    }
    if let (Some(min), Some(max)) = (req.min_price_minor, req.max_price_minor) {
        if min >= max {
            return api_error(ApiError::ValidationError, "min_price >= max_price");
        }
    }
    // Сбросы в NULL идут CASE WHEN $clear THEN NULL.
    let res = sqlx::query(
        "UPDATE platform_settings SET \
         commission_mode = COALESCE($1::text, commission_mode), \
         commission_percent_bp = CASE WHEN $2::boolean THEN NULL ELSE COALESCE($3::int, commission_percent_bp) END, \
         commission_fixed_minor = CASE WHEN $4::boolean THEN NULL ELSE COALESCE($5::bigint, commission_fixed_minor) END, \
         min_price_minor = CASE WHEN $6::boolean THEN NULL ELSE COALESCE($7::bigint, min_price_minor) END, \
         max_price_minor = CASE WHEN $8::boolean THEN NULL ELSE COALESCE($9::bigint, max_price_minor) END, \
         dj_share_of_remainder_bp = CASE WHEN $10::boolean THEN NULL ELSE COALESCE($11::int, dj_share_of_remainder_bp) END, \
         runtime_mode = COALESCE($12::text, runtime_mode), \
         updated_at = now() \
         WHERE id = 1",
    )
    .bind(&req.commission_mode)
    .bind(req.clear_commission_percent_bp)
    .bind(req.commission_percent_bp)
    .bind(req.clear_commission_fixed_minor)
    .bind(req.commission_fixed_minor)
    .bind(req.clear_min_price_minor)
    .bind(req.min_price_minor)
    .bind(req.clear_max_price_minor)
    .bind(req.max_price_minor)
    .bind(req.clear_dj_share_of_remainder_bp)
    .bind(req.dj_share_of_remainder_bp)
    .bind(&req.runtime_mode)
    .execute(&pool)
    .await;
    if res.is_err() {
        return api_error(ApiError::Internal, "db error");
    }
    // Аудит.
    let _ = sqlx::query(
        "INSERT INTO audit_log (actor_type, actor_id, action, entity, diff) \
         VALUES ('admin', $1, 'platform_settings.update', 'platform_settings', $2)",
    )
    .bind(ctx.subject_id())
    .bind(json!({
        "commission_mode": req.commission_mode,
        "commission_percent_bp": req.commission_percent_bp,
        "commission_fixed_minor": req.commission_fixed_minor,
        "min_price_minor": req.min_price_minor,
        "max_price_minor": req.max_price_minor,
        "dj_share_of_remainder_bp": req.dj_share_of_remainder_bp,
        "runtime_mode": req.runtime_mode,
    }))
    .execute(&pool)
    .await;
    admin_platform_settings(ctx, State(state)).await
}

/// Список диджеев (админ).
async fn admin_djs(ctx: AuthContext, State(state): State<ApiEnv>) -> Response {
    if ctx.role() != Role::Admin {
        return auth_error_response(ApiError::Forbidden);
    }
    let Ok(pool) = pool_of(&state) else {
        return pool_of(&state).unwrap_err();
    };
    let rows = sqlx::query_as::<_, (Uuid, String, String, Option<Uuid>, String)>(
        "SELECT id, display_name, email, club_id, status FROM dj ORDER BY created_at DESC LIMIT 100",
    )
    .fetch_all(&pool)
    .await
    .map_err(|_| ());
    let Ok(rows) = rows else {
        return api_error(ApiError::Internal, "db error");
    };
    (
        StatusCode::OK,
        Json(json!({
            "items": rows.into_iter().map(|(id, name, email, club_id, status)| json!({
                "id": id, "display_name": name, "email": email, "club_id": club_id, "status": status,
            })).collect::<Vec<_>>()
        })),
    )
        .into_response()
}

// ------------------------------------------------------------
// Админ: создание клуба, гости, сверка (W2-C)
// ------------------------------------------------------------

/// Тело `POST /v1/admin/clubs`.
#[derive(Deserialize)]
pub struct ClubCreateRequest {
    pub name: String,
    pub city: String,
    pub timezone: String,
    pub address: Option<String>,
}

/// Создание клуба администратором платформы.
///
/// Клуб создаётся в статусе `pending_verification` (верификация — отдельный
/// шаг админа). `qr_master_token` — постоянный токен входа клуба (128 бит,
/// hex), в БД хранится только sha256-хеш ([`auth::hash_refresh_token`]);
/// сам токен отдаётся один раз. e-mail/slug генерируются системно
/// (уникальность UNIQUE-ограничений); пароля нет — вход клуба идёт по
/// master-токену. Невалидный IANA-timezone → 400 `invalid_timezone`.
async fn admin_clubs_create(
    ctx: AuthContext,
    State(state): State<ApiEnv>,
    Json(req): Json<ClubCreateRequest>,
) -> Response {
    if ctx.role() != Role::Admin {
        return auth_error_response(ApiError::Forbidden);
    }
    let Ok(pool) = pool_of(&state) else {
        return pool_of(&state).unwrap_err();
    };

    let name = req.name.trim();
    let city = req.city.trim();
    if !(2..=80).contains(&name.len()) {
        return api_error(ApiError::ValidationError, "name length 2..80");
    }
    if !(2..=60).contains(&city.len()) {
        return api_error(ApiError::ValidationError, "city length 2..60");
    }
    // IANA-timezone валидируется через chrono-tz::Tz::from_str.
    if req.timezone.parse::<chrono_tz::Tz>().is_err() {
        return api_error_custom(
            StatusCode::BAD_REQUEST,
            "invalid_timezone",
            "timezone must be a valid IANA zone",
            json!({ "timezone": req.timezone }),
        );
    }

    let club_id = Uuid::now_v7();
    // slug = slugify(name) + суффикс из club_id — глобальная уникальность.
    let slug = format!("{}-{:x}", slugify(name), club_id.as_simple());
    // Системный e-mail: клуб аутентифицируется master-токеном, не паролем;
    // placeholder удовлетворяет NOT NULL + UNIQUE.
    let email = format!("{club_id}@dropit.local");
    let master_token = auth::generate_refresh_token();
    let master_hash = auth::hash_refresh_token(&master_token);

    let inserted = sqlx::query(
        "INSERT INTO club \
         (id, name, slug, email, password_hash, address, city, timezone, \
          qr_master_token_hash, status) \
         VALUES ($1, $2, $3, $4, '!', $5, $6, $7, $8, 'pending_verification')",
    )
    .bind(club_id)
    .bind(name)
    .bind(&slug)
    .bind(&email)
    .bind(
        req.address
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty()),
    )
    .bind(city)
    .bind(&req.timezone)
    .bind(&master_hash)
    .execute(&pool)
    .await;
    if inserted.is_err() {
        return api_error(ApiError::Internal, "db error");
    }

    let _ = sqlx::query(
        "INSERT INTO audit_log (actor_type, actor_id, action, entity, diff) \
         VALUES ('admin', $1, 'club.create', $2, $3)",
    )
    .bind(ctx.subject_id())
    .bind(club_id.to_string())
    .bind(json!({ "name": name, "city": city, "timezone": req.timezone }))
    .execute(&pool)
    .await;

    (
        StatusCode::CREATED,
        Json(json!({
            "club_id": club_id,
            "name": name,
            "qr_master_token": master_token,
        })),
    )
        .into_response()
}

/// Query `GET /v1/admin/guests`. `club_id` обязателен.
#[derive(Deserialize)]
pub struct GuestsParams {
    pub club_id: Uuid,
    pub limit: Option<i64>,
    pub cursor: Option<String>,
}

/// Список гостей клуба для админ-панели (read + block).
///
/// Возвращаются только устройства, взаимодействовавшие с клубом (есть
/// грант `session_guest` или запрос в сессии этого клуба). Курсорная
/// пагинация keyset по `(first_seen DESC, id DESC)` — без OFFSET.
/// Неизвестный клуб → 404 `club_not_found` (не 403 — не раскрываем).
#[allow(clippy::type_complexity)]
async fn admin_guests(
    ctx: AuthContext,
    State(state): State<ApiEnv>,
    Query(params): Query<GuestsParams>,
) -> Response {
    if ctx.role() != Role::Admin {
        return auth_error_response(ApiError::Forbidden);
    }
    let Ok(pool) = pool_of(&state) else {
        return pool_of(&state).unwrap_err();
    };

    // club_id обязательный; неизвестный → 404.
    let club_exists: Option<(Uuid,)> = sqlx::query_as("SELECT id FROM club WHERE id = $1")
        .bind(params.club_id)
        .fetch_optional(&pool)
        .await
        .unwrap_or(None);
    if club_exists.is_none() {
        return api_error_custom(
            StatusCode::NOT_FOUND,
            "club_not_found",
            "club not found",
            json!({ "club_id": params.club_id }),
        );
    }

    let limit = params.limit.unwrap_or(50).clamp(1, 100);
    // Курсор: "<first_seen RFC3339>\x1f<id>". RFC3339 — детерминированный
    // roundtrip (Display DateTime<Utc> не гарантированно парсится FromStr).
    let (cur_seen, cur_id) = parse_cursor(params.cursor.as_deref());

    let rows: Result<
        Vec<(
            Uuid,
            Option<String>,
            Option<String>,
            DateTime<Utc>,
            i64,
            DateTime<Utc>,
            bool,
        )>,
        sqlx::Error,
    > = sqlx::query_as(
        "SELECT g.id, g.display_name, g.email, g.first_seen, \
         (SELECT count(*) FROM request r JOIN session s ON s.id = r.session_id \
          WHERE r.guest_device_id = g.id AND s.club_id = $1) AS requests_count, \
         g.last_seen, g.banned \
         FROM guest_device g \
         WHERE (EXISTS (SELECT 1 FROM session_guest sg JOIN session s ON s.id = sg.session_id \
                 WHERE sg.guest_device_id = g.id AND s.club_id = $1) \
               OR EXISTS (SELECT 1 FROM request r JOIN session s ON s.id = r.session_id \
                 WHERE r.guest_device_id = g.id AND s.club_id = $1)) \
         AND ($3::timestamptz IS NULL OR \
              (g.first_seen, g.id) < ($3::timestamptz, $4::uuid)) \
         ORDER BY g.first_seen DESC, g.id DESC \
         LIMIT $2",
    )
    .bind(params.club_id)
    .bind(limit + 1)
    .bind(cur_seen)
    .bind(cur_id)
    .fetch_all(&pool)
    .await;

    let Ok(rows) = rows else {
        return api_error(ApiError::Internal, "db error");
    };

    // keyset: курсор берётся из последнего ВОЗВРАЩАЕМОГО элемента
    // (limit-го), не из «лишнего» (limit+1)-го — он нужен только как
    // признак наличия следующей страницы.
    let next_cursor = make_cursor(&rows, limit, |r| (r.3, r.0));
    let items: Vec<_> = rows.into_iter().take(limit as usize).collect();

    (
        StatusCode::OK,
        Json(json!({
            "items": items.into_iter().map(|(id, name, email, created, reqs, last_seen, banned)| json!({
                "id": id,
                "display_name": name,
                "email": email,
                "created_at": created,
                "requests_count": reqs,
                "last_seen_at": last_seen,
                "banned": banned,
            })).collect::<Vec<_>>(),
            "next_cursor": next_cursor,
        })),
    )
        .into_response()
}

/// Query `GET /v1/admin/reconciliation-runs`.
#[derive(Deserialize)]
pub struct ReconRunsParams {
    pub limit: Option<i64>,
    pub cursor: Option<String>,
}

/// История прогонов сверки (ledger vs события).
///
/// Таблица `reconciliation_run` заведена миграцией 0006 — миграции 0008 не
/// требуется. Пустая история — валидный ответ (`items: []`). Статус БД
/// (`ok`/`discrepancies`/`failed`/`running`) маппится в контракт
/// `ok`|`mismatch`|`failed`; `discrepancy_minor = actual_minor −
/// expected_minor` (null → 0). Курсорная пагинация keyset по
/// `(created_at DESC, id DESC)`.
#[allow(clippy::type_complexity)]
async fn admin_reconciliation_runs(
    ctx: AuthContext,
    State(state): State<ApiEnv>,
    Query(params): Query<ReconRunsParams>,
) -> Response {
    if ctx.role() != Role::Admin {
        return auth_error_response(ApiError::Forbidden);
    }
    let Ok(pool) = pool_of(&state) else {
        return pool_of(&state).unwrap_err();
    };

    let limit = params.limit.unwrap_or(50).clamp(1, 50);
    let (cur_created, cur_id) = parse_cursor(params.cursor.as_deref());

    let rows: Result<
        Vec<(
            Uuid,
            DateTime<Utc>,
            DateTime<Utc>,
            String,
            Option<i64>,
            Option<i64>,
            DateTime<Utc>,
        )>,
        sqlx::Error,
    > = sqlx::query_as(
        "SELECT id, period_start, period_end, status, expected_minor, actual_minor, created_at \
         FROM reconciliation_run \
         WHERE ($2::timestamptz IS NULL OR (created_at, id) < ($2::timestamptz, $3::uuid)) \
         ORDER BY created_at DESC, id DESC \
         LIMIT $1",
    )
    .bind(limit + 1)
    .bind(cur_created)
    .bind(cur_id)
    .fetch_all(&pool)
    .await;

    let Ok(rows) = rows else {
        return api_error(ApiError::Internal, "db error");
    };

    let next_cursor = make_cursor(&rows, limit, |r| (r.6, r.0));
    let items: Vec<_> = rows.into_iter().take(limit as usize).collect();

    (
        StatusCode::OK,
        Json(json!({
            "items": items.into_iter().map(|(id, p_start, p_end, status, expected, actual, created)| {
                let expected_minor = expected.unwrap_or(0);
                let actual_minor = actual.unwrap_or(0);
                json!({
                    "id": id,
                    "period_start": p_start,
                    "period_end": p_end,
                    "status": map_recon_status(&status),
                    "expected_minor": expected_minor,
                    "actual_minor": actual_minor,
                    "discrepancy_minor": actual_minor - expected_minor,
                    "created_at": created,
                })
            }).collect::<Vec<_>>(),
            "next_cursor": next_cursor,
        })),
    )
        .into_response()
}

/// Маппинг статуса БД → контракт `ok`|`mismatch`|`failed`.
fn map_recon_status(db_status: &str) -> &'static str {
    match db_status {
        "ok" => "ok",
        "discrepancies" => "mismatch",
        "failed" => "failed",
        _ => "mismatch",
    }
}

/// Разбор курсора `"<RFC3339>\x1f<uuid>"` в пару `(timestamp, id)`.
/// Невалидный курсор → `(None, None)` (страница без фильтра).
fn parse_cursor(c: Option<&str>) -> (Option<DateTime<Utc>>, Option<Uuid>) {
    let Some(c) = c else {
        return (None, None);
    };
    let Some((ts, id)) = c.rsplit_once('\x1f') else {
        return (None, None);
    };
    let ts = DateTime::parse_from_rfc3339(ts)
        .ok()
        .map(|dt| dt.with_timezone(&Utc));
    let id = id.parse::<Uuid>().ok();
    match (ts, id) {
        (Some(t), Some(i)) => (Some(t), Some(i)),
        _ => (None, None),
    }
}

/// Строит next_cursor из последнего возвращённого элемента (limit-го),
/// если всего выбрано больше limit (есть следующая страница). Курсор
/// кодируется как `"<RFC3339>\x1f<uuid>"`.
fn make_cursor<T, F>(rows: &[T], limit: i64, pick: F) -> Option<String>
where
    F: Fn(&T) -> (DateTime<Utc>, Uuid),
{
    if rows.len() as i64 > limit {
        let last = &rows[(limit - 1) as usize];
        let (ts, id) = pick(last);
        Some(format!("{}\x1f{}", ts.to_rfc3339(), id))
    } else {
        None
    }
}

// ============================================================
// DJ
// ============================================================

/// Текущая смена DJ (сессия).
async fn dj_shift(ctx: AuthContext, State(state): State<ApiEnv>) -> Response {
    if ctx.role() != Role::Dj {
        return auth_error_response(ApiError::Forbidden);
    }
    let Ok(pool) = pool_of(&state) else {
        return pool_of(&state).unwrap_err();
    };
    let row: Option<(Uuid, String, String, Option<Uuid>)> = sqlx::query_as(
        "SELECT s.id, s.status, s.timezone, s.club_id FROM session s \
         WHERE s.dj_id = $1 AND s.status <> 'closed' ORDER BY s.created_at DESC LIMIT 1",
    )
    .bind(ctx.subject_id())
    .fetch_optional(&pool)
    .await
    .unwrap_or(None);
    match row {
        Some((id, status, timezone, club_id)) => (
            StatusCode::OK,
            Json(json!({ "session": { "id": id, "status": status, "timezone": timezone, "club_id": club_id } })),
        )
            .into_response(),
        None => (
            StatusCode::OK,
            Json(json!({ "session": None::<serde_json::Value> })),
        )
            .into_response(),
    }
}

/// Статистика вечера DJ (dizayn-zadanie §1.7).
async fn dj_stats_today(ctx: AuthContext, State(state): State<ApiEnv>) -> Response {
    if ctx.role() != Role::Dj {
        return auth_error_response(ApiError::Forbidden);
    }
    let Ok(pool) = pool_of(&state) else {
        return pool_of(&state).unwrap_err();
    };
    let row = sqlx::query_as::<_, (i64, i64, i64, i64, i64)>(
        "SELECT \
         (SELECT count(*) FROM request r JOIN session s ON s.id = r.session_id \
          WHERE s.dj_id = $1 AND r.created_at > now() - interval '24 hours'), \
         (SELECT count(*) FROM request r JOIN session s ON s.id = r.session_id \
          WHERE s.dj_id = $1 AND r.status = 'accepted' \
            AND r.created_at > now() - interval '24 hours'), \
         (SELECT count(*) FROM request r JOIN session s ON s.id = r.session_id \
          WHERE s.dj_id = $1 AND r.status = 'played' \
            AND r.created_at > now() - interval '24 hours'), \
         (SELECT count(*) FROM request r JOIN session s ON s.id = r.session_id \
          WHERE s.dj_id = $1 AND r.status = 'rejected' \
            AND r.created_at > now() - interval '24 hours'), \
         (SELECT COALESCE(sum(sp.amount_minor - sp.reversed_minor), 0)::bigint FROM split sp \
          WHERE sp.payee_type = 'dj' AND sp.payee_id = $1)",
    )
    .bind(ctx.subject_id())
    .fetch_one(&pool)
    .await
    .map_err(|_| ());
    let Ok((requests, accepted, played, rejected, earned)) = row else {
        return api_error(ApiError::Internal, "db error");
    };
    (
        StatusCode::OK,
        Json(json!({
            "requests_today": requests,
            "accepted": accepted,
            "played": played,
            "rejected": rejected,
            "earned_total_minor": earned,
        })),
    )
        .into_response()
}
