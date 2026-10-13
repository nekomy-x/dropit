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
use dropit_common::{ApiError, Role};
use dropit_domain::RequestAction;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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

#[derive(Serialize)]
pub struct WsTicketResponse {
    pub ticket: String,
    /// TTL билета, сек (30).
    pub expires_in: i64,
}

// ============================================================
// Router
// ============================================================

pub fn requests_router() -> Router<ApiEnv> {
    Router::new()
        .route("/v1/requests", post(create_request))
        .route("/v1/my/requests", get(my_requests))
        .route("/v1/dj/queue", get(dj_queue))
        .route("/v1/dj/requests/:id/accept", post(dj_accept))
        .route("/v1/dj/requests/:id/reject", post(dj_reject))
        .route("/v1/dj/requests/:id/played", post(dj_played))
        .route("/v1/dj/acceptance/pause", post(dj_pause))
        .route("/v1/dj/acceptance/resume", post(dj_resume))
        .route("/v1/ws/ticket", post(ws_ticket))
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
            let response_body = serde_json::json!({
                "request_id": row.id,
                "status": row.status,
                "session_id": row.session_id,
                "queue_position": None::<i64>,
            });
            // Сохраняем replay-ответ (после успешного TX).
            let _ = requests
                .save_idempotency_response(&idem_key, guest_device_id, row.id, &response_body)
                .await;
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
        Ok((row, noop)) => (
            StatusCode::OK,
            Json(ActionResultResponse {
                request_id: row.id,
                status: row.status,
                noop,
            }),
        )
            .into_response(),
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
        Ok(s) => (
            StatusCode::OK,
            Json(serde_json::json!({ "session_id": s.id, "status": s.status })),
        )
            .into_response(),
        Err(_) => api_error(ApiError::Conflict, "invalid session transition"),
    }
}

// ============================================================
// WS-ticket: одноразовый, TTL 30 с (ADR-005, VII)
// ============================================================

/// Хранилище билетов. In-memory для F4-скелета: Redis — с F5/F8
/// (multi-node). Одноразовость: GETDEL-семантика при использовании.
static WS_TICKETS: std::sync::OnceLock<
    std::sync::Mutex<HashMap<String, (Uuid, std::time::Instant)>>,
> = std::sync::OnceLock::new();

fn ws_tickets() -> &'static std::sync::Mutex<HashMap<String, (Uuid, std::time::Instant)>> {
    WS_TICKETS.get_or_init(|| std::sync::Mutex::new(HashMap::new()))
}

/// TTL билета (сек).
pub const WS_TICKET_TTL_SECS: u64 = 30;

/// Выдаёт билет любому аутентифицированному субъекту (JWT).
async fn ws_ticket(ctx: AuthContext) -> Response {
    // Билет: 128 бит hex, привязан к subject+role.
    let mut bytes = [0u8; 16];
    rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut bytes);
    let ticket = hex::encode(bytes);
    ws_tickets().lock().expect("ws tickets lock").insert(
        ticket.clone(),
        (ctx.subject_id(), std::time::Instant::now()),
    );
    (
        StatusCode::OK,
        Json(WsTicketResponse {
            ticket,
            expires_in: WS_TICKET_TTL_SECS as i64,
        }),
    )
        .into_response()
}

/// Проверяет и ПОГЛОЩАЕТ билет (одноразовый). Возвращает subject.
/// Потребитель — gateway-ws бинарник (ADR-005) и e2e-тест ниже.
#[allow(dead_code)]
pub(crate) fn consume_ws_ticket(ticket: &str) -> Option<Uuid> {
    let mut store = ws_tickets().lock().expect("ws tickets lock");
    match store.remove(ticket) {
        Some((sub, at)) => {
            if at.elapsed().as_secs() <= WS_TICKET_TTL_SECS {
                Some(sub)
            } else {
                None // истёк
            }
        }
        None => None, // неизвестный или уже использованный
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
