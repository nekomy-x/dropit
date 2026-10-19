//! Роуты F4: запросы гостя и очередь DJ (без денег).
//!
//! Контракт (техкарта Часть IV):
//!   * `POST /v1/requests { track_id, style?, Idempotency-Key }` —
//!     гейты: живой session_guest-грант + session.status == active
//!     (не клиентский session_id!); дубль ключа → replay;
//!   * `GET /v1/dj/queue?sort=new|hot` — очередь DJ с карточками
//!     треков и счётчиком «той же песни» (canonical-ключ);
//!   * `POST /v1/dj/requests/:id/accept | /reject | /played` —
//!     no-op-семантика дублей (не 500!);
//!   * `POST /v1/dj/acceptance/pause | /resume` — пауза блокирует
//!     POST /requests (критерий приёмки F4);
//!   * `POST /v1/ws/ticket` — одноразовый билет WS (TTL 30 с);
//!     JWT в query НЕ передаётся (утечки в логи Cloudflare/Loki).
//!
//! WS-соединения (gateway-ws бинарник, ADR-005) строятся поверх
//! этих билетов; в F4 скелет: выдача билета + poll-уведомления из
//! outbox. Источник истины — REST/БД.

use crate::rbac::{auth_error_response, AuthContext};
use crate::routes::{api_error, ApiEnv};
use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use dropit_common::{ApiError, ErrorEnvelope, Role};
use dropit_domain::RequestAction;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ============================================================
// DTO
// ============================================================

#[derive(Deserialize)]
pub struct CreateRequest {
    /// track_cache.id — трек, серверно-валидированный (F3).
    pub track_id: Uuid,
    pub style: Option<String>,
    /// Сессия НЕ на веру: сервер сам находит активную грант-сессию
    /// гостя; поле игнорируется (совместимость контракта).
    #[allow(dead_code)]
    pub session_id: Option<Uuid>,
}

#[derive(Serialize)]
pub struct RequestCreatedResponse {
    pub request_id: Uuid,
    pub status: String,
    #[serde(rename = "session_id")]
    pub session_id: Uuid,
    #[serde(rename = "queue_position")]
    pub queue_position: Option<i64>,
}

#[derive(Deserialize)]
pub struct QueueParams {
    pub sort: Option<String>,
}

#[derive(Serialize)]
pub struct QueueResponse {
    pub items: Vec<QueueItemDto>,
}

#[derive(Serialize)]
pub struct QueueItemDto {
    pub id: Uuid,
    pub status: String,
    pub style: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub title: String,
    pub artist: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cover_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub isrc: Option<String>,
    /// Счётчик запросов той же песни в сессии (canonical-ключ).
    pub same_track_count: i64,
}

#[derive(Deserialize)]
pub struct RejectRequest {
    pub reason: Option<String>,
}

#[derive(Serialize)]
pub struct ActionResultResponse {
    pub request_id: Uuid,
    pub status: String,
    pub noop: bool,
}

// ============================================================
// DTO: экран «моя заявка» (GET /v1/my/requests/:id)
// ============================================================

#[derive(Serialize)]
pub struct MyRequestDetailResponse {
    pub request: RequestDetailDto,
}

#[derive(Serialize)]
pub struct RequestDetailDto {
    pub id: Uuid,
    #[serde(rename = "session_id")]
    pub session_id: Uuid,
    pub status: String,
    pub track: TrackDto,
    /// Место в очереди (1 = первая). Только для pending; иначе null.
    pub position: Option<i64>,
    /// Всего активных заявок в сессии (pending + accepted).
    pub queue_size: i64,
    /// Сколько раз этот трек (canonical-ключ) заказан в сессии (включая эту).
    pub same_song_count: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub played_at: Option<DateTime<Utc>>,
    pub decline_reason: Option<String>,
}

#[derive(Serialize)]
pub struct TrackDto {
    pub title: String,
    pub artist: String,
    pub artwork_url: Option<String>,
}

// ============================================================
// DTO: апгрейд гостя (POST /v1/guest/upgrade)
// ============================================================

#[derive(Deserialize)]
pub struct UpgradeRequest {
    pub email: String,
}

