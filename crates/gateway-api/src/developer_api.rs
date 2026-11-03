//! Developer API v1 — публичная поверхность по API-ключу (ADR-010).
//!
//! Все маршруты живут под `/v1/developer/**` и проходят через
//! [`crate::api_key_auth::gate`]: ключ → скоупы → лимит → журнал.
//! Хендлеры обязаны начинать с проверки скоупа
//! (`principal.require(Scope::…)`), иначе запрос не должен трогать данные.
//!
//! Список ресурсов, скоупы и семантика ошибок — `docs/adr/ADR-010-developer-api-v1.md`
//! и `docs/api/openapi-developer.yaml`.
//!
//! Правила, которые держит этот модуль:
//!   * **deny by default**: первая строка обработчика данных — `require(скоуп)`;
//!   * **клуб — граница аренды**: каждый запрос фильтруется `principal.club_id`,
//!     чужой id отвечает `404` (не `403`), чтобы id нельзя было перебирать;
//!   * **минимизация**: наружу не уходят `guest_device_id`, e-mail гостя,
//!     Stripe-идентификаторы и реквизиты провайдера каталога;
//!   * **конверт ошибки** один — [`error_envelope`] (`{code,message,details}`);
//!   * **деньги только в минорных единицах**, время — ISO-8601 UTC;
//!   * **создание заявки идемпотентно**: `Idempotency-Key` обязателен, replay
//!     отдаёт сохранённое тело первого запроса и не создаёт вторую заявку.

// Хендлеры возвращают `Response` напрямую (как весь gateway), поэтому
// вспомогательные функции тоже отдают `Result<_, Response>`: «большой» Err —
// это готовый к возврату ответ, оборачивать его в Box смысла нет.
// Паттерн тот же, что в `requests_routes::resolve_guest_device_id`.
#![allow(clippy::result_large_err)]

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Duration, Utc};
use dropit_common::Scope;
use dropit_domain::{RequestAction, RequestStatus, SessionTransition};
use dropit_infra_catalog::{cache_ttl, canonical_key, SearchQuery};
use dropit_infra_db::catalog::{CatalogRepo, CatalogUpsert};
use dropit_infra_db::developer::{
    api_guest_device_id, ActiveSessionRow, ClubProfileRow, DeveloperRepo, DeveloperRequestRow,
    DeveloperSessionRow, OverviewStats, SessionCounts, TopTrackRow,
};
use dropit_infra_db::{ApiKeyRepo, RequestError, RequestRepo, SessionError, SessionRepo};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::api_key_auth::{error_envelope, DevPrincipal};
use crate::developer_docs;
use crate::routes::ApiEnv;

/// Роутер Developer API. Слой аутентификации навешивается последним,
/// чтобы покрыть все добавленные маршруты.
pub fn developer_router(env: ApiEnv) -> Router<ApiEnv> {
    routes().layer(axum::middleware::from_fn_with_state(
        env,
        crate::api_key_auth::gate,
    ))
}

/// Ресурсы Developer API v1 (ADR-010, §4).
///
/// 18 путей и 19 операций: `GET` и `POST /v1/developer/sessions` делят один
/// путь — чтение под `sessions:read`, создание под `sessions:write`.
fn routes() -> Router<ApiEnv> {
    Router::<ApiEnv>::new()
        // meta / docs
        .route("/v1/developer/me", get(me))
        .route("/v1/developer/docs", get(docs_manifest))
        .route("/v1/developer/docs/:section", get(docs_section))
        .route("/v1/developer/openapi.json", get(docs_openapi))
        // profile
        .route("/v1/developer/profile", get(profile))
        // sessions
        .route(
            "/v1/developer/sessions",
            get(list_sessions).post(create_session),
        )
        .route("/v1/developer/sessions/:id", get(get_session))
        .route("/v1/developer/sessions/:id/close", post(close_session))
        .route("/v1/developer/sessions/:id/qr", post(issue_qr))
        .route("/v1/developer/sessions/:id/queue", get(session_queue))
        .route("/v1/developer/sessions/:id/requests", post(create_request))
        // requests
        .route("/v1/developer/requests", get(list_requests))
        .route("/v1/developer/requests/:id", get(get_request))
        .route("/v1/developer/requests/:id/accept", post(accept_request))
        .route("/v1/developer/requests/:id/reject", post(reject_request))
        .route("/v1/developer/requests/:id/played", post(played_request))
        // catalog / stats
        .route("/v1/developer/catalog/search", get(catalog_search))
        .route("/v1/developer/stats/overview", get(stats_overview))
}

// ============================================================
// Общие помощники
// ============================================================

/// Пул БД или `503`: без базы данных Developer API не может ответить честно.
fn pool_of(state: &ApiEnv) -> Result<sqlx::PgPool, Response> {
    state.repo.as_ref().map(|repo| repo.pool()).ok_or_else(|| {
        error_envelope(
            StatusCode::SERVICE_UNAVAILABLE,
            "external_unavailable",
            "The database is not configured.",
            json!({}),
        )
    })
}

/// Репозиторий Developer API поверх пула.
fn repo_of(state: &ApiEnv) -> Result<DeveloperRepo, Response> {
    pool_of(state).map(DeveloperRepo::new)
}

/// `404 not_found` без подсказки, существует ли объект у другого клуба.
fn not_found(what: &str) -> Response {
    error_envelope(
        StatusCode::NOT_FOUND,
        "not_found",
        &format!("{what} not found."),
        json!({ "resource": what }),
    )
}

/// `422 validation_error` с указанием поля.
fn invalid_field(field: &str, message: &str) -> Response {
    error_envelope(
        StatusCode::UNPROCESSABLE_ENTITY,
        "validation_error",
        message,
        json!({ "field": field }),
    )
}

/// `409 conflict` со стабильным кодом (состояние сессии/заявки).
fn conflict(code: &str, message: &str) -> Response {
    error_envelope(StatusCode::CONFLICT, code, message, json!({}))
}

// ============================================================
// Cursor-пагинация
// ============================================================

/// Разделитель внутри открытого курсора (до base64url).
const CURSOR_SEP: char = '|';

/// Кодирует `(created_at, id)` в непрозрачный `base64url` без padding.
fn encode_cursor(at: DateTime<Utc>, id: Uuid) -> String {
    use base64::Engine as _;
    let raw = format!("{}{CURSOR_SEP}{id}", at.timestamp_micros());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(raw)
}

