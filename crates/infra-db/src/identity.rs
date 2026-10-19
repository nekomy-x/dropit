//! Репозиторий идентичности (F1): device-token, субъекты, refresh-токены.
//!
//! Все запросы — sqlx. Хеширование/подпись — в gateway (auth.rs);
//! здесь только持久ное хранение. Никаких бизнес-значений.

use chrono::{DateTime, Utc};
use dropit_common::Role;
use sqlx::postgres::PgPoolOptions;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum IdentityError {
    #[error("database error")]
    Db(#[from] sqlx::Error),
    #[error("not found")]
    NotFound,
    #[error("refresh token reuse detected")]
    RefreshReused,
}

/// Пул соединений с разумными local-дефолтами (прод-тюнинг — F9).
pub async fn connect(url: &str) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new().max_connections(10).connect(url).await
}

/// Строка субъекта парольного входа (dj/club/admin).
#[derive(Debug, FromRow)]
pub struct PasswordSubject {
    pub id: Uuid,
    pub email: String,
    pub password_hash: String,
    pub totp_secret_enc: Option<Vec<u8>>,
    pub status: String,
}

/// Гость-устройство.
#[derive(Debug, FromRow)]
pub struct GuestDevice {
    pub id: Uuid,
    pub account_id: Option<Uuid>,
    pub platform: String,
    pub app_version: Option<String>,
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
}

/// Refresh-токен (stateful сторона).
#[derive(Debug, FromRow)]
pub struct RefreshTokenRow {
    pub id: Uuid,
    pub family_id: Uuid,
    pub subject_type: String,
    pub subject_id: Uuid,
    pub token_hash: String,
    pub expires_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub replaced_by: Option<Uuid>,
}

#[derive(Clone)]
pub struct IdentityRepo {
    pool: PgPool,
}

