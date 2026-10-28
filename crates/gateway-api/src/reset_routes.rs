//! Сброс пароля клуба / DJ / админа (Test Mode stand, без почты).
//!
//! Контракт — разделы `POST /v1/auth/password/reset-request` и
//! `POST /v1/auth/password/reset` в `docs/openapi.yaml`; фронт уже ждёт
//! именно эти формы (`apps/web-club/src/api/client.ts`).
//!
//! Правила:
//!   * оба роута публичные (как `/v1/auth/:role/login`): авторизация не
//!     нужна, пароль забыт именно потому, что войти нечем;
//!   * `POST /v1/auth/password/reset-request` отвечает `202 {requested:true}`
//!     ВСЕГДА, когда тело корректно — и для несуществующего адреса тоже.
//!     Перечислять пользователей нельзя: разный ответ на известный и
//!     неизвестный e-mail превращает форму в оракул «а есть ли у вас
//!     аккаунт на club.test»;
//!   * `role` — `club` | `dj` | `admin`, по умолчанию `club` (фронт всегда
//!     присылает его явно). Одно и то же имя ящика может существовать в
//!     разных таблицах, поэтому роль — часть ключа поиска;
//!   * лимит — 5 запросов в час на адрес И на IP (таблица
//!     `password_reset_token`, миграция 0015). Превышение → `429
//!     rate_limited`; считается и для неизвестных адресов, насколько это
//!     возможно без журнала запросов (см. `count_reset_requests_for_ip`);
//!   * токен — 32 случайных байта (тот же генератор, что у refresh-токена),
//!     в БД только `sha256(hex)`, TTL 30 минут, single use (атомарный
//!     `UPDATE … RETURNING` в репозитории);
//!   * успешный сброс меняет Argon2id-хеш и отзывает ВСЕ refresh-токены
//!     субъекта: украденная сессия не должна переживать смену пароля;
//!   * `POST /v1/auth/password/reset` — `204` без тела; неизвестный,
//!     использованный и просроченный токен дают один и тот же
//!     `401 invalid_reset_token` (без различения состояния токена).
//!
//! ## Про отсутствие почты (важно, читать перед включением на проде)
//!
//! В репозитории нет SMTP/почтового провайдера в пути сброса: письмо
//! отправлять некуда. Поэтому на стенде есть временный механизм —
//! `DROPIT_RESET_REVEAL_TOKEN=1|true|yes|on` (значения те же, что у
//! `DROPIT_OPEN_SIGNUP`). Когда флаг включён и аккаунт существует, ответ
//! `202` дополнительно содержит `reset_url` (`/reset-password/<token>`) и
//! `expires_at` (RFC3339) — стенд остаётся работоспособным без SMTP, а
//! проверяющий может пройти весь флоу руками. Флаг читается при каждом
//! запросе, чтобы его можно было включить на живом процессе.
//!
//! **Прод обязан работать с выключенным флагом и настоящим мейлером**:
//! `reset_url` в теле ответа — это выдача токена сброса любому, кто знает
//! адрес, то есть обход всей модели безопасности. При выключенном флаге
//! поля просто отсутствуют (`Option` + `skip_serializing_if`), и ответ
//! неотличим от ответа на неизвестный адрес.
//!
//! Независимо от флага ссылка (с сырым токеном, без пароля) пишется в
//! `tracing::info!`: на стенде это единственный способ увидеть письмо,
//! в проде — тот же лог, что у остальных security-событий.
//!
//! Роутер монтируется в `routes::router` БЕЗ `role_route` — как
//! `signup_router`.