#[derive(Serialize)]
pub struct UpgradeResponse {
    pub email: String,
    pub verified: bool,
    pub already_upgraded: bool,
}

// ============================================================
// Router
// ============================================================

pub fn requests_router() -> Router<ApiEnv> {
    Router::new()
        .route("/v1/requests", post(create_request))
        .route("/v1/my/requests", get(my_requests))
        .route("/v1/my/requests/:id", get(my_request_detail))
        .route("/v1/guest/upgrade", post(guest_upgrade))
        .route("/v1/dj/queue", get(dj_queue))
        .route("/v1/dj/requests/:id/accept", post(dj_accept))
        .route("/v1/dj/requests/:id/reject", post(dj_reject))
        .route("/v1/dj/requests/:id/played", post(dj_played))
        .route("/v1/dj/acceptance/pause", post(dj_pause))
        .route("/v1/dj/acceptance/resume", post(dj_resume))
}

// ============================================================
// Гость: создание запроса
// ============================================================

/// POST /v1/requests. Гейт: device-token гостя → живой грант →
/// active-сессия → INSERT + outbox. Idempotency-Key обязателен
/// (правило денег с первого дня, техкарта Часть IV).
async fn create_request(
    State(state): State<ApiEnv>,
    headers: HeaderMap,
    Json(req): Json<CreateRequest>,
) -> Response {
    let sessions = match state.sessions.clone() {
        Some(s) => s,
        None => {
            return api_error(
                ApiError::ExternalServiceUnavailable,
                "database not configured",
            )
        }
    };
    let requests = match state.requests.clone() {
        Some(r) => r,
        None => {
            return api_error(
                ApiError::ExternalServiceUnavailable,
                "database not configured",
            )
        }
    };

    // Гость: device-token (UUIDv7) в Bearer — как в /sessions/join.
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

    // Idempotency-Key обязателен.
    let idem_key = headers
        .get("idempotency-key")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let Some(idem_key) = idem_key else {
        return api_error(ApiError::ValidationError, "Idempotency-Key header required");
    };

    // Сессия: НЕ клиентская, а живая грант-сессия гостя.
    // (Если гость в нескольких сессиях — последняя активная; строго
    // одна в MVP.)
    let active_session = match sessions.find_active_session_of_guest(guest_device_id).await {
        Ok(s) => s,
        Err(_) => return api_error(ApiError::Internal, "db error"),
    };
    let Some(session) = active_session else {
        return api_error(ApiError::SessionClosed, "no active session for guest");
    };

    // Антифрод-грань (F8): лимит запросов/устройство/сессию.
    if let Some(jobs) = state.jobs.clone() {
        if let Some(verdict) = jobs
            .fraud_check_request(session.id, guest_device_id)
            .await
            .unwrap_or(None)
        {
            use dropit_domain::FraudVerdict::*;
            if let DenyDeviceRequestLimit { count, limit } = verdict {
                return (
                    StatusCode::TOO_MANY_REQUESTS,
                    Json(dropit_common::ErrorEnvelope {
                        code: "rate_limited".into(),
                        message: "Request limit for this session reached.".into(),
                        details: Some(serde_json::json!({ "count": count, "limit": limit })),
                    }),
                )
                    .into_response();
            }
        }
    }

    // Создание с гейтами (пауза/закрытие/грант — внутри repo).
    match requests
        .create_request(
            session.id,
            guest_device_id,
            req.track_id,
            req.style.as_deref(),
            &idem_key,
        )
        .await
    {
        // Создан.
        Ok(Ok(row)) => {
            // Идемпотентный ответ и outbox уже зафиксированы ВНУТРИ
            // транзакции create_request — здесь только best-effort
            // WS-уведомление (источник истины — REST/БД, ADR-005).
            if let Some(hub) = state.ws.clone() {
                // Название трека для карточки (кэш).
                let title = requests
                    .find_queue_item_title(row.track_cache_id)
                    .await
                    .unwrap_or_default()
                    .unwrap_or_default();
                let same = requests
                    .same_track_count(row.session_id, row.track_cache_id)
                    .await
                    .unwrap_or(1);
                hub.publish(crate::ws::events::request_created(
                    row.session_id,
                    row.id,
                    guest_device_id,
                    &title,
                    same,
                ));
            }
            (
                StatusCode::CREATED,
                Json(RequestCreatedResponse {
                    request_id: row.id,
                    status: row.status,
                    session_id: row.session_id,
                    queue_position: None,
                }),
            )
                .into_response()
        }
        // Дубль ключа → replay сохранённого ответа.
        Ok(Err(replay)) => (StatusCode::CREATED, Json(replay)).into_response(),
        // Пауза/закрытие сессии.
        Err(dropit_infra_db::RequestError::SessionNotActive) => (
            StatusCode::CONFLICT,
            Json(dropit_common::ErrorEnvelope {
                code: "session_paused".into(),
                message: "This session is not accepting requests right now.".into(),
                details: None,
            }),
        )
            .into_response(),
        Err(dropit_infra_db::RequestError::NoGrant) => {
            api_error(ApiError::Forbidden, "no live session grant")
        }
        Err(dropit_infra_db::RequestError::IdempotencyConflict) => {
            api_error(ApiError::Conflict, "idempotency key owned by another guest")
        }
        Err(_) => api_error(ApiError::Internal, "db error"),
    }
}

