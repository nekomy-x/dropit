//! Админский каталог пользователей и матрица ролей.
//!
//! Контракт и обоснование — `docs/audit/web-admin-gaps.md` §7.1 (users) и
//! §7.6 (roles/permissions). До этого модуля админ-панель держала список
//! пользователей и матрицу прав в `localStorage` (`apps/web-admin/src/data/
//! adminMock.ts`), то есть в проде «управление доступами» ничего не меняло.
//!
//! Границы модуля:
//!   * только чтение/мутации каталога (`admin_user`, `club`, `dj`, гости) и
//!     read-only матрица прав, выведенная из фактического кода RBAC;
//!   * деньги, сессии и выплаты здесь не трогаются (это `panel_routes.rs`).
//!
//! Решения по контракту (там, где §7.1 допускает трактовку):
//!   * «гость» в каталоге — строка `guest_account` (управляемая сущность:
//!     e-mail/верификация). Анонимные `guest_device` без аккаунта — не
//!     пользователи каталога: они живут в разрезе клуба
//!     (`GET /v1/admin/guests?club_id=`, `panel_routes.rs`). `last_seen_at`
//!     и `display_name` гостя берутся из его устройств;
//!   * `role` в ответе — principal-роль (`admin|club_staff|dj|guest`),
//!     у админа дополнительно `admin_role` (`superadmin|operator|readonly`) —
//!     без него UI не отличит readonly-админа от superadmin;
//!   * `PATCH {role}` для админа меняет `admin_user.role` (подроль), для
//!     остальных роль неизменяема (тип субъекта задан таблицей);
//!   * `DELETE` — мягкая блокировка (`status='blocked'`), гостю — бан его
//!     устройств; строки БД не удаляются (на них ссылаются сессии/история);
//!   * `status='invited'` не выставляется мутацией: это производное
//!     состояние (pending_verification клуба / непринятое приглашение DJ /
//!     неверифицированный гость), вернуть в него сущность нельзя.
//!
//! Каждая мутация пишет `audit_log` (`actor_type='admin'`, `entity`=<id>,
//! `diff={entity_type, entity_id, before, after}`) в той же транзакции, что
//! и сама запись: аудит и изменение не расходятся.
//!
//! Секреты наружу не отдаются: единственное исключение — одноразовый
//! enrollment-ответ `POST /v1/admin/users` (`totp_secret`, `otpauth_url`),
//! без него созданный админ не сможет войти (2FA обязательна).

use crate::rbac::{
    auth_error_response, current_admin_role, require_admin_write, AuthContext, ADMIN_ROLE_CODES,
};
use crate::routes::{api_error, ApiEnv};
use axum::{
    extract::{
        rejection::{JsonRejection, QueryRejection},
        Path, Query, State,
    },
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use dropit_common::{ApiError, ErrorEnvelope, Role};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::PgPool;
use std::collections::HashMap;
use uuid::Uuid;

/// Длина страницы по умолчанию и верхняя граница (`limit` клампится).
const DEFAULT_LIMIT: i64 = 50;
const MAX_LIMIT: i64 = 200;
/// Максимальная длина поисковой строки `q`.
const MAX_QUERY_LEN: usize = 200;
/// Граница display_name (имя не пустое и не бесконечное).
const MAX_DISPLAY_NAME_LEN: usize = 120;
/// Пароль нового админа: те же границы, что у регистрации/сброса.
const MIN_PASSWORD_LEN: usize = 8;
const MAX_PASSWORD_LEN: usize = 128;
/// Сколько id принимает bulk за один вызов.
const MAX_BULK_IDS: usize = 200;

/// Principal-роли каталога (значения фильтра `role`).
const DIRECTORY_ROLES: [&str; 4] = ["admin", "club_staff", "dj", "guest"];
/// Нормализованные статусы каталога (значения фильтра `status`).
const DIRECTORY_STATUSES: [&str; 4] = ["active", "invited", "suspended", "blocked"];
/// Статусы, которые можно выставить мутацией (`invited` — производное).
const MUTABLE_STATUSES: [&str; 3] = ["active", "suspended", "blocked"];
/// Допустимые действия bulk-операции.
const BULK_ACTIONS: [&str; 4] = ["approve", "suspend", "block", "role"];

/// Роутер админского каталога пользователей и ролей.
pub fn admin_users_router() -> Router<ApiEnv> {
    Router::new()
        .route(
            "/v1/admin/users",
            get(admin_users_list).post(admin_users_create),
        )
        // Статический сегмент регистрируется рядом с `:id` — axum отдаёт
        // приоритет статическому (та же схема, что `/v1/club/djs/invite`
        // и `/v1/club/djs/:id` в panel_routes.rs).
        .route("/v1/admin/users/bulk", post(admin_users_bulk))
        .route(
            "/v1/admin/users/:id",
            get(admin_users_get)
                .patch(admin_users_patch)
                .delete(admin_users_delete),
        )
        .route("/v1/admin/roles", get(admin_roles_matrix))
}

// ============================================================
// Общие помощники
// ============================================================

/// Проверка роли JWT: все маршруты модуля — только `Role::Admin`.
fn admin_only(ctx: &AuthContext) -> Option<Response> {
    (ctx.role() != Role::Admin).then(|| auth_error_response(ApiError::Forbidden))
}

/// Пул БД или `503`-конверт (как `panel_routes::pool_of`).
#[allow(clippy::result_large_err)]
fn pool_of(state: &ApiEnv) -> Result<PgPool, Response> {
    state.repo.as_ref().map(|r| r.pool()).ok_or_else(|| {
        api_error(
            ApiError::ExternalServiceUnavailable,
            "database not configured",
        )
    })
}

/// Конверт ошибки с бизнес-кодом (`role_not_creatable`, `last_superadmin`,
/// `email_taken`, …): стабильный `code` + `details`, без нового типа.
fn api_error_code(status: StatusCode, code: &str, message: &str, details: Value) -> Response {
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

fn validation_error(detail: &str, field: &str) -> Response {
    api_error_code(
        StatusCode::UNPROCESSABLE_ENTITY,
        "validation_error",
        "The request failed validation.",
        json!({ "detail": detail, "field": field }),
    )
}

fn bad_request(detail: &str) -> Response {
    api_error(ApiError::BadRequest, detail)
}

/// Разбор `:id` без axum-текста: мусорный uuid → 400 в конверте.
#[allow(clippy::result_large_err)]
fn parse_id(raw: &str) -> Result<Uuid, Response> {
    Uuid::parse_str(raw.trim()).map_err(|_| bad_request("invalid user id"))
}

/// Проверка query-параметра из фиксированного набора значений.
#[allow(clippy::result_large_err)]
fn optional_enum_param(
    params: &HashMap<String, String>,
    name: &str,
    allowed: &[&'static str],
) -> Result<Option<String>, Response> {
    let Some(raw) = params.get(name) else {
        return Ok(None);
    };
    let value = raw.trim();
    if value.is_empty() {
        return Ok(None);
    }
    if !allowed.contains(&value) {
        return Err(api_error_code(
            StatusCode::BAD_REQUEST,
            "bad_request",
            "Unsupported query parameter value.",
            json!({ "parameter": name, "value": value, "allowed": allowed }),
        ));
    }
    Ok(Some(value.to_string()))
}

/// `%…%` для ILIKE с экранированием `\`, `%` и `_` (иначе `q=%` матчил бы
/// всё, а `_` — любой символ).
fn like_pattern(q: &str) -> String {
    let mut pattern = String::with_capacity(q.len() + 2);
    pattern.push('%');
    for ch in q.chars() {
        if matches!(ch, '\\' | '%' | '_') {
            pattern.push('\\');
        }
        pattern.push(ch);
    }
    pattern.push('%');
    pattern
}

/// Курсор keyset: `"<RFC3339>\x1f<uuid>"` (формат `panel_routes.rs`).
fn parse_cursor(raw: &str) -> Option<(DateTime<Utc>, Uuid)> {
    let (ts, id) = raw.rsplit_once('\u{1f}')?;
    let ts = DateTime::parse_from_rfc3339(ts).ok()?.with_timezone(&Utc);
    let id = id.parse::<Uuid>().ok()?;
    Some((ts, id))
}

fn is_email_like(email: &str) -> bool {
    if email.is_empty() || email.chars().count() > 254 {
        return false;
    }
    if email.chars().any(char::is_whitespace) {
        return false;
    }
    let mut parts = email.split('@');
    let (Some(local), Some(domain), None) = (parts.next(), parts.next(), parts.next()) else {
        return false;
    };
    !local.is_empty() && !domain.is_empty() && domain.contains('.')
}

#[allow(clippy::result_large_err)]
fn clean_email(raw: &str) -> Result<String, Response> {
    let email = raw.trim();
    if !is_email_like(email) {
        return Err(validation_error("email is not a valid address", "email"));
    }
    Ok(email.to_string())
}

#[allow(clippy::result_large_err)]
fn clean_display_name(raw: &str) -> Result<String, Response> {
    let name = raw.trim();
    if name.is_empty() {
        return Err(validation_error(
            "display_name must not be empty",
            "display_name",
        ));
    }
    if name.chars().count() > MAX_DISPLAY_NAME_LEN {
        return Err(validation_error("display_name is too long", "display_name"));
    }
    Ok(name.to_string())
}

/// SQLSTATE 23505 (unique_violation) → 409 `email_taken`.
fn is_unique_violation(err: &sqlx::Error) -> bool {
    matches!(err, sqlx::Error::Database(db) if db.code().as_deref() == Some("23505"))
}

/// Argon2id-хеш пароля: тот же helper, что проверяет логин (`auth`).
#[allow(clippy::result_large_err)]
fn hash_password(password: &str) -> Result<String, Response> {
    match crate::auth::hash_password(password) {
        Ok(hash) => Ok(hash),
        Err(_) => Err(api_error(ApiError::Internal, "password hashing failed")),
    }
}

/// Шифрует TOTP-секрет (AES-256-GCM, ключ из `DROPIT_TOTP_KEY`).
///
/// Без ключа (local/тесты) пишем легаси-plain base32 — ровно как
/// `lib::encrypt_totp_secret_for_seed`; логин читает оба формата
/// (`routes::decode_totp_secret`), в проде ключ задан.
fn encrypt_enrollment_secret(base32: &str) -> Vec<u8> {
    match dropit_common::encrypt_totp_secret(base32) {
        Ok(enc) => enc,
        Err(err) => {
            tracing::warn!(error = %err, "totp encrypt failed, storing legacy plain secret");
            base32.as_bytes().to_vec()
        }
    }
}

/// Новый base32 TOTP-секрет (160 бит CSPRNG, RFC 4226).
fn generate_totp_base32() -> String {
    totp_rs::Secret::generate_secret().to_encoded().to_string()
}

/// Percent-encode для `otpauth://` label/issuer (RFC 3986 unreserved).
fn percent_encode(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for byte in raw.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// `otpauth://totp/DROP%20IT:<email>?secret=…&issuer=DROP%20IT&…`
///
/// Собирается вручную: фича `otpauth` крейта `totp-rs` в workspace не
/// включена (Cargo.toml — не файл этой задачи), а формат Key URI стандартный.
fn otpauth_url(base32: &str, email: &str) -> String {
    let issuer = percent_encode("DROP IT");
    let account = percent_encode(email);
    format!(
        "otpauth://totp/{issuer}:{account}?secret={base32}&issuer={issuer}&algorithm=SHA1&digits=6&period=30"
    )
}

/// Статус `admin_user` → каталоговый (`invited` = pending/invited).
fn map_admin_status(raw: &str) -> &'static str {
    match raw {
        "suspended" => "suspended",
        "blocked" => "blocked",
        "invited" | "pending_verification" => "invited",
        _ => "active",
    }
}

/// Статус `club` → каталоговый: `pending_verification` — это `invited`.
fn map_club_status(raw: &str) -> &'static str {
    match raw {
        "active" => "active",
        "suspended" => "suspended",
        "blocked" => "blocked",
        _ => "invited",
    }
}

/// Статус `dj` → каталоговый (в БД уже есть `invited`).
fn map_dj_status(raw: &str) -> &'static str {
    match raw {
        "invited" => "invited",
        "suspended" => "suspended",
        "blocked" => "blocked",
        _ => "active",
    }
}

/// Гость: забаненное устройство важнее верификации; иначе `invited`.
fn map_guest_status(verified: bool, banned: bool) -> &'static str {
    if banned {
        "blocked"
    } else if verified {
        "active"
    } else {
        "invited"
    }
}

