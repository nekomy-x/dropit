//! Аутентификация Developer API v1 по API-ключу (ADR-010).
//!
//! Один слой на весь `/v1/developer/**`:
//!   1. достаёт ключ из `Authorization: Bearer dit_...` или `X-API-Key`;
//!   2. разрешает его через [`ApiKeyRepo::authenticate`] (prefix → sha256
//!      в постоянном времени → статус → срок → allowlist IP);
//!   3. отказывает ключам неактивного клуба и `test`-ключам на live-стенде;
//!   4. применяет лимит запросов ключа (`X-RateLimit-*`, `429`+`Retry-After`);
//!   5. кладёт [`ApiKeyPrincipal`] в расширения запроса и пишет вызов
//!      в `api_key_usage` (без ключа и секрета).
//!
//! Хендлеры читают принципала экстрактором [`DevPrincipal`] и проверяют
//! скоуп через [`ApiKeyPrincipal::require`] — deny by default.

use std::net::{IpAddr, SocketAddr};

use axum::extract::{ConnectInfo, FromRequestParts, Request, State};
use axum::http::{request::Parts, HeaderMap, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use dropit_common::{Environment, ErrorEnvelope, Scope};
use dropit_infra_db::{ApiKeyError, ApiKeyRepo};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::routes::ApiEnv;

/// Альтернативный заголовок ключа (удобен для простых интеграций).
pub const API_KEY_HEADER: &str = "x-api-key";

/// Разрешённый ключ текущего запроса. Клонируется дёшево (scopes — до 9).
#[derive(Debug, Clone)]
pub struct ApiKeyPrincipal {
    pub key_id: Uuid,
    pub club_id: Uuid,
    pub prefix: String,
    pub environment: Environment,
    pub scopes: Vec<Scope>,
    pub rate_limit_per_min: i32,
}

impl ApiKeyPrincipal {
    pub fn allows(&self, scope: Scope) -> bool {
        self.scopes.contains(&scope)
    }

    /// Проверка скоупа для хендлера: `Err` уже готов к возврату из роута.
    // Конверт ошибки axum (`Response`); Boxing не нужен — early-return, не горячий путь.
    #[allow(clippy::result_large_err)]
    pub fn require(&self, scope: Scope) -> Result<(), Response> {
        if self.allows(scope) {
            Ok(())
        } else {
            Err(scope_denied(scope, &self.scopes))
        }
    }
}

/// Экстрактор принципала: middleware уже всё проверил.
pub struct DevPrincipal(pub ApiKeyPrincipal);

#[async_trait::async_trait]
impl FromRequestParts<ApiEnv> for DevPrincipal {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &ApiEnv,
    ) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<ApiKeyPrincipal>()
            .cloned()
            .map(DevPrincipal)
            .ok_or_else(|| {
                error_envelope(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal",
                    "API key context is missing.",
                    json!({}),
                )
            })
    }
}

/// Единый конверт ошибки Developer API (тот же формат, что у панелей).
pub fn error_envelope(status: StatusCode, code: &str, message: &str, details: Value) -> Response {
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

/// `403 insufficient_scope`: говорит, какого права не хватило и какие есть.
/// Выдавать чужие данные при этом нельзя — хендлер до данных не доходит.
pub fn scope_denied(required: Scope, granted: &[Scope]) -> Response {
    let granted: Vec<&str> = granted.iter().map(|s| s.as_str()).collect();
    error_envelope(
        StatusCode::FORBIDDEN,
        "insufficient_scope",
        "The API key does not grant the scope required by this endpoint.",
        json!({ "required_scope": required.as_str(), "granted_scopes": granted }),
    )
}

/// `401 invalid_api_key` — одинаковый ответ на неизвестный prefix, чужой
/// секрет и мусорный заголовок.
fn invalid_api_key() -> Response {
    error_envelope(
        StatusCode::UNAUTHORIZED,
        "invalid_api_key",
        "A valid API key is required. Use 'Authorization: Bearer dit_...' or 'X-API-Key'.",
        json!({}),
    )
}

/// Достаёт ключ из заголовков (`Authorization: Bearer dit_...` приоритетнее).
pub fn extract_raw_key(headers: &HeaderMap) -> Option<String> {
    if let Some(value) = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
    {
        if let Some(token) = value.strip_prefix("Bearer ") {
            let token = token.trim();
            if token.starts_with("dit_") {
                return Some(token.to_string());
            }
        }
    }
    headers
        .get(API_KEY_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| value.starts_with("dit_"))
        .map(str::to_string)
}

/// IP клиента из `ConnectInfo` (учитывает `TRUST_PROXY` как остальной API).
fn request_ip(req: &Request) -> IpAddr {
    let conn = req
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|info| info.0);
    crate::routes::client_ip(req.headers(), conn)
        .parse()
        .unwrap_or(IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED))
}