/// GET /v1/my/requests — история запросов гостя (статусы).
async fn my_requests(State(state): State<ApiEnv>, headers: HeaderMap) -> Response {
    let requests = match state.requests.clone() {
        Some(r) => r,
        None => {
            return api_error(
                ApiError::ExternalServiceUnavailable,
                "database not configured",
            )
        }
    };
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
    let Ok(guest_device_id) = Uuid::parse_str(&token) else {
        return auth_error_response(ApiError::Unauthorized);
    };
    let Some(session) = sessions
        .find_active_session_of_guest(guest_device_id)
        .await
        .unwrap_or(None)
    else {
        return (StatusCode::OK, Json(QueueResponse { items: vec![] })).into_response();
    };
    match requests.guest_requests(session.id, guest_device_id).await {
        Ok(items) => (
            StatusCode::OK,
            Json(QueueResponse {
                items: items.into_iter().map(dto_of).collect(),
            }),
        )
            .into_response(),
        Err(_) => api_error(ApiError::Internal, "db error"),
    }
}

// ============================================================
// Гость: экран «моя заявка» (GET /v1/my/requests/:id)
// ============================================================

/// Извлекает guest_device_id из Bearer. Поддерживает два формата:
/// (1) device-token — UUIDv7 (как в /sessions/join, /requests);
/// (2) гостевой JWT (Role::Guest, sub = guest_device_id).
/// Чужая роль JWT → 403; мусор/отсутствие → 401.
///
/// `Result<_, Response>` повторяет паттерн экстрактора `AuthContext`
/// (Rejection = Response); `result_large_err` подавляем осознанно —
/// ответ уже готов к возврату из хендлера без аллокации-обёртки.
#[allow(clippy::result_large_err)]
fn resolve_guest_device_id(
    headers: &HeaderMap,
    keys: &crate::auth::JwtKeys,
) -> Result<Uuid, Response> {
    let bearer = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::to_string);
    let Some(token) = bearer else {
        return Err(auth_error_response(ApiError::Unauthorized));
    };
    // (1) device-token — UUIDv7 (guest_device.id).
    if let Ok(id) = Uuid::parse_str(&token) {
        return Ok(id);
    }
    // (2) JWT: подпись → роль → Guest.
    match keys.verify_access(&token) {
        Ok(claims) if claims.role == Role::Guest => Ok(claims.sub),
        Ok(_) => Err(auth_error_response(ApiError::Forbidden)),
        Err(_) => Err(auth_error_response(ApiError::Unauthorized)),
    }
}

/// Кастомный конверт ошибки с заданным code (расширение поверх ApiError,
/// не новый тип). Используется для специфичных кодов: request_not_found,
/// email_taken, invalid_email.
fn envelope(status: StatusCode, code: &str, message: &str) -> Response {
    (
        status,
        Json(ErrorEnvelope {
            code: code.into(),
            message: message.into(),
            details: None,
        }),
    )
        .into_response()
}