/// Имя админа: колонка `display_name`, иначе локальная часть e-mail
/// (у существующих строк имени не было до миграции 0019).
fn admin_display_name(display_name: Option<String>, email: &str) -> String {
    match display_name
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
    {
        Some(name) => name,
        None => email.split('@').next().unwrap_or(email).to_string(),
    }
}

/// Строка каталога — единый JSON-вид для list/get/patch.
#[derive(Serialize)]
struct DirectoryItem {
    id: Uuid,
    role: String,
    status: String,
    display_name: String,
    email: String,
    club_id: Option<Uuid>,
    created_at: DateTime<Utc>,
    last_seen_at: Option<DateTime<Utc>>,
    /// Только для `role='admin'`: `superadmin|operator|readonly`.
    admin_role: Option<String>,
}

// ============================================================
// GET /v1/admin/users — список
// ============================================================

/// Тип строки UNION-выборки каталога (см. [`DIRECTORY_SQL`]).
type DirectoryRow = (
    Uuid,
    String,
    String,
    String,
    String,
    Option<Uuid>,
    DateTime<Utc>,
    Option<DateTime<Utc>>,
    Option<String>,
);

impl From<DirectoryRow> for DirectoryItem {
    fn from(row: DirectoryRow) -> Self {
        let (id, role, status, display_name, email, club_id, created_at, last_seen_at, admin_role) =
            row;
        Self {
            id,
            role,
            status,
            display_name,
            email,
            club_id,
            created_at,
            last_seen_at,
            admin_role,
        }
    }
}

