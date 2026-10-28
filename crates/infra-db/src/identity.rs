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
    /// E-mail уже занят (UNIQUE по `club.email` / `dj.email`, citext).
    #[error("email already taken")]
    EmailTaken,
    /// Slug клуба занят (UNIQUE по `club.slug`) — вызывающий берёт
    /// следующий кандидат (`base-2`, `base-3`, …).
    #[error("slug already taken")]
    SlugTaken,
}

/// Разбор ошибки вставки: UNIQUE-нарушения конвертируются в доменные
/// варианты [`IdentityError`], чтобы gateway не зависел от текста ошибки
/// Postgres. Имя констрейнта (`club_slug_key`, `club_email_key`,
/// `dj_email_key`) содержит искомое слово.
fn map_insert_error(err: sqlx::Error) -> IdentityError {
    if let Some(db_err) = err.as_database_error() {
        if db_err.is_unique_violation() {
            match db_err.constraint() {
                Some(c) if c.contains("slug") => return IdentityError::SlugTaken,
                Some(c) if c.contains("email") => return IdentityError::EmailTaken,
                _ => {}
            }
        }
    }
    IdentityError::Db(err)
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
        // `email = $1::citext`: без явного каста параметр приходит как text,
        // и сравнение `citext = text` в Postgres РЕГИСТРОЗАВИСИМО (проверено:
        // 'x'::citext = 'X'::text → false, = 'X'::citext → true). Логин по
        // «Owner@Barrio.TEST» обязан находить клуб/DJ, созданный как
        // «owner@barrio.test» (citext-уникальность в схеме это уже
        // гарантирует, а поиск — нет).
        let row = sqlx::query_as::<_, PasswordSubject>(&format!(
            "SELECT id, email, password_hash, totp_secret_enc, status \
             FROM {table} WHERE email = $1::citext"
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
             WHERE subject_type = $1 AND email = $2::citext AND NOT ok \
             AND at > now() - interval '15 minutes'",
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
            "INSERT INTO login_attempt (subject_type, email, ip, ok) \
             VALUES ($1, $2::citext, $3::inet, $4)",
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
    pub async fn count_failed_master_logins(&self, ip: &str) -> Result<i64, super::IdentityError> {
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

    // ---------------------------------------------------------
    // Открытая регистрация (Test Mode stand): docs/signup-contract.md.
    // Только хранилище — валидация, slug и лимиты в gateway.
    // ---------------------------------------------------------

    /// Создаёт клуб открытой регистрации сразу `status='active'`.
    ///
    /// В Test Mode клуб не ждёт верификации админом (иначе панель не пускает
    /// его в работу: `login` пускает `pending_verification`, но UI ждёт
    /// рабочий аккаунт). `city`/`country` — необязательные поля анкеты.
    ///
    /// Ошибки: [`IdentityError::EmailTaken`] (UNIQUE club.email, citext),
    /// [`IdentityError::SlugTaken`] (UNIQUE club.slug — вызывающий перебирает
    /// следующий суффикс `-2`, `-3`, …).
    #[allow(clippy::too_many_arguments)]
    pub async fn create_registered_club(
        &self,
        id: Uuid,
        name: &str,
        slug: &str,
        email: &str,
        password_hash: &str,
        city: Option<&str>,
        country: Option<&str>,
    ) -> Result<(), super::IdentityError> {
        sqlx::query(
            "INSERT INTO club (id, name, slug, email, password_hash, city, country, status) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, 'active')",
        )
        .bind(id)
        .bind(name)
        .bind(slug)
        .bind(email)
        .bind(password_hash)
        .bind(city)
        .bind(country)
        .execute(&self.pool)
        .await
        .map_err(map_insert_error)?;
        Ok(())
    }

    /// Создаёт DJ открытой регистрации: `status='active'`, `club_id` —
    /// `None` для фрилансера либо id клуба, найденного по `club_slug`.
    ///
    /// Ошибки: [`IdentityError::EmailTaken`] (UNIQUE dj.email, citext).
    pub async fn create_registered_dj(
        &self,
        id: Uuid,
        display_name: &str,
        email: &str,
        password_hash: &str,
        club_id: Option<Uuid>,
    ) -> Result<(), super::IdentityError> {
        sqlx::query(
            "INSERT INTO dj (id, club_id, display_name, email, password_hash, status) \
             VALUES ($1, $2, $3, $4, $5, 'active')",
        )
        .bind(id)
        .bind(club_id)
        .bind(display_name)
        .bind(email)
        .bind(password_hash)
        .execute(&self.pool)
        .await
        .map_err(map_insert_error)?;
        Ok(())
    }

    /// Ищет клуб по slug (привязка DJ при регистрации по `club_slug`).
    /// Регистр не важен: slug генерируется в нижнем регистре, но клиент
    /// может прислать «Barrio-Latino» — нормализация в gateway + `lower()`.
    pub async fn find_club_id_by_slug(
        &self,
        slug: &str,
    ) -> Result<Option<Uuid>, super::IdentityError> {
        let id: Option<Uuid> =
            sqlx::query_scalar("SELECT id FROM club WHERE lower(slug) = lower($1)")
                .bind(slug)
                .fetch_optional(&self.pool)
                .await?;
        Ok(id)
    }

    /// Считает регистрации с одного IP за последний час (лимит 10/час).
    /// Учёт в БД (таблица `signup_attempt`), а не в памяти процесса:
    /// лимит переживает рестарт и общий для нескольких узлов gateway.
    pub async fn count_signups_for_ip(&self, ip: &str) -> Result<i64, super::IdentityError> {
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM signup_attempt \
             WHERE ip = $1::inet AND at > now() - interval '1 hour'",
        )
        .bind(ip)
        .fetch_one(&self.pool)
        .await?;
        Ok(count)
    }

    /// Фиксирует регистрацию для лимита 10/час/IP (и аудита).
    pub async fn record_signup_attempt(
        &self,
        ip: &str,
        role: Role,
    ) -> Result<(), super::IdentityError> {
        sqlx::query("INSERT INTO signup_attempt (ip, role) VALUES ($1::inet, $2)")
            .bind(ip)
            .bind(role.as_str())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    // ---------------------------------------------------------
    // Сброс пароля (миграция 0015, `password_reset_token`).
    //
    // Только хранилище: валидация, лимиты и тексты ответов — в gateway
    // (`reset_routes.rs`). Токен живёт 30 минут и гасится один раз;
    // в БД лежит лишь sha256(hex) сырого токена — как у refresh-токенов.
    // ---------------------------------------------------------

    /// Считает запросы сброса пароля для адреса за последний час (лимит 5).
    ///
    /// Субъект ищется тем же [`IdentityRepo::find_password_subject`], что и
    /// при логине: таблица выбирается по роли, e-mail сравнивается как
    /// `citext`. Неизвестный адрес → `0`: под него не создаётся строк, а
    /// отдельного журнала запросов нет (ответ на неизвестный адрес всё
    /// равно одинаковый `202` — перечислять пользователей нельзя).
    pub async fn count_reset_requests_for_email(
        &self,
        role: Role,
        email: &str,
    ) -> Result<i64, super::IdentityError> {
        let Some(subject) = self.find_password_subject(role, email).await? else {
            return Ok(0);
        };
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM password_reset_token \
             WHERE subject_type = $1 AND subject_id = $2 \
             AND created_at > now() - interval '1 hour'",
        )
        .bind(role.as_str())
        .bind(subject.id)
        .fetch_one(&self.pool)
        .await?;
        Ok(count)
    }

    /// Считает запросы сброса пароля с одного IP за последний час (лимит 5).
    ///
    /// Считаются строки `password_reset_token` по `request_ip`: запрос на
    /// неизвестный адрес строку не создаёт (так и задумано — иначе таблица
    /// превращается в журнал попыток и хранит чужие адреса). Для известного
    /// адреса счётчик честный: строка появляется на каждый принятый запрос.
    pub async fn count_reset_requests_for_ip(&self, ip: &str) -> Result<i64, super::IdentityError> {
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM password_reset_token \
             WHERE request_ip = $1::inet AND created_at > now() - interval '1 hour'",
        )
        .bind(ip)
        .fetch_one(&self.pool)
        .await?;
        Ok(count)
    }

    /// Сохраняет одноразовый токен сброса (в БД — только хеш).
    ///
    /// `expires_at` задаёт вызывающий (TTL 30 минут), `request_ip` — для
    /// лимита 5/час; при неопределённом IP gateway подставляет `0.0.0.0`
    /// (валидный IPv4-сентинел для колонки `INET`) — так же, как это делает
    /// [`IdentityRepo::record_device_token_issue`].
    pub async fn insert_password_reset_token(
        &self,
        id: Uuid,
        role: Role,
        subject_id: Uuid,
        token_hash: &str,
        expires_at: DateTime<Utc>,
        ip: &str,
    ) -> Result<(), super::IdentityError> {
        sqlx::query(
            "INSERT INTO password_reset_token \
             (id, subject_type, subject_id, token_hash, expires_at, request_ip) \
             VALUES ($1, $2, $3, $4, $5, $6::inet)",
        )
        .bind(id)
        .bind(role.as_str())
        .bind(subject_id)
        .bind(token_hash)
        .bind(expires_at)
        .bind(ip)
        .execute(&self.pool)
        .await
        .map_err(map_insert_error)?;
        Ok(())
    }

    /// Погашает токен: атомарно ставит `used_at = now()`, если он ещё жив.
    ///
    /// Один `UPDATE … RETURNING`: две одновременные отправки формы с одним
    /// токеном не могут обе получить строку — вторая не найдёт подходящей
    /// (`used_at` уже не NULL) и получит `Ok(None)`. Наружу «нет такого»,
    /// «уже использован» и «истёк» не различаются: это один и тот же
    /// `401 invalid_reset_token` (контракт, без утечки состояния токена).
    pub async fn consume_password_reset_token(
        &self,
        token_hash: &str,
    ) -> Result<Option<(Role, Uuid)>, super::IdentityError> {
        let row: Option<(String, Uuid)> = sqlx::query_as(
            "UPDATE password_reset_token SET used_at = now() \
             WHERE token_hash = $1 AND used_at IS NULL AND expires_at > now() \
             RETURNING subject_type, subject_id",
        )
        .bind(token_hash)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.and_then(|(subject_type, subject_id)| {
            role_from_subject_type(&subject_type).map(|role| (role, subject_id))
        }))
    }

    /// Меняет хеш пароля субъекта; таблица выбирается по роли так же, как в
    /// [`IdentityRepo::find_password_subject`]. Гость пароля не имеет →
    /// [`IdentityError::NotFound`].
    ///
    /// `NotFound` также при нуле обновлённых строк: субъект исчез между
    /// погашением токена и записью — вызывающий отвечает `500`, а не врёт
    /// клиенту про успешный сброс.
    pub async fn update_password_hash(
        &self,
        role: Role,
        subject_id: Uuid,
        password_hash: &str,
    ) -> Result<(), super::IdentityError> {
        let table = match role {
            Role::Dj => "dj",
            Role::Club => "club",
            Role::Admin => "admin_user",
            Role::Guest => return Err(IdentityError::NotFound),
        };
        let res = sqlx::query(&format!(
            "UPDATE {table} SET password_hash = $1 WHERE id = $2"
        ))
        .bind(password_hash)
        .bind(subject_id)
        .execute(&self.pool)
        .await?;
        if res.rows_affected() == 0 {
            return Err(IdentityError::NotFound);
        }
        Ok(())
    }

    /// Отзывает все живые refresh-токены субъекта (все семьи).
    ///
    /// После сброса пароля старые сессии обязаны умереть: иначе украденный
    /// refresh-токен продолжал бы работать, и сброс был бы бесполезен как
    /// реакция на компрометацию. Таблица `refresh_token` — из 0002,
    /// новая не вводится. Возвращает число отозванных строк.
    pub async fn revoke_all_refresh_tokens(
        &self,
        role: Role,
        subject_id: Uuid,
    ) -> Result<u64, super::IdentityError> {
        let res = sqlx::query(
            "UPDATE refresh_token SET revoked_at = now() \
             WHERE subject_type = $1 AND subject_id = $2 AND revoked_at IS NULL",
        )
        .bind(role.as_str())
        .bind(subject_id)
        .execute(&self.pool)
        .await?;
        Ok(res.rows_affected())
    }
}

/// `subject_type` строки БД → [`Role`] (`'club' | 'dj' | 'admin'`).
///
/// Нужен [`IdentityRepo::consume_password_reset_token`]: тип субъекта в
/// таблице хранится строкой (как в `refresh_token` / `login_attempt`), а
/// наружу отдаётся типизированная роль. Неизвестное значение → `None`
/// (вызывающий отвечает `401 invalid_reset_token`, а не паникует).
fn role_from_subject_type(subject_type: &str) -> Option<Role> {
    match subject_type {
        "club" => Some(Role::Club),
        "dj" => Some(Role::Dj),
        "admin" => Some(Role::Admin),
        _ => None,
    }
}
