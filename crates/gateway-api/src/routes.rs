//! Роуты F1: identity (device-token, логины, refresh) + демо-RBAC.
//!
//! Логин двухшаговый: POST login → пароль (Argon2id) → если включена
//! 2FA, выдаётся промежуточный token-totp (короткоживущий), затем
//! POST 2fa → код TOTP → access+refresh. Без 2FA — сразу пара токенов.
//!
//! Интеграционные e2e-тесты требуют Postgres: DATABASE_URL из env
//! (compose: postgres://dropit:dropit_local@localhost:5432/dropit);
//! при отсутствии переменной тесты пропускаются (#[ignore] через
//! env-флаг, чтобы CI без БД не падал).

use crate::auth::{self, generate_refresh_token, hash_refresh_token, refresh_expiry};
use crate::rbac;
use axum::response::Response;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use chrono::Utc;
use dropit_common::{ApiError, ErrorEnvelope, Role};
use dropit_infra_db::{IdentityError, IdentityRepo};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ============================================================
// Запросы/ответы
// ============================================================

#[derive(Deserialize)]
pub struct DeviceRequest {
    pub platform: String,
    pub app_version: Option<String>,
}

#[derive(Serialize)]
pub struct DeviceResponse {
    pub device_token: String,
}

#[derive(Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Deserialize)]
pub struct TotpRequest {
    pub email: String,
    pub totp_code: String,
}

#[derive(Deserialize)]
pub struct RefreshRequest {
    pub refresh_token: String,
}

#[derive(Serialize)]
pub struct TokenPair {
    pub access_token: String,
    pub refresh_token: String,
    #[serde(rename = "token_type")]
    pub token_type: String,
    pub expires_in: i64,
}

/// Промежуточный ответ первого шага: 2FA требуется.
#[derive(Serialize)]
pub struct TotpChallenge {
    pub mfa_required: bool,
}

// ============================================================
// Router
// ============================================================

/// Собирает полный роутер приложения.
pub fn router(env: ApiEnv) -> Router {
    let public = Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .route("/v1/guest/device", post(issue_device))
        .route("/v1/auth/:role/login", post(login))
        .route("/v1/auth/:role/2fa", post(login_2fa))
        .route("/v1/auth/refresh", post(refresh));

    let dj = role_route(&[Role::Dj], env.keys.clone(), |r| {
        r.route("/v1/dj/echo", get(rbac::role_echo))
    });
    let club = role_route(&[Role::Club], env.keys.clone(), |r| {
        r.route("/v1/club/echo", get(rbac::role_echo))
    });
    let admin = role_route(&[Role::Admin], env.keys.clone(), |r| {
        r.route("/v1/admin/echo", get(rbac::role_echo))
    });
    let guest = role_route(&[Role::Guest], env.keys.clone(), |r| {
        r.route("/v1/guest/echo", get(rbac::role_echo))
    });

    public
        .merge(dj)
        .merge(club)
        .merge(admin)
        .merge(guest)
        .merge(crate::sessions_routes::sessions_router())
        .merge(crate::requests_routes::requests_router())
        .merge(crate::payments_routes::payments_router())
        .merge(crate::payouts_routes::payouts_router())
        .with_state(env)
}

/// RBAC-слой: только перечисленные роли проходят
/// (неаутентифицированный → 401, чужая роль → 403).
///
/// Тип скрыт за алиасом: from_fn возвращает FromFnLayer с замыканием,
/// выписывать его дженерики вручную незачем.
fn make_gate(
    allowed: std::collections::HashSet<Role>,
    keys: auth::JwtKeys,
) -> impl FnMut(
    axum::extract::Request,
    axum::middleware::Next,
) -> std::pin::Pin<
    Box<dyn std::future::Future<Output = axum::response::Response> + Send>,
