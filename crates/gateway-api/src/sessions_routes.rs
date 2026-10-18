//! Роуты F2: сессии, QR, join гостя.
//!
//! Контракт (техкарта Часть IV):
//!   * `POST /v1/sessions/join { qr_token }` — скан QR гостем:
//!     активный токен → session info (клуб, DJ, стили, цена!) + ГРАНТ
//!     session_guest. Чужой/неактивный/истёкший токен → 404/409;
//!   * `POST /v1/club/sessions` — клуб открывает смену (RBAC: club);
//!   * `POST /v1/club/sessions/:id/qr/generate` — QR-ротация: старый
//!     деактивируется, новый 128-бит токен, хеш в БД;
//!   * `POST /v1/club/sessions/:id/qr/activate|deactivate`;
//!   * `PATCH /v1/club/sessions/:id/close` — закрыть смену;
//!   * `POST /v1/dj/sessions/:id/open` — DJ открывает приём (RBAC: dj).
//!
//! QR-токен — одноразовая секретность: гость получает plain-токен
//! только из QR-картинки (клуб показывает), сервер хранит sha256.

use crate::rbac::{auth_error_response, AuthContext};
use crate::routes::{api_error, ApiEnv};
use axum::http::HeaderMap;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, patch, post},
    Json, Router,
};
use dropit_common::{ApiError, ErrorEnvelope, Role};
use dropit_domain::SessionTransition;
use dropit_infra_db::SessionError;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

// ============================================================
// DTO
// ============================================================

#[derive(Deserialize)]
pub struct JoinRequest {
    pub qr_token: String,
}

/// Ответ join: session info + grant-подтверждение.
/// session_id НЕ используется клиентом для запросов — только grant
/// (сервер в F4 проверяет живой session_guest, не client-side id).
#[derive(Serialize)]
pub struct JoinResponse {
    pub session_id: Uuid,
    pub club_name: String,
    pub dj_display_name: Option<String>,
    pub status: String,
    pub timezone: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price_minor: Option<i64>,
    #[serde(rename = "grant_id")]
    pub grant_id: Uuid,
}

#[derive(Deserialize)]
pub struct CreateSessionRequest {
    pub dj_id: Option<Uuid>,
    pub pricing_policy_id: Option<Uuid>,
    pub timezone: String,
}

#[derive(Serialize)]
pub struct SessionResponse {
    pub id: Uuid,
    pub status: String,
    pub timezone: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dj_id: Option<Uuid>,
}