/// Разбирает курсор. `None`/пусто — первая страница; мусор — `Err`, чтобы
/// клиент получил `422`, а не молча «первую страницу» (иначе пагинация
/// зацикливается и это выглядит как потеря данных).
fn decode_cursor(raw: Option<&str>) -> Result<Option<(DateTime<Utc>, Uuid)>, ()> {
    use base64::Engine as _;
    let Some(raw) = raw.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(raw)
        .map_err(|_| ())?;
    let text = String::from_utf8(bytes).map_err(|_| ())?;
    let (micros, id) = text.rsplit_once(CURSOR_SEP).ok_or(())?;
    let micros: i64 = micros.parse().map_err(|_| ())?;
    let at = DateTime::<Utc>::from_timestamp_micros(micros).ok_or(())?;
    let id = Uuid::parse_str(id).map_err(|_| ())?;
    Ok(Some((at, id)))
}

/// `limit` из query: по умолчанию 50, допустимо `1..=100`.
fn limit_param(raw: Option<i64>) -> Result<i64, Response> {
    match raw.unwrap_or(50) {
        value if (1..=100).contains(&value) => Ok(value),
        _ => Err(invalid_field("limit", "limit must be between 1 and 100")),
    }
}

/// `next_cursor`, если запрошено `limit + 1` строк и есть ещё страница.
fn next_cursor<T, F>(rows: &[T], limit: i64, pick: F) -> Option<String>
where
    F: Fn(&T) -> (DateTime<Utc>, Uuid),
{
    if rows.len() as i64 > limit {
        rows.get((limit - 1) as usize).map(|row| {
            let (at, id) = pick(row);
            encode_cursor(at, id)
        })
    } else {
        None
    }
}

// ============================================================
// meta: GET /v1/developer/me
// ============================================================

/// Идентичность ключа. Скоупа нет: ключ уже прошёл аутентификацию, а ответ
/// описывает только его самого (не данные клуба).
async fn me(principal: DevPrincipal, State(_state): State<ApiEnv>) -> Response {
    let principal = principal.0;
    let scopes: Vec<&str> = principal
        .scopes
        .iter()
        .map(|scope| scope.as_str())
        .collect();
    (
        StatusCode::OK,
        Json(json!({
            "key_id": principal.key_id,
            "prefix": principal.prefix,
            "environment": principal.environment.as_str(),
            "scopes": scopes,
            "rate_limit_per_min": principal.rate_limit_per_min,
            "club_id": principal.club_id,
            "api_version": developer_docs::DEVELOPER_API_VERSION,
            "docs_url": "/v1/developer/docs",
            "openapi_url": "/v1/developer/openapi.json",
        })),
    )
        .into_response()
}

// ============================================================
// docs
// ============================================================

/// `GET /v1/developer/docs` — манифест, отфильтрованный скоупами ключа.
async fn docs_manifest(principal: DevPrincipal, State(_state): State<ApiEnv>) -> Response {
    if let Err(response) = principal.0.require(Scope::DocsRead) {
        return response;
    }
    (
        StatusCode::OK,
        Json(developer_docs::manifest(Some(&principal.0.scopes))),
    )
        .into_response()
}

/// `GET /v1/developer/openapi.json` — тот же документ, но только с теми
/// операции, на которые у ключа есть скоуп.
async fn docs_openapi(principal: DevPrincipal, State(_state): State<ApiEnv>) -> Response {
    if let Err(response) = principal.0.require(Scope::DocsRead) {
        return response;
    }
    (
        StatusCode::OK,
        Json(developer_docs::openapi_json(Some(&principal.0.scopes))),
    )
        .into_response()
}

/// `GET /v1/developer/docs/{section}` — полный markdown секции.
///
/// Незнакомый id — `404`; известный, но недоступный по скоупам — `403
/// insufficient_scope` с первым недополученным скоупом. Тело недоступной
/// секции не отдаётся даже частично.
async fn docs_section(
    principal: DevPrincipal,
    State(_state): State<ApiEnv>,
    Path(section_id): Path<String>,
) -> Response {
    let principal = principal.0;
    if let Err(response) = principal.require(Scope::DocsRead) {
        return response;
    }
    let Some(section) = developer_docs::sections()
        .iter()
        .find(|section| section.id == section_id)
    else {
        return not_found("Documentation section");
    };
    if !developer_docs::section_allowed(section, &principal.scopes) {
        let missing = section
            .requires
            .iter()
            .find(|required| {
                Scope::parse(required)
                    .map(|scope| !principal.scopes.contains(&scope))
                    .unwrap_or(true)
            })
            .copied()
            .unwrap_or(Scope::DocsRead.as_str());
        let granted: Vec<&str> = principal
            .scopes
            .iter()
            .map(|scope| scope.as_str())
            .collect();
        return error_envelope(
            StatusCode::FORBIDDEN,
            "insufficient_scope",
            "The API key does not grant the scope required by this section.",
            json!({ "required_scope": missing, "granted_scopes": granted }),
        );
    }
    match developer_docs::section_payload(&section_id) {
        Some(payload) => (StatusCode::OK, Json(payload)).into_response(),
        None => not_found("Documentation section"),
    }
}

// ============================================================
// profile
// ============================================================

/// `GET /v1/developer/profile` — профиль клуба ключа.
async fn profile(principal: DevPrincipal, State(state): State<ApiEnv>) -> Response {
    if let Err(response) = principal.0.require(Scope::ProfileRead) {
        return response;
    }
    let repo = match repo_of(&state) {
        Ok(repo) => repo,
        Err(response) => return response,
    };
    match repo.club_profile(principal.0.club_id).await {
        // Ключ без клуба — битые данные: отвечаем как на отсутствующий ресурс.
        Ok(None) => not_found("Club profile"),
        Ok(Some(row)) => {
            let ClubProfileRow {
                id,
                name,
                slug,
                address,
                country,
                email,
                phone,
                logo_url,
                timezone,
                status,
            } = row;
            (
                StatusCode::OK,
                Json(json!({
                    "club": {
                        "id": id,
                        "name": name,
                        "slug": slug,
                        "address": address,
                        "country": country,
                        "email": email,
                        "phone": phone,
                        "logo_url": logo_url,
                        "timezone": timezone,
                        "status": status,
                    }
                })),
            )
                .into_response()
        }
        Err(err) => db_error(err.to_string()),
    }
}

// ============================================================
// Сессии
// ============================================================

#[derive(Deserialize)]
struct SessionListParams {
    status: Option<String>,
    limit: Option<i64>,
    cursor: Option<String>,
}

#[derive(Deserialize)]
struct CreateSessionBody {
    dj_id: Option<Uuid>,
    pricing_policy_id: Option<Uuid>,
    timezone: String,
}