/// Каталог: `admin_user` ∪ `club` ∪ `dj` ∪ `guest_account`.
///
/// Статус нормализуется до `active|invited|suspended|blocked` прямо в CTE —
/// фильтр `status` должен работать по каталоговому значению, а не по
/// сырому статусу таблицы (`pending_verification` у клуба = `invited`).
const DIRECTORY_SQL: &str = r#"
WITH directory AS (
    SELECT a.id,
           'admin'::text AS role,
           CASE a.status
               WHEN 'suspended' THEN 'suspended'
               WHEN 'blocked'   THEN 'blocked'
               WHEN 'invited'   THEN 'invited'
               WHEN 'pending_verification' THEN 'invited'
               ELSE 'active'
           END AS status,
           COALESCE(NULLIF(btrim(a.display_name), ''), split_part(a.email::text, '@', 1)) AS display_name,
           a.email::text AS email,
           NULL::uuid AS club_id,
           a.created_at,
           NULL::timestamptz AS last_seen_at,
           a.role AS admin_role
    FROM admin_user a
    UNION ALL
    SELECT c.id,
           'club_staff'::text,
           CASE c.status
               WHEN 'active'    THEN 'active'
               WHEN 'suspended' THEN 'suspended'
               WHEN 'blocked'   THEN 'blocked'
               ELSE 'invited'
           END,
           c.name,
           c.email::text,
           c.id,
           c.created_at,
           NULL::timestamptz,
           NULL::text
    FROM club c
    UNION ALL
    SELECT d.id,
           'dj'::text,
           CASE d.status
               WHEN 'invited'   THEN 'invited'
               WHEN 'suspended' THEN 'suspended'
               WHEN 'blocked'   THEN 'blocked'
               ELSE 'active'
           END,
           d.display_name,
           d.email::text,
           d.club_id,
           d.created_at,
           d.last_seen_at,
           NULL::text
    FROM dj d
    UNION ALL
    SELECT g.id,
           'guest'::text,
           CASE
               WHEN NOT g.verified THEN 'invited'
               WHEN EXISTS (SELECT 1 FROM guest_device gd WHERE gd.account_id = g.id AND gd.banned) THEN 'blocked'
               ELSE 'active'
           END,
           COALESCE(
               (SELECT NULLIF(btrim(gd.display_name), '')
                FROM guest_device gd
                WHERE gd.account_id = g.id AND NULLIF(btrim(gd.display_name), '') IS NOT NULL
                ORDER BY gd.last_seen DESC
                LIMIT 1),
               g.email_display
           ),
           g.email_display,
           NULL::uuid,
           g.created_at,
           (SELECT max(gd.last_seen) FROM guest_device gd WHERE gd.account_id = g.id),
           NULL::text
    FROM guest_account g
)
SELECT id, role, status, display_name, email, club_id, created_at, last_seen_at, admin_role
FROM directory
WHERE ($1::text IS NULL OR role = $1)
  AND ($2::text IS NULL OR status = $2)
  AND ($3::text IS NULL OR display_name ILIKE $3 ESCAPE '\' OR email ILIKE $3 ESCAPE '\')
  AND ($4::timestamptz IS NULL OR (created_at, id) < ($4::timestamptz, $5::uuid))
ORDER BY created_at DESC, id DESC
LIMIT $6
"#;

/// `GET /v1/admin/users?role=&status=&q=&limit=&cursor=`
async fn admin_users_list(
    ctx: AuthContext,
    State(state): State<ApiEnv>,
    query: Result<Query<HashMap<String, String>>, QueryRejection>,
) -> Response {
    if let Some(resp) = admin_only(&ctx) {
        return resp;
    }
    let pool = match pool_of(&state) {
        Ok(pool) => pool,
        Err(resp) => return resp,
    };
    let Query(params) = match query {
        Ok(query) => query,
        Err(_) => return bad_request("malformed query string"),
    };

    let role = match optional_enum_param(&params, "role", &DIRECTORY_ROLES) {
        Ok(value) => value,
        Err(resp) => return resp,
    };
    let status = match optional_enum_param(&params, "status", &DIRECTORY_STATUSES) {
        Ok(value) => value,
        Err(resp) => return resp,
    };
    let limit = match params.get("limit") {
        None => DEFAULT_LIMIT,
        Some(raw) => match raw.trim().parse::<i64>() {
            Ok(value) => value.clamp(1, MAX_LIMIT),
            Err(_) => {
                return api_error_code(
                    StatusCode::BAD_REQUEST,
                    "bad_request",
                    "limit must be an integer.",
                    json!({ "parameter": "limit", "value": raw }),
                )
            }
        },
    };
    let (cur_ts, cur_id) = match params.get("cursor").map(String::as_str) {
        None | Some("") => (None, None),
        Some(raw) => match parse_cursor(raw) {
            Some((ts, id)) => (Some(ts), Some(id)),
            None => {
                return api_error_code(
                    StatusCode::BAD_REQUEST,
                    "bad_request",
                    "cursor is malformed.",
                    json!({ "parameter": "cursor" }),
                )
            }
        },
    };
    let q = params
        .get("q")
        .map(|raw| raw.trim().to_string())
        .filter(|raw| !raw.is_empty());
    if let Some(q) = q.as_deref() {
        if q.chars().count() > MAX_QUERY_LEN {
            return validation_error("q is too long", "q");
        }
    }
    let pattern = q.as_deref().map(like_pattern);

    let rows = sqlx::query_as::<_, DirectoryRow>(DIRECTORY_SQL)
        .bind(role)
        .bind(status)
        .bind(pattern)
        .bind(cur_ts)
        .bind(cur_id)
        .bind(limit + 1)
        .fetch_all(&pool)
        .await;
    let Ok(rows) = rows else {
        return api_error(ApiError::Internal, "db error");
    };

    // keyset: курсор — последний ВОЗВРАЩАЕМЫЙ элемент (limit-й), а
    // (limit+1)-й нужен только как признак следующей страницы.
    let next_cursor = if rows.len() as i64 > limit {
        let last = &rows[(limit - 1) as usize];
        Some(format!("{}\u{1f}{}", last.6.to_rfc3339(), last.0))
    } else {
        None
    };
    let items: Vec<DirectoryItem> = rows
        .into_iter()
        .take(limit as usize)
        .map(DirectoryItem::from)
        .collect();

    (
        StatusCode::OK,
        Json(json!({ "items": items, "next_cursor": next_cursor })),
    )
        .into_response()
}

// ============================================================
// GET /v1/admin/users/:id — одна запись
// ============================================================

async fn admin_users_get(
    ctx: AuthContext,
    State(state): State<ApiEnv>,
    Path(raw_id): Path<String>,
) -> Response {
    if let Some(resp) = admin_only(&ctx) {
        return resp;
    }
    let pool = match pool_of(&state) {
        Ok(pool) => pool,
        Err(resp) => return resp,
    };
    let id = match parse_id(&raw_id) {
        Ok(id) => id,
        Err(resp) => return resp,
    };
    match directory_item_by_id(&pool, id).await {
        Ok(Some(item)) => (StatusCode::OK, Json(item)).into_response(),
        Ok(None) => api_error(ApiError::NotFound, "user not found"),
        Err(_) => api_error(ApiError::Internal, "db error"),
    }
}

/// Ищет строку каталога по id во всех четырёх источниках.
///
/// Порядок фиксирован (admin → club → dj → guest); UUID-коллизия между
/// таблицами невозможна на практике, а явный порядок делает ответ
/// детерминированным.
async fn directory_item_by_id(
    pool: &PgPool,
    id: Uuid,
) -> Result<Option<DirectoryItem>, sqlx::Error> {
    #[allow(clippy::type_complexity)]
    let admin: Option<(Uuid, String, Option<String>, String, String, DateTime<Utc>)> =
        sqlx::query_as(
            "SELECT id, email::text, display_name, role, status, created_at \
             FROM admin_user WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(pool)
        .await?;
    if let Some((id, email, display_name, admin_role, status, created_at)) = admin {
        return Ok(Some(DirectoryItem {
            id,
            role: "admin".into(),
            status: map_admin_status(&status).into(),
            display_name: admin_display_name(display_name, &email),
            email,
            club_id: None,
            created_at,
            last_seen_at: None,
            admin_role: Some(admin_role),
        }));
    }

    let club: Option<(Uuid, String, String, String, DateTime<Utc>)> =
        sqlx::query_as("SELECT id, name, email::text, status, created_at FROM club WHERE id = $1")
            .bind(id)
            .fetch_optional(pool)
            .await?;
    if let Some((id, name, email, status, created_at)) = club {
        return Ok(Some(DirectoryItem {
            id,
            role: "club_staff".into(),
            status: map_club_status(&status).into(),
            display_name: name,
            email,
            // Для сотрудника клуба `club_id` — его собственный клуб.
            club_id: Some(id),
            created_at,
            last_seen_at: None,
            admin_role: None,
        }));
    }

    #[allow(clippy::type_complexity)]
    let dj: Option<(
        Uuid,
        String,
        String,
        String,
        Option<Uuid>,
        DateTime<Utc>,
        Option<DateTime<Utc>>,
    )> = sqlx::query_as(
        "SELECT id, display_name, email::text, status, club_id, created_at, last_seen_at \
         FROM dj WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    if let Some((id, display_name, email, status, club_id, created_at, last_seen_at)) = dj {
        return Ok(Some(DirectoryItem {
            id,
            role: "dj".into(),
            status: map_dj_status(&status).into(),
            display_name,
            email,
            club_id,
            created_at,
            last_seen_at,
            admin_role: None,
        }));
    }

    #[allow(clippy::type_complexity)]
    let guest: Option<(
        Uuid,
        String,
        bool,
        DateTime<Utc>,
        Option<DateTime<Utc>>,
        Option<String>,
        bool,
    )> = sqlx::query_as(
        "SELECT g.id, g.email_display, g.verified, g.created_at, \
         (SELECT max(gd.last_seen) FROM guest_device gd WHERE gd.account_id = g.id), \
         (SELECT NULLIF(btrim(gd.display_name), '') FROM guest_device gd \
          WHERE gd.account_id = g.id AND NULLIF(btrim(gd.display_name), '') IS NOT NULL \
          ORDER BY gd.last_seen DESC LIMIT 1), \
         EXISTS (SELECT 1 FROM guest_device gd WHERE gd.account_id = g.id AND gd.banned) \
         FROM guest_account g WHERE g.id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    if let Some((id, email, verified, created_at, last_seen_at, device_name, banned)) = guest {
        let display_name = device_name
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| email.clone());
        return Ok(Some(DirectoryItem {
            id,
            role: "guest".into(),
            status: map_guest_status(verified, banned).into(),
            display_name,
            email,
            club_id: None,
            created_at,
            last_seen_at,
            admin_role: None,
        }));
    }

    Ok(None)
}

// ============================================================
// POST /v1/admin/users — создание администратора
// ============================================================

#[derive(Deserialize)]
struct CreateUserBody {
    role: Option<String>,
    email: Option<String>,
    display_name: Option<String>,
    password: Option<String>,
    admin_role: Option<String>,
}

/// `POST /v1/admin/users` — создаёт ТОЛЬКО администратора.
///
/// Клуб/DJ/гость создаются своими маршрутами (у них разный побочный
/// эффект: slug/Stripe, invite-токен, device-token), подменять их
/// «созданием пользователя» нельзя — отвечаем 422 `role_not_creatable`
/// с подсказкой, куда идти.
async fn admin_users_create(
    ctx: AuthContext,
    State(state): State<ApiEnv>,
    body: Result<Json<Value>, JsonRejection>,
) -> Response {
    if let Some(resp) = admin_only(&ctx) {
        return resp;
    }
    if let Err(resp) = require_admin_write(&ctx, &state).await {
        return resp;
    }
    let pool = match pool_of(&state) {
        Ok(pool) => pool,
        Err(resp) => return resp,
    };
    let Json(value) = match body {
        Ok(body) => body,
        Err(_) => return bad_request("malformed json body"),
    };
    let request: CreateUserBody = match serde_json::from_value(value) {
        Ok(request) => request,
        Err(err) => return validation_error(&err.to_string(), "body"),
    };

    let role = match request.role.as_deref().map(str::trim) {
        None | Some("") => return validation_error("role is required", "role"),
        Some(role) => role,
    };
    if role != "admin" {
        let create_via: Vec<&str> = match role {
            "club_staff" => vec!["POST /v1/admin/clubs"],
            "dj" => vec!["POST /v1/club/djs/invite", "POST /v1/admin/djs"],
            "guest" => vec!["POST /v1/guest/device", "POST /v1/guest/upgrade"],
            _ => vec![],
        };
        return api_error_code(
            StatusCode::UNPROCESSABLE_ENTITY,
            "role_not_creatable",
            "This route creates administrators only; create the entity via its own flow.",
            json!({
                "role": role,
                "creatable_roles": ["admin"],
                "create_via": create_via,
            }),
        );
    }

    let email = match request.email.as_deref() {
        Some(raw) => match clean_email(raw) {
            Ok(email) => email,
            Err(resp) => return resp,
        },
        None => return validation_error("email is required", "email"),
    };
    let display_name = match request.display_name.as_deref() {
        Some(raw) => match clean_display_name(raw) {
            Ok(name) => name,
            Err(resp) => return resp,
        },
        // Имя не обязательно: без него каталог покажет локальную часть e-mail.
        None => admin_display_name(None, &email),
    };
    let password = match request.password.as_deref() {
        Some(password) => password,
        None => return validation_error("password is required", "password"),
    };
    let password_len = password.chars().count();
    if !(MIN_PASSWORD_LEN..=MAX_PASSWORD_LEN).contains(&password_len) {
        return validation_error(
            "password length must be between 8 and 128 characters",
            "password",
        );
    }
    let admin_role = match request.admin_role.as_deref().map(str::trim) {
        None | Some("") => "operator".to_string(), // не создаём superadmin молча
        Some(code) if ADMIN_ROLE_CODES.contains(&code) => code.to_string(),
        Some(code) => {
            return api_error_code(
                StatusCode::UNPROCESSABLE_ENTITY,
                "unknown_admin_role",
                "admin_role must be one of superadmin, operator, readonly.",
                json!({ "admin_role": code, "allowed": ADMIN_ROLE_CODES }),
            )
        }
    };

    // Отдельная проверка до INSERT: понятный 409 вместо «db error» на 23505.
    match admin_email_taken(&pool, &email, None).await {
        Ok(true) => {
            return api_error_code(
                StatusCode::CONFLICT,
                "email_taken",
                "This e-mail is already used by another administrator.",
                json!({ "field": "email" }),
            )
        }
        Ok(false) => {}
        Err(_) => return api_error(ApiError::Internal, "db error"),
    }

    let password_hash = match hash_password(password) {
        Ok(hash) => hash,
        Err(resp) => return resp,
    };
    let totp_secret = generate_totp_base32();
    let totp_enc = encrypt_enrollment_secret(&totp_secret);
    let otpauth = otpauth_url(&totp_secret, &email);
    let id = Uuid::now_v7();

    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(_) => return api_error(ApiError::Internal, "db error"),
    };
    let inserted = sqlx::query_as::<_, (DateTime<Utc>,)>(
        "INSERT INTO admin_user \
         (id, email, password_hash, totp_secret_enc, totp_confirmed_at, role, status, display_name) \
         VALUES ($1, $2, $3, $4, now(), $5, 'active', $6) \
         RETURNING created_at",
    )
    .bind(id)
    .bind(&email)
    .bind(&password_hash)
    .bind(&totp_enc)
    .bind(&admin_role)
    .bind(&display_name)
    .fetch_one(&mut *tx)
    .await;
    let created_at = match inserted {
        Ok(row) => row.0,
        Err(err) if is_unique_violation(&err) => {
            return api_error_code(
                StatusCode::CONFLICT,
                "email_taken",
                "This e-mail is already used by another administrator.",
                json!({ "field": "email" }),
            )
        }
        Err(_) => return api_error(ApiError::Internal, "db error"),
    };
    let diff = json!({
        "entity_type": "admin_user",
        "entity_id": id,
        "before": Value::Null,
        "after": {
            "email": email,
            "display_name": display_name,
            "admin_role": admin_role,
            "status": "active",
        },
    });
    if audit_in_tx(
        &mut tx,
        ctx.subject_id(),
        "admin.user.create",
        &id.to_string(),
        diff,
    )
    .await
    .is_err()
    {
        return api_error(ApiError::Internal, "db error");
    }
    if tx.commit().await.is_err() {
        return api_error(ApiError::Internal, "db error");
    }

    let payload = json!({
        "id": id,
        "role": "admin",
        "admin_role": admin_role,
        "status": "active",
        "display_name": display_name,
        "email": email,
        // Одноразовый enrollment: без него админ не введёт TOTP и не войдёт.
        "totp_secret": totp_secret,
        "otpauth_url": otpauth,
        "created_at": created_at,
    });
    let mut response = (StatusCode::CREATED, Json(payload)).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

/// Занят ли e-mail другим админом (`exclude` — id, который не считаем).
async fn admin_email_taken(
    pool: &PgPool,
    email: &str,
    exclude: Option<Uuid>,
) -> Result<bool, sqlx::Error> {
    let row: Option<(i32,)> = sqlx::query_as(
        "SELECT 1 FROM admin_user WHERE email = $1::citext AND ($2::uuid IS NULL OR id <> $2)",
    )
    .bind(email)
    .bind(exclude)
    .fetch_optional(pool)
    .await?;
    Ok(row.is_some())
}

// ============================================================
// PATCH /v1/admin/users/:id
// ============================================================

#[derive(Deserialize)]
struct PatchUserBody {
    display_name: Option<String>,
    email: Option<String>,
    role: Option<String>,
    status: Option<String>,
}

/// Проверенные поля патча (уже normalised).
struct PatchFields {
    display_name: Option<String>,
    email: Option<String>,
    role: Option<String>,
    status: Option<String>,
}

/// Текущее состояние найденной сущности каталога.
enum Target {
    Admin(AdminTarget),
    Club(ClubTarget),
    Dj(DjTarget),
    Guest(GuestTarget),
}

struct AdminTarget {
    id: Uuid,
    email: String,
    display_name: Option<String>,
    role: String,
    status: String,
}

struct ClubTarget {
    id: Uuid,
    email: String,
    name: String,
    status: String,
}

struct DjTarget {
    id: Uuid,
    email: String,
    display_name: String,
    status: String,
}

struct GuestTarget {
    id: Uuid,
    email: String,
    verified: bool,
    banned: bool,
}

async fn find_target(pool: &PgPool, id: Uuid) -> Result<Option<Target>, sqlx::Error> {
    #[allow(clippy::type_complexity)]
    let admin: Option<(Uuid, String, Option<String>, String, String)> = sqlx::query_as(
        "SELECT id, email::text, display_name, role, status FROM admin_user WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    if let Some((id, email, display_name, role, status)) = admin {
        return Ok(Some(Target::Admin(AdminTarget {
            id,
            email,
            display_name,
            role,
            status,
        })));
    }

    let club: Option<(Uuid, String, String, String)> =
        sqlx::query_as("SELECT id, email::text, name, status FROM club WHERE id = $1")
            .bind(id)
            .fetch_optional(pool)
            .await?;
    if let Some((id, email, name, status)) = club {
        return Ok(Some(Target::Club(ClubTarget {
            id,
            email,
            name,
            status,
        })));
    }

    let dj: Option<(Uuid, String, String, String)> =
        sqlx::query_as("SELECT id, email::text, display_name, status FROM dj WHERE id = $1")
            .bind(id)
            .fetch_optional(pool)
            .await?;
    if let Some((id, email, display_name, status)) = dj {
        return Ok(Some(Target::Dj(DjTarget {
            id,
            email,
            display_name,
            status,
        })));
    }

    let guest: Option<(Uuid, String, bool, bool)> = sqlx::query_as(
        "SELECT g.id, g.email_display, g.verified, \
         EXISTS (SELECT 1 FROM guest_device gd WHERE gd.account_id = g.id AND gd.banned) \
         FROM guest_account g WHERE g.id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    if let Some((id, email, verified, banned)) = guest {
        return Ok(Some(Target::Guest(GuestTarget {
            id,
            email,
            verified,
            banned,
        })));
    }

    Ok(None)
}

/// Парсит и валидирует тело патча до обращения к БД.
#[allow(clippy::result_large_err)]
fn parse_patch(value: Value) -> Result<PatchFields, Response> {
    let request: PatchUserBody =
        serde_json::from_value(value).map_err(|err| validation_error(&err.to_string(), "body"))?;
    if request.display_name.is_none()
        && request.email.is_none()
        && request.role.is_none()
        && request.status.is_none()
    {
        return Err(bad_request("patch body has no fields to update"));
    }
    let display_name = match request.display_name.as_deref() {
        Some(raw) => Some(clean_display_name(raw)?),
        None => None,
    };
    let email = match request.email.as_deref() {
        Some(raw) => Some(clean_email(raw)?),
        None => None,
    };
    let status = match request.status.as_deref() {
        None => None,
        Some(raw) => {
            let value = raw.trim();
            if !MUTABLE_STATUSES.contains(&value) {
                return Err(api_error_code(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "status_not_supported",
                    "Only active, suspended and blocked can be set explicitly.",
                    json!({
                        "status": value,
                        "allowed": MUTABLE_STATUSES,
                        "note": "invited is derived from table state (pending verification / not accepted invite / unverified guest)",
                    }),
                ));
            }
            Some(value.to_string())
        }
    };
    let role = request
        .role
        .as_deref()
        .map(str::trim)
        .filter(|role| !role.is_empty())
        .map(str::to_string);
    Ok(PatchFields {
        display_name,
        email,
        role,
        status,
    })
}

/// `PATCH /v1/admin/users/:id` — имя/почта/роль/статус.
async fn admin_users_patch(
    ctx: AuthContext,
    State(state): State<ApiEnv>,
    Path(raw_id): Path<String>,
    body: Result<Json<Value>, JsonRejection>,
) -> Response {
    if let Some(resp) = admin_only(&ctx) {
        return resp;
    }
    if let Err(resp) = require_admin_write(&ctx, &state).await {
        return resp;
    }
    let pool = match pool_of(&state) {
        Ok(pool) => pool,
        Err(resp) => return resp,
    };
    let id = match parse_id(&raw_id) {
        Ok(id) => id,
        Err(resp) => return resp,
    };
    let Json(value) = match body {
        Ok(body) => body,
        Err(_) => return bad_request("malformed json body"),
    };
    let fields = match parse_patch(value) {
        Ok(fields) => fields,
        Err(resp) => return resp,
    };
    let target = match find_target(&pool, id).await {
        Ok(Some(target)) => target,
        Ok(None) => return api_error(ApiError::NotFound, "user not found"),
        Err(_) => return api_error(ApiError::Internal, "db error"),
    };

    let updated = match target {
        Target::Admin(row) => patch_admin(&ctx, &pool, row, fields).await,
        Target::Club(row) => patch_club(&ctx, &pool, row, fields).await,
        Target::Dj(row) => patch_dj(&ctx, &pool, row, fields).await,
        Target::Guest(row) => patch_guest(&ctx, &pool, row, fields).await,
    };
    if let Err(resp) = updated {
        return resp;
    }

    // Отвечаем свежей строкой каталога: UI не должен склеивать ответ сам.
    match directory_item_by_id(&pool, id).await {
        Ok(Some(item)) => (StatusCode::OK, Json(item)).into_response(),
        Ok(None) => api_error(ApiError::Internal, "user disappeared after update"),
        Err(_) => api_error(ApiError::Internal, "db error"),
    }
}

/// Параметры UPDATE, которые биндятся в фиксированном порядке
/// (`id`, `email`, `display_name`, `role`, `status`); `None` = не менялось.
struct PatchBinds<'a> {
    email: Option<&'a str>,
    display_name: Option<&'a str>,
    role: Option<&'a str>,
    status: Option<&'a str>,
    revoke_refresh: bool,
}