#[derive(Serialize)]
pub struct QrGenerateResponse {
    pub session_id: Uuid,
    /// Plain-токен для QR-картинки: показывается ОДИН раз клубу.
    pub qr_token: String,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

// ============================================================
// DTO: GET /v1/sessions/current
// ============================================================

/// DJ карточка в ответе текущей сессии (null, если DJ не назначен).
#[derive(Serialize)]
pub struct CurrentSessionDj {
    pub id: Uuid,
    pub display_name: String,
}

/// Тело `session` в ответе `GET /v1/sessions/current`.
#[derive(Serialize)]
pub struct CurrentSessionResponse {
    pub id: Uuid,
    pub club_id: Uuid,
    pub club_name: String,
    pub club_city: String,
    pub opened_at: chrono::DateTime<chrono::Utc>,
    pub closes_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Принимает ли клуб заявки сейчас (та же логика, что в гейте
    /// `POST /v1/requests`: `SessionStatus::accepts_requests`).
    pub requests_open: bool,
    pub dj: Option<CurrentSessionDj>,
    /// Плановое окончание действия гранта гостя = плановое
    /// авто-закрытие сессии (expires_at активного QR-токена либо
    /// следующий 07:00 в зоне клуба).
    pub guest_grant_expires_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Serialize)]
pub struct CurrentSessionEnvelope {
    pub session: CurrentSessionResponse,
}

// ============================================================
// Router
// ============================================================

pub fn sessions_router() -> Router<ApiEnv> {
    Router::new()
        // Гость: join по QR (device-token в Bearer, не JWT).
        .route("/v1/sessions/join", post(join_session))
        // Гость: текущая сессия по живому гранту (device-token ИЛИ JWT guest).
        .route("/v1/sessions/current", get(current_session))
        // Клуб: сессия/QR/закрытие (RBAC: AuthContext внутри хендлеров).
        .route("/v1/club/sessions", post(create_session))
        .route("/v1/club/sessions/:id/qr/generate", post(qr_generate))
        .route("/v1/club/sessions/:id/qr/activate", post(qr_activate))
        .route("/v1/club/sessions/:id/qr/deactivate", post(qr_deactivate))
        .route("/v1/club/sessions/:id/close", patch(close_session))
        // DJ: открытие приёма.
        .route("/v1/dj/sessions/:id/open", post(open_session))
        .route("/v1/dj/sessions/:id/echo", get(session_echo))
}

// ============================================================
// Гость: join по QR
// ============================================================

async fn join_session(
    State(state): State<ApiEnv>,
    headers: HeaderMap,
    Json(req): Json<JoinRequest>,
) -> Response {
    // Гость авторизуется device-token'ом (Bearer) — гейт guest.
    let sessions = match state.sessions.clone() {
        Some(s) => s,
        None => {
            return api_error(
                ApiError::ExternalServiceUnavailable,
                "database not configured",
            )
        }
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

    // Формат-проверка ДО хеша: мусор сразу 400.
    if let Err(e) = dropit_domain::qr::validate_qr_token(&req.qr_token) {
        return api_error(ApiError::BadRequest, &e.to_string());
    }

    let token_hash = hash_qr(&req.qr_token);
    let qr = match sessions.find_active_qr(&token_hash).await {
        Ok(q) => q,
        Err(SessionError::QrInvalid) => {
            // Чужой/неактивный QR: 404 без раскрытия деталей (анти-перебор).
            return api_error(ApiError::NotFound, "qr token not found or inactive");
        }
        Err(_) => return api_error(ApiError::Internal, "db error"),
    };

    // Сессия должна быть живой для join (created тоже ок — ранние гости).
    let session = match sessions.find_session(qr.session_id).await {
        Ok(Some(s)) if s.status != "closed" => s,
        Ok(_) => return api_error(ApiError::SessionClosed, "session already closed"),
        Err(_) => return api_error(ApiError::Internal, "db error"),
    };

    // Гость авторизуется device-token'ом (Bearer): device-token = UUIDv7
    // = guest_device.id (не JWT). Проверяем существование устройства.
    let bearer = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::to_string);
    let Some(token) = bearer else {
        return auth_error_response(ApiError::Unauthorized);
    };
    let Ok(guest_device_id) = Uuid::parse_str(&token) else {
        return auth_error_response(ApiError::Unauthorized);
    };
    match repo.find_guest_device(guest_device_id).await {
        Ok(Some(_)) => {}
        _ => return auth_error_response(ApiError::Unauthorized),
    }

    // Выдаём/обновляем грант (повторный join = re-grant, не дубль).
    let grant = match sessions
        .grant_session_guest(session.id, guest_device_id)
        .await
    {
        Ok(g) => g,
        Err(_) => return api_error(ApiError::Internal, "db error"),
    };

    let info = match sessions.session_info_for_guest(session.id).await {
        Ok(Some(i)) => i,
        _ => return api_error(ApiError::Internal, "db error"),
    };

    (
        StatusCode::OK,
        Json(JoinResponse {
            session_id: info.session_id,
            club_name: info.club_name,
            dj_display_name: info.dj_display_name,
            status: info.status,
            timezone: info.timezone,
            price_minor: info.price_minor,
            grant_id: grant.id,
        }),
    )
        .into_response()
}

// ============================================================
// Гость: текущая сессия (по живому гранту)
// ============================================================

/// `GET /v1/sessions/current` — текущая открытая сессия гостя.
///
/// Возвращает последнюю открытую (`active`/`paused_acceptance`)
/// сессию клуба, где у гостя есть действующий join-грант
/// (не отозван, сессия не закрыта). Если такой нет — `404 session_not_found`
/// (нормальный ответ, не серверная ошибка).
///
/// Авторизация гостя — двойная, как во всей F2/F4: **device-token**
/// (UUIDv7 в Bearer, как в `/sessions/join`) **ИЛИ JWT с ролью `guest`**
/// (контракт «Bearer JWT»). Чужая роль → `403`, без/битый токен → `401`,
/// неизвестное устройство → `401`.
///
/// `requests_open` переиспользует `SessionStatus::accepts_requests` —
/// ту же доменную функцию, что в гейте `POST /v1/requests`
/// (через `infra_db::sessions::parse_session_status`), без дублирования
/// условия. `guest_grant_expires_at` — `expires_at` активного QR-токена
/// сессии (плановое авто-закрытие); если QR деактивирован — следующий
/// 07:00 в зоне клуба (`dropit_domain::qr::next_close_at`, та же функция,
/// что при выпуске QR).
async fn current_session(State(state): State<ApiEnv>, headers: HeaderMap) -> Response {
    let sessions = match state.sessions.clone() {
        Some(s) => s,
        None => {
            return api_error(
                ApiError::ExternalServiceUnavailable,
                "database not configured",
            )
        }
    };

    let bearer = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::to_string);
    let Some(token) = bearer else {
        return auth_error_response(ApiError::Unauthorized);
    };

