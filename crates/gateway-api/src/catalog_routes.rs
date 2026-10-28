//! Роуты пакета W2-D: поиск каталога, push-токены гостей, стили DJ.
//!
//! Три эндпоинта, каждый под своим RBAC-гейтом (монтируются в `routes::router`
//! через `role_route` для ролей `Guest` и `Dj`):
//!
//! * `GET  /v1/catalog/search`  — гостевой поиск трека во внешнем каталоге
//!   с кешем в `track_cache` + индексом `catalog_query_cache`.
//! * `POST /v1/guest/push-token` — идемпотентная регистрация push-токена гостя.
//! * `PATCH /v1/dj/styles`       — замена массива стилей DJ-профиля.
//!
//! Кеш поиска: ключ запроса — `canonical_key("", q)`; треки кешируются в
//! `track_cache` с `song_key = canonical_key(artist, title)`; TTL —
//! `cache_ttl(has_isrc)` (24 ч с ISRC, 1 ч без). Промах кеша → запрос к
//! внешнему каталогу (`ApiEnv.catalog`) и upsert; недоступность внешнего
//! каталога → 503 `catalog_unavailable` (не 500).

use crate::rbac::AuthContext;
use crate::routes::{api_error, ApiEnv};
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, patch, post},
    Json, Router,
};
use dropit_common::{ApiError, ErrorEnvelope};
use dropit_infra_catalog::{cache_ttl, canonical_key, CatalogHit, SearchQuery};
use dropit_infra_db::catalog::{
    CatalogRepo, CatalogUpsert, DjProfileRepo, PushTokenRepo, PUSH_PLATFORMS,
};
use serde::{Deserialize, Serialize};

// ============================================================
// DTO
// ============================================================

/// Параметры `GET /v1/catalog/search`.
#[derive(Deserialize)]
pub struct SearchParams {
    /// Текст поиска (название/исполнитель).
    pub q: String,
    /// Опциональный клубный стиль для уточнения терма.
    pub style: Option<String>,
    /// Лимит результатов `1..=25` (по умолчанию 10).
    pub limit: Option<u32>,
}

/// Ответ `GET /v1/catalog/search`.
#[derive(Serialize)]
pub struct SearchResponse {
    /// Найденные треки (нормализованное представление `CatalogHit`).
    pub items: Vec<CatalogItemDto>,
    /// `true`, если ответ отдан из кеша БД без обращения к внешнему каталогу.
    pub cached: bool,
}

/// Клиентский результат включает UUID строки `track_cache`: именно его
/// нужно передавать в POST /v1/requests. Source id для этого не подходит.
#[derive(Serialize)]
pub struct CatalogItemDto {
    pub id: uuid::Uuid,
    pub source: String,
    pub source_id: String,
    pub title: String,
    pub artist: String,
    pub album: Option<String>,
    pub duration_ms: i64,
    pub isrc: Option<String>,
    pub artwork_url: Option<String>,
    pub preview_url: Option<String>,
    pub source_url: Option<String>,
    pub explicit: bool,
}

/// Тело `POST /v1/guest/push-token`.
#[derive(Deserialize)]
pub struct PushTokenRequest {
    pub token: String,
    pub platform: String,
}

/// Тело `PATCH /v1/dj/styles`.
#[derive(Deserialize)]
pub struct DjStylesRequest {
    pub styles: Vec<String>,
}

/// Ответ `PATCH /v1/dj/styles`.
#[derive(Serialize)]
pub struct DjStylesResponse {
    pub styles: Vec<String>,
}

// ============================================================
// Router
// ============================================================

/// Роуты гостя с JWT-гейтом: регистрация push-токена.
///
/// Монтируется под `role_route(&[Role::Guest], …)` в `routes::router`.
pub fn guest_routes() -> Router<ApiEnv> {
    Router::new().route("/v1/guest/push-token", post(register_push_token))
}

/// Поиск каталога авторизуется device-token в мобильном клиенте, а не JWT.
/// Поэтому маршрут монтируется отдельно от role middleware.
pub fn public_catalog_routes() -> Router<ApiEnv> {
    Router::new().route("/v1/catalog/search", get(search_catalog))
}

/// Роуты DJ: управление стилями профиля.
///
/// Монтируется под `role_route(&[Role::Dj], …)` в `routes::router`.
pub fn dj_routes() -> Router<ApiEnv> {
    Router::new().route("/v1/dj/styles", patch(update_dj_styles))
}

// ============================================================
// GET /v1/catalog/search
// ============================================================

