//! RBAC: экстрактор аутентификации и формат ошибок доступа (F1).
//!
//! Гейт ролей строится в `routes::role_gated`: чужая роль → 403
//! с единым envelope. Порядок: подпись JWT (401) → роль (403).

use axum::{
    extract::FromRequestParts,
    http::request::Parts,
    response::{IntoResponse, Response},
    Json,
};
use dropit_common::{AccessClaims, ApiError, Role};

/// Аутентифицированный субъект, доступный хендлеру через экстрактор.
#[derive(Debug, Clone)]
pub struct AuthContext {
    pub claims: AccessClaims,
}

impl AuthContext {
    pub fn role(&self) -> Role {
        self.claims.role
    }

    pub fn subject_id(&self) -> uuid::Uuid {
        self.claims.sub
    }
}

/// Ошибка аутентификации/авторизации → единый envelope.
pub fn auth_error_response(err: ApiError) -> Response {
    (
        axum::http::StatusCode::from_u16(err.http_status()).unwrap(),
        Json(err.envelope()),
    )
        .into_response()
}

/// Извлекает Bearer-токен из Authorization.
fn bearer_token(parts: &Parts) -> Option<&str> {
    parts
        .headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
}

/// Состояние приложения: ключи JWT + опциональный репозиторий.
pub use crate::routes::ApiEnv as AppState;

#[async_trait::async_trait]
impl FromRequestParts<crate::routes::ApiEnv> for AuthContext {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let token = bearer_token(parts)
            .ok_or_else(|| auth_error_response(ApiError::Unauthorized))?
            .to_string();
        let claims = state
            .keys
            .verify_access(&token)
            .map_err(|_| auth_error_response(ApiError::Unauthorized))?;
        Ok(Self { claims })
    }
}

/// Пустой успешный ответ проверки доступа — используется демо-роутом F1.
pub async fn role_echo(ctx: AuthContext) -> impl IntoResponse {
    (
        axum::http::StatusCode::OK,
        Json(serde_json::json!({ "role": ctx.role(), "sub": ctx.subject_id() })),
    )
}