/// Общий хвост патча: UPDATE + отзыв refresh + аудит в одной транзакции.
#[allow(clippy::too_many_arguments)]
async fn finish_patch(
    ctx: &AuthContext,
    pool: &PgPool,
    id: Uuid,
    entity_type: &str,
    before: Value,
    after: Value,
    update_sql: &str,
    binds: PatchBinds<'_>,
) -> Result<(), Response> {
    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(_) => return Err(api_error(ApiError::Internal, "db error")),
    };
    let mut query = sqlx::query(update_sql).bind(id);
    if let Some(email) = binds.email {
        query = query.bind(email);
    }
    if let Some(name) = binds.display_name {
        query = query.bind(name);
    }
    if let Some(role) = binds.role {
        query = query.bind(role);
    }
    if let Some(status) = binds.status {
        query = query.bind(status);
    }
    if let Err(err) = query.execute(&mut *tx).await {
        if is_unique_violation(&err) {
            return Err(api_error_code(
                StatusCode::CONFLICT,
                "email_taken",
                "This e-mail is already used by another account of the same type.",
                json!({ "field": "email" }),
            ));
        }
        return Err(api_error(ApiError::Internal, "db error"));
    }
    if binds.revoke_refresh {
        let revoked = sqlx::query(
            "UPDATE refresh_token SET revoked_at = now() \
             WHERE subject_type = $1 AND subject_id = $2 AND revoked_at IS NULL",
        )
        .bind(entity_type)
        .bind(id)
        .execute(&mut *tx)
        .await;
        if revoked.is_err() {
            return Err(api_error(ApiError::Internal, "db error"));
        }
    }
    let diff = json!({
        "entity_type": entity_type,
        "entity_id": id,
        "before": before,
        "after": after,
    });
    if audit_in_tx(
        &mut tx,
        ctx.subject_id(),
        "admin.user.update",
        &id.to_string(),
        diff,
    )
    .await
    .is_err()
    {
        return Err(api_error(ApiError::Internal, "db error"));
    }
    if tx.commit().await.is_err() {
        return Err(api_error(ApiError::Internal, "db error"));
    }
    Ok(())
}

