//! Репозиторий API-ключей Developer API v1 (ADR-010).
//!
//! Инварианты:
//!   * plaintext-ключ не хранится и не возвращается списочными методами —
//!     только при создании/ротации (`NewApiKey::revealed`);
//!   * любой запрос Developer API сначала разрешается в [`AuthenticatedKey`]
//!     (club_id + scopes + лимиты), и только потом трогает данные клуба;
//!   * отзыв и истечение проверяются на каждом запросе, не кэшируются;
//!   * лимит считается по `api_key_usage` за последние 60 секунд;
//!   * изменение скоупов только сужает набор (расширение = новый ключ).

use chrono::{DateTime, Duration, Utc};
use dropit_common::{
    hash_api_key, normalize_scopes, secret_eq, Environment, Scope, ScopeError,
    DEFAULT_RATE_LIMIT_PER_MIN, MAX_ACTIVE_KEYS_PER_CLUB, MAX_RATE_LIMIT_PER_MIN,
    MIN_RATE_LIMIT_PER_MIN,
};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum ApiKeyError {
    #[error("database error")]
    Db(#[from] sqlx::Error),
    #[error("api key not found")]
    NotFound,
    #[error("unknown or malformed api key")]
    InvalidKey,
    #[error("api key is revoked")]
    Revoked,
    #[error("api key expired")]
    Expired,
    #[error("ip address is not allowed for this key")]
    IpNotAllowed,
    #[error("rate limit exceeded")]
    RateLimited,
    #[error("too many active keys for this club")]
    TooManyKeys,
    #[error("invalid scope: {0}")]
    InvalidScope(String),
    #[error("at least one scope is required")]
    EmptyScopes,
    #[error("scopes can only be narrowed, not widened")]
    ScopeWidening,
    #[error("invalid ip rule: {0}")]
    InvalidIpRule(String),
    #[error("rate limit out of range")]
    RateLimitOutOfRange,
    #[error("invalid input: {0}")]
    Invalid(String),
    #[error("club not found")]
    ClubNotFound,
}

impl From<ScopeError> for ApiKeyError {
    fn from(err: ScopeError) -> Self {
        match err {
            ScopeError::Unknown(value) => ApiKeyError::InvalidScope(value),
            ScopeError::Empty => ApiKeyError::EmptyScopes,
        }
    }
}

/// Строка ключа без секрета (то, что можно отдавать в панель).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ApiKeyRow {
    pub id: Uuid,
    pub club_id: Uuid,
    pub name: String,
    pub prefix: String,
    pub environment: String,
    pub scopes: Vec<String>,
    pub status: String,
    pub rate_limit_per_min: i32,
    pub allowed_cidrs: Vec<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub created_by: Option<Uuid>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub rotated_from: Option<Uuid>,
}

impl ApiKeyRow {
    /// Типизированный набор скоупов (неизвестные строки отбрасываются:
    /// ключ не должен получить больше прав из-за опечатки в БД).
    pub fn scope_set(&self) -> Vec<Scope> {
        self.scopes
            .iter()
            .filter_map(|value| Scope::parse(value))
            .collect()
    }

    pub fn environment_typed(&self) -> Option<Environment> {
        Environment::parse(&self.environment)
    }

    pub fn is_active(&self, now: DateTime<Utc>) -> bool {
        self.status == "active" && self.expires_at.map(|at| at > now).unwrap_or(true)
    }
}

/// Результат создания/ротации: ключ + plaintext, который показывается один раз.
#[derive(Debug, Clone)]
pub struct NewApiKey {
    pub key: ApiKeyRow,
    /// Полный ключ. Не логируется, не хранится; отдаётся клиенту один раз.
    pub revealed: String,
}

/// Разрешённый ключ для текущего запроса Developer API.
#[derive(Debug, Clone)]
pub struct AuthenticatedKey {
    pub key_id: Uuid,
    pub club_id: Uuid,
    pub prefix: String,
    pub environment: Environment,
    pub scopes: Vec<Scope>,
    pub rate_limit_per_min: i32,
}