use axum::{
    extract::{ConnectInfo, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use chrono::{DateTime, Duration, Utc};
use dropit_common::{ApiError, Role};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth;
use crate::routes::{api_error, api_error_with_code, client_ip, ApiEnv};

/// Минимальная длина пароля — синхронно с `signup_routes.rs`
/// (`MIN_PASSWORD_LEN`): один и тот же пароль обязан проходить и там, и тут.
const MIN_PASSWORD_LEN: usize = 8;
/// Максимальная длина пароля — синхронно с `signup_routes.rs`
/// (`MAX_PASSWORD_LEN`): Argon2id хеширует всё тело, длинный пароль — это
/// CPU-вектор.
const MAX_PASSWORD_LEN: usize = 128;

/// Лимит запросов сброса: 5 на адрес и 5 на IP в час (контракт).
///
/// «Больше 5» = шестой запрос за час получает `429`: ровно пять первых
/// проходят.
const RESET_RATE_LIMIT_PER_HOUR: i64 = 5;

/// Срок жизни токена сброса. 30 минут — компромисс: человек успевает
/// открыть письмо, но украденная ссылка быстро протухает.
const RESET_TOKEN_TTL_MINUTES: i64 = 30;

/// Включающий env-флаг Test Mode (`DROPIT_RESET_REVEAL_TOKEN`).
const REVEAL_ENV: &str = "DROPIT_RESET_REVEAL_TOKEN";

// ============================================================
// Тела запросов и ответы
// ============================================================

/// Тело `POST /v1/auth/password/reset-request`.
#[derive(Deserialize)]
pub struct ResetRequest {
    pub email: String,
    /// `club` | `dj` | `admin`; по умолчанию `club` (фронт присылает всегда).
    #[serde(default)]
    pub role: Option<String>,
}

/// Тело `POST /v1/auth/password/reset`.
#[derive(Deserialize)]
pub struct ResetConfirmRequest {
    /// Одноразовый токен из письма (или из `reset_url` на стенде).
    pub token: String,
    /// Новый пароль (8…=128 символов).
    pub password: String,
}

/// Ответ `POST /v1/auth/password/reset-request`: `202 {requested:true}`.
///
/// `reset_url`/`expires_at` появляются ТОЛЬКО при включённом
/// [`REVEAL_ENV`] и только для существующего аккаунта; при выключенном
/// флаге полей нет вовсе (`skip_serializing_if`), поэтому ответ на
/// известный и неизвестный адрес неразличим.
#[derive(Serialize)]
pub struct ResetRequestedResponse {
    pub requested: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reset_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
}

// ============================================================
// Роутер
// ============================================================

/// Публичные роуты сброса пароля (монтируются в общий роутер).
pub fn reset_router() -> Router<ApiEnv> {
    Router::new()
        .route("/v1/auth/password/reset-request", post(request_reset))
        .route("/v1/auth/password/reset", post(confirm_reset))
}

// ============================================================
// Хендлеры
// ============================================================

/// `POST /v1/auth/password/reset-request` → `202 {requested:true}`.
///
/// Порядок проверок: роль/адрес (400, дешёвые отказы) → БД (503 без
/// `DATABASE_URL`) → лимиты (429) → выпуск токена. Ветка «аккаунт есть» и
/// «аккаунта нет» отличается ТОЛЬКО содержимым БД: наружу обе дают
/// одинаковый `202`.
async fn request_reset(
    State(state): State<ApiEnv>,
    headers: HeaderMap,
    conn: Option<ConnectInfo<std::net::SocketAddr>>,
    Json(req): Json<ResetRequest>,
) -> Response {
    let role = match parse_reset_role(req.role.as_deref()) {
        Ok(r) => r,
        Err(detail) => return validation_failed(&detail),
    };
    let email = normalize_email(&req.email);
    if let Err(detail) = validate_email(&email) {
        return validation_failed(&detail);
    }

    let repo = match state.repo.clone() {
        Some(r) => r,
        None => return database_not_configured(),
    };
    let ip = client_ip(&headers, conn.map(|c| c.0));

    // Лимит: адрес и IP считаются раздельно, превышение любого → 429.
    // Ошибка счётчика не блокирует запрос (fail-open, как у регистрации:
    // лимитер — антифрод, а не условие корректности), инцидент — в warn.
    for (scope, count) in [
        (
            "email",
            repo.count_reset_requests_for_email(role, &email).await,
        ),
        ("ip", repo.count_reset_requests_for_ip(&ip).await),
    ] {
        match count {
            Ok(n) if n >= RESET_RATE_LIMIT_PER_HOUR => {
                return api_error_with_code(
                    StatusCode::TOO_MANY_REQUESTS,
                    "rate_limited",
                    "Too many password reset requests. Try again later.",
                    &format!("reset limit {RESET_RATE_LIMIT_PER_HOUR}/hour per {scope}"),
                )
            }
            Ok(_) => {}
            Err(e) => {
                tracing::warn!(error = %e, scope, "password reset rate-limit counter failed")
            }
        }
    }

    let subject = match repo.find_password_subject(role, &email).await {
        Ok(found) => found,
        Err(_) => return api_error(ApiError::Internal, "db error"),
    };

    let mut reset_url = None;
    let mut expires_at = None;
    if let Some(subject) = subject {
        let raw_token = auth::generate_refresh_token();
        let token_hash = auth::hash_refresh_token(&raw_token);
        let expires = Utc::now() + Duration::minutes(RESET_TOKEN_TTL_MINUTES);
        if repo
            .insert_password_reset_token(
                Uuid::now_v7(),
                role,
                subject.id,
                &token_hash,
                expires,
                &ip,
            )
            .await
            .is_err()
        {
            return api_error(ApiError::Internal, "db error");
        }

        let path = format!("/reset-password/{raw_token}");
        // Ссылка — в лог всегда (на стенде это «письмо»), пароль — никогда.
        let link = format!("{}{}", crate::infra::mail_app_base_url(), path);
        tracing::info!(
            role = role.as_str(),
            subject_id = %subject.id,
            expires_at = %expires,
            link = %link,
            "password reset requested"
        );

        if reveal_token_enabled() {
            reset_url = Some(path);
            expires_at = Some(expires);
        }
    } else {
        // Неизвестный адрес: строку в БД не пишем, ответ — тот же 202.
        // В логе нет ни ссылки, ни токена: придумывать нечего.
        tracing::info!(
            role = role.as_str(),
            "password reset requested for unknown address (no token issued)"
        );
    }

    (
        StatusCode::ACCEPTED,
        Json(ResetRequestedResponse {
            requested: true,
            reset_url,
            expires_at,
        }),
    )
        .into_response()
}

/// `POST /v1/auth/password/reset` → `204 No Content`.
///
/// Неизвестный, использованный и просроченный токен дают один и тот же
/// `401 invalid_reset_token`. Валидация пароля идёт ДО погашения токена:
/// `400` не должен сжигать ссылку, иначе пользователь с опечаткой в
/// пароле вынужден запрашивать новый.
async fn confirm_reset(
    State(state): State<ApiEnv>,
    Json(req): Json<ResetConfirmRequest>,
) -> Response {
    if let Err(detail) = validate_password(&req.password) {
        return validation_failed(&detail);
    }
    let repo = match state.repo.clone() {
        Some(r) => r,
        None => return database_not_configured(),
    };

    let token = req.token.trim();
    let token_hash = auth::hash_refresh_token(token);
    let (role, subject_id) = match repo.consume_password_reset_token(&token_hash).await {
        Ok(Some(subject)) => subject,
        Ok(None) => return invalid_reset_token(),
        Err(_) => return api_error(ApiError::Internal, "db error"),
    };

    let password_hash = match auth::hash_password(&req.password) {
        Ok(h) => h,
        Err(_) => return api_error(ApiError::Internal, "password hashing failed"),
    };
    if repo
        .update_password_hash(role, subject_id, &password_hash)
        .await
        .is_err()
    {
        return api_error(ApiError::Internal, "db error");
    }
    // Украденная сессия не переживает смену пароля. Ошибка ревока не
    // отменяет сброс (пароль уже новый), но обязана быть видна в логе.
    match repo.revoke_all_refresh_tokens(role, subject_id).await {
        Ok(revoked) => tracing::info!(
            role = role.as_str(),
            subject_id = %subject_id,
            revoked,
            "password reset completed"
        ),
        Err(e) => {
            tracing::warn!(error = %e, "password reset: refresh token revoke failed");
            tracing::info!(
                role = role.as_str(),
                subject_id = %subject_id,
                "password reset completed"
            );
        }
    }

    StatusCode::NO_CONTENT.into_response()
}

// ============================================================
// Валидация и разбор
// ============================================================

/// Роль из тела запроса: `None` → `club`; неизвестное значение → `400`.
///
/// Регистр и пробелы нормализуются (`" Club "` = `club`): значение
/// приходит из формы, а не из машины, и отвергать его из-за регистра
/// незачем. Допустимы только парольные роли — `guest` отвергается как
/// неизвестное значение (у гостя нет пароля).
fn parse_reset_role(role: Option<&str>) -> Result<Role, String> {
    let Some(raw) = role.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(Role::Club);
    };
    match raw.to_ascii_lowercase().as_str() {
        "club" => Ok(Role::Club),
        "dj" => Ok(Role::Dj),
        "admin" => Ok(Role::Admin),
        other => Err(format!(
            "role must be one of club, dj, admin (got {other:?})"
        )),
    }
}

/// `400 validation_failed` в формате [`dropit_common::ErrorEnvelope`]
/// (тот же конверт, что у регистрации).
fn validation_failed(detail: &str) -> Response {
    api_error_with_code(
        StatusCode::BAD_REQUEST,
        "validation_failed",
        "The request failed validation.",
        detail,
    )
}

/// `401 invalid_reset_token` — одно сообщение на все случаи жизни токена.
fn invalid_reset_token() -> Response {
    api_error_with_code(
        StatusCode::UNAUTHORIZED,
        "invalid_reset_token",
        "This password reset link is invalid or has expired.",
        "reset token unknown, already used or expired",
    )
}

/// `503 external_service_unavailable` — БД не настроена.
///
/// Тот же статус и та же причина, что у регистрации (там `503` отдаёт
/// [`ApiError::ExternalServiceUnavailable`]), но код конверта здесь явный:
/// контракт сброса называет именно `external_service_unavailable`, а
/// `ApiError::code()` даёт более короткое `external_unavailable`.
fn database_not_configured() -> Response {
    api_error_with_code(
        StatusCode::SERVICE_UNAVAILABLE,
        "external_service_unavailable",
        "An external service is unavailable.",
        "database not configured",
    )
}

/// E-mail: одна `@`, непустые части, домен с точкой, без пробелов.
///
/// Дублирует (а не импортирует) `signup_routes::validate_email`: правило
/// одно и то же — адрес нигде не подтверждается, нужен лишь вменяемый
/// формат, — но чужой модуль под этот роут не реструктурируется.
fn validate_email(email: &str) -> Result<(), String> {
    if email.is_empty() {
        return Err("email must not be empty".to_string());
    }
    if email.len() > 254 {
        return Err("email is too long".to_string());
    }
    if email.chars().any(char::is_whitespace) {
        return Err("email must not contain spaces".to_string());
    }
    let mut parts = email.split('@');
    let local = parts.next().unwrap_or_default();
    let domain = parts.next().unwrap_or_default();
    if local.is_empty() || domain.is_empty() || parts.next().is_some() {
        return Err("email is malformed".to_string());
    }
    if !domain.contains('.') || domain.starts_with('.') || domain.ends_with('.') {
        return Err("email domain is malformed".to_string());
    }
    Ok(())
}

/// Пароль: 8…=[`MAX_PASSWORD_LEN`] символов (границы — как у регистрации).
fn validate_password(password: &str) -> Result<(), String> {
    let len = password.chars().count();
    if len < MIN_PASSWORD_LEN {
        return Err(format!(
            "password must be at least {MIN_PASSWORD_LEN} chars"
        ));
    }
    if len > MAX_PASSWORD_LEN {
        return Err(format!("password must be at most {MAX_PASSWORD_LEN} chars"));
    }
    Ok(())
}

/// Нормализация e-mail: trim + нижний регистр (как при регистрации и
/// логине; сравнение в БД всё равно `citext`).
fn normalize_email(email: &str) -> String {
    email.trim().to_lowercase()
}

/// Включён ли Test Mode-флаг [`REVEAL_ENV`] (`1|true|yes|on`).
///
/// Читается на каждом запросе (не при старте), чтобы флаг можно было
/// включить на стенде без перезапуска. Выключенное состояние — безопасный
/// дефолт: всё остальное, включая `0`, `false`, мусор и отсутствие
/// переменной, НЕ отдаёт ссылку в ответе.
fn reveal_token_enabled() -> bool {
    match std::env::var(REVEAL_ENV) {
        Ok(v) => matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        ),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reset_role_defaults_to_club_and_rejects_unknown() {
        assert_eq!(parse_reset_role(None).unwrap(), Role::Club);
        assert_eq!(parse_reset_role(Some("")).unwrap(), Role::Club);
        assert_eq!(parse_reset_role(Some("  ")).unwrap(), Role::Club);
        assert_eq!(parse_reset_role(Some("club")).unwrap(), Role::Club);
        assert_eq!(parse_reset_role(Some("DJ")).unwrap(), Role::Dj);
        assert_eq!(parse_reset_role(Some(" admin ")).unwrap(), Role::Admin);
        // Регистр и пробелы нормализуются — значение приходит из формы.
        assert_eq!(parse_reset_role(Some(" Club ")).unwrap(), Role::Club);
        // Роль есть, но не из контракта (в т.ч. гость — у него нет пароля).
        for bad in ["guest", "owner", "dj!", "administrator"] {
            assert!(
                parse_reset_role(Some(bad)).is_err(),
                "должна быть отвергнута: {bad:?}"
            );
        }
    }

    #[test]
    fn email_validation_matches_signup_rules() {
        for bad in [
            "",
            "no-at-sign",
            "@nodomain.test",
            "user@",
            "user@@double.test",
            "user@nodot",
            "with space@test.io",
        ] {
            assert!(
                validate_email(bad).is_err(),
                "должен быть отклонён: {bad:?}"
            );
        }
        for good in ["owner@club.test", "a.b+c@sub.domain.io"] {
            assert!(validate_email(good).is_ok(), "должен пройти: {good:?}");
        }
        assert_eq!(
            normalize_email("  Owner@Club.TEST "),
            "owner@club.test",
            "адрес нормализуется перед поиском"
        );
    }

    #[test]
    fn password_bounds_match_signup() {
        assert!(validate_password("1234567").is_err(), "7 символов — мало");
        assert!(validate_password("12345678").is_ok());
        assert!(validate_password(&"x".repeat(MAX_PASSWORD_LEN)).is_ok());
        assert!(validate_password(&"x".repeat(MAX_PASSWORD_LEN + 1)).is_err());
    }

    #[test]
    fn reveal_flag_reads_documented_values() {
        let prev = std::env::var(REVEAL_ENV).ok();
        for v in ["1", "true", "TRUE", " yes ", "on"] {
            std::env::set_var(REVEAL_ENV, v);
            assert!(
                reveal_token_enabled(),
                "{v:?} должен включать выдачу ссылки"
            );
        }
        for v in ["0", "false", "no", "off", "", "garbage"] {
            std::env::set_var(REVEAL_ENV, v);
            assert!(
                !reveal_token_enabled(),
                "{v:?} не должен включать выдачу ссылки"
            );
        }
        std::env::remove_var(REVEAL_ENV);
        assert!(!reveal_token_enabled(), "без переменной флаг выключен");
        match prev {
            Some(v) => std::env::set_var(REVEAL_ENV, v),
            None => std::env::remove_var(REVEAL_ENV),
        }
    }
}
