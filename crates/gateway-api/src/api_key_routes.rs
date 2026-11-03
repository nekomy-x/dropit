//! Управление API-ключами Developer API v1 из панелей (ADR-010).
//!
//! Клуб (`/v1/club/api-keys/**`): создание (секрет показывается один раз),
//! список, переименование, сужение скоупов, ротация с окном перекрытия,
//! отзыв, журнал вызовов.
//! Админ (`/v1/admin/api-keys/**`): список всех ключей и отзыв любого.
//!
//! Все ответы, кроме создания/ротации, не содержат секрета — его нет
//! в БД в открытом виде. Каждое изменение пишет строку в `audit_log`
//! (`api_key.*`), диф без секрета и без `secret_hash`.
//!
//! Ошибки репозитория переводятся в стабильные коды конверта:
//! `key_limit_reached` (409), `scope_widening_forbidden` (422),
//! `validation_error` (422 с деталями), `not_found` (404), `api_key_revoked`
//! (409). Чужой ключ для клуба — всегда 404, чтобы нельзя было перебором
//! id узнать о существовании ключей другого клуба.

use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use dropit_common::{ApiError, Environment, Role};
use dropit_infra_db::api_keys::AdminApiKeyRow;
use dropit_infra_db::{ApiKeyError, ApiKeyRepo, ApiKeyView, NewApiKeyInput};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use crate::api_key_auth::error_envelope;
use crate::rbac::{auth_error_response, AuthContext};
use crate::routes::{api_error, ApiEnv};

/// Предупреждение рядом с единственным показом секрета.
const SECRET_WARNING: &str =
    "This is the only time the full key is shown. Store it in a secret manager now; \
     it cannot be retrieved later.";

/// Сколько последних вызовов отдаёт лента журнала в панели.
const USAGE_FEED_LIMIT: i64 = 50;

pub fn api_key_routes() -> Router<ApiEnv> {
    Router::new()
        // Клуб: ключи своего клуба.
        .route(
            "/v1/club/api-keys",
            get(club_list_keys).post(club_create_key),
        )
        .route("/v1/club/api-keys/scopes", get(club_scopes))
        .route(
            "/v1/club/api-keys/:id",
            get(club_get_key)
                .patch(club_patch_key)
                .delete(club_revoke_key),
        )
        .route("/v1/club/api-keys/:id/rotate", post(club_rotate_key))
        .route("/v1/club/api-keys/:id/usage", get(club_key_usage))
        // Админ: обзор всех ключей платформы.
        .route("/v1/admin/api-keys", get(admin_list_keys))
        .route("/v1/admin/api-keys/scopes", get(admin_scopes))
        .route("/v1/admin/api-keys/:id/revoke", post(admin_revoke_key))
        .route("/v1/admin/api-keys/:id/usage", get(admin_key_usage))
}

// ============================================================
// Тело запросов
// ============================================================

#[derive(Deserialize)]
pub struct CreateApiKeyRequest {
    pub name: String,
    #[serde(default)]
    pub scopes: Vec<String>,
    #[serde(default)]
    pub environment: Option<String>,
    #[serde(default)]
    pub expires_in_days: Option<i64>,
    #[serde(default)]
    pub rate_limit_per_min: Option<i32>,
    #[serde(default)]
    pub allowed_cidrs: Vec<String>,
}

#[derive(Deserialize)]
pub struct UpdateApiKeyRequest {
    #[serde(default)]
    pub name: Option<String>,
    /// Только сужение: расширение отклоняется 422 `scope_widening_forbidden`.
    #[serde(default)]
    pub scopes: Option<Vec<String>>,
}

#[derive(Deserialize, Default)]
pub struct RotateApiKeyRequest {
    /// Окно, в которое старый секрет ещё работает (0..=72, по умолчанию 24).
    #[serde(default)]
    pub grace_hours: Option<i64>,
}

#[derive(Deserialize, Default)]
pub struct AdminListQuery {
    #[serde(default)]
    pub limit: Option<i64>,
}

// ============================================================
// Вспомогательное
// ============================================================

fn require_club(ctx: &AuthContext) -> Option<Response> {
    if ctx.role() == Role::Club {
        None
    } else {
        Some(auth_error_response(ApiError::Forbidden))
    }
}