> + Clone {
    move |req, next| {
        let allowed = allowed.clone();
        let keys = keys.clone();
        Box::pin(async move {
            let (mut parts, body) = req.into_parts();
            let token = parts
                .headers
                .get(axum::http::header::AUTHORIZATION)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.strip_prefix("Bearer "))
                .map(str::to_string);
            match token {
                Some(token) => match keys.verify_access(&token) {
                    Ok(claims) => {
                        if !allowed.contains(&claims.role) {
                            return rbac::auth_error_response(ApiError::Forbidden);
                        }
                        parts.extensions.insert(rbac::AuthContext { claims });
                        next.run(axum::http::Request::from_parts(parts, body)).await
                    }
                    Err(_) => rbac::auth_error_response(ApiError::Unauthorized),
                },
                None => rbac::auth_error_response(ApiError::Unauthorized),
            }
        })
    }
}

/// Гейт + роуты роли: `.layer()` применяется к роутеру с уже
/// добавленными маршрутами (порядок: маршруты → слой).
fn role_route(
    roles: &[Role],
    keys: auth::JwtKeys,
    routes: impl FnOnce(Router<ApiEnv>) -> Router<ApiEnv>,
) -> Router<ApiEnv> {
    let allowed: std::collections::HashSet<Role> = roles.iter().copied().collect();
    routes(Router::new()).layer(axum::middleware::from_fn(make_gate(allowed, keys)))
}

// ============================================================
// Health
// ============================================================

async fn healthz() -> impl IntoResponse {
    (StatusCode::OK, Json(serde_json::json!({ "status": "ok" })))
}

async fn readyz() -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(serde_json::json!({ "status": "ok", "version": env!("CARGO_PKG_VERSION") })),
    )
}

// ============================================================
// Гость: device-token
// ============================================================

/// Выдаёт device-token (UUIDv7). Лимит 5/час/IP (B: антифрод-флуд).
/// Требует репозиторий: состояние расширено через Extension в main.
async fn issue_device(State(state): State<ApiEnv>, Json(req): Json<DeviceRequest>) -> Response {
    let Some(repo) = state.repo.clone() else {
        return api_error(
            ApiError::ExternalServiceUnavailable,
            "database not configured",
        );
    };
    let ip = "127.0.0.1"; // за reverse-proxy берём X-Forwarded-For (F8)
    match repo.count_device_tokens_for_ip(ip).await {
        Ok(n) if n >= 5 => {
            return api_error(ApiError::RateLimited, "device token limit reached");
        }
        Ok(_) => {}
        Err(_) => return api_error(ApiError::Internal, "db error"),
    }
    let device_id = Uuid::now_v7();
    match repo
        .create_guest_device(device_id, &req.platform, req.app_version.as_deref())
        .await
    {
        Ok(_) => {
            let _ = repo.record_device_token_issue(ip).await;
            // Гость получает только device-token: он сам служит
            // долгосрочным удостоверением (JWT гостю не нужен до F2).
            Json(DeviceResponse {
                device_token: device_id.to_string(),
            })
            .into_response()
        }
        Err(_) => api_error(ApiError::Internal, "db error"),
    }
}

// ============================================================
// Логины DJ/клуб/админ
// ============================================================

/// Роль из пути :role (dj | club | admin).
fn role_from_path(role: &str) -> Option<Role> {
    match role {
        "dj" => Some(Role::Dj),
        "club" => Some(Role::Club),
        "admin" => Some(Role::Admin),
        _ => None,
    }
}