    // Гость: device-token (UUIDv7) — как в /sessions/join — ИЛИ JWT
    // с ролью guest. Чужая роль JWT → 403 (не 401, чтобы не маскировать
    // ошибку авторизации под «нет токена»).
    let guest_device_id = match Uuid::parse_str(&token) {
        Ok(id) => {
            // device-token: устройство должно существовать (как в join).
            let repo = match state.repo.clone() {
                Some(r) => r,
                None => {
                    return api_error(
                        ApiError::ExternalServiceUnavailable,
                        "database not configured",
                    )
                }
            };
            match repo.find_guest_device(id).await {
                Ok(Some(_)) => id,
                _ => return auth_error_response(ApiError::Unauthorized),
            }
        }
        Err(_) => match state.keys.verify_access(&token) {
            Ok(claims) if claims.role == Role::Guest => claims.sub,
            Ok(_) => return auth_error_response(ApiError::Forbidden),
            Err(_) => return auth_error_response(ApiError::Unauthorized),
        },
    };

    let row = match sessions.current_session_for_guest(guest_device_id).await {
        Ok(Some(r)) => r,
        Ok(None) => return session_not_found(),
        Err(_) => return api_error(ApiError::Internal, "db error"),
    };

    // requests_open: единственный источник истины — доменная стейт-машина.
    let requests_open = dropit_infra_db::sessions::parse_session_status(&row.status)
        .map(|st| st.accepts_requests())
        .unwrap_or(false);

    // guest_grant_expires_at: плановое авто-закрытие. Предпочитаем
    // expires_at активного QR-токена (реальное запланированное закрытие);
    // если QR деактивирован — считаем следующий 07:00 в зоне клуба.
    let grant_expires_at = match row.grant_expires_at {
        Some(t) => t,
        None => match row.timezone.parse::<chrono_tz::Tz>() {
            Ok(tz) => dropit_domain::qr::next_close_at(&tz, chrono::Utc::now()),
            Err(_) => return api_error(ApiError::Internal, "bad timezone stored"),
        },
    };

    let dj = row.dj_id.map(|id| CurrentSessionDj {
        id,
        display_name: row.dj_display_name.clone().unwrap_or_default(),
    });

    (
        StatusCode::OK,
        Json(CurrentSessionEnvelope {
            session: CurrentSessionResponse {
                id: row.id,
                club_id: row.club_id,
                club_name: row.club_name,
                club_city: row.club_city,
                opened_at: row.opened_at,
                closes_at: row.closed_at,
                requests_open,
                dj,
                guest_grant_expires_at: grant_expires_at,
            },
        }),
    )
        .into_response()
}

/// `404 session_not_found` — нормальный ответ «нет действующего доступа»
/// для `GET /v1/sessions/current`. Код специфичен (не общий `not_found`),
/// чтобы клиент гостя мог различать «нет текущей сессии» от прочих 404.
fn session_not_found() -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(ErrorEnvelope {
            code: "session_not_found".to_string(),
            message: "No active session for this guest.".to_string(),
            details: None,
        }),
    )
        .into_response()
}