/// Гостевой поиск трека: кеш → внешний каталог → upsert.
///
/// Валидация: `q` 2..80 символов, `limit` 1..=25. Внешний каталог недоступен
/// → 503 `catalog_unavailable`.
async fn search_catalog(
    State(state): State<ApiEnv>,
    Query(params): Query<SearchParams>,
) -> Response {
    let q = params.q.trim();
    let qlen = q.chars().count();
    if !(2..=80).contains(&qlen) {
        return err_response(
            "invalid_query",
            StatusCode::BAD_REQUEST,
            "q must be 2..80 characters",
        );
    }
    let limit = match params.limit.unwrap_or(10) {
        l if (1..=25).contains(&l) => l as u8,
        _ => {
            return err_response(
                "invalid_query",
                StatusCode::BAD_REQUEST,
                "limit must be between 1 and 25",
            )
        }
    };
    let style = params
        .style
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    // Колонка `style` кеша NOT NULL (часть составного PRIMARY KEY):
    // пустая строка — сантинел «без стиля».
    let style_key = style.unwrap_or("");

    let repo = match state.repo.clone() {
        Some(r) => r,
        None => {
            return err_response(
                "catalog_unavailable",
                StatusCode::SERVICE_UNAVAILABLE,
                "database not configured",
            )
        }
    };
    let pool = repo.pool();
    let catalog_repo = CatalogRepo::new(pool);

    // Канонический ключ запроса для индекса catalog_query_cache.
    let query_key = canonical_key("", q);

    // 1. Кеш: свежая запись индекса → отдаём треки из БД.
    if let Ok(Some(qrow)) = catalog_repo.find_query_cache(&query_key, style_key).await {
        let ttl = cache_ttl(qrow.has_isrc);
        if CatalogRepo::is_fresh(&qrow, ttl) {
            match catalog_repo.load_tracks_by_ids(&qrow.track_ids).await {
                Ok(tracks) => {
                    let items: Vec<CatalogItemDto> = tracks.into_iter().map(dto_from_row).collect();
                    return (
                        StatusCode::OK,
                        Json(SearchResponse {
                            items,
                            cached: true,
                        }),
                    )
                        .into_response();
                }
                Err(_) => {
                    return err_response(
                        "catalog_unavailable",
                        StatusCode::SERVICE_UNAVAILABLE,
                        "cache read failed",
                    )
                }
            }
        }
    }

    // 2. Промах: внешний каталог.
    let catalog = match state.catalog.clone() {
        Some(c) => c,
        None => {
            return err_response(
                "catalog_unavailable",
                StatusCode::SERVICE_UNAVAILABLE,
                "catalog not configured",
            )
        }
    };
    let query = SearchQuery {
        q: q.to_string(),
        limit,
        style: style.map(str::to_string),
    };
    let hits = match catalog.search(query).await {
        Ok(hits) => hits,
        Err(_) => {
            return err_response(
                "catalog_unavailable",
                StatusCode::SERVICE_UNAVAILABLE,
                "external catalog unavailable",
            )
        }
    };

    // 3. Upsert хитов в track_cache + сохранение индекса запроса.
    // (canonical_key / song_key живут в owned-векторе, переживающем borrow.)
    let enriched: Vec<(CatalogHit, String, String)> = hits
        .into_iter()
        .map(|h| {
            let canonical = format!("{}:{}", h.source, h.source_id);
            let song_key = canonical_key(&h.artist, &h.title);
            (h, canonical, song_key)
        })
        .collect();
    let upserts: Vec<CatalogUpsert> = enriched
        .iter()
        .map(|(h, canonical, song_key)| CatalogUpsert {
            source: &h.source,
            source_id: &h.source_id,
            canonical_key: canonical,
            isrc: h.isrc.as_deref(),
            title: &h.title,
            artist: &h.artist,
            artwork_url: h.artwork_url.as_deref(),
            preview_url: h.preview_url.as_deref(),
            album: h.album.as_deref(),
            duration_ms: h.duration_ms,
            source_url: h.source_url.as_deref(),
            explicit: h.explicit,
            song_key,
        })
        .collect();
    let ids = match catalog_repo.upsert_hits(&upserts).await {
        Ok(ids) => ids,
        Err(_) => {
            return err_response(
                "catalog_unavailable",
                StatusCode::SERVICE_UNAVAILABLE,
                "cache write failed",
            )
        }
    };
    let has_isrc = enriched.iter().any(|(h, _, _)| h.isrc.is_some());
    let _ = catalog_repo
        .save_query_cache(&query_key, style_key, &ids, has_isrc)
        .await;

    let items = enriched
        .into_iter()
        .zip(ids)
        .map(|((h, _, _), id)| CatalogItemDto {
            id,
            source: h.source,
            source_id: h.source_id,
            title: h.title,
            artist: h.artist,
            album: h.album,
            duration_ms: h.duration_ms,
            isrc: h.isrc,
            artwork_url: h.artwork_url,
            preview_url: h.preview_url,
            source_url: h.source_url,
            explicit: h.explicit,
        })
        .collect();
    (
        StatusCode::OK,
        Json(SearchResponse {
            items,
            cached: false,
        }),
    )
        .into_response()
}