impl AuthenticatedKey {
    pub fn allows(&self, scope: Scope) -> bool {
        self.scopes.contains(&scope)
    }
}

/// Параметры создания ключа (валидируются в [`ApiKeyRepo::create`]).
#[derive(Debug, Clone)]
pub struct NewApiKeyInput {
    pub club_id: Uuid,
    pub name: String,
    pub environment: Environment,
    pub scopes: Vec<String>,
    pub rate_limit_per_min: Option<i32>,
    pub allowed_cidrs: Vec<String>,
    /// Срок жизни, дни. `None` — бессрочный.
    pub expires_in_days: Option<i64>,
    pub created_by: Option<Uuid>,
}

/// Публичная карточка ключа для панели: без секрета, с производными полями.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ApiKeyView {
    pub id: Uuid,
    pub name: String,
    pub prefix: String,
    pub environment: String,
    pub scopes: Vec<String>,
    pub status: String,
    pub rate_limit_per_min: i32,
    pub allowed_cidrs: Vec<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub created_by: Option<Uuid>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub rotated_from: Option<Uuid>,
    /// Истёк ли ключ на момент ответа (удобно для бейджа в списке).
    pub expired: bool,
}

impl From<ApiKeyRow> for ApiKeyView {
    fn from(row: ApiKeyRow) -> Self {
        let now = Utc::now();
        let expired = row.expires_at.map(|at| at <= now).unwrap_or(false);
        ApiKeyView {
            id: row.id,
            name: row.name,
            prefix: row.prefix,
            environment: row.environment,
            scopes: row.scopes,
            status: row.status,
            rate_limit_per_min: row.rate_limit_per_min,
            allowed_cidrs: row.allowed_cidrs,
            expires_at: row.expires_at,
            created_at: row.created_at,
            created_by: row.created_by,
            last_used_at: row.last_used_at,
            revoked_at: row.revoked_at,
            rotated_from: row.rotated_from,
            expired,
        }
    }
}

/// Строка ключа для админ-обзора: карточка ключа + название клуба.
///
/// Секрета здесь нет — админ видит только публичный `prefix`.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AdminApiKeyRow {
    /// Название клуба-владельца (join по `club_id`).
    pub club_name: String,
    #[sqlx(flatten)]
    pub key: ApiKeyRow,
}

/// Одна запись журнала вызовов (лента в панели).
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct UsageRow {
    pub at: DateTime<Utc>,
    pub method: String,
    pub path: String,
    pub status: i16,
}

/// Сводка по ключу за период (30 дней журнала).
#[derive(Debug, Clone, serde::Serialize)]
pub struct UsageSummary {
    pub calls_24h: i64,
    pub errors_24h: i64,
    pub calls_30d: i64,
    pub last_call_at: Option<DateTime<Utc>>,
}

const KEY_COLUMNS: &str = "id, club_id, name, prefix, environment, scopes, status, \
     rate_limit_per_min, allowed_cidrs, expires_at, created_at, created_by, last_used_at, \
     revoked_at, rotated_from";

/// Те же колонки с алиасом `k.` для join'ов (админ-обзор с названием клуба).
const KEY_COLUMNS_ALIASED: &str = "k.id, k.club_id, k.name, k.prefix, k.environment, k.scopes, \
     k.status, k.rate_limit_per_min, k.allowed_cidrs, k.expires_at, k.created_at, k.created_by, \
     k.last_used_at, k.revoked_at, k.rotated_from";

#[derive(Clone)]
pub struct ApiKeyRepo {
    pool: PgPool,
}