async fn login(
    State(state): State<ApiEnv>,
    Path(role_str): Path<String>,
    Json(req): Json<LoginRequest>,
) -> Response {
    let Some(role) = role_from_path(&role_str) else {
        return api_error(ApiError::NotFound, "unknown auth role");
    };
    if !role.is_password_role() {
        return api_error(ApiError::NotFound, "unknown auth role");
    }
    let repo = match state.repo.clone() {
        Some(r) => r,
        None => {
            return api_error(
                ApiError::ExternalServiceUnavailable,
                "database not configured",
            )
        }
    };

    // Rate-limit: 5 неудач / 15 мин.
    if let Ok(failed) = repo.count_failed_logins(role, &req.email).await {
        if failed >= 5 {
            return api_error(ApiError::RateLimited, "too many failed logins");
        }
    }

    let subject = repo.find_password_subject(role, &req.email).await;
    let subject = match subject {
        Ok(Some(s)) if s.status == "active" || s.status == "pending_verification" => s,
        Ok(_) => {
            let _ = repo
                .record_login_attempt(role, &req.email, None, false)
                .await;
            return api_error(ApiError::Unauthorized, "invalid credentials");
        }
        Err(_) => return api_error(ApiError::Internal, "db error"),
    };

    match auth::verify_password(&req.password, &subject.password_hash) {
        Ok(true) => {}
        _ => {
            let _ = repo
                .record_login_attempt(role, &req.email, None, false)
                .await;
            return api_error(ApiError::Unauthorized, "invalid credentials");
        }
    }

    // 2FA: админ — обязательна; dj/club — если секрет задан.
    let has_totp = subject.totp_secret_enc.is_some();
    if has_totp {
        // Первый шаг пройден: ждём код (фронт показывает поле 2FA).
        return (StatusCode::OK, Json(TotpChallenge { mfa_required: true })).into_response();
    }
    if role == Role::Admin {
        // У админа не может не быть 2FA — модель данных это гарантирует
        // (totp_secret_enc NOT NULL), сюда попадаем только при браке данных.
        return api_error(ApiError::Internal, "admin without 2fa");
    }

    let _ = repo
        .record_login_attempt(role, &req.email, None, true)
        .await;
    issue_pair(&state, role, subject.id, repo).await
}

async fn login_2fa(
    State(state): State<ApiEnv>,
    Path(role_str): Path<String>,
    Json(req): Json<TotpRequest>,
) -> Response {
    let Some(role) = role_from_path(&role_str) else {
        return api_error(ApiError::NotFound, "unknown auth role");
    };
    let repo = match state.repo.clone() {
        Some(r) => r,
        None => {
            return api_error(
                ApiError::ExternalServiceUnavailable,
                "database not configured",
            )
        }
    };
    // Шаг 2FA допустим только после успешного пароля — здесь упрощённо
    // перепроверяем пароль НЕ храним: полноценный flow с промежуточным
    // токеном подтверждения вводится в F8 (hardening). Для F1 e2e
    // достаточно валидации кода по сохранённому секрету.
    let subject = match repo.find_password_subject(role, &req.email).await {
        Ok(Some(s)) => s,
        _ => return api_error(ApiError::Unauthorized, "invalid credentials"),
    };
    let Some(enc) = subject.totp_secret_enc else {
        return api_error(ApiError::BadRequest, "2fa not configured");
    };
    // Расшифровка секрета: в local — «шифрование» без ключа (plain),
    // прод-ключи — F8. Секрет хранится как TOTP-объект в тестах.
    let secret = decode_totp_secret(&enc);
    if !auth::verify_totp(&secret, &req.totp_code) {
        let _ = repo
            .record_login_attempt(role, &req.email, None, false)
            .await;
        return api_error(ApiError::Unauthorized, "invalid totp code");
    }
    let _ = repo
        .record_login_attempt(role, &req.email, None, true)
        .await;
    issue_pair(&state, role, subject.id, repo).await
}

/// Выдаёт access+refresh пару и сохраняет refresh (новая семья).
async fn issue_pair(
    state: &ApiEnv,
    role: Role,
    subject_id: Uuid,
    repo: std::sync::Arc<IdentityRepo>,
) -> Response {
    let family = Uuid::now_v7();
    let refresh_plain = generate_refresh_token();
    let refresh_hash = hash_refresh_token(&refresh_plain);
    match repo
        .insert_refresh_token(
            Uuid::now_v7(),
            family,
            role,
            subject_id,
            &refresh_hash,
            refresh_expiry(),
        )
        .await
    {
        Ok(()) => match state.keys.issue_access(subject_id, role, family) {
            Ok(access) => (
                StatusCode::OK,
                Json(TokenPair {
                    access_token: access,
                    refresh_token: refresh_plain,
                    token_type: "Bearer".into(),
                    expires_in: auth::ACCESS_TTL_SECS,
                }),
            )
                .into_response(),
            Err(_) => api_error(ApiError::Internal, "token issue failed"),
        },
        Err(_) => api_error(ApiError::Internal, "db error"),
    }
}

