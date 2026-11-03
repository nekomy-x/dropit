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

/// Коды ролей администратора, зафиксированные миграцией `0002_identity.sql`
/// (`admin_user.role`): `superadmin` | `operator` | `readonly`.
pub const ADMIN_ROLE_CODES: [&str; 3] = ["superadmin", "operator", "readonly"];

/// Фактическая роль администратора из БД.
///
/// JWT несёт только `Role::Admin` (одна роль на всю панель), поэтому
/// гранулярность прав берётся из `admin_user.role`. Неизвестный код
/// трактуется как `readonly` (наименьшие привилегии), отсутствующая строка —
/// `None` (доступ запрещён).
pub async fn admin_role_of(state: &AppState, admin_id: uuid::Uuid) -> Option<String> {
    let pool = state.repo.as_ref()?.pool();
    let code: Option<String> =
        sqlx::query_scalar("SELECT role FROM admin_user WHERE id = $1 AND status = 'active'")
            .bind(admin_id)
            .fetch_optional(&pool)
            .await
            .ok()?;
    Some(match code.as_deref() {
        Some("superadmin") => "superadmin".into(),
        Some("operator") => "operator".into(),
        Some("readonly") => "readonly".into(),
        // Новая/неизвестная роль в БД не должна молча давать запись.
        Some(_) => "readonly".into(),
        None => return None,
    })
}

/// Гейт админских мутаций: `readonly` управлять не может.
///
/// Читающие админ-роуты (`GET`) проходят как раньше; гейт вызывается только
/// в пишущих хендлерах (`POST/PATCH/DELETE`) до записи в БД. Без настроенной
/// БД вернуть 503 нельзя отличить от «нет строки», поэтому сначала
/// проверяется наличие пула.
pub async fn require_admin_write(ctx: &AuthContext, state: &AppState) -> Result<(), Response> {
    if ctx.role() != Role::Admin {
        return Err(auth_error_response(ApiError::Forbidden));
    }
    if state.repo.is_none() {
        return Err(auth_error_response(ApiError::ExternalServiceUnavailable));
    }
    match admin_role_of(state, ctx.subject_id()).await {
        Some(role) if role == "readonly" => Err(auth_error_response(ApiError::Forbidden)),
        Some(_) => Ok(()),
        // Строка admin_user не найдена (удалён/заблокирован) — писать нельзя.
        None => Err(auth_error_response(ApiError::Forbidden)),
    }
}

/// Admin-роль текущего субъекта в виде строки (`superadmin`/`operator`/
/// `readonly`); `None` — БД недоступна или админа нет.
pub async fn current_admin_role(ctx: &AuthContext, state: &AppState) -> Option<String> {
    admin_role_of(state, ctx.subject_id()).await
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