/// Допустимые статусы сессии (та же стейт-машина, что в домене).
fn known_session_status(value: &str) -> bool {
    matches!(value, "created" | "active" | "paused_acceptance" | "closed")
}

/// Сериализация сессии для ответов Developer API (без внутренних id клуба).
fn session_json(row: &DeveloperSessionRow) -> Value {
    json!({
        "id": row.id,
        "status": row.status,
        "timezone": row.timezone,
        "dj_id": row.dj_id,
        "pricing_policy_id": row.pricing_policy_id,
        "opened_at": row.opened_at,
        "closed_at": row.closed_at,
        "created_at": row.created_at,
    })
}

/// `GET /v1/developer/sessions` — страница сессий клуба.
async fn list_sessions(
    principal: DevPrincipal,
    State(state): State<ApiEnv>,
    Query(params): Query<SessionListParams>,
) -> Response {
    if let Err(response) = principal.0.require(Scope::SessionsRead) {
        return response;
    }
    let repo = match repo_of(&state) {
        Ok(repo) => repo,
        Err(response) => return response,
    };
    let limit = match limit_param(params.limit) {
        Ok(limit) => limit,
        Err(response) => return response,
    };
    let status = params
        .status
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if let Some(value) = status {
        if !known_session_status(value) {
            return invalid_field(
                "status",
                "status must be one of created, active, paused_acceptance, closed",
            );
        }
    }
    let cursor = match decode_cursor(params.cursor.as_deref()) {
        Ok(cursor) => cursor,
        Err(()) => return invalid_field("cursor", "cursor is not a valid pagination cursor"),
    };
    match repo
        .list_sessions(principal.0.club_id, status, limit + 1, cursor)
        .await
    {
        Ok(mut rows) => {
            let next = next_cursor(&rows, limit, |row| (row.created_at, row.id));
            if rows.len() as i64 > limit {
                rows.truncate(limit as usize);
            }
            let items: Vec<Value> = rows.iter().map(session_json).collect();
            (
                StatusCode::OK,
                Json(json!({ "items": items, "next_cursor": next })),
            )
                .into_response()
        }
        Err(err) => db_error(err.to_string()),
    }
}

/// `POST /v1/developer/sessions` — открыть смену (статус `created`).
///
/// Гейт: `sessions:write`; DJ и тариф обязаны принадлежать этому клубу —
/// иначе `404` (чужой id не подтверждаем). Часовой пояс — IANA, проверяется
/// `chrono_tz`: мусор → `422`.
async fn create_session(
    principal: DevPrincipal,
    State(state): State<ApiEnv>,
    Json(body): Json<CreateSessionBody>,
) -> Response {
    if let Err(response) = principal.0.require(Scope::SessionsWrite) {
        return response;
    }
    let repo = match repo_of(&state) {
        Ok(repo) => repo,
        Err(response) => return response,
    };
    let club_id = principal.0.club_id;
    let timezone = body.timezone.trim().to_string();
    if timezone.is_empty() || timezone.parse::<chrono_tz::Tz>().is_err() {
        return invalid_field("timezone", "timezone must be a valid IANA time zone");
    }
    if let Some(dj_id) = body.dj_id {
        match repo.dj_belongs_to_club(club_id, dj_id).await {
            Ok(true) => {}
            Ok(false) => return not_found("DJ"),
            Err(err) => return db_error(err.to_string()),
        }
    }
    if let Some(policy_id) = body.pricing_policy_id {
        match repo
            .pricing_policy_belongs_to_club(club_id, policy_id)
            .await
        {
            Ok(true) => {}
            Ok(false) => return not_found("Pricing policy"),
            Err(err) => return db_error(err.to_string()),
        }
    }
    match repo
        .create_session(club_id, body.dj_id, body.pricing_policy_id, &timezone)
        .await
    {
        Ok(row) => (StatusCode::CREATED, Json(session_json(&row))).into_response(),
        Err(err) => db_error(err.to_string()),
    }
}

/// `GET /v1/developer/sessions/{id}` — сессия и счётчики её заявок.
async fn get_session(
    principal: DevPrincipal,
    State(state): State<ApiEnv>,
    Path(session_id): Path<Uuid>,
) -> Response {
    if let Err(response) = principal.0.require(Scope::SessionsRead) {
        return response;
    }
    let repo = match repo_of(&state) {
        Ok(repo) => repo,
        Err(response) => return response,
    };
    let club_id = principal.0.club_id;
    let row = match repo.find_session(club_id, session_id).await {
        Ok(Some(row)) => row,
        Ok(None) => return not_found("Session"),
        Err(err) => return db_error(err.to_string()),
    };
    let counts = match repo.session_counts(club_id, session_id).await {
        Ok(counts) => counts,
        Err(err) => return db_error(err.to_string()),
    };
    let mut body = session_json(&row);
    if let Some(object) = body.as_object_mut() {
        object.insert("counts".into(), counts_json(&counts));
    }
    (StatusCode::OK, Json(body)).into_response()
}

/// Счётчики заявок сессии в одном формате для карточки сессии.
fn counts_json(counts: &SessionCounts) -> Value {
    json!({
        "total": counts.total,
        "pending": counts.pending,
        "accepted": counts.accepted,
        "rejected": counts.rejected,
        "played": counts.played,
        "queue": counts.queue,
    })
}

/// `POST /v1/developer/sessions/{id}/close` — закрыть смену.
///
/// Переход применяет доменная стейт-машина (`SessionRepo::apply_transition`):
/// `created|active|paused_acceptance → closed`, закрытая сессия → `409`.
async fn close_session(
    principal: DevPrincipal,
    State(state): State<ApiEnv>,
    Path(session_id): Path<Uuid>,
) -> Response {
    if let Err(response) = principal.0.require(Scope::SessionsWrite) {
        return response;
    }
    let repo = match repo_of(&state) {
        Ok(repo) => repo,
        Err(response) => return response,
    };
    let club_id = principal.0.club_id;
    // Владение проверяем ДО перехода: чужая сессия — 404, а не 409.
    match repo.find_session(club_id, session_id).await {
        Ok(Some(_)) => {}
        Ok(None) => return not_found("Session"),
        Err(err) => return db_error(err.to_string()),
    }
    let sessions = SessionRepo::new(repo.pool().clone());
    match sessions
        .apply_transition(session_id, SessionTransition::Close)
        .await
    {
        Ok(row) => match repo.find_session(club_id, row.id).await {
            Ok(Some(fresh)) => (StatusCode::OK, Json(session_json(&fresh))).into_response(),
            Ok(None) => not_found("Session"),
            Err(err) => db_error(err.to_string()),
        },
        Err(SessionError::InvalidTransition) => {
            conflict("session_closed", "This session is already closed.")
        }
        Err(SessionError::NotFound) => not_found("Session"),
        Err(err) => db_error(err.to_string()),
    }
}

