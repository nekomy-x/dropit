//! Открытая регистрация клуба и DJ (Test Mode stand).
//!
//! Контракт — `docs/signup-contract.md` (единственный источник правды для
//! панелей и backend на время реализации; после реализации продублирован
//! в `docs/openapi.yaml`).
//!
//! Правила:
//!   * регистрация открытая, но только при `DROPIT_OPEN_SIGNUP=1`; иначе
//!     оба эндпоинта отвечают `403 signup_disabled` (флаг читается при
//!     старте процесса и живёт в [`ApiEnv::open_signup`]);
//!   * аккаунт сразу рабочий (Test Mode): клуб `status='active'`, DJ
//!     `status='active'` с `club_id=NULL` (фрилансер) или привязкой по
//!     `club_slug`;
//!   * ответ — сразу пара токенов (авто-логин), выданная тем же кодом, что
//!     обычный логин ([`crate::routes::issue_pair`]): refresh-ротация,
//!     reuse-детекция и RBAC работают без отдельной ветки;
//!   * пароли — Argon2id (`auth::hash_password`);
//!   * лимит — 10 регистраций с одного IP в час (`signup_attempt`);
//!   * админ публично не регистрируется: `POST /v1/auth/admin/register`
//!     всегда `403 forbidden` (роут существует, чтобы клиент получал
//!     внятный отказ, а не 404).
//!
//! Роутер монтируется в `routes::router` БЕЗ `role_route`: это публичные
//! роуты (как `/v1/auth/:role/login`), авторизация им не нужна.