async fn patch_admin(
    ctx: &AuthContext,
    pool: &PgPool,
    row: AdminTarget,
    fields: PatchFields,
) -> Result<(), Response> {
    let PatchFields {
        display_name,
        email,
        role: requested_role,
        status,
    } = fields;
    let email_provided = email.is_some();
    let mut new_role = row.role.clone();
    if let Some(requested) = requested_role.as_deref() {
        if !ADMIN_ROLE_CODES.contains(&requested) {
            return Err(api_error_code(
                StatusCode::UNPROCESSABLE_ENTITY,
                "unknown_admin_role",
                "Admin role must be superadmin, operator or readonly.",
                json!({ "admin_role": requested, "allowed": ADMIN_ROLE_CODES }),
            ));
        }
        if row.id == ctx.subject_id() && requested != row.role {
            return Err(api_error_code(
                StatusCode::UNPROCESSABLE_ENTITY,
                "cannot_change_own_role",
                "You cannot change your own administrator role.",
                json!({ "id": row.id, "current_role": row.role }),
            ));
        }
        new_role = requested.to_string();
    }
    let new_status = status.unwrap_or_else(|| row.status.clone());
    let new_email = email.unwrap_or_else(|| row.email.clone());
    let new_name = display_name.or_else(|| row.display_name.clone());
    if row.id == ctx.subject_id() && new_status != "active" {
        return Err(api_error_code(
            StatusCode::UNPROCESSABLE_ENTITY,
            "cannot_block_self",
            "You cannot suspend or block your own account.",
            json!({ "id": row.id }),
        ));
    }

    // Последний активный superadmin: разжаловать/заблокировать нельзя —
    // иначе платформа останется без владельца (и без ролей, которые
    // выдаются только superadmin'ом).
    let demotion = row.role == "superadmin" && new_role != "superadmin";
    let blocking = row.status == "active" && new_status != "active";
    if row.role == "superadmin" && row.status == "active" && (demotion || blocking) {
        let count: (i64,) = sqlx::query_as(
            "SELECT count(*) FROM admin_user WHERE role = 'superadmin' AND status = 'active'",
        )
        .fetch_one(pool)
        .await
        .map_err(|_| api_error(ApiError::Internal, "db error"))?;
        if count.0 <= 1 {
            return Err(api_error_code(
                StatusCode::CONFLICT,
                "last_superadmin",
                "The last active superadmin cannot be demoted or blocked.",
                json!({ "id": row.id }),
            ));
        }
    }

    if email_provided && new_email != row.email {
        match admin_email_taken(pool, &new_email, Some(row.id)).await {
            Ok(true) => {
                return Err(api_error_code(
                    StatusCode::CONFLICT,
                    "email_taken",
                    "This e-mail is already used by another administrator.",
                    json!({ "field": "email" }),
                ))
            }
            Ok(false) => {}
            Err(_) => return Err(api_error(ApiError::Internal, "db error")),
        }
    }

    let before = json!({
        "email": row.email,
        "display_name": admin_display_name(row.display_name.clone(), &row.email),
        "admin_role": row.role,
        "status": map_admin_status(&row.status),
    });
    let after = json!({
        "email": new_email,
        "display_name": admin_display_name(new_name.clone(), &new_email),
        "admin_role": new_role,
        "status": map_admin_status(&new_status),
    });
    finish_patch(
        ctx,
        pool,
        row.id,
        "admin",
        before,
        after,
        "UPDATE admin_user SET email = COALESCE($2::citext, email), \
         display_name = CASE WHEN $3::text IS NULL THEN display_name ELSE $3 END, \
         role = COALESCE($4::text, role), status = COALESCE($5::text, status) \
         WHERE id = $1",
        PatchBinds {
            email: Some(&new_email),
            display_name: new_name.as_deref(),
            role: Some(&new_role),
            status: Some(&new_status),
            revoke_refresh: matches!(new_status.as_str(), "suspended" | "blocked"),
        },
    )
    .await
}

async fn patch_club(
    ctx: &AuthContext,
    pool: &PgPool,
    row: ClubTarget,
    fields: PatchFields,
) -> Result<(), Response> {
    let PatchFields {
        display_name,
        email,
        role: requested_role,
        status,
    } = fields;
    let email_provided = email.is_some();
    if let Some(requested) = requested_role.as_deref() {
        if requested != "club_staff" {
            return Err(api_error_code(
                StatusCode::UNPROCESSABLE_ENTITY,
                "role_immutable",
                "A club account cannot be turned into another principal role.",
                json!({ "current_role": "club_staff", "requested_role": requested }),
            ));
        }
    }
    let new_email = email.unwrap_or_else(|| row.email.clone());
    let new_name = display_name.unwrap_or_else(|| row.name.clone());
    // Если статус не меняется — оставляем СЫРОЕ значение таблицы
    // (`pending_verification` нельзя перезаписать каталоговым `invited`).
    let new_status = status.unwrap_or_else(|| row.status.clone());
    if email_provided && new_email != row.email {
        match email_taken(pool, "club", &new_email, row.id).await {
            Ok(true) => {
                return Err(api_error_code(
                    StatusCode::CONFLICT,
                    "email_taken",
                    "This e-mail is already used by another club.",
                    json!({ "field": "email" }),
                ))
            }
            Ok(false) => {}
            Err(_) => return Err(api_error(ApiError::Internal, "db error")),
        }
    }

    let before = json!({
        "email": row.email,
        "display_name": row.name,
        "status": map_club_status(&row.status),
    });
    let after = json!({
        "email": new_email,
        "display_name": new_name,
        "status": map_club_status(&new_status),
    });
    finish_patch(
        ctx,
        pool,
        row.id,
        "club",
        before,
        after,
        "UPDATE club SET email = COALESCE($2::citext, email), \
         name = COALESCE($3::text, name), status = COALESCE($4::text, status) \
         WHERE id = $1",
        PatchBinds {
            email: Some(&new_email),
            display_name: Some(&new_name),
            role: None,
            status: Some(&new_status),
            revoke_refresh: matches!(new_status.as_str(), "suspended" | "blocked"),
        },
    )
    .await
}

async fn patch_dj(
    ctx: &AuthContext,
    pool: &PgPool,
    row: DjTarget,
    fields: PatchFields,
) -> Result<(), Response> {
    let PatchFields {
        display_name,
        email,
        role: requested_role,
        status,
    } = fields;
    let email_provided = email.is_some();
    if let Some(requested) = requested_role.as_deref() {
        if requested != "dj" {
            return Err(api_error_code(
                StatusCode::UNPROCESSABLE_ENTITY,
                "role_immutable",
                "A DJ account cannot be turned into another principal role.",
                json!({ "current_role": "dj", "requested_role": requested }),
            ));
        }
    }
    let new_email = email.unwrap_or_else(|| row.email.clone());
    let new_name = display_name.unwrap_or_else(|| row.display_name.clone());
    let new_status = status.unwrap_or_else(|| row.status.clone());
    if email_provided && new_email != row.email {
        match email_taken(pool, "dj", &new_email, row.id).await {
            Ok(true) => {
                return Err(api_error_code(
                    StatusCode::CONFLICT,
                    "email_taken",
                    "This e-mail is already used by another DJ.",
                    json!({ "field": "email" }),
                ))
            }
            Ok(false) => {}
            Err(_) => return Err(api_error(ApiError::Internal, "db error")),
        }
    }

    let before = json!({
        "email": row.email,
        "display_name": row.display_name,
        "status": map_dj_status(&row.status),
    });
    let after = json!({
        "email": new_email,
        "display_name": new_name,
        "status": map_dj_status(&new_status),
    });
    finish_patch(
        ctx,
        pool,
        row.id,
        "dj",
        before,
        after,
        "UPDATE dj SET email = COALESCE($2::citext, email), \
         display_name = COALESCE($3::text, display_name), status = COALESCE($4::text, status) \
         WHERE id = $1",
        PatchBinds {
            email: Some(&new_email),
            display_name: Some(&new_name),
            role: None,
            status: Some(&new_status),
            revoke_refresh: matches!(new_status.as_str(), "suspended" | "blocked"),
        },
    )
    .await
}