// ============================================================
// QR гостевого входа
// ============================================================

/// `POST /v1/developer/sessions/{id}/qr` — выпуск/ротация QR-токена.
///
/// Токен — та же одноразовая тайна, что клуб печатает в QR: гость отдаёт её
/// в `POST /v1/sessions/join`. В ответе он приходит **один раз**, в БД лежит
/// только `sha256`. `join_url` — необязательное удобство: он собирается из
/// `DROPIT_GUEST_JOIN_BASE_URL`, если стенд его задал; иначе поля нет.
async fn issue_qr(
    principal: DevPrincipal,
    State(state): State<ApiEnv>,
    Path(session_id): Path<Uuid>,
) -> Response {
    if let Err(response) = principal.0.require(Scope::QrWrite) {
        return response;
    }
    let repo = match repo_of(&state) {
        Ok(repo) => repo,
        Err(response) => return response,
    };
    let club_id = principal.0.club_id;
    let session = match repo.find_session(club_id, session_id).await {
        Ok(Some(row)) => row,
        Ok(None) => return not_found("Session"),
        Err(err) => return db_error(err.to_string()),
    };
    if session.status == "closed" {
        return conflict(
            "session_closed",
            "Cannot issue a QR token for a closed session.",
        );
    }
    let tz: chrono_tz::Tz = match session.timezone.parse() {
        Ok(tz) => tz,
        Err(_) => {
            tracing::error!(session_id = %session.id, "stored session timezone is invalid");
            return error_envelope(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal",
                "The session time zone is invalid.",
                json!({}),
            );
        }
    };
    let expires_at = dropit_domain::qr::next_close_at(&tz, Utc::now());

    // 256 бит энтропии → hex 64; в БД только sha256(plain).
    let mut bytes = [0u8; 32];
    rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut bytes);
    let plain = hex::encode(bytes);
    let token_hash = hash_qr(&plain);
    let sessions = SessionRepo::new(repo.pool().clone());
    match sessions
        .issue_qr_token(session.id, &token_hash, expires_at)
        .await
    {
        Ok(row) => {
            let mut body = json!({
                "session_id": row.session_id,
                "qr_token": plain,
                "expires_at": row.expires_at,
            });
            if let Some(join_url) = guest_join_url(body["qr_token"].as_str().unwrap_or_default()) {
                body["join_url"] = Value::String(join_url);
            }
            (StatusCode::OK, Json(body)).into_response()
        }
        Err(err) => db_error(err.to_string()),
    }
}

/// Ссылка на страницу входа гостя, если стенд её хостит
/// (`DROPIT_GUEST_JOIN_BASE_URL`). Секрет — только в query, как в QR.
fn guest_join_url(token: &str) -> Option<String> {
    let base = std::env::var("DROPIT_GUEST_JOIN_BASE_URL")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())?;
    let separator = if base.contains('?') { '&' } else { '?' };
    Some(format!("{base}{separator}token={token}"))
}

/// sha256 QR-токена (hex) — то, что хранится в БД (как в `sessions_routes`).
fn hash_qr(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
}

// ============================================================
// Очередь и заявки
// ============================================================

#[derive(Deserialize)]
struct QueueParams {
    sort: Option<String>,
}

#[derive(Deserialize)]
struct CreateRequestBody {
    track_id: Uuid,
    style: Option<String>,
}

#[derive(Deserialize)]
struct RequestListParams {
    status: Option<String>,
    session_id: Option<Uuid>,
    limit: Option<i64>,
    cursor: Option<String>,
}

#[derive(Deserialize)]
struct RejectBody {
    reason: Option<String>,
}

/// Членство статуса заявки в доменном множестве.
fn known_request_status(value: &str) -> bool {
    matches!(
        value,
        "pending" | "accepted" | "rejected" | "played" | "expired"
    )
}

/// Карточка заявки для Developer API: трек и времена, без гостя и платежей.
fn request_json(row: &DeveloperRequestRow) -> Value {
    json!({
        "id": row.id,
        "session_id": row.session_id,
        "status": row.status,
        "style": row.style,
        "price_minor": row.price_minor,
        "decline_reason": row.reject_reason,
        "played_at": row.played_at,
        "created_at": row.created_at,
        "track": {
            "title": row.title,
            "artist": row.artist,
            "artwork_url": row.cover_url,
            "preview_url": row.preview_url,
            "isrc": row.isrc,
        },
    })
}

/// `GET /v1/developer/sessions/{id}/queue` — активная очередь сессии.
async fn session_queue(
    principal: DevPrincipal,
    State(state): State<ApiEnv>,
    Path(session_id): Path<Uuid>,
    Query(params): Query<QueueParams>,
) -> Response {
    if let Err(response) = principal.0.require(Scope::RequestsRead) {
        return response;
    }
    let repo = match repo_of(&state) {
        Ok(repo) => repo,
        Err(response) => return response,
    };
    let club_id = principal.0.club_id;
    match repo.find_session(club_id, session_id).await {
        Ok(Some(_)) => {}
        Ok(None) => return not_found("Session"),
        Err(err) => return db_error(err.to_string()),
    }
    let sort = params.sort.as_deref().unwrap_or("new").trim();
    if !matches!(sort, "new" | "hot") {
        return invalid_field("sort", "sort must be one of new, hot");
    }
    let requests = RequestRepo::new(repo.pool().clone());
    match requests.dj_queue(session_id, sort).await {
        Ok(items) => {
            let items: Vec<Value> = items
                .into_iter()
                .map(|item| {
                    json!({
                        "id": item.id,
                        "session_id": item.session_id,
                        "status": item.status,
                        "style": item.style,
                        "created_at": item.created_at,
                        "same_track_count": item.same_track_count,
                        "track": {
                            "title": item.title,
                            "artist": item.artist,
                            "artwork_url": item.cover_url,
                            "preview_url": item.preview_url,
                            "isrc": item.isrc,
                        },
                    })
                })
                .collect();
            (StatusCode::OK, Json(json!({ "items": items }))).into_response()
        }
        Err(err) => db_error(err.to_string()),
    }
}