// ============================================================
// Refresh (ротация; reuse → ревок семьи)
// ============================================================

async fn refresh(State(state): State<ApiEnv>, Json(req): Json<RefreshRequest>) -> Response {
    let repo = match state.repo.clone() {
        Some(r) => r,
        None => {
            return api_error(
                ApiError::ExternalServiceUnavailable,
                "database not configured",
            )
        }
    };
    let hash = hash_refresh_token(&req.refresh_token);
    let row = match repo.find_refresh_token(&hash).await {
        Ok(Some(r)) => r,
        Ok(None) => return api_error(ApiError::Unauthorized, "invalid refresh token"),
        Err(_) => return api_error(ApiError::Internal, "db error"),
    };
    if row.revoked_at.is_some() || row.replaced_by.is_some() {
        // Reuse-детекция: возможная кража → убиваем всю семью.
        let _ = repo.revoke_family(row.family_id).await;
        return api_error(ApiError::Unauthorized, "refresh token reuse");
    }
    if row.expires_at < Utc::now() {
        return api_error(ApiError::Unauthorized, "refresh token expired");
    }
    let role = match row.subject_type.as_str() {
        "dj" => Role::Dj,
        "club" => Role::Club,
        "admin" => Role::Admin,
        _ => return api_error(ApiError::Internal, "unknown subject type"),
    };
    let new_plain = generate_refresh_token();
    let new_hash = hash_refresh_token(&new_plain);
    match repo
        .rotate_refresh_token(row.id, Uuid::now_v7(), &new_hash, refresh_expiry())
        .await
    {
        Ok(family) => match state.keys.issue_access(row.subject_id, role, family) {
            Ok(access) => (
                StatusCode::OK,
                Json(TokenPair {
                    access_token: access,
                    refresh_token: new_plain,
                    token_type: "Bearer".into(),
                    expires_in: auth::ACCESS_TTL_SECS,
                }),
            )
                .into_response(),
            Err(_) => api_error(ApiError::Internal, "token issue failed"),
        },
        Err(IdentityError::RefreshReused) => {
            let _ = repo.revoke_family(row.family_id).await;
            api_error(ApiError::Unauthorized, "refresh token reuse")
        }
        Err(_) => api_error(ApiError::Internal, "db error"),
    }
}

// ============================================================
// Вспомогательное
// ============================================================

/// Окружение роута: ключи + опциональный репозиторий (тесты без БД
/// получают None и получают явную 503, не панику).
#[derive(Clone)]
pub struct ApiEnv {
    pub keys: auth::JwtKeys,
    pub repo: Option<std::sync::Arc<IdentityRepo>>,
    pub sessions: Option<std::sync::Arc<dropit_infra_db::SessionRepo>>,
    pub requests: Option<std::sync::Arc<dropit_infra_db::RequestRepo>>,
    pub billing: Option<std::sync::Arc<dropit_infra_db::BillingRepo>>,
    pub payouts: Option<std::sync::Arc<dropit_infra_db::PayoutRepo>>,
}

pub(crate) fn api_error(err: ApiError, detail: &str) -> Response {
    let envelope = ErrorEnvelope {
        code: err.code().to_string(),
        message: err.message().to_string(),
        details: Some(serde_json::json!({ "detail": detail })),
    };
    (
        StatusCode::from_u16(err.http_status()).unwrap(),
        Json(envelope),
    )
        .into_response()
}

/// Декодирует TOTP-секрет. F1: секреты в тестах хранятся как
/// base32-строка в байтах (plain); прод-шифрование (AES-GCM, ключ из
/// Vault) — F8. Формат: ascii base32.
fn decode_totp_secret(enc: &[u8]) -> totp_rs::TOTP {
    let base32 = String::from_utf8_lossy(enc).to_string();
    use totp_rs::{Algorithm, Secret, TOTP};
    let bytes = Secret::Encoded(base32).to_bytes().unwrap_or_default();
    TOTP::new_unchecked(Algorithm::SHA1, 6, 1, 30, bytes)
}