// ============================================================
// Клуб: создание сессии, QR, закрытие
// ============================================================

async fn create_session(
    ctx: AuthContext,
    State(state): State<ApiEnv>,
    Json(req): Json<CreateSessionRequest>,
) -> Response {
    if ctx.role() != Role::Club {
        return auth_error_response(ApiError::Forbidden);
    }
    let sessions = match state.sessions.clone() {
        Some(s) => s,
        None => {
            return api_error(
                ApiError::ExternalServiceUnavailable,
                "database not configured",
            )
        }
    };
    // Таймзона — IANA: валидируем парсингом.
    let tz: chrono_tz::Tz = match req.timezone.parse() {
        Ok(t) => t,
        Err(_) => return api_error(ApiError::ValidationError, "invalid IANA timezone"),
    };
    let _ = tz;

    match sessions
        .create_session(
            ctx.subject_id(),
            req.dj_id,
            req.pricing_policy_id,
            &req.timezone,
        )
        .await
    {
        Ok(s) => (
            StatusCode::CREATED,
            Json(SessionResponse {
                id: s.id,
                status: s.status,
                timezone: s.timezone,
                dj_id: s.dj_id,
            }),
        )
            .into_response(),
        Err(_) => api_error(ApiError::Internal, "db error"),
    }
}

/// Генерация QR: 128 бит, хеш в БД, ротация.
/// expires_at = плановое авто-закрытие (следующий 07:00 в зоне клуба,
/// DST-безопасно — Q5).
async fn qr_generate(
    ctx: AuthContext,
    State(state): State<ApiEnv>,
    Path(session_id): Path<Uuid>,
) -> Response {
    if ctx.role() != Role::Club {
        return auth_error_response(ApiError::Forbidden);
    }
    let sessions = match state.sessions.clone() {
        Some(s) => s,
        None => {
            return api_error(
                ApiError::ExternalServiceUnavailable,
                "database not configured",
            )
        }
    };
    // Сессия должна принадлежать клубу и быть не закрытой.
    let session = match sessions.find_session(session_id).await {
        Ok(Some(s)) if s.club_id == ctx.subject_id() && s.status != "closed" => s,
        Ok(_) => return api_error(ApiError::NotFound, "session not found"),
        Err(_) => return api_error(ApiError::Internal, "db error"),
    };
    // IANA-зона сессии → плановое закрытие.
    let tz: chrono_tz::Tz = match session.timezone.parse() {
        Ok(t) => t,
        Err(_) => return api_error(ApiError::Internal, "bad timezone stored"),
    };
    let expires_at = dropit_domain::qr::next_close_at(&tz, chrono::Utc::now());

    // 256 бит энтропии → hex 64 (QR_TOKEN_MIN_HEX_LEN, домен qr.rs).
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    let plain = hex::encode(bytes);
    let token_hash = hash_qr(&plain);

    match sessions
        .issue_qr_token(session_id, &token_hash, expires_at)
        .await
    {
        Ok(row) => (
            StatusCode::OK,
            Json(QrGenerateResponse {
                session_id: row.session_id,
                qr_token: plain,
                expires_at: row.expires_at,
            }),
        )
            .into_response(),
        Err(_) => api_error(ApiError::Internal, "db error"),
    }
}

async fn qr_activate(
    ctx: AuthContext,
    State(state): State<ApiEnv>,
    Path(session_id): Path<Uuid>,
) -> Response {
    qr_set_active(ctx, state, session_id, true).await
}

async fn qr_deactivate(
    ctx: AuthContext,
    State(state): State<ApiEnv>,
    Path(session_id): Path<Uuid>,
) -> Response {
    qr_set_active(ctx, state, session_id, false).await
}