/// GET /v1/my/requests/:id — карточка заявки гостя: статус, позиция в
/// очереди, размер очереди, счётчик той же песни. Заявка ищется в
/// активной сессии гостя: чужая заявка или заявка из другой сессии →
/// 404 request_not_found (без раскрытия существования).
async fn my_request_detail(
    State(state): State<ApiEnv>,
    headers: HeaderMap,
    Path(request_id): Path<Uuid>,
) -> Response {
    let requests = match state.requests.clone() {
        Some(r) => r,
        None => {
            return api_error(
                ApiError::ExternalServiceUnavailable,
                "database not configured",
            )
        }
    };
    let sessions = match state.sessions.clone() {
        Some(s) => s,
        None => {
            return api_error(
                ApiError::ExternalServiceUnavailable,
                "database not configured",
            )
        }
    };
    let guest_device_id = match resolve_guest_device_id(&headers, &state.keys) {
        Ok(id) => id,
        Err(r) => return r,
    };
    // Активная сессия гостя — та же, что в POST /requests и GET /my/requests.
    let active_session = match sessions.find_active_session_of_guest(guest_device_id).await {
        Ok(Some(s)) => s,
        Ok(None) => {
            return envelope(StatusCode::NOT_FOUND, "request_not_found", "Request not found.")
        }
        Err(_) => return api_error(ApiError::Internal, "db error"),
    };
    // Поиск в активной сессии + владение гостем.
    let detail = match requests
        .find_guest_request_detail(active_session.id, guest_device_id, request_id)
        .await
    {
        Ok(Some(d)) => d,
        Ok(None) => {
            return envelope(StatusCode::NOT_FOUND, "request_not_found", "Request not found.")
        }
        Err(_) => return api_error(ApiError::Internal, "db error"),
    };
    // Позиция — только для pending; иначе null.
    let position = if detail.status == "pending" {
        requests
            .active_queue_position(detail.session_id, request_id)
            .await
            .unwrap_or(None)
    } else {
        None
    };
    let queue_size = requests
        .active_queue_size(detail.session_id)
        .await
        .unwrap_or(0);
    let same_song_count = requests
        .same_track_count(detail.session_id, detail.track_cache_id)
        .await
        .unwrap_or(0);
    // updated_at: отдельной колонки нет (не в рамках пакета) —
    // используем played_at при наличии, иначе created_at.
    let updated_at = detail.played_at.unwrap_or(detail.created_at);
    (
        StatusCode::OK,
        Json(MyRequestDetailResponse {
            request: RequestDetailDto {
                id: detail.id,
                session_id: detail.session_id,
                status: detail.status,
                track: TrackDto {
                    title: detail.title,
                    artist: detail.artist,
                    artwork_url: detail.cover_url,
                },
                position,
                queue_size,
                same_song_count,
                created_at: detail.created_at,
                updated_at,
                played_at: detail.played_at,
                decline_reason: detail.reject_reason,
            },
        }),
    )
        .into_response()
}

// ============================================================
// Гость: апгрейд — привязка e-mail (POST /v1/guest/upgrade)
// ============================================================

/// Нормализует e-mail: trim + lowercase. Возвращает None, если значение
/// пустое, длиннее 320 символов, без @, без локальной части или без
/// домена с точкой.
fn normalize_email(raw: &str) -> Option<String> {
    let s = raw.trim();
    if s.is_empty() || s.len() > 320 {
        return None;
    }
    let lower = s.to_lowercase();
    let at = lower.find('@')?;
    let (local, rest) = lower.split_at(at);
    let domain = &rest[1..]; // пропустить '@'
    if local.is_empty() || domain.is_empty() {
        return None;
    }
    if !domain.contains('.') {
        return None;
    }
    if domain.starts_with('.') || domain.ends_with('.') {
        return None;
    }
    Some(lower)
}