async fn patch_guest(
    ctx: &AuthContext,
    pool: &PgPool,
    row: GuestTarget,
    fields: PatchFields,
) -> Result<(), Response> {
    let PatchFields {
        display_name,
        email,
        role: requested_role,
        status,
    } = fields;
    let email_provided = email.is_some();
    if let Some(requested) = requested_role.as_deref() {
        if requested != "guest" {
            return Err(api_error_code(
                StatusCode::UNPROCESSABLE_ENTITY,
                "role_immutable",
                "A guest account cannot be turned into another principal role.",
                json!({ "current_role": "guest", "requested_role": requested }),
            ));
        }
    }
    let new_email = email.unwrap_or_else(|| row.email.clone());
    let new_status = status;
    // У гостя нет колонки status: `blocked`/`suspended` = бан всех его
    // устройств (это и есть реальный эффект блокировки), `active` = снять бан.
    let (ban, unban) = match new_status.as_deref() {
        Some("blocked" | "suspended") => (true, false),
        Some("active") => (false, true),
        _ => (false, false),
    };

    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(_) => return Err(api_error(ApiError::Internal, "db error")),
    };
    if email_provided && new_email != row.email {
        let normalized = new_email.trim().to_lowercase();
        match email_taken(pool, "guest", &normalized, row.id).await {
            Ok(true) => {
                return Err(api_error_code(
                    StatusCode::CONFLICT,
                    "email_taken",
                    "This e-mail is already used by another guest account.",
                    json!({ "field": "email" }),
                ))
            }
            Ok(false) => {}
            Err(_) => return Err(api_error(ApiError::Internal, "db error")),
        }
        let updated = sqlx::query(
            "UPDATE guest_account SET email_display = $2, email_normalized = $3 WHERE id = $1",
        )
        .bind(row.id)
        .bind(&new_email)
        .bind(&normalized)
        .execute(&mut *tx)
        .await;
        match updated {
            Ok(_) => {}
            Err(err) if is_unique_violation(&err) => {
                return Err(api_error_code(
                    StatusCode::CONFLICT,
                    "email_taken",
                    "This e-mail is already used by another guest account.",
                    json!({ "field": "email" }),
                ))
            }
            Err(_) => return Err(api_error(ApiError::Internal, "db error")),
        }
    }
    if let Some(name) = display_name.as_deref() {
        // У гостя нет собственного display_name: имя живёт на устройствах.
        let updated =
            sqlx::query("UPDATE guest_device SET display_name = $2 WHERE account_id = $1")
                .bind(row.id)
                .bind(name)
                .execute(&mut *tx)
                .await;
        if updated.is_err() {
            return Err(api_error(ApiError::Internal, "db error"));
        }
    }
    if ban || unban {
        let updated = sqlx::query("UPDATE guest_device SET banned = $2 WHERE account_id = $1")
            .bind(row.id)
            .bind(ban)
            .execute(&mut *tx)
            .await;
        if updated.is_err() {
            return Err(api_error(ApiError::Internal, "db error"));
        }
    }
    let before = json!({
        "email": row.email,
        "status": map_guest_status(row.verified, row.banned),
    });
    let after_status = if ban {
        "blocked"
    } else if unban {
        map_guest_status(row.verified, false)
    } else {
        map_guest_status(row.verified, row.banned)
    };
    let after = json!({ "email": new_email, "status": after_status });
    let diff = json!({
        "entity_type": "guest",
        "entity_id": row.id,
        "before": before,
        "after": after,
    });
    if audit_in_tx(
        &mut tx,
        ctx.subject_id(),
        "admin.user.update",
        &row.id.to_string(),
        diff,
    )
    .await
    .is_err()
    {
        return Err(api_error(ApiError::Internal, "db error"));
    }
    if tx.commit().await.is_err() {
        return Err(api_error(ApiError::Internal, "db error"));
    }
    Ok(())
}

/// Проверка занятости e-mail внутри одной таблицы (клуб/DJ/гость).
async fn email_taken(
    pool: &PgPool,
    entity: &str,
    email: &str,
    exclude: Uuid,
) -> Result<bool, sqlx::Error> {
    let sql = match entity {
        "club" => "SELECT 1 FROM club WHERE email = $1::citext AND id <> $2",
        "dj" => "SELECT 1 FROM dj WHERE email = $1::citext AND id <> $2",
        "guest" => "SELECT 1 FROM guest_account WHERE email_normalized = $1 AND id <> $2",
        _ => "SELECT 1 FROM admin_user WHERE email = $1::citext AND id <> $2",
    };
    let row: Option<(i32,)> = sqlx::query_as(sql)
        .bind(email)
        .bind(exclude)
        .fetch_optional(pool)
        .await?;
    Ok(row.is_some())
}

// ============================================================
// DELETE /v1/admin/users/:id — мягкая блокировка
// ============================================================

/// `DELETE` не удаляет строку: на субъекта ссылаются сессии, заявки,
/// платежи и выплаты. Семантика — «заблокирован» (`status='blocked'`),
/// плюс отзыв refresh-токенов, чтобы активная сессия умерла сразу.
async fn admin_users_delete(
    ctx: AuthContext,
    State(state): State<ApiEnv>,
    Path(raw_id): Path<String>,
) -> Response {
    if let Some(resp) = admin_only(&ctx) {
        return resp;
    }
    if let Err(resp) = require_admin_write(&ctx, &state).await {
        return resp;
    }
    let pool = match pool_of(&state) {
        Ok(pool) => pool,
        Err(resp) => return resp,
    };
    let id = match parse_id(&raw_id) {
        Ok(id) => id,
        Err(resp) => return resp,
    };
    if id == ctx.subject_id() {
        return api_error_code(
            StatusCode::UNPROCESSABLE_ENTITY,
            "cannot_delete_self",
            "You cannot delete your own account.",
            json!({ "id": id }),
        );
    }
    let target = match find_target(&pool, id).await {
        Ok(Some(target)) => target,
        Ok(None) => return api_error(ApiError::NotFound, "user not found"),
        Err(_) => return api_error(ApiError::Internal, "db error"),
    };

    let (entity_type, update_sql, before) = match &target {
        Target::Admin(row) => {
            if row.role == "superadmin" && row.status == "active" {
                match active_superadmin_count(&pool).await {
                    Ok(count) if count <= 1 => {
                        return api_error_code(
                            StatusCode::CONFLICT,
                            "last_superadmin",
                            "The last active superadmin cannot be deleted.",
                            json!({ "id": id }),
                        )
                    }
                    Ok(_) => {}
                    Err(_) => return api_error(ApiError::Internal, "db error"),
                }
            }
            (
                "admin",
                "UPDATE admin_user SET status = 'blocked' WHERE id = $1",
                json!({ "status": map_admin_status(&row.status), "admin_role": row.role }),
            )
        }
        Target::Club(row) => (
            "club",
            "UPDATE club SET status = 'blocked' WHERE id = $1",
            json!({ "status": map_club_status(&row.status) }),
        ),
        Target::Dj(row) => (
            "dj",
            "UPDATE dj SET status = 'blocked' WHERE id = $1",
            json!({ "status": map_dj_status(&row.status) }),
        ),
        Target::Guest(row) => (
            "guest",
            "UPDATE guest_device SET banned = true WHERE account_id = $1",
            json!({ "status": map_guest_status(row.verified, row.banned) }),
        ),
    };

    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(_) => return api_error(ApiError::Internal, "db error"),
    };
    if sqlx::query(update_sql)
        .bind(id)
        .execute(&mut *tx)
        .await
        .is_err()
    {
        return api_error(ApiError::Internal, "db error");
    }
    if entity_type != "guest" {
        let revoked = sqlx::query(
            "UPDATE refresh_token SET revoked_at = now() \
             WHERE subject_type = $1 AND subject_id = $2 AND revoked_at IS NULL",
        )
        .bind(entity_type)
        .bind(id)
        .execute(&mut *tx)
        .await;
        if revoked.is_err() {
            return api_error(ApiError::Internal, "db error");
        }
    }
    let diff = json!({
        "entity_type": entity_type,
        "entity_id": id,
        "before": before,
        "after": { "status": "blocked" },
    });
    if audit_in_tx(
        &mut tx,
        ctx.subject_id(),
        "admin.user.delete",
        &id.to_string(),
        diff,
    )
    .await
    .is_err()
    {
        return api_error(ApiError::Internal, "db error");
    }
    if tx.commit().await.is_err() {
        return api_error(ApiError::Internal, "db error");
    }

    StatusCode::NO_CONTENT.into_response()
}

async fn active_superadmin_count(pool: &PgPool) -> Result<i64, sqlx::Error> {
    let row: (i64,) = sqlx::query_as(
        "SELECT count(*) FROM admin_user WHERE role = 'superadmin' AND status = 'active'",
    )
    .fetch_one(pool)
    .await?;
    Ok(row.0)
}

// ============================================================
// POST /v1/admin/users/bulk
// ============================================================

#[derive(Deserialize)]
struct BulkBody {
    ids: Option<Vec<Uuid>>,
    action: Option<String>,
    role: Option<String>,
}