use axum::{
    extract::{ConnectInfo, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use dropit_common::{ApiError, Role};
use dropit_infra_db::IdentityError;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth;
use crate::routes::{api_error, api_error_with_code, client_ip, ApiEnv, TokenPair};

/// Минимальная длина пароля (контракт: короче 8 → `400 validation_failed`).
const MIN_PASSWORD_LEN: usize = 8;
/// Максимальная длина пароля: Argon2id хеширует всё тело, длинный пароль —
/// это CPU-вектор (10 КБ пароля = заметная нагрузка на ядро). 128 — с запасом.
const MAX_PASSWORD_LEN: usize = 128;
/// Максимальная длина имени/display_name (колонки TEXT, но UI-поле короче).
const MAX_NAME_LEN: usize = 120;
/// Сколько суффиксов перебираем при коллизии slug (`-2` … `-20`).
const SLUG_MAX_SUFFIX: u32 = 20;

// ============================================================
// Тела запросов и ответы
// ============================================================

/// Тело `POST /v1/auth/club/register`.
#[derive(Deserialize)]
pub struct ClubRegisterRequest {
    pub name: String,
    pub email: String,
    pub password: String,
    #[serde(default)]
    pub city: Option<String>,
    /// ISO-3166 alpha-2 (Q13), необязательно.
    #[serde(default)]
    pub country: Option<String>,
}

/// Тело `POST /v1/auth/dj/register`.
#[derive(Deserialize)]
pub struct DjRegisterRequest {
    pub display_name: String,
    pub email: String,
    pub password: String,
    /// Slug клуба для привязки; без него DJ — фрилансер (`club_id = NULL`).
    #[serde(default)]
    pub club_slug: Option<String>,
}

/// Клуб в ответе регистрации (доп. поле к [`TokenPair`], клиенты могут
/// игнорировать — контракт §`POST /v1/auth/club/register`).
#[derive(Serialize)]
pub struct RegisteredClub {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub email: String,
}

/// DJ в ответе регистрации.
#[derive(Serialize)]
pub struct RegisteredDj {
    pub id: Uuid,
    pub display_name: String,
    pub email: String,
    /// `null` — фрилансер (Test Mode допустим).
    pub club_id: Option<Uuid>,
}

/// Ответ регистрации клуба: `TokenPair` + `club`.
///
/// Поля `TokenPair` разворачиваются в тот же объект (`#[serde(flatten)]`):
/// фронт получает `access_token`/`refresh_token`/`token_type`/`expires_in`
/// ровно там же, где у логина, и не разбирает вложенность.
#[derive(Serialize)]
pub struct ClubRegisterResponse {
    #[serde(flatten)]
    pub tokens: TokenPair,
    pub club: RegisteredClub,
}

/// Ответ регистрации DJ: `TokenPair` + `dj`.
#[derive(Serialize)]
pub struct DjRegisterResponse {
    #[serde(flatten)]
    pub tokens: TokenPair,
    pub dj: RegisteredDj,
}

// ============================================================
// Роутер
// ============================================================

/// Публичные роуты открытой регистрации (монтируются в общий роутер).
pub fn signup_router() -> Router<ApiEnv> {
    Router::new()
        .route("/v1/auth/club/register", post(register_club))
        .route("/v1/auth/dj/register", post(register_dj))
        .route("/v1/auth/admin/register", post(register_admin))
}

// ============================================================
// Хендлеры
// ============================================================

/// `POST /v1/auth/club/register` → `201` + токены + `club`.
async fn register_club(
    State(state): State<ApiEnv>,
    headers: HeaderMap,
    conn: Option<ConnectInfo<std::net::SocketAddr>>,
    Json(req): Json<ClubRegisterRequest>,
) -> Response {
    if !state.open_signup {
        return signup_disabled();
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

    // Валидация до обращения к БД: дешёвые отказы не тратят лимит и не
    // нагружают Argon2id.
    let name = req.name.trim().to_string();
    let email = normalize_email(&req.email);
    let city = req.city.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let country = req
        .country
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    if let Err(detail) = validate_name(&name, "name")
        .and_then(|_| validate_email(&email))
        .and_then(|_| validate_password(&req.password))
        .and_then(|_| validate_country(country))
    {
        return validation_failed(&detail);
    }

    let ip = client_ip(&headers, conn.map(|c| c.0));
    if let Some(resp) = check_signup_limit(&repo, &ip, state.signup_rate_per_hour).await {
        return resp;
    }

    let password_hash = match auth::hash_password(&req.password) {
        Ok(h) => h,
        Err(_) => return api_error(ApiError::Internal, "password hashing failed"),
    };

    let club_id = Uuid::now_v7();
    let base_slug = slugify(&name);
    // Вставка с ретраем по UNIQUE(slug): гонка двух одновременных
    // регистраций с одинаковым именем даёт `-2`, `-3`, … E-mail проверяется
    // той же вставкой (UNIQUE club.email) — отдельный SELECT не нужен и не
    // создаёт TOCTOU-окно.
    let mut last_slug_error = false;
    for suffix in 1..=SLUG_MAX_SUFFIX {
        let slug = slug_candidate(&base_slug, suffix);
        match repo
            .create_registered_club(club_id, &name, &slug, &email, &password_hash, city, country)
            .await
        {
            Ok(()) => {
                // Лимит считаем ПОСЛЕ успеха: неудачные попытки (409/400)
                // не должны съедать квоту живого пользователя.
                let _ = repo.record_signup_attempt(&ip, Role::Club).await;
                tracing::info!(
                    club_id = %club_id,
                    slug = %slug,
                    "club_created (open signup)"
                );
                // Аудит: репозитория activity нет, поэтому audit_log напрямую
                // (та же таблица, что у master-входа) + trace выше.
                let _ = sqlx::query(
                    "INSERT INTO audit_log (actor_type, actor_id, action, entity, diff) \
                     VALUES ('club', $1, 'club_created', $2, $3)",
                )
                .bind(club_id)
                .bind(club_id.to_string())
                .bind(serde_json::json!({ "source": "open_signup", "slug": slug }))
                .execute(&repo.pool())
                .await;
                let club = RegisteredClub {
                    id: club_id,
                    name,
                    slug,
                    email,
                };
                return match crate::routes::issue_pair_parts(
                    &state,
                    Role::Club,
                    club_id,
                    repo.clone(),
                )
                .await
                {
                    Ok(tokens) => (
                        StatusCode::CREATED,
                        Json(ClubRegisterResponse { tokens, club }),
                    )
                        .into_response(),
                    Err(resp) => resp,
                };
            }
            Err(IdentityError::SlugTaken) => {
                last_slug_error = true;
                continue;
            }
            Err(IdentityError::EmailTaken) => {
                return api_error_with_code(
                    StatusCode::CONFLICT,
                    "email_taken",
                    "This e-mail is already registered.",
                    "email already taken",
                )
            }
            Err(_) => return api_error(ApiError::Internal, "db error"),
        }
    }
    if last_slug_error {
        // Все суффиксы заняты — практически невозможно, но лучше 409 с
        // внятным кодом, чем 500.
        return api_error_with_code(
            StatusCode::CONFLICT,
            "slug_taken",
            "This club name is already in use.",
            "slug candidates exhausted",
        );
    }
    api_error(ApiError::Internal, "db error")
}

/// `POST /v1/auth/dj/register` → `201` + токены + `dj`.
async fn register_dj(
    State(state): State<ApiEnv>,
    headers: HeaderMap,
    conn: Option<ConnectInfo<std::net::SocketAddr>>,
    Json(req): Json<DjRegisterRequest>,
) -> Response {
    if !state.open_signup {
        return signup_disabled();
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

    let display_name = req.display_name.trim().to_string();
    let email = normalize_email(&req.email);
    let club_slug = req
        .club_slug
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    if let Err(detail) = validate_name(&display_name, "display_name")
        .and_then(|_| validate_email(&email))
        .and_then(|_| validate_password(&req.password))
    {
        return validation_failed(&detail);
    }

    let ip = client_ip(&headers, conn.map(|c| c.0));
    if let Some(resp) = check_signup_limit(&repo, &ip, state.signup_rate_per_hour).await {
        return resp;
    }

    // Привязка к клубу по slug: неизвестный slug → 404 club_not_found
    // (контракт). Без slug — фрилансер, club_id = NULL.
    let club_id = match club_slug {
        Some(slug) => match repo.find_club_id_by_slug(slug).await {
            Ok(Some(id)) => Some(id),
            Ok(None) => {
                return api_error_with_code(
                    StatusCode::NOT_FOUND,
                    "club_not_found",
                    "Club not found.",
                    "unknown club_slug",
                )
            }
            Err(_) => return api_error(ApiError::Internal, "db error"),
        },
        None => None,
    };

    let password_hash = match auth::hash_password(&req.password) {
        Ok(h) => h,
        Err(_) => return api_error(ApiError::Internal, "password hashing failed"),
    };

    let dj_id = Uuid::now_v7();
    match repo
        .create_registered_dj(dj_id, &display_name, &email, &password_hash, club_id)
        .await
    {
        Ok(()) => {
            let _ = repo.record_signup_attempt(&ip, Role::Dj).await;
            tracing::info!(
                dj_id = %dj_id,
                club_id = ?club_id,
                "dj_created (open signup)"
            );
            let _ = sqlx::query(
                "INSERT INTO audit_log (actor_type, actor_id, action, entity, diff) \
                 VALUES ('dj', $1, 'dj_created', $2, $3)",
            )
            .bind(dj_id)
            .bind(dj_id.to_string())
            .bind(serde_json::json!({ "source": "open_signup", "club_id": club_id }))
            .execute(&repo.pool())
            .await;
            let dj = RegisteredDj {
                id: dj_id,
                display_name,
                email,
                club_id,
            };
            match crate::routes::issue_pair_parts(&state, Role::Dj, dj_id, repo.clone()).await {
                Ok(tokens) => {
                    (StatusCode::CREATED, Json(DjRegisterResponse { tokens, dj })).into_response()
                }
                Err(resp) => resp,
            }
        }
        Err(IdentityError::EmailTaken) => api_error_with_code(
            StatusCode::CONFLICT,
            "email_taken",
            "This e-mail is already registered.",
            "email already taken",
        ),
        Err(_) => api_error(ApiError::Internal, "db error"),
    }
}

/// `POST /v1/auth/admin/register` — админ публично НЕ регистрируется.
///
/// Роут нужен, чтобы клиент получил внятный `403 forbidden`, а не 404
/// (контракт §Ограничения). Тело не читаем: ответ одинаков при любом.
async fn register_admin() -> Response {
    api_error_with_code(
        StatusCode::FORBIDDEN,
        "forbidden",
        "Admin accounts are not created via public registration.",
        "admin signup is not allowed",
    )
}

// ============================================================
// Валидация
// ============================================================

/// Отказ флага: `403 signup_disabled` (контракт).
fn signup_disabled() -> Response {
    api_error_with_code(
        StatusCode::FORBIDDEN,
        "signup_disabled",
        "Open registration is disabled.",
        "DROPIT_OPEN_SIGNUP is not enabled",
    )
}

/// Лимит регистраций/час/IP → `429 rate_limited`.
///
/// Сам лимит — из окружения (`DROPIT_SIGNUP_RATE_PER_HOUR`, дефолт 10,
/// на Test Mode стенде 100): читается один раз при старте в
/// [`ApiEnv::signup_rate_per_hour`], чтобы значение не «мигало».
///
/// Ошибка БД при подсчёте не блокирует регистрацию: лимит — антифрод-мера,
/// а не условие корректности (fail-open осознанно, иначе падение счётчика
/// ломает онбординг). Инцидент виден в логах.
async fn check_signup_limit(
    repo: &std::sync::Arc<dropit_infra_db::IdentityRepo>,
    ip: &str,
    limit: i64,
) -> Option<Response> {
    match repo.count_signups_for_ip(ip).await {
        Ok(n) if n >= limit => Some(api_error_with_code(
            StatusCode::TOO_MANY_REQUESTS,
            "rate_limited",
            "Too many registrations from this address. Try again later.",
            &format!("signup limit {limit}/hour per ip"),
        )),
        Ok(_) => None,
        Err(e) => {
            tracing::warn!(error = %e, "signup rate-limit counter failed, allowing signup");
            None
        }
    }
}

/// `400 validation_failed` в формате [`dropit_common::ErrorEnvelope`].
fn validation_failed(detail: &str) -> Response {
    api_error_with_code(
        StatusCode::BAD_REQUEST,
        "validation_failed",
        "The request failed validation.",
        detail,
    )
}

/// Имя/display_name: непустое, не длиннее [`MAX_NAME_LEN`].
///
/// Валидаторы возвращают `Result<(), String>` с деталью для
/// `details.detail`, а не готовый `Response`: `Response` в `Err` — крупный
/// тип (clippy::result_large_err), а конверт `validation_failed` один.
fn validate_name(value: &str, field: &str) -> Result<(), String> {
    if value.is_empty() {
        return Err(format!("{field} must not be empty"));
    }
    if value.chars().count() > MAX_NAME_LEN {
        return Err(format!("{field} must be at most {MAX_NAME_LEN} chars"));
    }
    Ok(())
}

/// E-mail: одна `@`, непустые части, домен с точкой, без пробелов.
/// Сознательно без полноценного RFC-парсера: адрес не подтверждается
/// (открытая регистрация без верификации), нужен лишь вменяемый формат.
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

/// Пароль: 8..=[`MAX_PASSWORD_LEN`] символов (контракт: короче 8 — 400).
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

/// Страна: ISO-3166 alpha-2 (ровно 2 латинские буквы), если задана.
fn validate_country(country: Option<&str>) -> Result<(), String> {
    let Some(c) = country else { return Ok(()) };
    if c.len() != 2 || !c.chars().all(|ch| ch.is_ascii_alphabetic()) {
        return Err("country must be ISO-3166 alpha-2".to_string());
    }
    Ok(())
}

/// Нормализация e-mail: trim + нижний регистр. `citext` сравнивает без учёта
/// регистра и так, но нормализация делает ответ (`club.email`) предсказуемым
/// и совпадает с тем, что увидит пользователь при логине.
fn normalize_email(email: &str) -> String {
    email.trim().to_lowercase()
}

// ============================================================
// Slug
// ============================================================

/// Таблица транслитерации RU → латиница. Slug клуба должен быть латинским
/// (`[a-z0-9-]`): имена в Test Mode — «Barrio Latino», «Клуб 12», «Подвал».
const TRANSLIT: &[(char, &str)] = &[
    ('а', "a"),
    ('б', "b"),
    ('в', "v"),
    ('г', "g"),
    ('д', "d"),
    ('е', "e"),
    ('ё', "e"),
    ('ж', "zh"),
    ('з', "z"),
    ('и', "i"),
    ('й', "y"),
    ('к', "k"),
    ('л', "l"),
    ('м', "m"),
    ('н', "n"),
    ('о', "o"),
    ('п', "p"),
    ('р', "r"),
    ('с', "s"),
    ('т', "t"),
    ('у', "u"),
    ('ф', "f"),
    ('х', "kh"),
    ('ц', "ts"),
    ('ч', "ch"),
    ('ш', "sh"),
    ('щ', "shch"),
    ('ъ', ""),
    ('ы', "y"),
    ('ь', ""),
    ('э', "e"),
    ('ю', "yu"),
    ('я', "ya"),
    ('і', "i"),
    ('ї', "yi"),
    ('є', "ye"),
    ('ґ', "g"),
    ('ў', "u"),
];

/// Генерирует slug из названия: транслит, нижний регистр, `[a-z0-9-]`,
/// без повторных и краевых дефисов.
///
/// Если после нормализации не осталось ни одного `[a-z0-9]` (например,
/// имя из одних эмодзи), возвращается `"club"` — базовый кандидат, а
/// уникальность обеспечит суффикс в [`slug_candidate`].
pub fn slugify(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        let lower = ch.to_lowercase().next().unwrap_or(ch);
        if let Some((_, latin)) = TRANSLIT.iter().find(|(c, _)| *c == lower) {
            out.push_str(latin);
        } else if lower.is_ascii_alphanumeric() {
            out.push(lower);
        } else {
            out.push('-');
        }
    }
    let normalized = out
        .split('-')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if normalized.is_empty() {
        "club".to_string()
    } else {
        // Ограничение длины: slug попадает в URL и в UNIQUE-индекс; 60
        // символов хватает с запасом на суффикс `-20`.
        normalized.chars().take(60).collect()
    }
}

/// Кандидат slug для попытки `suffix` (1 — базовый, 2 — `-2`, …).
fn slug_candidate(base: &str, suffix: u32) -> String {
    if suffix <= 1 {
        base.to_string()
    } else {
        format!("{base}-{suffix}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_latin_and_digits() {
        assert_eq!(slugify("Barrio Latino"), "barrio-latino");
        assert_eq!(slugify("  Club  24/7  "), "club-24-7");
        assert_eq!(slugify("DJ_Noche!"), "dj-noche");
    }

    #[test]
    fn slugify_transliterates_cyrillic() {
        assert_eq!(slugify("Клуб Подвал"), "klub-podval");
        assert_eq!(slugify("Щука"), "shchuka");
        assert_eq!(slugify("Ёлка"), "elka");
        // Мягкий/твёрдый знак не дают дефисов в середине слова.
        assert_eq!(slugify("Объект"), "obekt");
    }

    #[test]
    fn slugify_falls_back_when_nothing_left() {
        // Эмодзи/пунктуация без букв и цифр → базовый кандидат.
        assert_eq!(slugify("🎧🎧"), "club");
        assert_eq!(slugify("!!!"), "club");
        assert_eq!(slugify(""), "club");
    }

    #[test]
    fn slugify_truncates_long_names() {
        let long = "a".repeat(200);
        assert_eq!(slugify(&long).chars().count(), 60);
    }

    #[test]
    fn slug_candidates_are_suffixed() {
        assert_eq!(slug_candidate("barrio", 1), "barrio");
        assert_eq!(slug_candidate("barrio", 2), "barrio-2");
        assert_eq!(slug_candidate("barrio", 20), "barrio-20");
    }

    #[test]
    fn email_validation_rejects_garbage() {
        for bad in [
            "",
            "no-at-sign",
            "@nodomain.test",
            "user@",
            "user@@double.test",
            "user@nodot",
            "user@.test",
            "user@test.",
            "with space@test.io",
        ] {
            assert!(
                validate_email(bad).is_err(),
                "должен быть отклонён: {bad:?}"
            );
        }
        for good in ["owner@barrio.test", "a.b+c@sub.domain.io"] {
            assert!(validate_email(good).is_ok(), "должен пройти: {good:?}");
        }
    }

    #[test]
    fn password_length_bounds() {
        assert!(validate_password("1234567").is_err(), "7 символов — мало");
        assert!(validate_password("12345678").is_ok());
        assert!(validate_password(&"x".repeat(MAX_PASSWORD_LEN + 1)).is_err());
    }

    #[test]
    fn name_and_country_validation() {
        assert!(validate_name("", "name").is_err());
        assert!(validate_name(&"x".repeat(MAX_NAME_LEN + 1), "name").is_err());
        assert!(validate_name("Barrio Latino", "name").is_ok());
        assert!(validate_country(None).is_ok());
        assert!(validate_country(Some("ES")).is_ok());
        assert!(validate_country(Some("esp")).is_err());
        assert!(validate_country(Some("E1")).is_err());
    }

    #[test]
    fn email_is_normalized() {
        assert_eq!(normalize_email("  Owner@Barrio.TEST "), "owner@barrio.test");
    }
}