/// `POST /v1/developer/sessions/{id}/requests` — заявка от интеграции.
///
/// Заявка домена всегда принадлежит гостю, поэтому у ключа есть
/// детерминированное синтетическое устройство (`guest_device.platform='api'`)
/// и живой `session_guest`-грант этой сессии. Наружу эта идентичность не
/// выходит.
///
/// `Idempotency-Key` обязателен: повтор с тем же ключом отдаёт `201` с телом
/// первого ответа и не создаёт вторую заявку. Ключ replay-таблицы
/// префиксуется `key:{key_id}:`, чтобы два клуба/ключа не сталкивались на
/// одинаковом клиентском значении.
async fn create_request(
    principal: DevPrincipal,
    State(state): State<ApiEnv>,
    Path(session_id): Path<Uuid>,
    headers: HeaderMap,
    Json(body): Json<CreateRequestBody>,
) -> Response {
    if let Err(response) = principal.0.require(Scope::RequestsWrite) {
        return response;
    }
    let repo = match repo_of(&state) {
        Ok(repo) => repo,
        Err(response) => return response,
    };
    let club_id = principal.0.club_id;

    let idem_key = headers
        .get("idempotency-key")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let Some(idem_key) = idem_key else {
        return invalid_field(
            "Idempotency-Key",
            "The Idempotency-Key header is required for request creation.",
        );
    };
    if idem_key.chars().count() > 200 {
        return invalid_field(
            "Idempotency-Key",
            "Idempotency-Key must be at most 200 characters",
        );
    }
    let style = body
        .style
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if let Some(style) = style {
        if style.chars().count() > 40 {
            return invalid_field("style", "style must be at most 40 characters");
        }
    }

    match repo.find_session(club_id, session_id).await {
        Ok(Some(_)) => {}
        Ok(None) => return not_found("Session"),
        Err(err) => return db_error(err.to_string()),
    }

    // Повтор уже обработанного ключа: отдаём сохранённое тело, не выполняя
    // работу заново (ответ на replay = 201, как у первого запроса).
    // Проверяем ДО трека и состояния сессии: идемпотентность важнее текущего
    // состояния — иначе повтор после закрытия сессии отвечал бы 409 на уже
    // созданную заявку.
    match ApiKeyRepo::new(repo.pool().clone())
        .find_idempotent_response(principal.0.key_id, &idem_key)
        .await
    {
        Ok(Some(stored)) => return (StatusCode::CREATED, Json(stored)).into_response(),
        Ok(None) => {}
        Err(err) => return db_error(err.to_string()),
    }

    match repo.track_exists(body.track_id).await {
        Ok(true) => {}
        Ok(false) => return invalid_field("track_id", "Unknown track: search the catalog first."),
        Err(err) => return db_error(err.to_string()),
    }

    let guest_device_id = api_guest_device_id(principal.0.key_id);
    match repo
        .ensure_api_guest(club_id, session_id, guest_device_id)
        .await
    {
        Ok(true) => {}
        Ok(false) => return not_found("Session"),
        Err(err) => return db_error(err.to_string()),
    }

    let requests = RequestRepo::new(repo.pool().clone());
    let scoped_key = format!("key:{}:{idem_key}", principal.0.key_id);
    let outcome = requests
        .create_request(
            session_id,
            guest_device_id,
            body.track_id,
            style,
            &scoped_key,
        )
        .await;

    // Создание заявки: домен уже сгейтил сессию/грант и зафиксировал
    // идемпотентный ответ в СВОЕЙ таблице (`key:{key_id}:…`). Наружу отдаём
    // карточку в публичном формате — без гостя и платёжных полей.
    let created: DeveloperRequestRow = match outcome {
        Ok(Ok(row)) => {
            // Best-effort live-уведомление консоли (источник истины — REST/БД).
            publish_request_created(&state, &requests, &row).await;
            match repo.find_request(club_id, row.id).await {
                Ok(Some(fresh)) => fresh,
                // Заявка только что вставлена: её отсутствие — битые данные.
                Ok(None) => return db_error("request disappeared after insert".into()),
                Err(err) => return db_error(err.to_string()),
            }
        }
        Ok(Err(replay)) => {
            // Ключ уже зафиксирован гостевой таблицей (например, параллельный
            // POST): читаем заявку по id из replay-тела и отдаём её же.
            let Some(request_id) = replay
                .get("request_id")
                .and_then(Value::as_str)
                .and_then(|value| Uuid::parse_str(value).ok())
            else {
                return db_error("replay response without request_id".into());
            };
            match repo.find_request(club_id, request_id).await {
                Ok(Some(fresh)) => fresh,
                Ok(None) => return not_found("Request"),
                Err(err) => return db_error(err.to_string()),
            }
        }
        Err(RequestError::SessionNotActive) => {
            return conflict(
                "session_not_active",
                "This session is not accepting requests right now.",
            )
        }
        Err(RequestError::IdempotencyConflict) => {
            return conflict(
                "idempotency_conflict",
                "The Idempotency-Key was already used with a different payload.",
            )
        }
        // Синтетический гость с грантом только что создан; отсутствие гранта
        // здесь — нарушение инварианта, а не ответ клиенту.
        Err(RequestError::NoGrant) => {
            tracing::error!(
                session_id = %session_id,
                "developer api guest grant vanished between ensure and create"
            );
            return db_error("api guest grant missing".into());
        }
        Err(err) => return db_error(err.to_string()),
    };

    let response_body = request_json(&created);
    // Таблица replay для Developer API: зовём через ApiKeyRepo. Ошибка записи
    // не должна превращать успешно созданную заявку в 500 — повтор клиента
    // всё равно упрётся в гостевой replay по тому же `key:{key_id}:…`.
    if let Err(err) = ApiKeyRepo::new(repo.pool().clone())
        .store_idempotent_response(principal.0.key_id, &idem_key, &response_body)
        .await
    {
        tracing::warn!(error = %err, "developer api idempotency store failed");
    }
    (StatusCode::CREATED, Json(response_body)).into_response()
}

/// WS-уведомление о новой заявке (не влияет на ответ клиенту).
async fn publish_request_created(
    state: &ApiEnv,
    requests: &RequestRepo,
    row: &dropit_infra_db::RequestRow,
) {
    let Some(hub) = state.ws.as_ref() else {
        return;
    };
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
        row.guest_device_id,
        &title,
        same,
    ));
}