impl IdentityRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Пул для построения других репозиториев (общее соединение).
    pub fn pool(&self) -> PgPool {
        self.pool.clone()
    }

    // ---------------------------------------------------------
    // Парольные субъекты (таблица выбирается по роли)
    // ---------------------------------------------------------

    /// Ищет dj/club/admin по email. Гость не входит: у него нет пароля.
    pub async fn find_password_subject(
        &self,
        role: Role,
        email: &str,
    ) -> Result<Option<PasswordSubject>, super::IdentityError> {
        let table = match role {
            Role::Dj => "dj",
            Role::Club => "club",
            Role::Admin => "admin_user",
            Role::Guest => return Err(IdentityError::NotFound),
        };
        let row = sqlx::query_as::<_, PasswordSubject>(&format!(
            "SELECT id, email, password_hash, totp_secret_enc, status \
             FROM {table} WHERE email = $1"
        ))
        .bind(email)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    // ---------------------------------------------------------
    // Гость: device-token
    // ---------------------------------------------------------

    /// Ищет устройство по id (= device-token). Для join-гейта F2.
    pub async fn find_guest_device(
        &self,
        id: Uuid,
    ) -> Result<Option<GuestDevice>, super::IdentityError> {
        let row = sqlx::query_as::<_, GuestDevice>(
            "SELECT id, account_id, platform, app_version, first_seen, last_seen              FROM guest_device WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    /// Создаёт новое устройство; `id` = сам device-token (UUIDv7).
    pub async fn create_guest_device(
        &self,
        device_id: Uuid,
        platform: &str,
        app_version: Option<&str>,
    ) -> Result<GuestDevice, super::IdentityError> {
        let row = sqlx::query_as::<_, GuestDevice>(
            "INSERT INTO guest_device (id, platform, app_version) \
             VALUES ($1, $2, $3) \
             RETURNING id, account_id, platform, app_version, first_seen, last_seen",
        )
        .bind(device_id)
        .bind(platform)
        .bind(app_version)
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    /// Считает device-token, выданные IP за последний час (лимит 5/ч, B).
    pub async fn count_device_tokens_for_ip(&self, ip: &str) -> Result<i64, super::IdentityError> {
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM device_token_issue WHERE ip = $1::inet AND at > now() - interval '1 hour'",
        )
        .bind(ip)
        .fetch_one(&self.pool)
        .await?;
        Ok(count)
    }

    /// Фиксирует выдачу device-token (для лимита/аудита).
    pub async fn record_device_token_issue(&self, ip: &str) -> Result<(), super::IdentityError> {
        sqlx::query("INSERT INTO device_token_issue (ip) VALUES ($1::inet)")
            .bind(ip)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    // ---------------------------------------------------------
    // Refresh-токены: ротация с семьями
    // ---------------------------------------------------------

    /// Сохраняет новый refresh-токен (новая семья = новый логин).
    pub async fn insert_refresh_token(
        &self,
        id: Uuid,
        family_id: Uuid,
        role: Role,
        subject_id: Uuid,
        token_hash: &str,
        expires_at: DateTime<Utc>,
    ) -> Result<(), super::IdentityError> {
        sqlx::query(
            "INSERT INTO refresh_token \
             (id, family_id, subject_type, subject_id, token_hash, expires_at) \
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(id)
        .bind(family_id)
        .bind(role.as_str())
        .bind(subject_id)
        .bind(token_hash)
        .bind(expires_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Ищет живой refresh-токен по хешу. Возвращает строку с семьёй.
    pub async fn find_refresh_token(
        &self,
        token_hash: &str,
    ) -> Result<Option<RefreshTokenRow>, super::IdentityError> {
        let row = sqlx::query_as::<_, RefreshTokenRow>(
            "SELECT id, family_id, subject_type, subject_id, token_hash, \
             expires_at, revoked_at, replaced_by \
             FROM refresh_token WHERE token_hash = $1",
        )
        .bind(token_hash)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    /// Ротация: помечает старый токен заменённым и создаёт новый
    /// в той же семье. Если старый уже заменён/отозван — Err(Reused):
    /// это кража, вызывающий обязан отозвать всю семью.
    pub async fn rotate_refresh_token(
        &self,
        old_id: Uuid,
        new_id: Uuid,
        new_hash: &str,
        expires_at: DateTime<Utc>,
    ) -> Result<Uuid, super::IdentityError> {
        let mut tx = self.pool.begin().await?;
        let updated = sqlx::query(
            "UPDATE refresh_token SET revoked_at = now(), replaced_by = $2 \
             WHERE id = $1 AND revoked_at IS NULL AND replaced_by IS NULL",
        )
        .bind(old_id)
        .bind(new_id)
        .execute(&mut *tx)
        .await?;
        if updated.rows_affected() != 1 {
            // Уже заменён или отозван → reuse-детекция.
            tx.rollback().await?;
            return Err(IdentityError::RefreshReused);
        }
        let (family_id, subject_type, subject_id): (Uuid, String, Uuid) = sqlx::query_as(
            "SELECT family_id, subject_type, subject_id FROM refresh_token WHERE id = $1",
        )
        .bind(old_id)
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO refresh_token \
             (id, family_id, subject_type, subject_id, token_hash, expires_at) \
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(new_id)
        .bind(family_id)
        .bind(&subject_type)
        .bind(subject_id)
        .bind(new_hash)
        .bind(expires_at)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(family_id)
    }

    /// Отзывает всю семью (reuse-детекция / logout-all).
    pub async fn revoke_family(&self, family_id: Uuid) -> Result<u64, super::IdentityError> {
        let res = sqlx::query(
            "UPDATE refresh_token SET revoked_at = now() \
             WHERE family_id = $1 AND revoked_at IS NULL",
        )
        .bind(family_id)
        .execute(&self.pool)
        .await?;
        Ok(res.rows_affected())
    }

    // ---------------------------------------------------------
    // Логины: rate-limit 5/15 мин
    // ---------------------------------------------------------

    /// Считает неуспешные попытки за 15 мин (лимит 5).
    pub async fn count_failed_logins(
        &self,
        role: Role,
        email: &str,
    ) -> Result<i64, super::IdentityError> {
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM login_attempt \
             WHERE subject_type = $1 AND email = $2 AND NOT ok AND at > now() - interval '15 minutes'",
        )
        .bind(role.as_str())
        .bind(email)
        .fetch_one(&self.pool)
        .await?;
        Ok(count)
    }

    /// Фиксирует попытку логина (успех/неудача) для лимита и аудита.
    pub async fn record_login_attempt(
        &self,
        role: Role,
        email: &str,
        ip: Option<&str>,
        ok: bool,
    ) -> Result<(), super::IdentityError> {
        sqlx::query(
            "INSERT INTO login_attempt (subject_type, email, ip, ok) VALUES ($1, $2, $3::inet, $4)",
        )
        .bind(role.as_str())
        .bind(email)
        .bind(ip)
        .bind(ok)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    // ---------------------------------------------------------
    // Вход клуба по master-токену (W5-C): поиск по хешу + лимит 10/15 мин/IP.
    // Переиспользуется таблица login_attempt (subject_type='club_master'),
    // отдельная миграция не нужна: email NOT NULL заполняется сентиinel-значением.
    // ---------------------------------------------------------

    /// Ищет клуб по хешу master-токена. Возвращает `(id, status, stored_hash)` —
    /// хеш нужен вызывающему для сравнения в постоянном времени (defense-in-depth:
    /// БД уже фильтрует по хешу, но явное ct-сравнение защищает от нюансов
    /// сравнения строк в Postgres и удовлетворяет контракту безопасности).
    pub async fn find_club_by_master_token_hash(
        &self,
        hash: &str,
    ) -> Result<Option<(Uuid, String, String)>, super::IdentityError> {
        let row = sqlx::query_as::<_, (Uuid, String, String)>(
            "SELECT id, status, qr_master_token_hash \
             FROM club WHERE qr_master_token_hash = $1 LIMIT 1",
        )
        .bind(hash)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    /// Считает неудачные попытки входа по master-токену с одного IP за 15 мин
    /// (лимит 10). Использует `subject_type='club_master'` в `login_attempt`.
    pub async fn count_failed_master_logins(
        &self,
        ip: &str,
    ) -> Result<i64, super::IdentityError> {
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM login_attempt \
             WHERE subject_type = 'club_master' AND ip = $1::inet \
             AND NOT ok AND at > now() - interval '15 minutes'",
        )
        .bind(ip)
        .fetch_one(&self.pool)
        .await?;
        Ok(count)
    }

    /// Фиксирует попытку входа по master-токену (успех/неудача) для лимита и
    /// аудита. `email` — сentiinel `'master'` (колонка NOT NULL; реальный
    /// идентификатор клуба — `qr_master_token_hash`, а лимит считается по IP).
    pub async fn record_master_login_attempt(
        &self,
        ip: Option<&str>,
        ok: bool,
    ) -> Result<(), super::IdentityError> {
        sqlx::query(
            "INSERT INTO login_attempt (subject_type, email, ip, ok) \
             VALUES ('club_master', 'master', $1::inet, $2)",
        )
        .bind(ip)
        .bind(ok)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