/// `POST /v1/admin/users/bulk` — массовое действие одной транзакцией.
///
/// `affected` — число найденных (и затронутых) сущностей; `ids` — они же.
/// Одна запись аудита на всю операцию (`admin.user.bulk`) с before/after.
async fn admin_users_bulk(
    ctx: AuthContext,
    State(state): State<ApiEnv>,
    body: Result<Json<Value>, JsonRejection>,
) -> Response {
    if let Some(resp) = admin_only(&ctx) {
        return resp;
    }
    if let Err(resp) = require_admin_write(&ctx, &state).await {
        return resp;
    }
    let pool = match pool_of(&state) {
        Ok(pool) => pool,
        Err(resp) => return resp,
    };
    let Json(value) = match body {
        Ok(body) => body,
        Err(_) => return bad_request("malformed json body"),
    };
    let request: BulkBody = match serde_json::from_value(value) {
        Ok(request) => request,
        Err(err) => return validation_error(&err.to_string(), "body"),
    };
    let ids = match request.ids {
        Some(ids) if ids.is_empty() => {
            return validation_error("ids must not be empty", "ids");
        }
        Some(ids) if ids.len() > MAX_BULK_IDS => {
            return validation_error("too many ids in one bulk request", "ids");
        }
        Some(mut ids) => {
            ids.sort_unstable();
            ids.dedup();
            ids
        }
        None => return validation_error("ids is required", "ids"),
    };
    let action = match request.action.as_deref().map(str::trim) {
        Some(action) if BULK_ACTIONS.contains(&action) => action.to_string(),
        Some(action) => {
            return api_error_code(
                StatusCode::UNPROCESSABLE_ENTITY,
                "unsupported_action",
                "action must be approve, suspend, block or role.",
                json!({ "action": action, "allowed": BULK_ACTIONS }),
            )
        }
        None => return validation_error("action is required", "action"),
    };
    let role = match request.role.as_deref().map(str::trim) {
        Some(role) if !role.is_empty() => Some(role.to_string()),
        _ => None,
    };
    if action == "role" {
        match role.as_deref() {
            None => return validation_error("role is required for action=role", "role"),
            Some(role) if !ADMIN_ROLE_CODES.contains(&role) => {
                return api_error_code(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "unknown_admin_role",
                    "Admin role must be superadmin, operator or readonly.",
                    json!({ "admin_role": role, "allowed": ADMIN_ROLE_CODES }),
                )
            }
            Some(_) => {}
        }
    }
    // Себя массово блокировать нельзя — иначе оператор выключит себе доступ
    // и не сможет вернуть его обратно (запись требует живого админа).
    if matches!(action.as_str(), "suspend" | "block") && ids.contains(&ctx.subject_id()) {
        return api_error_code(
            StatusCode::UNPROCESSABLE_ENTITY,
            "cannot_block_self",
            "You cannot suspend or block your own account in a bulk action.",
            json!({ "id": ctx.subject_id() }),
        );
    }

    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(_) => return api_error(ApiError::Internal, "db error"),
    };

    // Пред-резолв: какие id реально существуют для этого действия
    // (для role — только админы; для остальных — все четыре таблицы).
    let admin_ids: Vec<Uuid> =
        match sqlx::query_scalar("SELECT id FROM admin_user WHERE id = ANY($1) ORDER BY id")
            .bind(ids.as_slice())
            .fetch_all(&mut *tx)
            .await
        {
            Ok(found) => found,
            Err(_) => return api_error(ApiError::Internal, "db error"),
        };
    let (club_ids, dj_ids, guest_ids) = if action == "role" {
        (Vec::new(), Vec::new(), Vec::new())
    } else {
        let club_ids: Vec<Uuid> =
            match sqlx::query_scalar("SELECT id FROM club WHERE id = ANY($1) ORDER BY id")
                .bind(ids.as_slice())
                .fetch_all(&mut *tx)
                .await
            {
                Ok(found) => found,
                Err(_) => return api_error(ApiError::Internal, "db error"),
            };
        let dj_ids: Vec<Uuid> =
            match sqlx::query_scalar("SELECT id FROM dj WHERE id = ANY($1) ORDER BY id")
                .bind(ids.as_slice())
                .fetch_all(&mut *tx)
                .await
            {
                Ok(found) => found,
                Err(_) => return api_error(ApiError::Internal, "db error"),
            };
        let guest_ids: Vec<Uuid> =
            match sqlx::query_scalar("SELECT id FROM guest_account WHERE id = ANY($1) ORDER BY id")
                .bind(ids.as_slice())
                .fetch_all(&mut *tx)
                .await
            {
                Ok(found) => found,
                Err(_) => return api_error(ApiError::Internal, "db error"),
            };
        (club_ids, dj_ids, guest_ids)
    };

    // before-снимок для аудита.
    let before = match bulk_snapshot(&mut tx, &admin_ids, &club_ids, &dj_ids, &guest_ids).await {
        Ok(snapshot) => snapshot,
        Err(_) => return api_error(ApiError::Internal, "db error"),
    };

    if action == "role" {
        // Последний superadmin: массовое разжалование не должно оставить
        // платформу без владельца — 409 до любых записей.
        if let Some(new_role) = role.as_deref() {
            if new_role != "superadmin" && !admin_ids.is_empty() {
                let demoted: (i64,) = match sqlx::query_as(
                    "SELECT count(*) FROM admin_user \
                     WHERE id = ANY($1) AND role = 'superadmin' AND status = 'active'",
                )
                .bind(admin_ids.as_slice())
                .fetch_one(&mut *tx)
                .await
                {
                    Ok(row) => row,
                    Err(_) => return api_error(ApiError::Internal, "db error"),
                };
                if demoted.0 > 0 {
                    let total = match active_superadmin_count(&pool).await {
                        Ok(total) => total,
                        Err(_) => return api_error(ApiError::Internal, "db error"),
                    };
                    if total - demoted.0 <= 0 {
                        return api_error_code(
                            StatusCode::CONFLICT,
                            "last_superadmin",
                            "Bulk demotion would leave the platform without an active superadmin.",
                            json!({ "superadmins": total, "demoted": demoted.0 }),
                        );
                    }
                }
            }
            let changed = sqlx::query("UPDATE admin_user SET role = $2 WHERE id = ANY($1)")
                .bind(admin_ids.as_slice())
                .bind(new_role)
                .execute(&mut *tx)
                .await;
            if changed.is_err() {
                return api_error(ApiError::Internal, "db error");
            }
        }
    } else {
        let (db_status, banned): (&str, bool) = match action.as_str() {
            "approve" => ("active", false),
            "suspend" => ("suspended", true),
            _ => ("blocked", true),
        };
        if !admin_ids.is_empty() {
            let updated = sqlx::query("UPDATE admin_user SET status = $2 WHERE id = ANY($1)")
                .bind(admin_ids.as_slice())
                .bind(db_status)
                .execute(&mut *tx)
                .await;
            if updated.is_err() {
                return api_error(ApiError::Internal, "db error");
            }
        }
        if !club_ids.is_empty() {
            let updated = sqlx::query("UPDATE club SET status = $2 WHERE id = ANY($1)")
                .bind(club_ids.as_slice())
                .bind(db_status)
                .execute(&mut *tx)
                .await;
            if updated.is_err() {
                return api_error(ApiError::Internal, "db error");
            }
        }
        if !dj_ids.is_empty() {
            let updated = sqlx::query("UPDATE dj SET status = $2 WHERE id = ANY($1)")
                .bind(dj_ids.as_slice())
                .bind(db_status)
                .execute(&mut *tx)
                .await;
            if updated.is_err() {
                return api_error(ApiError::Internal, "db error");
            }
        }
        if !guest_ids.is_empty() {
            let updated =
                sqlx::query("UPDATE guest_device SET banned = $2 WHERE account_id = ANY($1)")
                    .bind(guest_ids.as_slice())
                    .bind(banned)
                    .execute(&mut *tx)
                    .await;
            if updated.is_err() {
                return api_error(ApiError::Internal, "db error");
            }
        }
        // Блокировка/приостановка убивает живые сессии админов/клубов/DJ.
        if matches!(action.as_str(), "suspend" | "block") {
            for (subject_type, subject_ids) in
                [("admin", &admin_ids), ("club", &club_ids), ("dj", &dj_ids)]
            {
                if subject_ids.is_empty() {
                    continue;
                }
                let revoked = sqlx::query(
                    "UPDATE refresh_token SET revoked_at = now() \
                     WHERE subject_type = $1 AND subject_id = ANY($2) AND revoked_at IS NULL",
                )
                .bind(subject_type)
                .bind(subject_ids.as_slice())
                .execute(&mut *tx)
                .await;
                if revoked.is_err() {
                    return api_error(ApiError::Internal, "db error");
                }
            }
        }
    }

    let mut affected_ids: Vec<Uuid> = Vec::with_capacity(ids.len() + admin_ids.len());
    for found in [&admin_ids, &club_ids, &dj_ids, &guest_ids] {
        affected_ids.extend(found.iter().copied());
    }
    affected_ids.sort_unstable();
    affected_ids.dedup();
    let affected = affected_ids.len();

    let after = json!({
        "action": action,
        "role": role,
        "ids": affected_ids,
    });
    let diff = json!({
        "entity_type": "directory",
        "before": before,
        "after": after,
    });
    if audit_in_tx(
        &mut tx,
        ctx.subject_id(),
        "admin.user.bulk",
        "directory",
        diff,
    )
    .await
    .is_err()
    {
        return api_error(ApiError::Internal, "db error");
    }
    if tx.commit().await.is_err() {
        return api_error(ApiError::Internal, "db error");
    }

    (
        StatusCode::OK,
        Json(json!({ "affected": affected, "ids": affected_ids })),
    )
        .into_response()
}