/// `GET /v1/developer/requests` — заявки клуба (все сессии) с курсором.
async fn list_requests(
    principal: DevPrincipal,
    State(state): State<ApiEnv>,
    Query(params): Query<RequestListParams>,
) -> Response {
    if let Err(response) = principal.0.require(Scope::RequestsRead) {
        return response;
    }
    let repo = match repo_of(&state) {
        Ok(repo) => repo,
        Err(response) => return response,
    };
    let limit = match limit_param(params.limit) {
        Ok(limit) => limit,
        Err(response) => return response,
    };
    let status = params
        .status
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if let Some(value) = status {
        if !known_request_status(value) {
            return invalid_field(
                "status",
                "status must be one of pending, accepted, rejected, played, expired",
            );
        }
    }
    if let Some(session_id) = params.session_id {
        match repo.find_session(principal.0.club_id, session_id).await {
            Ok(Some(_)) => {}
            Ok(None) => return not_found("Session"),
            Err(err) => return db_error(err.to_string()),
        }
    }
    let cursor = match decode_cursor(params.cursor.as_deref()) {
        Ok(cursor) => cursor,
        Err(()) => return invalid_field("cursor", "cursor is not a valid pagination cursor"),
    };
    match repo
        .list_requests(
            principal.0.club_id,
            status,
            params.session_id,
            limit + 1,
            cursor,
        )
        .await
    {
        Ok(mut rows) => {
            let next = next_cursor(&rows, limit, |row| (row.created_at, row.id));
            if rows.len() as i64 > limit {
                rows.truncate(limit as usize);
            }
            let items: Vec<Value> = rows.iter().map(request_json).collect();
            (
                StatusCode::OK,
                Json(json!({ "items": items, "next_cursor": next })),
            )
                .into_response()
        }
        Err(err) => db_error(err.to_string()),
    }
}

/// `GET /v1/developer/requests/{id}` — карточка заявки клуба.
async fn get_request(
    principal: DevPrincipal,
    State(state): State<ApiEnv>,
    Path(request_id): Path<Uuid>,
) -> Response {
    if let Err(response) = principal.0.require(Scope::RequestsRead) {
        return response;
    }
    let repo = match repo_of(&state) {
        Ok(repo) => repo,
        Err(response) => return response,
    };
    match repo.find_request(principal.0.club_id, request_id).await {
        Ok(Some(row)) => (StatusCode::OK, Json(request_json(&row))).into_response(),
        Ok(None) => not_found("Request"),
        Err(err) => db_error(err.to_string()),
    }
}

/// Скоуп проверяет [`request_action`] первой строкой; обёртки данные не
/// трогают — они только называют действие.
async fn accept_request(
    principal: DevPrincipal,
    State(state): State<ApiEnv>,
    Path(request_id): Path<Uuid>,
) -> Response {
    request_action(principal, state, request_id, RequestAction::Accept, None).await
}

async fn played_request(
    principal: DevPrincipal,
    State(state): State<ApiEnv>,
    Path(request_id): Path<Uuid>,
) -> Response {
    request_action(principal, state, request_id, RequestAction::Play, None).await
}