// ============================================================
// POST /v1/guest/push-token
// ============================================================

/// Идемпотентная регистрация push-токена гостя.
///
/// Валидация: `token` 16..4096 символов, `platform` ∈ {android, ios}.
/// Повторная регистрация того же токена обновляет `last_seen_at` и
/// `guest_id` (перевод токена другому гостю).
async fn register_push_token(
    State(state): State<ApiEnv>,
    ctx: AuthContext,
    Json(req): Json<PushTokenRequest>,
) -> Response {
    let token = req.token.trim();
    let tlen = token.chars().count();
    if !(16..=4096).contains(&tlen) {
        return err_response(
            "invalid_token",
            StatusCode::BAD_REQUEST,
            "token must be 16..4096 characters",
        );
    }
    let platform = req.platform.trim().to_ascii_lowercase();
    if !PUSH_PLATFORMS.contains(&platform.as_str()) {
        return err_response(
            "invalid_platform",
            StatusCode::BAD_REQUEST,
            "platform must be 'android' or 'ios'",
        );
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
    let push_repo = PushTokenRepo::new(repo.pool());
    match push_repo
        .upsert_token(ctx.subject_id(), token, &platform)
        .await
    {
        Ok(()) => (
            StatusCode::CREATED,
            Json(serde_json::json!({ "registered": true })),
        )
            .into_response(),
        Err(_) => api_error(ApiError::Internal, "db error"),
    }
}

// ============================================================
// PATCH /v1/dj/styles
// ============================================================

/// Замена массива стилей DJ-профиля.
///
/// Валидация: 0..=8 элементов, каждый 2..40 символов. Нормализация:
/// trim + lowercase + дедуп со стабильным порядком (первое вхождение).
/// DJ не найден → 404 (не раскрывает существование).
async fn update_dj_styles(
    State(state): State<ApiEnv>,
    ctx: AuthContext,
    Json(req): Json<DjStylesRequest>,
) -> Response {
    if req.styles.len() > 8 {
        return err_response(
            "invalid_styles",
            StatusCode::BAD_REQUEST,
            "at most 8 styles allowed",
        );
    }
    let mut seen = std::collections::HashSet::new();
    let mut normalized: Vec<String> = Vec::with_capacity(req.styles.len());
    for s in req.styles {
        let trimmed = s.trim();
        let clen = trimmed.chars().count();
        if !(2..=40).contains(&clen) {
            return err_response(
                "invalid_styles",
                StatusCode::BAD_REQUEST,
                "each style must be 2..40 characters",
            );
        }
        let lower = trimmed.to_lowercase();
        if seen.insert(lower.clone()) {
            normalized.push(lower);
        }
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
    let dj_repo = DjProfileRepo::new(repo.pool());
    match dj_repo.set_styles(ctx.subject_id(), &normalized).await {
        Ok(true) => (
            StatusCode::OK,
            Json(DjStylesResponse { styles: normalized }),
        )
            .into_response(),
        Ok(false) => api_error(ApiError::NotFound, "dj not found"),
        Err(_) => api_error(ApiError::Internal, "db error"),
    }
}

// ============================================================
// Вспомогательное
// ============================================================

fn dto_from_row(r: dropit_infra_db::catalog::CachedTrackRow) -> CatalogItemDto {
    CatalogItemDto {
        id: r.id,
        source: r.source,
        source_id: r.source_id,
        title: r.title,
        artist: r.artist,
        album: r.album,
        duration_ms: r.duration_ms,
        isrc: r.isrc,
        artwork_url: r.cover_url,
        preview_url: r.preview_url,
        source_url: r.source_url,
        explicit: r.explicit,
    }
}

/// Ответ ошибки с явным стабильным `code` (расширяет `api_error` для
/// пакетных кодов `invalid_query` / `catalog_unavailable` / `invalid_token`
/// и т. п., не добавляя новых вариантов в `ApiError`).
fn err_response(code: &str, status: StatusCode, detail: &str) -> Response {
    let envelope = ErrorEnvelope {
        code: code.to_string(),
        message: detail.to_string(),
        details: Some(serde_json::json!({ "detail": detail })),
    };
    (status, Json(envelope)).into_response()
}