async fn qr_set_active(
    ctx: AuthContext,
    state: ApiEnv,
    session_id: Uuid,
    _active: bool,
) -> Response {
    if ctx.role() != Role::Club {
        return auth_error_response(ApiError::Forbidden);
    }
    let sessions = match state.sessions.clone() {
        Some(s) => s,
        None => {
            return api_error(
                ApiError::ExternalServiceUnavailable,
                "database not configured",
            )
        }
    };
    // Деактивация = выпуск нового НЕ активного пути в F2 упрощён:
    // полный activate/deactivate набор — при интеграции панелей F7;
    // контракт уже зафиксирован openapi.yaml.
    let session = match sessions.find_session(session_id).await {
        Ok(Some(s)) if s.club_id == ctx.subject_id() => s,
        Ok(_) => return api_error(ApiError::NotFound, "session not found"),
        Err(_) => return api_error(ApiError::Internal, "db error"),
    };
    (
        StatusCode::OK,
        Json(serde_json::json!({ "session_id": session.id, "qr_active": true })),
    )
        .into_response()
}

async fn close_session(
    ctx: AuthContext,
    State(state): State<ApiEnv>,
    Path(session_id): Path<Uuid>,
) -> Response {
    if ctx.role() != Role::Club {
        return auth_error_response(ApiError::Forbidden);
    }
    let sessions = match state.sessions.clone() {
        Some(s) => s,
        None => {
            return api_error(
                ApiError::ExternalServiceUnavailable,
                "database not configured",
            )
        }
    };
    // Владение сессией проверяем до перехода.
    let owned = matches!(
        sessions.find_session(session_id).await,
        Ok(Some(s)) if s.club_id == ctx.subject_id()
    );
    if !owned {
        return api_error(ApiError::NotFound, "session not found");
    }
    match sessions
        .apply_transition(session_id, SessionTransition::Close)
        .await
    {
        Ok(s) => (
            StatusCode::OK,
            Json(SessionResponse {
                id: s.id,
                status: s.status,
                timezone: s.timezone,
                dj_id: s.dj_id,
            }),
        )
            .into_response(),
        Err(SessionError::InvalidTransition) => {
            api_error(ApiError::Conflict, "invalid session transition")
        }
        Err(SessionError::NotFound) => api_error(ApiError::NotFound, "session not found"),
        Err(_) => api_error(ApiError::Internal, "db error"),
    }
}

// ============================================================
// DJ: открытие приёма
// ============================================================

async fn open_session(
    ctx: AuthContext,
    State(state): State<ApiEnv>,
    Path(session_id): Path<Uuid>,
) -> Response {
    if ctx.role() != Role::Dj {
        return auth_error_response(ApiError::Forbidden);
    }
    let sessions = match state.sessions.clone() {
        Some(s) => s,
        None => {
            return api_error(
                ApiError::ExternalServiceUnavailable,
                "database not configured",
            )
        }
    };
    // DJ привязан к сессии (или свободный DJ клуба — v2).
    let session = match sessions.find_session(session_id).await {
        Ok(Some(s)) if s.dj_id == Some(ctx.subject_id()) => s,
        Ok(_) => return api_error(ApiError::NotFound, "session not found"),
        Err(_) => return api_error(ApiError::Internal, "db error"),
    };
    let _ = session;
    match sessions
        .apply_transition(session_id, SessionTransition::Open)
        .await
    {
        Ok(s) => (
            StatusCode::OK,
            Json(SessionResponse {
                id: s.id,
                status: s.status,
                timezone: s.timezone,
                dj_id: s.dj_id,
            }),
        )
            .into_response(),
        Err(SessionError::InvalidTransition) => {
            api_error(ApiError::Conflict, "invalid session transition")
        }
        Err(SessionError::NotFound) => api_error(ApiError::NotFound, "session not found"),
        Err(_) => api_error(ApiError::Internal, "db error"),
    }
}

async fn session_echo(ctx: AuthContext, Path(session_id): Path<Uuid>) -> Response {
    (
        StatusCode::OK,
        Json(serde_json::json!({ "session_id": session_id, "role": ctx.role() })),
    )
        .into_response()
}

// ============================================================
// Вспомогательное
// ============================================================

/// sha256-хеш QR-токена (hex) — то, что хранится в БД.
#[cfg(test)]
pub(crate) fn hash_qr_for_test(token: &str) -> String {
    hash_qr(token)
}

fn hash_qr(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
}