fn require_admin(ctx: &AuthContext) -> Option<Response> {
    if ctx.role() == Role::Admin {
        None
    } else {
        Some(auth_error_response(ApiError::Forbidden))
    }
}

#[allow(clippy::result_large_err)]
fn repo_of(state: &ApiEnv) -> Result<ApiKeyRepo, Response> {
    state
        .repo
        .as_ref()
        .map(|repo| ApiKeyRepo::new(repo.pool()))
        .ok_or_else(|| {
            api_error(
                ApiError::ExternalServiceUnavailable,
                "database not configured",
            )
        })
}

/// Ошибка валидации тела запроса (тот же конверт, что и у остальных панелей).
fn validation_error(details: Value) -> Response {
    error_envelope(
        StatusCode::UNPROCESSABLE_ENTITY,
        "validation_error",
        "The request failed validation.",
        details,
    )
}

/// `environment` по умолчанию `live`; всё, что не `live`/`test`, — 422.
#[allow(clippy::result_large_err)]
fn parse_environment(value: Option<&str>) -> Result<Environment, Response> {
    match value {
        None => Ok(Environment::Live),
        Some(raw) => Environment::parse(raw).ok_or_else(|| {
            validation_error(json!({
                "invalid_environment": raw,
                "allowed": [Environment::Live.as_str(), Environment::Test.as_str()],
            }))
        }),
    }
}

/// Карточка ключа → JSON без секрета.
fn view_json(view: ApiKeyView) -> Value {
    serde_json::to_value(view).unwrap_or_else(|_| json!({}))
}

/// Ответ создания: карточка + единственный показ секрета и предупреждение.
fn revealed_json(view: ApiKeyView, revealed: &str) -> Value {
    let mut value = view_json(view);
    if let Some(object) = value.as_object_mut() {
        object.insert("key".into(), json!(revealed));
        object.insert("warning".into(), json!(SECRET_WARNING));
    }
    value
}

/// Ответ ротации: карточка + секрет + id заменённого ключа (без `warning`:
/// контракт ротации возвращает `replaces`, панель показывает его в диалоге).
fn rotated_json(view: ApiKeyView, revealed: &str, replaces: Uuid) -> Value {
    let mut value = view_json(view);
    if let Some(object) = value.as_object_mut() {
        object.insert("key".into(), json!(revealed));
        object.insert("replaces".into(), json!(replaces));
    }
    value
}

/// Админ-строка: карточка ключа + `club_name`.
fn admin_key_json(row: AdminApiKeyRow) -> Value {
    let mut value = view_json(ApiKeyView::from(row.key));
    if let Some(object) = value.as_object_mut() {
        object.insert("club_name".into(), json!(row.club_name));
    }
    value
}