impl ApiKeyRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    // ---------------------------------------------------------
    // Управление ключами (панель)
    // ---------------------------------------------------------

    /// Создаёт ключ: валидирует скоупы/IP/лимит/срок, считает активные ключи,
    /// генерирует секрет, пишет только sha256. Возвращает plaintext один раз.
    pub async fn create(&self, input: NewApiKeyInput) -> Result<NewApiKey, ApiKeyError> {
        let name = input.name.trim();
        if name.is_empty() || name.chars().count() > 64 {
            return Err(ApiKeyError::Invalid("name must be 1..64 characters".into()));
        }
        let scopes = normalize_scopes(&input.scopes)?;
        let rate = input
            .rate_limit_per_min
            .unwrap_or(DEFAULT_RATE_LIMIT_PER_MIN);
        if !(MIN_RATE_LIMIT_PER_MIN..=MAX_RATE_LIMIT_PER_MIN).contains(&rate) {
            return Err(ApiKeyError::RateLimitOutOfRange);
        }
        dropit_common::validate_ip_rules(&input.allowed_cidrs)
            .map_err(|err| ApiKeyError::InvalidIpRule(err.to_string()))?;
        let expires_at = match input.expires_in_days {
            Some(days) if !(1..=365).contains(&days) => {
                return Err(ApiKeyError::Invalid(
                    "expires_in_days must be 1..365".into(),
                ))
            }
            Some(days) => Some(Utc::now() + Duration::days(days)),
            None => None,
        };

        let mut tx = self.pool.begin().await?;
        // Клуб должен существовать: ключ без владельца бессмысленен.
        let club: Option<(Uuid,)> = sqlx::query_as("SELECT id FROM club WHERE id = $1")
            .bind(input.club_id)
            .fetch_optional(&mut *tx)
            .await?;
        if club.is_none() {
            return Err(ApiKeyError::ClubNotFound);
        }
        let active: (i64,) = sqlx::query_as(
            "SELECT count(*) FROM api_key WHERE club_id = $1 AND status = 'active' \
             AND (expires_at IS NULL OR expires_at > now())",
        )
        .bind(input.club_id)
        .fetch_one(&mut *tx)
        .await?;
        if active.0 >= MAX_ACTIVE_KEYS_PER_CLUB {
            return Err(ApiKeyError::TooManyKeys);
        }

        let generated = dropit_common::generate_api_key(input.environment);
        let scope_strings = dropit_common::scopes_to_strings(&scopes);
        let id = Uuid::now_v7();
        let row = sqlx::query_as::<_, ApiKeyRow>(&format!(
            "INSERT INTO api_key (id, club_id, name, prefix, secret_hash, environment, \
             scopes, rate_limit_per_min, allowed_cidrs, expires_at, created_by) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11) \
             RETURNING {KEY_COLUMNS}"
        ))
        .bind(id)
        .bind(input.club_id)
        .bind(name)
        .bind(&generated.prefix)
        .bind(&generated.hash)
        .bind(generated.environment.as_str())
        .bind(&scope_strings)
        .bind(rate)
        .bind(&input.allowed_cidrs)
        .bind(expires_at)
        .bind(input.created_by)
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;

        Ok(NewApiKey {
            key: row,
            revealed: generated.plaintext,
        })
    }

    /// Список ключей клуба (включая отозванные: история видна в панели).
    pub async fn list_for_club(&self, club_id: Uuid) -> Result<Vec<ApiKeyRow>, ApiKeyError> {
        let rows = sqlx::query_as::<_, ApiKeyRow>(&format!(
            "SELECT {KEY_COLUMNS} FROM api_key WHERE club_id = $1 \
             ORDER BY (status = 'active') DESC, created_at DESC"
        ))
        .bind(club_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Список всех ключей платформы (админ, read-only аудит).
    pub async fn list_all(&self, limit: i64) -> Result<Vec<ApiKeyRow>, ApiKeyError> {
        let rows = sqlx::query_as::<_, ApiKeyRow>(&format!(
            "SELECT {KEY_COLUMNS} FROM api_key ORDER BY created_at DESC LIMIT $1"
        ))
        .bind(limit.clamp(1, 200))
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Все ключи платформы вместе с названием клуба (админ-обзор).
    ///
    /// Один запрос вместо N+1: админ-таблица показывает владельца рядом
    /// с ключом, чтобы разбирать инцидент без перехода в карточку клуба.
    pub async fn list_all_with_club(&self, limit: i64) -> Result<Vec<AdminApiKeyRow>, ApiKeyError> {
        let rows = sqlx::query_as::<_, AdminApiKeyRow>(&format!(
            "SELECT {KEY_COLUMNS_ALIASED}, c.name AS club_name \
             FROM api_key k JOIN club c ON c.id = k.club_id \
             ORDER BY k.created_at DESC LIMIT $1"
        ))
        .bind(limit.clamp(1, 200))
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn find(&self, club_id: Uuid, id: Uuid) -> Result<Option<ApiKeyRow>, ApiKeyError> {
        let row = sqlx::query_as::<_, ApiKeyRow>(&format!(
            "SELECT {KEY_COLUMNS} FROM api_key WHERE id = $1 AND club_id = $2"
        ))
        .bind(id)
        .bind(club_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    /// Ключ по id без ограничения клубом — только для админ-оверсайта.
    /// В клубных хендлерах используется [`ApiKeyRepo::find`], чтобы чужой
    /// ключ отвечал `404`, а не раскрывал своё существование.
    pub async fn find_any(&self, id: Uuid) -> Result<Option<ApiKeyRow>, ApiKeyError> {
        let row = sqlx::query_as::<_, ApiKeyRow>(&format!(
            "SELECT {KEY_COLUMNS} FROM api_key WHERE id = $1"
        ))
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    /// Переименование и сужение скоупов. Расширение запрещено: повышение
    /// прав делается новым ключом, чтобы это было видно в аудите.
    pub async fn update(
        &self,
        club_id: Uuid,
        id: Uuid,
        name: Option<&str>,
        scopes: Option<&[String]>,
    ) -> Result<ApiKeyRow, ApiKeyError> {
        let current = self.find(club_id, id).await?.ok_or(ApiKeyError::NotFound)?;
        if current.status != "active" {
            return Err(ApiKeyError::Revoked);
        }
        let next_name = match name {
            Some(value) => {
                let trimmed = value.trim();
                if trimmed.is_empty() || trimmed.chars().count() > 64 {
                    return Err(ApiKeyError::Invalid("name must be 1..64 characters".into()));
                }
                trimmed.to_string()
            }
            None => current.name.clone(),
        };
        let next_scopes = match scopes {
            Some(values) => {
                let parsed = normalize_scopes(values)?;
                let existing = current.scope_set();
                for scope in &parsed {
                    if !existing.contains(scope) {
                        return Err(ApiKeyError::ScopeWidening);
                    }
                }
                dropit_common::scopes_to_strings(&parsed)
            }
            None => current.scopes.clone(),
        };
        let row = sqlx::query_as::<_, ApiKeyRow>(&format!(
            "UPDATE api_key SET name = $3, scopes = $4 WHERE id = $1 AND club_id = $2 \
             AND status = 'active' RETURNING {KEY_COLUMNS}"
        ))
        .bind(id)
        .bind(club_id)
        .bind(next_name)
        .bind(next_scopes)
        .fetch_optional(&self.pool)
        .await?;
        row.ok_or(ApiKeyError::NotFound)
    }

    /// Отзыв ключа. Идемпотентно: повторный отзыв возвращает NotFound.
    pub async fn revoke(&self, club_id: Uuid, id: Uuid) -> Result<(), ApiKeyError> {
        let res = sqlx::query(
            "UPDATE api_key SET status = 'revoked', revoked_at = now() \
             WHERE id = $1 AND club_id = $2 AND status = 'active'",
        )
        .bind(id)
        .bind(club_id)
        .execute(&self.pool)
        .await?;
        if res.rows_affected() == 0 {
            return Err(ApiKeyError::NotFound);
        }
        Ok(())
    }

    /// Отзыв любого ключа платформы — админ-оверсайт (инцидент, утечка).
    /// Клуб не проверяется намеренно: админ отзывает ключ независимо от
    /// владельца, но владелец остаётся в аудите (его читает хендлер).
    pub async fn revoke_any(&self, id: Uuid) -> Result<(), ApiKeyError> {
        let res = sqlx::query(
            "UPDATE api_key SET status = 'revoked', revoked_at = now() \
             WHERE id = $1 AND status = 'active'",
        )
        .bind(id)
        .execute(&self.pool)
        .await?;
        if res.rows_affected() == 0 {
            return Err(ApiKeyError::NotFound);
        }
        Ok(())
    }

    /// Ротация: новый секрет в новом ключе, старый живёт ещё `grace_hours`.
    ///
    /// Скоупы, окружение, лимит и allowlist наследуются. `rotated_from`
    /// связывает ключи, чтобы панель показала цепочку.
    pub async fn rotate(
        &self,
        club_id: Uuid,
        id: Uuid,
        grace_hours: Option<i64>,
    ) -> Result<NewApiKey, ApiKeyError> {
        let grace = grace_hours.unwrap_or(dropit_common::DEFAULT_ROTATION_GRACE_HOURS);
        if !(0..=dropit_common::MAX_ROTATION_GRACE_HOURS).contains(&grace) {
            return Err(ApiKeyError::Invalid("grace_hours must be 0..=72".into()));
        }
        let current = self.find(club_id, id).await?.ok_or(ApiKeyError::NotFound)?;
        if current.status != "active" {
            return Err(ApiKeyError::Revoked);
        }
        let environment = current
            .environment_typed()
            .ok_or(ApiKeyError::Invalid("stored environment is unknown".into()))?;

        let mut tx = self.pool.begin().await?;
        let generated = dropit_common::generate_api_key(environment);
        let new_id = Uuid::now_v7();
        let row = sqlx::query_as::<_, ApiKeyRow>(&format!(
            "INSERT INTO api_key (id, club_id, name, prefix, secret_hash, environment, \
             scopes, rate_limit_per_min, allowed_cidrs, expires_at, created_by, rotated_from) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12) \
             RETURNING {KEY_COLUMNS}"
        ))
        .bind(new_id)
        .bind(club_id)
        .bind(&current.name)
        .bind(&generated.prefix)
        .bind(&generated.hash)
        .bind(environment.as_str())
        .bind(&current.scopes)
        .bind(current.rate_limit_per_min)
        .bind(&current.allowed_cidrs)
        .bind(current.expires_at)
        .bind(current.created_by)
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;

        // Старый ключ гаснет: сразу (grace = 0) или по истечении окна.
        let old_expiry = Utc::now() + Duration::hours(grace);
        sqlx::query(
            "UPDATE api_key SET expires_at = LEAST(COALESCE(expires_at, $2), $2) \
             WHERE id = $1",
        )
        .bind(id)
        .bind(old_expiry)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;

        Ok(NewApiKey {
            key: row,
            revealed: generated.plaintext,
        })
    }

    // ---------------------------------------------------------
    // Аутентификация Developer API
    // ---------------------------------------------------------

    /// Разрешает ключ из заголовка: формат → prefix → sha256-сверка →
    /// статус/срок → allowlist IP. Ничего не различает по коду ответа:
    /// вызывающий получит [`ApiKeyError::InvalidKey`] на любую неудачу,
    /// чтобы не подсказывать атакующему существование prefix.
    pub async fn authenticate(
        &self,
        raw_key: &str,
        ip: std::net::IpAddr,
    ) -> Result<AuthenticatedKey, ApiKeyError> {
        let parsed = dropit_common::parse_api_key(raw_key).ok_or(ApiKeyError::InvalidKey)?;
        let row = sqlx::query_as::<
            _,
            (
                Uuid,
                Uuid,
                String,
                String,
                Vec<String>,
                i32,
                Vec<String>,
                Option<DateTime<Utc>>,
                String,
            ),
        >(
            "SELECT id, club_id, environment, secret_hash, scopes, rate_limit_per_min, \
             allowed_cidrs, expires_at, status FROM api_key WHERE prefix = $1",
        )
        .bind(&parsed.prefix)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(ApiKeyError::InvalidKey)?;
        let (
            key_id,
            club_id,
            environment,
            secret_hash,
            scopes,
            rate_limit_per_min,
            allowed_cidrs,
            expires_at,
            status,
        ) = row;

        // Постоянное время: сравниваем хеши, не секреты.
        if !secret_eq(&hash_api_key(raw_key), &secret_hash) {
            return Err(ApiKeyError::InvalidKey);
        }
        if status != "active" {
            return Err(ApiKeyError::Revoked);
        }
        let now = Utc::now();
        if expires_at.map(|at| at <= now).unwrap_or(false) {
            return Err(ApiKeyError::Expired);
        }
        if !dropit_common::ip_allowed(&allowed_cidrs, ip) {
            return Err(ApiKeyError::IpNotAllowed);
        }
        let environment = Environment::parse(&environment).ok_or(ApiKeyError::InvalidKey)?;
        let scopes: Vec<Scope> = scopes.iter().filter_map(|v| Scope::parse(v)).collect();

        Ok(AuthenticatedKey {
            key_id,
            club_id,
            prefix: parsed.prefix,
            environment,
            scopes,
            rate_limit_per_min,
        })
    }

    /// Клуб активен? Ключ неактивного клуба не работает (403 в хендлере).
    pub async fn club_is_active(&self, club_id: Uuid) -> Result<bool, ApiKeyError> {
        let row: Option<(String,)> = sqlx::query_as("SELECT status FROM club WHERE id = $1")
            .bind(club_id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(|(status,)| status == "active").unwrap_or(false))
    }

    // ---------------------------------------------------------
    // Лимит и журнал
    // ---------------------------------------------------------

    /// Считает вызовы ключа за последние 60 секунд.
    pub async fn calls_last_minute(&self, key_id: Uuid) -> Result<i64, ApiKeyError> {
        let row: (i64,) = sqlx::query_as(
            "SELECT count(*) FROM api_key_usage \
             WHERE api_key_id = $1 AND at > now() - interval '60 seconds'",
        )
        .bind(key_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.0)
    }

    /// Записывает вызов и обновляет `last_used_at` одним шагом.
    pub async fn record_usage(
        &self,
        key_id: Uuid,
        club_id: Uuid,
        method: &str,
        path: &str,
        status: u16,
    ) -> Result<(), ApiKeyError> {
        sqlx::query(
            "INSERT INTO api_key_usage (api_key_id, club_id, method, path, status) \
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(key_id)
        .bind(club_id)
        .bind(method)
        .bind(path)
        .bind(status as i16)
        .execute(&self.pool)
        .await?;
        sqlx::query("UPDATE api_key SET last_used_at = now() WHERE id = $1")
            .bind(key_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Последние вызовы ключа (лента в панели).
    pub async fn recent_usage(
        &self,
        club_id: Uuid,
        key_id: Uuid,
        limit: i64,
    ) -> Result<Vec<UsageRow>, ApiKeyError> {
        let rows = sqlx::query_as::<_, UsageRow>(
            "SELECT at, method, path, status FROM api_key_usage \
             WHERE api_key_id = $1 AND club_id = $2 ORDER BY at DESC LIMIT $3",
        )
        .bind(key_id)
        .bind(club_id)
        .bind(limit.clamp(1, 100))
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Сводка использования за 24 часа / 30 дней.
    pub async fn usage_summary(
        &self,
        club_id: Uuid,
        key_id: Uuid,
    ) -> Result<UsageSummary, ApiKeyError> {
        let row: (i64, i64, i64, Option<DateTime<Utc>>) = sqlx::query_as(
            "SELECT \
               count(*) FILTER (WHERE at > now() - interval '24 hours'), \
               count(*) FILTER (WHERE at > now() - interval '24 hours' AND status >= 400), \
               count(*) FILTER (WHERE at > now() - interval '30 days'), \
               max(at) \
             FROM api_key_usage WHERE api_key_id = $1 AND club_id = $2",
        )
        .bind(key_id)
        .bind(club_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(UsageSummary {
            calls_24h: row.0,
            errors_24h: row.1,
            calls_30d: row.2,
            last_call_at: row.3,
        })
    }

    // ---------------------------------------------------------
    // Идемпотентность Developer API
    // ---------------------------------------------------------

    /// Сохранённый replay-ответ по (ключ, Idempotency-Key).
    pub async fn find_idempotent_response(
        &self,
        key_id: Uuid,
        idem_key: &str,
    ) -> Result<Option<serde_json::Value>, ApiKeyError> {
        let row: Option<(serde_json::Value,)> = sqlx::query_as(
            "SELECT response FROM developer_idempotency \
             WHERE api_key_id = $1 AND idem_key = $2",
        )
        .bind(key_id)
        .bind(idem_key)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|(value,)| value))
    }

    pub async fn store_idempotent_response(
        &self,
        key_id: Uuid,
        idem_key: &str,
        response: &serde_json::Value,
    ) -> Result<(), ApiKeyError> {
        sqlx::query(
            "INSERT INTO developer_idempotency (api_key_id, idem_key, response) \
             VALUES ($1, $2, $3) ON CONFLICT (api_key_id, idem_key) DO NOTHING",
        )
        .bind(key_id)
        .bind(idem_key)
        .bind(response)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Чистка журнала и replay-кэша (воркер retention).
    pub async fn purge_usage(&self, older_than_days: i64) -> Result<u64, ApiKeyError> {
        let usage =
            sqlx::query("DELETE FROM api_key_usage WHERE at < now() - make_interval(days => $1)")
                .bind(older_than_days.max(1))
                .execute(&self.pool)
                .await?;
        let idem = sqlx::query(
            "DELETE FROM developer_idempotency WHERE created_at < now() - interval '24 hours'",
        )
        .execute(&self.pool)
        .await?;
        Ok(usage.rows_affected() + idem.rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn view_hides_secret_and_marks_expiry() {
        let row = ApiKeyRow {
            id: Uuid::now_v7(),
            club_id: Uuid::now_v7(),
            name: "CI".into(),
            prefix: "0123456789".into(),
            environment: "live".into(),
            scopes: vec!["docs:read".into()],
            status: "active".into(),
            rate_limit_per_min: 120,
            allowed_cidrs: vec![],
            expires_at: Some(Utc::now() - Duration::days(1)),
            created_at: Utc::now(),
            created_by: None,
            last_used_at: None,
            revoked_at: None,
            rotated_from: None,
        };
        let view = ApiKeyView::from(row);
        assert!(view.expired);
        let json = serde_json::to_string(&view).unwrap();
        assert!(!json.contains("secret"));
        assert!(!json.contains("hash"));
    }

    #[test]
    fn scope_set_ignores_unknown_strings() {
        let row = ApiKeyRow {
            id: Uuid::now_v7(),
            club_id: Uuid::now_v7(),
            name: "x".into(),
            prefix: "0123456789".into(),
            environment: "test".into(),
            scopes: vec!["docs:read".into(), "root:*".into()],
            status: "active".into(),
            rate_limit_per_min: 120,
            allowed_cidrs: vec![],
            expires_at: None,
            created_at: Utc::now(),
            created_by: None,
            last_used_at: None,
            revoked_at: None,
            rotated_from: None,
        };
        assert_eq!(row.scope_set(), vec![Scope::DocsRead]);
    }
}