/// Снимок «до» для аудита bulk: id → {role, status} по каждой таблице.
#[allow(clippy::type_complexity)]
async fn bulk_snapshot(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    admin_ids: &[Uuid],
    club_ids: &[Uuid],
    dj_ids: &[Uuid],
    guest_ids: &[Uuid],
) -> Result<Value, sqlx::Error> {
    let admins: Vec<(Uuid, String, String)> =
        sqlx::query_as("SELECT id, role, status FROM admin_user WHERE id = ANY($1) ORDER BY id")
            .bind(admin_ids)
            .fetch_all(&mut **tx)
            .await?;
    let clubs: Vec<(Uuid, String)> =
        sqlx::query_as("SELECT id, status FROM club WHERE id = ANY($1) ORDER BY id")
            .bind(club_ids)
            .fetch_all(&mut **tx)
            .await?;
    let djs: Vec<(Uuid, String)> =
        sqlx::query_as("SELECT id, status FROM dj WHERE id = ANY($1) ORDER BY id")
            .bind(dj_ids)
            .fetch_all(&mut **tx)
            .await?;
    let guests: Vec<(Uuid, bool, bool)> = sqlx::query_as(
        "SELECT g.id, g.verified, \
         EXISTS (SELECT 1 FROM guest_device gd WHERE gd.account_id = g.id AND gd.banned) \
         FROM guest_account g WHERE g.id = ANY($1) ORDER BY g.id",
    )
    .bind(guest_ids)
    .fetch_all(&mut **tx)
    .await?;
    Ok(json!({
        "admins": admins
            .iter()
            .map(|(id, role, status)| json!({
                "id": id, "role": role, "status": map_admin_status(status),
            }))
            .collect::<Vec<_>>(),
        "clubs": clubs
            .iter()
            .map(|(id, status)| json!({ "id": id, "status": map_club_status(status) }))
            .collect::<Vec<_>>(),
        "djs": djs
            .iter()
            .map(|(id, status)| json!({ "id": id, "status": map_dj_status(status) }))
            .collect::<Vec<_>>(),
        "guests": guests
            .iter()
            .map(|(id, verified, banned)| json!({
                "id": id, "status": map_guest_status(*verified, *banned),
            }))
            .collect::<Vec<_>>(),
    }))
}

// ============================================================
// GET /v1/admin/roles — матрица прав из фактического кода
// ============================================================

/// `GET /v1/admin/roles` — read-only матрица.
///
/// Раньше `/manage/access` рисовал матрицу из `localStorage`
/// (`RolePermissionMatrix.tsx`), то есть экран не имел отношения к RBAC.
/// Здесь матрица выводится из кода: `enforced_by` указывает модуль или
/// роут-семейство, которое реально проверяет право; `planned` — правило
/// описано в §7.6, но пока не применяется на всех роутах (честно, а не
/// «зелёная галочка»).
///
/// Новых таблиц не вводим: единственный источник истины для ролей —
/// `admin_user.role` (`superadmin|operator|readonly`) и JWT-клейм роли.
async fn admin_roles_matrix(ctx: AuthContext, State(state): State<ApiEnv>) -> Response {
    if let Some(resp) = admin_only(&ctx) {
        return resp;
    }
    // Подроль текущего оператора: UI по ней гасит недоступные действия.
    // Без БД (`None`) матрица остаётся доступной — это справочник из кода.
    let requester_admin_role = current_admin_role(&ctx, &state).await;
    (
        StatusCode::OK,
        Json(json!({
            "current_admin_role": requester_admin_role,
            "roles": [
                {
                    "code": "guest",
                    "label": "Guest",
                    "capabilities": [
                        { "code": "guest.device_token", "allowed": true, "enforced_by": "routes::issue_device (public)" },
                        { "code": "guest.catalog_search", "allowed": true, "enforced_by": "catalog_routes::guest_routes" },
                        { "code": "guest.session_join", "allowed": true, "enforced_by": "sessions_routes::join_session" },
                        { "code": "guest.request_create", "allowed": true, "enforced_by": "requests_routes::create_request" },
                        { "code": "guest.account_upgrade", "allowed": true, "enforced_by": "requests_routes::guest_upgrade" },
                        { "code": "club.djs_manage", "allowed": false, "enforced_by": "routes::role_gated (Role::Club)" },
                        { "code": "admin.panel", "allowed": false, "enforced_by": "routes::role_gated (Role::Admin)" },
                    ],
                },
                {
                    "code": "dj",
                    "label": "DJ",
                    "capabilities": [
                        { "code": "dj.catalog_search", "allowed": true, "enforced_by": "catalog_routes::dj_routes" },
                        { "code": "dj.queue_read", "allowed": true, "enforced_by": "requests_routes::dj_queue" },
                        { "code": "dj.request_decide", "allowed": true, "enforced_by": "requests_routes::dj_accept/reject/played" },
                        { "code": "dj.session_open", "allowed": true, "enforced_by": "sessions_routes::open_session" },
                        { "code": "dj.shift_and_stats", "allowed": true, "enforced_by": "panel_routes (Role::Dj handlers)" },
                        { "code": "dj.presence", "allowed": true, "enforced_by": "panel_routes::dj_presence" },
                        { "code": "club.payouts_read", "allowed": false, "enforced_by": "routes::role_gated (Role::Club)" },
                        { "code": "admin.panel", "allowed": false, "enforced_by": "routes::role_gated (Role::Admin)" },
                    ],
                },
                {
                    "code": "club",
                    "label": "Club staff",
                    "capabilities": [
                        { "code": "club.dashboard_and_profile", "allowed": true, "enforced_by": "panel_routes (Role::Club handlers)" },
                        { "code": "club.pricing", "allowed": true, "enforced_by": "panel_routes::club_pricing" },
                        { "code": "club.djs_manage", "allowed": true, "enforced_by": "panel_routes::club_djs_invite/patch/remove" },
                        { "code": "club.sessions_and_qr", "allowed": true, "enforced_by": "sessions_routes (Role::Club handlers)" },
                        { "code": "club.payouts_read", "allowed": true, "enforced_by": "panel_routes::club_payouts" },
                        { "code": "club.stripe_onboarding", "allowed": true, "enforced_by": "panel_routes::club_stripe_*" },
                        { "code": "club.api_keys", "allowed": true, "enforced_by": "api_key_routes (Role::Club)" },
                        { "code": "admin.panel", "allowed": false, "enforced_by": "routes::role_gated (Role::Admin)" },
                    ],
                },
                {
                    "code": "admin",
                    "label": "Platform admin",
                    "capabilities": [
                        { "code": "admin.users_read", "allowed": true, "enforced_by": "admin_users_routes (Role::Admin)" },
                        { "code": "admin.users_write", "allowed": true, "enforced_by": "admin_users_routes + rbac::require_admin_write" },
                        { "code": "admin.roles_read", "allowed": true, "enforced_by": "admin_users_routes::admin_roles_matrix" },
                        { "code": "admin.roles_manage", "allowed": false, "enforced_by": "not_implemented (§7.6: PUT /v1/admin/roles/:code/permissions)" },
                        { "code": "admin.clubs_manage", "allowed": true, "enforced_by": "panel_routes (Role::Admin handlers)" },
                        { "code": "admin.payments_read", "allowed": true, "enforced_by": "panel_routes::admin_payments" },
                        { "code": "admin.payouts_and_reconciliation", "allowed": true, "enforced_by": "payouts_routes (Role::Admin)" },
                        { "code": "admin.platform_settings", "allowed": true, "enforced_by": "panel_routes::admin_platform_settings" },
                        { "code": "admin.audit_read", "allowed": true, "enforced_by": "panel_routes::admin_audit_log" },
                        { "code": "admin.mail", "allowed": true, "enforced_by": "mail_routes (Role::Admin)" },
                        { "code": "admin.users_export", "allowed": false, "enforced_by": "not_implemented (§7.10: export)" },
                    ],
                },
            ],
            "admin_roles": [
                {
                    "code": "superadmin",
                    "can_write": true,
                    "can_manage_users": true,
                    "description": "Full control: catalog mutations, role changes, platform settings.",
                },
                {
                    "code": "operator",
                    "can_write": true,
                    "can_manage_users": false,
                    "description": "Writes allowed today (catalog edits, bulk); changing roles and creating admins is reserved for superadmin per §7.6 — the restriction is planned, not yet enforced on every route (enforced_by: planned).",
                },
                {
                    "code": "readonly",
                    "can_write": false,
                    "can_manage_users": false,
                    "description": "GET only: every mutating handler in this module calls rbac::require_admin_write; a readonly admin or an unknown/blocked admin row gets 403.",
                },
            ],
        })),
    )
        .into_response()
}

// ============================================================
// Аудит
// ============================================================

/// Пишет запись аудита в переданной транзакции.
///
/// `entity` — id сущности строкой (конвенция `panel_routes.rs` /
/// `api_key_routes.rs`): колонки `entity_id` в `audit_log` нет, поэтому
/// тип и id дублируются в `diff` (`entity_type`/`entity_id`).
async fn audit_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    actor_id: Uuid,
    action: &str,
    entity: &str,
    diff: Value,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO audit_log (actor_type, actor_id, action, entity, diff) \
         VALUES ('admin', $1, $2, $3, $4)",
    )
    .bind(actor_id)
    .bind(action)
    .bind(entity)
    .bind(diff)
    .execute(&mut **tx)
    .await
    .map(|_| ())
}
