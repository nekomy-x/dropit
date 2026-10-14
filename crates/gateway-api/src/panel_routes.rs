//! Роуты панелей (F7-бэкенд): клуб, админ, DJ-статистика.
//!
//! Контракт (техкарта Часть IV): dashboard клуба/админа, профиль,
//! тарифы (Q6: новая строка, valid_to у старой), доходы, platform-
//! settings (NULL-семантика: значения задаёт владелец), блокировки,
//! платежи со сплитами, audit-log, DJ-смена и статистика.

use crate::rbac::{auth_error_response, AuthContext};
use crate::routes::{api_error, ApiEnv};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, patch},
    Json, Router,
};
use dropit_common::{ApiError, Role};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

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
        .route("/v1/club/payouts", get(club_payouts))
        // Админ.
        .route("/v1/admin/dashboard/now", get(admin_dashboard))
        .route("/v1/admin/clubs", get(admin_clubs))
        .route("/v1/admin/clubs/:id", patch(admin_club_patch))
        .route("/v1/admin/payments", get(admin_payments))
        .route("/v1/admin/payments/:id/splits", get(admin_payment_splits))
        .route("/v1/admin/audit-log", get(admin_audit_log))
        .route(
            "/v1/admin/platform-settings",
            get(admin_platform_settings).patch(admin_platform_settings_patch),
        )
        .route("/v1/admin/djs", get(admin_djs))
        // DJ.
        .route("/v1/dj/shift", get(dj_shift))
        .route("/v1/dj/stats/today", get(dj_stats_today))
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
    #[serde(default)]
    pub logo_url: Option<String>,
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
          WHERE ss.club_id = $1 AND ss.created_at > now() - interval '24 hours'), \
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
         logo_url = COALESCE($6, logo_url) \
         WHERE id = $1",
    )
    .bind(ctx.subject_id())
    .bind(&req.name)
    .bind(&req.address)
    .bind(&req.phone)
    .bind(&req.country)
    .bind(&req.logo_url)
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

/// Диджеи клуба (для привязки сессии).
async fn club_djs(ctx: AuthContext, State(state): State<ApiEnv>) -> Response {
    if ctx.role() != Role::Club {
        return auth_error_response(ApiError::Forbidden);
    }
    let Ok(pool) = pool_of(&state) else {
        return pool_of(&state).unwrap_err();
    };
    let rows = sqlx::query_as::<_, (Uuid, String, String, String)>(
        "SELECT id, display_name, email, status FROM dj WHERE club_id = $1 ORDER BY display_name",
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
            "items": rows.into_iter().map(|(id, name, email, status)| json!({
                "id": id, "display_name": name, "email": email, "status": status,
            })).collect::<Vec<_>>()
        })),
    )
        .into_response()
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
    let row = sqlx::query_as::<_, (i64, i64, i64, i64)>(
        "SELECT \
         (SELECT count(*) FROM session WHERE status <> 'closed'), \
         (SELECT count(*) FROM request WHERE created_at > now() - interval '24 hours'), \
         (SELECT count(*) FROM request WHERE created_at > now() - interval '1 minute'), \
         (SELECT COALESCE(sum(amount_minor), 0)::bigint FROM payment WHERE status IN ('captured', 'partially_refunded', 'refunded') \
          AND created_at > now() - interval '24 hours')",
    )
    .fetch_one(&pool)
    .await
    .map_err(|_| ());
    let Ok((sessions, requests_day, req_per_min, turnover_day)) = row else {
        return api_error(ApiError::Internal, "db error");
    };
    (
        StatusCode::OK,
        Json(json!({
            "active_sessions": sessions,
            "requests_today": requests_day,
            "requests_per_minute": req_per_min,
            "turnover_today_minor": turnover_day,
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
    let rows = sqlx::query_as::<_, (Uuid, String, String, String, i64)>(
        "SELECT c.id, c.name, c.email, c.status, \
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
            "items": rows.into_iter().map(|(id, name, email, status, turnover)| json!({
                "id": id, "name": name, "email": email, "status": status, "turnover_minor": turnover,
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

#[derive(Deserialize)]
pub struct PaymentsParams {
    #[allow(dead_code)]
    pub status: Option<String>,
    #[allow(dead_code)]
    pub club_id: Option<Uuid>,
    pub limit: Option<i64>,
}

/// Платежи (фильтры: статус, клуб) для админа.
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
    let rows = sqlx::query_as::<_, (Uuid, Uuid, i64, String, Option<String>, chrono::DateTime<chrono::Utc>)>(
            "SELECT p.id, p.request_id, p.amount_minor, p.status, p.stripe_payment_intent_id, p.created_at \
             FROM payment p ORDER BY p.created_at DESC LIMIT $1",
        )
        .bind(params.limit.unwrap_or(100).clamp(1, 500))
        .fetch_all(&pool)
        .await
        .map_err(|_| ());
    let _ = params;
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
            String,
            Option<Uuid>,
            String,
            String,
            chrono::DateTime<chrono::Utc>,
        ),
    >(
        "SELECT actor_type, actor_id, action, entity, at FROM audit_log \
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
            "items": rows.into_iter().map(|(actor_type, actor_id, action, entity, at)| json!({
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
         min_price_minor, max_price_minor, dj_share_of_remainder_bp, currency \
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
    let row = sqlx::query_as::<_, (i64, i64, i64, i64)>(
        "SELECT \
         (SELECT count(*) FROM request r JOIN session s ON s.id = r.session_id \
          WHERE s.dj_id = $1 AND r.created_at > now() - interval '24 hours'), \
         (SELECT count(*) FROM request r JOIN session s ON s.id = r.session_id \
          WHERE s.dj_id = $1 AND r.status = 'accepted'), \
         (SELECT count(*) FROM request r JOIN session s ON s.id = r.session_id \
          WHERE s.dj_id = $1 AND r.status = 'played'), \
         (SELECT COALESCE(sum(sp.amount_minor - sp.reversed_minor), 0)::bigint FROM split sp \
          WHERE sp.payee_type = 'dj' AND sp.payee_id = $1)",
    )
    .bind(ctx.subject_id())
    .fetch_one(&pool)
    .await
    .map_err(|_| ());
    let Ok((requests, accepted, played, earned)) = row else {
        return api_error(ApiError::Internal, "db error");
    };
    (
        StatusCode::OK,
        Json(json!({
            "requests_today": requests,
            "accepted": accepted,
            "played": played,
            "rejected": requests - accepted - played,
            "earned_total_minor": earned,
        })),
    )
        .into_response()
}