/// POST /v1/guest/upgrade — привязка e-mail к гостевому профилю.
/// Идемпотентна: повторный вызов с тем же e-mail → 200 already_upgraded.
/// Занятый другим гостем e-mail → 409 email_taken. Невалидный → 400.
async fn guest_upgrade(
    State(state): State<ApiEnv>,
    headers: HeaderMap,
    Json(req): Json<UpgradeRequest>,
) -> Response {
    let requests = match state.requests.clone() {
        Some(r) => r,
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
    let guest_device_id = match resolve_guest_device_id(&headers, &state.keys) {
        Ok(id) => id,
        Err(r) => return r,
    };
    // Устройство должно существовать (write-операция на guest_device).
    match repo.find_guest_device(guest_device_id).await {
        Ok(Some(_)) => {}
        Ok(None) => return auth_error_response(ApiError::Unauthorized),
        Err(_) => return api_error(ApiError::Internal, "db error"),
    }
    let normalized = match normalize_email(&req.email) {
        Some(n) => n,
        None => {
            return envelope(
                StatusCode::BAD_REQUEST,
                "invalid_email",
                "Email is empty, too long, or missing @/domain.",
            )
        }
    };
    match requests
        .upgrade_guest_email(guest_device_id, &normalized, &normalized)
        .await
    {
        Ok(dropit_infra_db::requests::UpgradeOutcome::Bound {
            email,
            already_upgraded,
        }) => (
            StatusCode::OK,
            Json(UpgradeResponse {
                email,
                verified: false,
                already_upgraded,
            }),
        )
            .into_response(),
        Ok(dropit_infra_db::requests::UpgradeOutcome::EmailTaken) => envelope(
            StatusCode::CONFLICT,
            "email_taken",
            "Email already bound to another guest.",
        ),
        Err(dropit_infra_db::RequestError::Db(sqlx::Error::RowNotFound)) => {
            auth_error_response(ApiError::Unauthorized)
        }
        Err(_) => api_error(ApiError::Internal, "db error"),
    }
}

// ============================================================
// DJ: очередь и действия
// ============================================================

async fn dj_queue(
    ctx: AuthContext,
    State(state): State<ApiEnv>,
    Query(params): Query<QueueParams>,
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
    let requests = match state.requests.clone() {
        Some(r) => r,
        None => {
            return api_error(
                ApiError::ExternalServiceUnavailable,
                "database not configured",
            )
        }
    };
    // Текущая сессия DJ.
    let Some(session) = sessions
        .find_current_dj_session(ctx.subject_id())
        .await
        .unwrap_or(None)
    else {
        return (StatusCode::OK, Json(QueueResponse { items: vec![] })).into_response();
    };
    let sort = params.sort.as_deref().unwrap_or("new");
    match requests.dj_queue(session.id, sort).await {
        Ok(items) => (
            StatusCode::OK,
            Json(QueueResponse {
                items: items.into_iter().map(dto_of).collect(),
            }),
        )
            .into_response(),
        Err(_) => api_error(ApiError::Internal, "db error"),
    }
}

async fn dj_accept(
    ctx: AuthContext,
    State(state): State<ApiEnv>,
    Path(request_id): Path<Uuid>,
) -> Response {
    dj_action(ctx, state, request_id, RequestAction::Accept, None).await
}

async fn dj_reject(
    ctx: AuthContext,
    State(state): State<ApiEnv>,
    Path(request_id): Path<Uuid>,
    Json(body): Json<RejectRequest>,
) -> Response {
    dj_action(
        ctx,
        state,
        request_id,
        RequestAction::Reject,
        body.reason.as_deref(),
    )
    .await
}

async fn dj_played(
    ctx: AuthContext,
    State(state): State<ApiEnv>,
    Path(request_id): Path<Uuid>,
) -> Response {
    dj_action(ctx, state, request_id, RequestAction::Play, None).await
}

/// DJ-действие с no-op-семантикой: дубль — 200 + noop=true,
/// недопустимый переход — 409, чужой запрос — 404.
async fn dj_action(
    ctx: AuthContext,
    state: ApiEnv,
    request_id: Uuid,
    action: RequestAction,
    reject_reason: Option<&str>,
) -> Response {
    if ctx.role() != Role::Dj {
        return auth_error_response(ApiError::Forbidden);
    }
    let requests = match state.requests.clone() {
        Some(r) => r,
        None => {
            return api_error(
                ApiError::ExternalServiceUnavailable,
                "database not configured",
            )
        }
    };
    // Владение: запрос в сессии этого DJ.
    let row = match requests.find_request(request_id).await {
        Ok(Some(r)) => r,
        Ok(None) => return api_error(ApiError::NotFound, "request not found"),
        Err(_) => return api_error(ApiError::Internal, "db error"),
    };
    let sessions = match state.sessions.clone() {
        Some(s) => s,
        None => {
            return api_error(
                ApiError::ExternalServiceUnavailable,
                "database not configured",
            )
        }
    };
    let owned = matches!(
        sessions.find_session(row.session_id).await,
        Ok(Some(s)) if s.dj_id == Some(ctx.subject_id())
    );
    if !owned {
        return api_error(ApiError::NotFound, "request not found");
    }
    match requests
        .apply_action(request_id, action, reject_reason)
        .await
    {
        Ok((row, noop)) => {
            // WS-push: смена статуса (гость + очередь DJ), если не no-op.
            if !noop {
                if let Some(hub) = state.ws.clone() {
                    let ev = match action {
                        RequestAction::Accept => crate::ws::events::request_accepted(
                            row.session_id,
                            row.id,
                            row.guest_device_id,
                        ),
                        RequestAction::Reject => crate::ws::events::request_rejected(
                            row.session_id,
                            row.id,
                            row.guest_device_id,
                            row.reject_reason.as_deref(),
                        ),
                        RequestAction::Play => crate::ws::events::request_played(
                            row.session_id,
                            row.id,
                            row.guest_device_id,
                        ),
                        // Expire — системный переход (воркер F8), не
                        // HTTP-действие DJ: сюда не приходит.
                        RequestAction::Expire => crate::ws::events::request_rejected(
                            row.session_id,
                            row.id,
                            row.guest_device_id,
                            None,
                        ),
                    };
                    hub.publish(ev);
                }
            }
            (
                StatusCode::OK,
                Json(ActionResultResponse {
                    request_id: row.id,
                    status: row.status,
                    noop,
                }),
            )
                .into_response()
        }
        // Недопустимый переход (например accept после played).
        Err(_) => api_error(ApiError::Conflict, "invalid request transition"),
    }
}

// ============================================================
// Пауза приёма DJ (критерий приёмки F4: пауза блокирует POST)
// ============================================================

async fn dj_pause(ctx: AuthContext, State(state): State<ApiEnv>) -> Response {
    dj_acceptance(ctx, state, dropit_domain::SessionTransition::Pause).await
}

async fn dj_resume(ctx: AuthContext, State(state): State<ApiEnv>) -> Response {
    dj_acceptance(ctx, state, dropit_domain::SessionTransition::Resume).await
}

async fn dj_acceptance(
    ctx: AuthContext,
    state: ApiEnv,
    transition: dropit_domain::SessionTransition,
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
    let Some(session) = sessions
        .find_current_dj_session(ctx.subject_id())
        .await
        .unwrap_or(None)
    else {
        return api_error(ApiError::NotFound, "no active DJ session");
    };
    match sessions.apply_transition(session.id, transition).await {
        Ok(s) => {
            // WS-push: баннер паузы всем участникам сессии.
            if let Some(hub) = state.ws.clone() {
                let ev = if matches!(transition, dropit_domain::SessionTransition::Pause) {
                    crate::ws::events::session_paused(s.id)
                } else {
                    crate::ws::events::session_resumed(s.id)
                };
                hub.publish(ev);
            }
            (
                StatusCode::OK,
                Json(serde_json::json!({ "session_id": s.id, "status": s.status })),
            )
                .into_response()
        }
        Err(_) => api_error(ApiError::Conflict, "invalid session transition"),
    }
}

fn dto_of(item: dropit_infra_db::QueueItem) -> QueueItemDto {
    QueueItemDto {
        id: item.id,
        status: item.status,
        style: item.style,
        created_at: item.created_at,
        title: item.title,
        artist: item.artist,
        cover_url: item.cover_url,
        preview_url: item.preview_url,
        isrc: item.isrc,
        same_track_count: item.same_track_count,
    }
}