/// `POST /v1/developer/requests/{id}/reject` — тело необязательно
/// (`{"reason": "..."}`); отсутствие тела = отклонение без причины.
async fn reject_request(
    principal: DevPrincipal,
    State(state): State<ApiEnv>,
    Path(request_id): Path<Uuid>,
    body: Option<Json<RejectBody>>,
) -> Response {
    let reason = body
        .as_ref()
        .and_then(|Json(body)| body.reason.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    request_action(principal, state, request_id, RequestAction::Reject, reason).await
}

/// Действие над заявкой с проверкой аренды и no-op-семантикой повтора.
///
/// Допустимость перехода решает домен (`RequestStatus::apply`): повтор того
/// же действия — `200` с `noop: true` (не ошибка), запрещённый переход
/// (например `accept` после `played`) — `409` до записи в БД.
async fn request_action(
    principal: DevPrincipal,
    state: ApiEnv,
    request_id: Uuid,
    action: RequestAction,
    reason: Option<&str>,
) -> Response {
    if let Err(response) = principal.0.require(Scope::RequestsWrite) {
        return response;
    }
    let repo = match repo_of(&state) {
        Ok(repo) => repo,
        Err(response) => return response,
    };
    let club_id = principal.0.club_id;
    // Владение: заявка живёт в сессии этого клуба. Чужая → 404 (не 403).
    let current = match repo.find_request(club_id, request_id).await {
        Ok(Some(row)) => row,
        Ok(None) => return not_found("Request"),
        Err(err) => return db_error(err.to_string()),
    };
    let Some(status) = request_status_of(&current.status) else {
        tracing::error!(request_id = %request_id, status = %current.status, "unknown request status");
        return db_error("unknown stored request status".into());
    };
    if status.apply(action).is_err() {
        return conflict(
            "invalid_request_transition",
            "This action is not allowed from the request's current state.",
        );
    }

    let requests = RequestRepo::new(repo.pool().clone());
    let (row, noop) = match requests.apply_action(request_id, action, reason).await {
        Ok(result) => result,
        Err(RequestError::Db(sqlx::Error::RowNotFound)) => return not_found("Request"),
        Err(err) => return db_error(err.to_string()),
    };
    if !noop {
        if let Some(hub) = state.ws.as_ref() {
            let event = match action {
                RequestAction::Accept => {
                    crate::ws::events::request_accepted(row.session_id, row.id, row.guest_device_id)
                }
                RequestAction::Reject => crate::ws::events::request_rejected(
                    row.session_id,
                    row.id,
                    row.guest_device_id,
                    row.reject_reason.as_deref(),
                ),
                RequestAction::Play => {
                    crate::ws::events::request_played(row.session_id, row.id, row.guest_device_id)
                }
                RequestAction::Expire => crate::ws::events::request_rejected(
                    row.session_id,
                    row.id,
                    row.guest_device_id,
                    None,
                ),
            };
            hub.publish(event);
        }
    }
    let card = match repo.find_request(club_id, row.id).await {
        Ok(Some(fresh)) => request_json(&fresh),
        Ok(None) => return not_found("Request"),
        Err(err) => return db_error(err.to_string()),
    };
    let mut body = card;
    if let Some(object) = body.as_object_mut() {
        object.insert("noop".into(), Value::Bool(noop));
    }
    (StatusCode::OK, Json(body)).into_response()
}

/// Строковый статус заявки → доменный (единственный маппинг для API).
fn request_status_of(value: &str) -> Option<RequestStatus> {
    match value {
        "pending" => Some(RequestStatus::Pending),
        "accepted" => Some(RequestStatus::Accepted),
        "rejected" => Some(RequestStatus::Rejected),
        "played" => Some(RequestStatus::Played),
        "expired" => Some(RequestStatus::Expired),
        _ => None,
    }
}

// ============================================================
// Каталог
// ============================================================

#[derive(Deserialize)]
struct CatalogParams {
    q: String,
    limit: Option<u32>,
}

/// `GET /v1/developer/catalog/search` — поиск трека в каталоге.
///
/// Отдаём нормализованное представление (`id` = `track_cache.id`, который
/// нужен для создания заявки) и никогда — сырой ответ провайдера,
/// `source_id`/`source_url` и прочие внутренние поля кэша.
async fn catalog_search(
    principal: DevPrincipal,
    State(state): State<ApiEnv>,
    Query(params): Query<CatalogParams>,
) -> Response {
    if let Err(response) = principal.0.require(Scope::CatalogRead) {
        return response;
    }
    let q = params.q.trim();
    if !(2..=80).contains(&q.chars().count()) {
        return invalid_field("q", "q must be 2..80 characters");
    }
    let limit = match params.limit.unwrap_or(10) {
        value if (1..=25).contains(&value) => value,
        _ => return invalid_field("limit", "limit must be between 1 and 25"),
    };
    let repo = match repo_of(&state) {
        Ok(repo) => repo,
        Err(response) => return response,
    };
    let catalog_repo = CatalogRepo::new(repo.pool().clone());
    let query_key = canonical_key("", q);
    // Стиль в публичном API не параметр поиска: кэш-ключ — «без стиля».
    let style_key = "";

    if let Ok(Some(row)) = catalog_repo.find_query_cache(&query_key, style_key).await {
        if CatalogRepo::is_fresh(&row, cache_ttl(row.has_isrc)) {
            match catalog_repo.load_tracks_by_ids(&row.track_ids).await {
                // Кэш отдаём только если ВСЕ его треки ещё живы: retention
                // мог удалить строки `track_cache`, и «свежий» индекс тогда
                // отвечал бы пустым списком вместо честного промаха.
                Ok(tracks) if tracks.len() == row.track_ids.len() => {
                    let items: Vec<Value> = tracks.iter().map(catalog_item_json).collect();
                    return (
                        StatusCode::OK,
                        Json(json!({ "items": items, "cached": true })),
                    )
                        .into_response();
                }
                Ok(_) => {}
                Err(err) => {
                    tracing::warn!(error = %err, "developer api catalog cache read failed");
                }
            }
        }
    }

    let Some(catalog) = state.catalog.clone() else {
        return error_envelope(
            StatusCode::SERVICE_UNAVAILABLE,
            "catalog_unavailable",
            "The catalog is not configured on this deployment.",
            json!({}),
        );
    };
    let hits = match catalog
        .search(SearchQuery {
            q: q.to_string(),
            limit: limit as u8,
            style: None,
        })
        .await
    {
        Ok(hits) => hits,
        Err(err) => {
            tracing::warn!(error = %err, "developer api catalog search failed");
            return error_envelope(
                StatusCode::SERVICE_UNAVAILABLE,
                "catalog_unavailable",
                "The track catalog is temporarily unavailable.",
                json!({}),
            );
        }
    };

    let enriched: Vec<(dropit_infra_catalog::CatalogHit, String, String)> = hits
        .into_iter()
        .map(|hit| {
            let canonical = format!("{}:{}", hit.source, hit.source_id);
            let song_key = canonical_key(&hit.artist, &hit.title);
            (hit, canonical, song_key)
        })
        .collect();
    let upserts: Vec<CatalogUpsert> = enriched
        .iter()
        .map(|(hit, canonical, song_key)| CatalogUpsert {
            source: &hit.source,
            source_id: &hit.source_id,
            canonical_key: canonical,
            isrc: hit.isrc.as_deref(),
            title: &hit.title,
            artist: &hit.artist,
            artwork_url: hit.artwork_url.as_deref(),
            preview_url: hit.preview_url.as_deref(),
            album: hit.album.as_deref(),
            duration_ms: hit.duration_ms,
            source_url: hit.source_url.as_deref(),
            explicit: hit.explicit,
            song_key,
        })
        .collect();
    let ids = match catalog_repo.upsert_hits(&upserts).await {
        Ok(ids) => ids,
        Err(err) => {
            tracing::warn!(error = %err, "developer api catalog cache write failed");
            return error_envelope(
                StatusCode::SERVICE_UNAVAILABLE,
                "catalog_unavailable",
                "The catalog cache is temporarily unavailable.",
                json!({}),
            );
        }
    };
    let has_isrc = enriched.iter().any(|(hit, _, _)| hit.isrc.is_some());
    let _ = catalog_repo
        .save_query_cache(&query_key, style_key, &ids, has_isrc)
        .await;

    let items: Vec<Value> = enriched
        .iter()
        .zip(ids)
        .map(|((hit, _, _), id)| {
            json!({
                "id": id,
                "title": hit.title,
                "artist": hit.artist,
                "album": hit.album,
                "duration_ms": hit.duration_ms,
                "isrc": hit.isrc,
                "artwork_url": hit.artwork_url,
                "preview_url": hit.preview_url,
                "explicit": hit.explicit,
            })
        })
        .collect();
    (
        StatusCode::OK,
        Json(json!({ "items": items, "cached": false })),
    )
        .into_response()
}

/// Строка кэша каталога в публичном виде (без `source_id`/`source_url`).
fn catalog_item_json(row: &dropit_infra_db::catalog::CachedTrackRow) -> Value {
    json!({
        "id": row.id,
        "title": row.title,
        "artist": row.artist,
        "album": row.album,
        "duration_ms": row.duration_ms,
        "isrc": row.isrc,
        "artwork_url": row.cover_url,
        "preview_url": row.preview_url,
        "explicit": row.explicit,
    })
}

// ============================================================
// Статистика
// ============================================================

#[derive(Deserialize)]
struct StatsParams {
    period: Option<String>,
}

/// `GET /v1/developer/stats/overview` — агрегаты клуба за период.
///
/// Окно: `day` — 24 ч, `week` — 7 дней, `month` — 30 дней (до `to=now`).
/// Деньги — целые минорные единицы, без курсов и округлений.
async fn stats_overview(
    principal: DevPrincipal,
    State(state): State<ApiEnv>,
    Query(params): Query<StatsParams>,
) -> Response {
    if let Err(response) = principal.0.require(Scope::StatsRead) {
        return response;
    }
    let repo = match repo_of(&state) {
        Ok(repo) => repo,
        Err(response) => return response,
    };
    let period = params.period.as_deref().unwrap_or("day").trim();
    let (period, span) = match period {
        "day" => ("day", Duration::days(1)),
        "week" => ("week", Duration::days(7)),
        "month" => ("month", Duration::days(30)),
        _ => return invalid_field("period", "period must be one of day, week, month"),
    };
    let to = Utc::now();
    let from = to - span;
    let club_id = principal.0.club_id;
    let stats: OverviewStats = match repo.overview(club_id, from, to).await {
        Ok(stats) => stats,
        Err(err) => return db_error(err.to_string()),
    };
    let top: Vec<TopTrackRow> = match repo.top_tracks(club_id, from, to, 5).await {
        Ok(rows) => rows,
        Err(err) => return db_error(err.to_string()),
    };
    let active: Option<ActiveSessionRow> = match repo.active_session(club_id).await {
        Ok(row) => row,
        Err(err) => return db_error(err.to_string()),
    };
    (
        StatusCode::OK,
        Json(json!({
            "period": period,
            "from": from,
            "to": to,
            "sessions": stats.sessions,
            "active_session": active.map(|row| json!({
                "id": row.id,
                "status": row.status,
                "opened_at": row.opened_at,
            })),
            "requests": {
                "total": stats.requests_total,
                "pending": stats.pending,
                "accepted": stats.accepted,
                "rejected": stats.rejected,
                "played": stats.played,
            },
            "revenue": {
                "currency": stats.currency,
                "captured_minor": stats.captured_minor,
            },
            "top_tracks": top.iter().map(|row| json!({
                "title": row.title,
                "artist": row.artist,
                "count": row.requests,
            })).collect::<Vec<Value>>(),
        })),
    )
        .into_response()
}

// ============================================================
// Ошибки
// ============================================================

/// `500 internal` — деталь только в лог, клиенту без SQL и путей.
fn db_error(detail: String) -> Response {
    tracing::error!(error = %detail, "developer api database error");
    error_envelope(
        StatusCode::INTERNAL_SERVER_ERROR,
        "internal",
        "An internal error occurred.",
        json!({}),
    )
}

// ============================================================
// Тесты чистой логики (без БД)
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_roundtrip_is_opaque_and_strict() {
        let at = Utc::now();
        let id = Uuid::now_v7();
        let encoded = encode_cursor(at, id);
        assert!(
            !encoded.contains('|'),
            "курсор не должен раскрывать разделитель"
        );
        let decoded = decode_cursor(Some(&encoded)).expect("валидный курсор");
        let (decoded_at, decoded_id) = decoded.expect("курсор есть");
        assert_eq!(decoded_at.timestamp_micros(), at.timestamp_micros());
        assert_eq!(decoded_id, id);
        // Пустой/отсутствующий курсор — первая страница, не ошибка.
        assert!(decode_cursor(None).unwrap().is_none());
        assert!(decode_cursor(Some("  ")).unwrap().is_none());
        // Мусор и чужая кодировка — ошибка (422), а не «первая страница».
        assert!(decode_cursor(Some("not-a-cursor")).is_err());
        let bogus = base64::Engine::encode(
            &base64::engine::general_purpose::URL_SAFE_NO_PAD,
            "123|not-a-uuid",
        );
        assert!(decode_cursor(Some(&bogus)).is_err());
    }

    #[test]
    fn limit_bounds() {
        assert_eq!(limit_param(None).unwrap(), 50);
        assert_eq!(limit_param(Some(1)).unwrap(), 1);
        assert_eq!(limit_param(Some(100)).unwrap(), 100);
        assert!(limit_param(Some(0)).is_err());
        assert!(limit_param(Some(101)).is_err());
    }

    #[test]
    fn join_url_needs_explicit_configuration() {
        // Без переменной стенда поля нет — никакого «угаданного» домена.
        std::env::remove_var("DROPIT_GUEST_JOIN_BASE_URL");
        assert!(guest_join_url("abc").is_none());
        std::env::set_var("DROPIT_GUEST_JOIN_BASE_URL", "https://app.example/join");
        assert_eq!(
            guest_join_url("abc").as_deref(),
            Some("https://app.example/join?token=abc")
        );
        std::env::set_var("DROPIT_GUEST_JOIN_BASE_URL", "https://x.test/join?room=1");
        assert_eq!(
            guest_join_url("abc").as_deref(),
            Some("https://x.test/join?room=1&token=abc")
        );
        std::env::remove_var("DROPIT_GUEST_JOIN_BASE_URL");
    }

    #[test]
    fn status_validators_match_domain() {
        for status in ["created", "active", "paused_acceptance", "closed"] {
            assert!(known_session_status(status), "{status}");
        }
        assert!(!known_session_status("deleted"));
        for status in ["pending", "accepted", "rejected", "played", "expired"] {
            assert!(known_request_status(status), "{status}");
        }
        assert!(!known_request_status("refunded"));
    }

    /// Каждый ресурс ADR-010 §4 объявлен в контракте ровно тем методом,
    /// которым он зарегистрирован в роутере (разбор YAML, не подстрока).
    #[test]
    fn routes_cover_the_adr_surface() {
        let doc: Value =
            serde_yaml::from_str(include_str!("../../../docs/api/openapi-developer.yaml"))
                .expect("developer openapi yaml parses");
        let paths = doc["paths"].as_object().expect("paths object");
        let expected = [
            ("get", "/v1/developer/me"),
            ("get", "/v1/developer/docs"),
            ("get", "/v1/developer/docs/{section}"),
            ("get", "/v1/developer/openapi.json"),
            ("get", "/v1/developer/profile"),
            ("get", "/v1/developer/sessions"),
            ("post", "/v1/developer/sessions"),
            ("get", "/v1/developer/sessions/{id}"),
            ("post", "/v1/developer/sessions/{id}/close"),
            ("post", "/v1/developer/sessions/{id}/qr"),
            ("get", "/v1/developer/sessions/{id}/queue"),
            ("post", "/v1/developer/sessions/{id}/requests"),
            ("get", "/v1/developer/requests"),
            ("get", "/v1/developer/requests/{id}"),
            ("post", "/v1/developer/requests/{id}/accept"),
            ("post", "/v1/developer/requests/{id}/reject"),
            ("post", "/v1/developer/requests/{id}/played"),
            ("get", "/v1/developer/catalog/search"),
            ("get", "/v1/developer/stats/overview"),
        ];
        let mut operations = 0;
        for (path, item) in paths {
            for method in ["get", "post", "put", "patch", "delete"] {
                if item.get(method).is_some() {
                    operations += 1;
                    assert!(
                        expected.contains(&(method, path.as_str())),
                        "в контракте есть операция вне поверхности ADR-010: {method} {path}"
                    );
                }
            }
        }
        assert_eq!(operations, expected.len(), "19 операций Developer API");
        for (method, path) in expected {
            let op = &paths
                .get(path)
                .unwrap_or_else(|| panic!("нет пути {path} в контракте"))[method];
            assert!(op.is_object(), "{method} {path}: операция не описана");
        }
    }
}