fn set_rate_headers(response: &mut Response, limit: i64, remaining: i64) {
    let headers = response.headers_mut();
    let value = |v: i64| axum::http::HeaderValue::from_str(&v.to_string()).unwrap();
    headers.insert("x-ratelimit-limit", value(limit));
    headers.insert("x-ratelimit-remaining", value(remaining.max(0)));
    headers.insert("x-ratelimit-reset", value(60));
    // Ответы Developer API зависят от прав ключа — кэшировать нельзя.
    headers.insert(
        axum::http::header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
}

/// Middleware `/v1/developer/**`.
pub async fn gate(State(state): State<ApiEnv>, mut req: Request, next: Next) -> Response {
    let Some(raw) = extract_raw_key(req.headers()) else {
        return invalid_api_key();
    };
    let Some(repo) = state
        .repo
        .as_ref()
        .map(|repo| ApiKeyRepo::new(repo.pool().clone()))
    else {
        return error_envelope(
            StatusCode::SERVICE_UNAVAILABLE,
            "external_unavailable",
            "The database is not configured.",
            json!({}),
        );
    };
    let ip = request_ip(&req);

    let auth = match repo.authenticate(&raw, ip).await {
        Ok(auth) => auth,
        Err(ApiKeyError::IpNotAllowed) => {
            return error_envelope(
                StatusCode::FORBIDDEN,
                "ip_not_allowed",
                "This IP address is not allowed for the API key.",
                json!({}),
            )
        }
        Err(ApiKeyError::Expired) => {
            return error_envelope(
                StatusCode::UNAUTHORIZED,
                "api_key_expired",
                "The API key has expired. Create a new key in the panel.",
                json!({}),
            )
        }
        Err(ApiKeyError::Revoked) => {
            return error_envelope(
                StatusCode::UNAUTHORIZED,
                "api_key_revoked",
                "The API key has been revoked.",
                json!({}),
            )
        }
        Err(ApiKeyError::InvalidKey) => return invalid_api_key(),
        Err(_) => {
            return error_envelope(
                StatusCode::SERVICE_UNAVAILABLE,
                "external_unavailable",
                "The API key could not be verified.",
                json!({}),
            )
        }
    };

    match repo.club_is_active(auth.club_id).await {
        Ok(true) => {}
        Ok(false) => {
            return error_envelope(
                StatusCode::FORBIDDEN,
                "club_suspended",
                "The club account is not active.",
                json!({}),
            )
        }
        Err(_) => {
            return error_envelope(
                StatusCode::SERVICE_UNAVAILABLE,
                "external_unavailable",
                "The club could not be verified.",
                json!({}),
            )
        }
    }

    // Test-ключ на боевом стенде — отказ, а не молчаливое смешивание данных.
    if auth.environment == Environment::Test && crate::routes::payments_mode() == "live" {
        return error_envelope(
            StatusCode::FORBIDDEN,
            "test_key_not_allowed",
            "Test keys are refused on a live deployment. Use a live key.",
            json!({}),
        );
    }

    let limit = i64::from(auth.rate_limit_per_min.max(1));
    let used = repo.calls_last_minute(auth.key_id).await.unwrap_or(0);
    if used >= limit {
        let mut response = error_envelope(
            StatusCode::TOO_MANY_REQUESTS,
            "rate_limited",
            "Rate limit exceeded for this API key. Retry after the window resets.",
            json!({ "limit": limit, "window_seconds": 60 }),
        );
        response.headers_mut().insert(
            axum::http::header::RETRY_AFTER,
            axum::http::HeaderValue::from_static("60"),
        );
        set_rate_headers(&mut response, limit, 0);
        return response;
    }
    let remaining = limit - used - 1;

    let principal = ApiKeyPrincipal {
        key_id: auth.key_id,
        club_id: auth.club_id,
        prefix: auth.prefix.clone(),
        environment: auth.environment,
        scopes: auth.scopes.clone(),
        rate_limit_per_min: auth.rate_limit_per_min,
    };
    req.extensions_mut().insert(principal.clone());

    let method = req.method().to_string();
    let path = req.uri().path().to_string();
    let mut response = next.run(req).await;
    let status = response.status().as_u16();
    // Журнал: ошибка записи не должна ломать ответ клиенту.
    if let Err(err) = repo
        .record_usage(auth.key_id, auth.club_id, &method, &path, status)
        .await
    {
        tracing::warn!(error = %err, "developer api usage log failed");
    }
    set_rate_headers(&mut response, limit, remaining);
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_bearer_and_header() {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::AUTHORIZATION,
            axum::http::HeaderValue::from_static(
                "Bearer dit_live_0123456789_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            ),
        );
        assert!(extract_raw_key(&headers).is_some());

        let mut headers = HeaderMap::new();
        headers.insert(
            API_KEY_HEADER,
            axum::http::HeaderValue::from_static(
                "dit_test_0123456789_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            ),
        );
        assert!(extract_raw_key(&headers).is_some());

        // JWT не должен быть принят как API-ключ.
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::AUTHORIZATION,
            axum::http::HeaderValue::from_static("Bearer eyJhbGciOi.payload.sig"),
        );
        assert!(extract_raw_key(&headers).is_none());
        assert!(extract_raw_key(&HeaderMap::new()).is_none());
    }

    #[test]
    fn does_not_leak_granted_scopes_on_success_check() {
        let principal = ApiKeyPrincipal {
            key_id: Uuid::now_v7(),
            club_id: Uuid::now_v7(),
            prefix: "0123456789".into(),
            environment: Environment::Live,
            scopes: vec![Scope::DocsRead],
            rate_limit_per_min: 120,
        };
        assert!(principal.require(Scope::DocsRead).is_ok());
        assert!(principal.require(Scope::StatsRead).is_err());
    }
}