/// Секрет не должен попадать в кэши (браузер/прокси) — `no-store`.
fn no_store(mut response: Response) -> Response {
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

/// Аудит изменения ключа. `entity` — id затронутого ключа строкой (та же
/// конвенция, что у `dj.*` в `panel_routes.rs`). `diff` никогда не содержит
/// секрет или его хеш. Ошибка записи не валит бизнес-операцию — как в
/// остальных панельных роутах.
async fn audit_key(
    pool: &PgPool,
    actor_type: &str,
    actor_id: Uuid,
    action: &str,
    entity: Uuid,
    diff: Value,
) {
    let _ = sqlx::query(
        "INSERT INTO audit_log (actor_type, actor_id, action, entity, diff) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(actor_type)
    .bind(actor_id)
    .bind(action)
    .bind(entity.to_string())
    .bind(diff)
    .execute(pool)
    .await;
}

fn scopes_payload() -> Response {
    (
        StatusCode::OK,
        Json(json!({
            "scopes": crate::developer_docs::scope_catalogue_json(),
            "presets": crate::developer_docs::scope_presets_json(),
        })),
    )
        .into_response()
}

/// Единый маппинг ошибок репозитория в стабильные коды панельного API.
fn map_api_key_error(err: ApiKeyError) -> Response {
    match err {
        ApiKeyError::TooManyKeys => error_envelope(
            StatusCode::CONFLICT,
            "key_limit_reached",
            "A club can have at most 25 active API keys. Revoke an unused key first.",
            json!({ "limit": dropit_common::MAX_ACTIVE_KEYS_PER_CLUB }),
        ),
        ApiKeyError::InvalidScope(scope) => validation_error(json!({ "unknown_scope": scope })),
        ApiKeyError::EmptyScopes => validation_error(json!({
            "detail": "at least one scope is required",
        })),
        ApiKeyError::ScopeWidening => error_envelope(
            StatusCode::UNPROCESSABLE_ENTITY,
            "scope_widening_forbidden",
            "Scopes can only be narrowed. Create a new key to grant more access.",
            json!({}),
        ),
        ApiKeyError::RateLimitOutOfRange => validation_error(json!({
            "min_rate_limit_per_min": dropit_common::MIN_RATE_LIMIT_PER_MIN,
            "max_rate_limit_per_min": dropit_common::MAX_RATE_LIMIT_PER_MIN,
        })),
        ApiKeyError::InvalidIpRule(rule) => validation_error(json!({ "invalid_ip_rule": rule })),
        ApiKeyError::Invalid(detail) => validation_error(json!({ "detail": detail })),
        ApiKeyError::NotFound => error_envelope(
            StatusCode::NOT_FOUND,
            "not_found",
            "API key not found.",
            json!({}),
        ),
        ApiKeyError::ClubNotFound => error_envelope(
            StatusCode::NOT_FOUND,
            "club_not_found",
            "Club not found.",
            json!({}),
        ),
        ApiKeyError::Revoked => error_envelope(
            StatusCode::CONFLICT,
            "api_key_revoked",
            "The API key is already revoked.",
            json!({}),
        ),
        ApiKeyError::Expired => error_envelope(
            StatusCode::CONFLICT,
            "api_key_expired",
            "The API key has expired.",
            json!({}),
        ),
        ApiKeyError::IpNotAllowed => error_envelope(
            StatusCode::FORBIDDEN,
            "ip_not_allowed",
            "This IP address is not allowed for the API key.",
            json!({}),
        ),
        ApiKeyError::RateLimited => error_envelope(
            StatusCode::TOO_MANY_REQUESTS,
            "rate_limited",
            "Rate limit exceeded for this API key.",
            json!({}),
        ),
        ApiKeyError::InvalidKey => error_envelope(
            StatusCode::UNAUTHORIZED,
            "invalid_api_key",
            "The API key is invalid.",
            json!({}),
        ),
        ApiKeyError::Db(error) => {
            tracing::error!(error = %error, "api key repository error");
            api_error(ApiError::Internal, "db error")
        }
    }
}

// ============================================================
// Клуб
// ============================================================

async fn club_list_keys(ctx: AuthContext, State(state): State<ApiEnv>) -> Response {
    if let Some(denied) = require_club(&ctx) {
        return denied;
    }
    let repo = match repo_of(&state) {
        Ok(repo) => repo,
        Err(response) => return response,
    };
    match repo.list_for_club(ctx.subject_id()).await {
        Ok(rows) => {
            let items: Vec<Value> = rows
                .into_iter()
                .map(|row| view_json(ApiKeyView::from(row)))
                .collect();
            (StatusCode::OK, Json(json!({ "items": items }))).into_response()
        }
        Err(err) => map_api_key_error(err),
    }
}

async fn club_create_key(
    ctx: AuthContext,
    State(state): State<ApiEnv>,
    Json(req): Json<CreateApiKeyRequest>,
) -> Response {
    if let Some(denied) = require_club(&ctx) {
        return denied;
    }
    let repo = match repo_of(&state) {
        Ok(repo) => repo,
        Err(response) => return response,
    };
    let environment = match parse_environment(req.environment.as_deref()) {
        Ok(environment) => environment,
        Err(response) => return response,
    };
    let input = NewApiKeyInput {
        club_id: ctx.subject_id(),
        name: req.name,
        environment,
        scopes: req.scopes,
        rate_limit_per_min: req.rate_limit_per_min,
        allowed_cidrs: req.allowed_cidrs,
        expires_in_days: req.expires_in_days,
        created_by: Some(ctx.subject_id()),
    };
    match repo.create(input).await {
        Ok(created) => {
            let view = ApiKeyView::from(created.key);
            let diff = json!({
                "id": view.id,
                "name": view.name,
                "prefix": view.prefix,
                "environment": view.environment,
                "scopes": view.scopes,
                "rate_limit_per_min": view.rate_limit_per_min,
                "allowed_cidrs": view.allowed_cidrs,
                "expires_at": view.expires_at,
            });
            audit_key(
                repo.pool(),
                "club",
                ctx.subject_id(),
                "api_key.created",
                view.id,
                diff,
            )
            .await;
            let response = (
                StatusCode::CREATED,
                Json(revealed_json(view, &created.revealed)),
            )
                .into_response();
            no_store(response)
        }
        Err(err) => map_api_key_error(err),
    }
}

async fn club_scopes(ctx: AuthContext, State(_state): State<ApiEnv>) -> Response {
    if let Some(denied) = require_club(&ctx) {
        return denied;
    }
    scopes_payload()
}

async fn club_get_key(
    ctx: AuthContext,
    State(state): State<ApiEnv>,
    Path(id): Path<Uuid>,
) -> Response {
    if let Some(denied) = require_club(&ctx) {
        return denied;
    }
    let repo = match repo_of(&state) {
        Ok(repo) => repo,
        Err(response) => return response,
    };
    match repo.find(ctx.subject_id(), id).await {
        Ok(Some(row)) => (StatusCode::OK, Json(view_json(ApiKeyView::from(row)))).into_response(),
        Ok(None) => map_api_key_error(ApiKeyError::NotFound),
        Err(err) => map_api_key_error(err),
    }
}

async fn club_patch_key(
    ctx: AuthContext,
    State(state): State<ApiEnv>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateApiKeyRequest>,
) -> Response {
    if let Some(denied) = require_club(&ctx) {
        return denied;
    }
    let repo = match repo_of(&state) {
        Ok(repo) => repo,
        Err(response) => return response,
    };
    let club_id = ctx.subject_id();
    let previous = match repo.find(club_id, id).await {
        Ok(Some(row)) => row,
        Ok(None) => return map_api_key_error(ApiKeyError::NotFound),
        Err(err) => return map_api_key_error(err),
    };
    match repo
        .update(club_id, id, req.name.as_deref(), req.scopes.as_deref())
        .await
    {
        Ok(row) => {
            let view = ApiKeyView::from(row);
            let diff = json!({
                "id": view.id,
                "name": view.name,
                "scopes": view.scopes,
                "previous_name": previous.name,
                "previous_scopes": previous.scopes,
            });
            audit_key(
                repo.pool(),
                "club",
                club_id,
                "api_key.updated",
                view.id,
                diff,
            )
            .await;
            (StatusCode::OK, Json(view_json(view))).into_response()
        }
        Err(err) => map_api_key_error(err),
    }
}

async fn club_rotate_key(
    ctx: AuthContext,
    State(state): State<ApiEnv>,
    Path(id): Path<Uuid>,
    body: Option<Json<RotateApiKeyRequest>>,
) -> Response {
    if let Some(denied) = require_club(&ctx) {
        return denied;
    }
    let repo = match repo_of(&state) {
        Ok(repo) => repo,
        Err(response) => return response,
    };
    let grace_hours = body.and_then(|Json(req)| req.grace_hours);
    match repo.rotate(ctx.subject_id(), id, grace_hours).await {
        Ok(created) => {
            let replaces = created.key.rotated_from.unwrap_or(id);
            let view = ApiKeyView::from(created.key);
            let diff = json!({
                "id": view.id,
                "replaces": replaces,
                "prefix": view.prefix,
                "grace_hours": grace_hours
                    .unwrap_or(dropit_common::DEFAULT_ROTATION_GRACE_HOURS),
            });
            audit_key(
                repo.pool(),
                "club",
                ctx.subject_id(),
                "api_key.rotated",
                view.id,
                diff,
            )
            .await;
            let response = (
                StatusCode::CREATED,
                Json(rotated_json(view, &created.revealed, replaces)),
            )
                .into_response();
            no_store(response)
        }
        Err(err) => map_api_key_error(err),
    }
}

async fn club_revoke_key(
    ctx: AuthContext,
    State(state): State<ApiEnv>,
    Path(id): Path<Uuid>,
) -> Response {
    if let Some(denied) = require_club(&ctx) {
        return denied;
    }
    let repo = match repo_of(&state) {
        Ok(repo) => repo,
        Err(response) => return response,
    };
    let club_id = ctx.subject_id();
    let existing = match repo.find(club_id, id).await {
        Ok(Some(row)) => row,
        Ok(None) => return map_api_key_error(ApiKeyError::NotFound),
        Err(err) => return map_api_key_error(err),
    };
    match repo.revoke(club_id, id).await {
        Ok(()) => {
            let diff = json!({
                "id": id,
                "prefix": existing.prefix,
                "status": "revoked",
            });
            audit_key(repo.pool(), "club", club_id, "api_key.revoked", id, diff).await;
            StatusCode::NO_CONTENT.into_response()
        }
        Err(err) => map_api_key_error(err),
    }
}

async fn club_key_usage(
    ctx: AuthContext,
    State(state): State<ApiEnv>,
    Path(id): Path<Uuid>,
) -> Response {
    if let Some(denied) = require_club(&ctx) {
        return denied;
    }
    let repo = match repo_of(&state) {
        Ok(repo) => repo,
        Err(response) => return response,
    };
    let club_id = ctx.subject_id();
    match repo.find(club_id, id).await {
        Ok(Some(_)) => usage_response(&repo, club_id, id).await,
        Ok(None) => map_api_key_error(ApiKeyError::NotFound),
        Err(err) => map_api_key_error(err),
    }
}

// ============================================================
// Админ
// ============================================================

async fn admin_list_keys(
    ctx: AuthContext,
    State(state): State<ApiEnv>,
    Query(query): Query<AdminListQuery>,
) -> Response {
    if let Some(denied) = require_admin(&ctx) {
        return denied;
    }
    let repo = match repo_of(&state) {
        Ok(repo) => repo,
        Err(response) => return response,
    };
    match repo.list_all_with_club(query.limit.unwrap_or(50)).await {
        Ok(rows) => {
            let items: Vec<Value> = rows.into_iter().map(admin_key_json).collect();
            (StatusCode::OK, Json(json!({ "items": items }))).into_response()
        }
        Err(err) => map_api_key_error(err),
    }
}

async fn admin_scopes(ctx: AuthContext, State(_state): State<ApiEnv>) -> Response {
    if let Some(denied) = require_admin(&ctx) {
        return denied;
    }
    scopes_payload()
}

async fn admin_revoke_key(
    ctx: AuthContext,
    State(state): State<ApiEnv>,
    Path(id): Path<Uuid>,
) -> Response {
    if let Some(denied) = require_admin(&ctx) {
        return denied;
    }
    let repo = match repo_of(&state) {
        Ok(repo) => repo,
        Err(response) => return response,
    };
    let existing = match repo.find_any(id).await {
        Ok(Some(row)) => row,
        Ok(None) => return map_api_key_error(ApiKeyError::NotFound),
        Err(err) => return map_api_key_error(err),
    };
    match repo.revoke_any(id).await {
        Ok(()) => {
            let diff = json!({
                "id": id,
                "prefix": existing.prefix,
                "club_id": existing.club_id,
                "status": "revoked",
            });
            audit_key(
                repo.pool(),
                "admin",
                ctx.subject_id(),
                "api_key.revoked",
                id,
                diff,
            )
            .await;
            StatusCode::NO_CONTENT.into_response()
        }
        Err(err) => map_api_key_error(err),
    }
}

async fn admin_key_usage(
    ctx: AuthContext,
    State(state): State<ApiEnv>,
    Path(id): Path<Uuid>,
) -> Response {
    if let Some(denied) = require_admin(&ctx) {
        return denied;
    }
    let repo = match repo_of(&state) {
        Ok(repo) => repo,
        Err(response) => return response,
    };
    match repo.find_any(id).await {
        Ok(Some(row)) => usage_response(&repo, row.club_id, id).await,
        Ok(None) => map_api_key_error(ApiKeyError::NotFound),
        Err(err) => map_api_key_error(err),
    }
}

/// Сводка + последние вызовы ключа (общая форма для клуба и админа).
async fn usage_response(repo: &ApiKeyRepo, club_id: Uuid, key_id: Uuid) -> Response {
    let summary = match repo.usage_summary(club_id, key_id).await {
        Ok(summary) => summary,
        Err(err) => return map_api_key_error(err),
    };
    let items = match repo.recent_usage(club_id, key_id, USAGE_FEED_LIMIT).await {
        Ok(items) => items,
        Err(err) => return map_api_key_error(err),
    };
    (
        StatusCode::OK,
        Json(json!({ "summary": summary, "items": items })),
    )
        .into_response()
}
