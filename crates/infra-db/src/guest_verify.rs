//! Подтверждение e-mail гостя: одноразовый код из письма → `verified`.
//!
//! Колонка `guest_account.verified` появилась миграцией `0010` и всегда была
//! `FALSE`. Появление почтового транспорта (ADR-008) позволяет закрыть флоу:
//! при апгрейде гостя уходит письмо с кодом, гость присылает код, и только
//! тогда адрес считается подтверждённым.
//!
//! # Почему код, а не ссылка
//!
//! Ссылка требует, чтобы её открыли в браузере с тем же устройством; гости
//! DROP IT — мобильные приложения, и deep-link в письме всё равно вернулся бы
//! к вводу кода. Код из шести цифр вводится в том же экране, где гость ввёл
//! e-mail, и работает без сети между письмом и приложением.
//!
//! # Безопасность
//!
//! * В БД лежит только SHA-256 от `"{account_id}:{code}"` — не сам код.
//! * Срок жизни 15 минут, проверяется в SQL (`expires_at > now()`).
//! * Пять неверных попыток удаляют код: перебрать 10^6 вариантов за минуту
//!   нельзя, а после пяти промахов нужен новый апгрейд.
//! * Один активный код на аккаунт (PRIMARY KEY по `account_id`): повторная
//!   привязка e-mail перезаписывает код, старый перестаёт работать.

use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

/// Срок жизни кода подтверждения.
pub const VERIFICATION_TTL_MINUTES: i64 = 15;

/// Сколько неверных попыток допускается до гашения кода.
pub const VERIFICATION_MAX_ATTEMPTS: i32 = 5;

/// Ошибки подтверждения e-mail.
#[derive(Debug, thiserror::Error)]
pub enum GuestVerifyError {
    /// Ошибка БД.
    #[error("database error")]
    Db(#[from] sqlx::Error),
}

/// Итог проверки кода.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerifyOutcome {
    /// Код верный, адрес подтверждён (или уже был подтверждён).
    Verified,
    /// Кода нет, он истёк или уже погашен перебором.
    CodeNotFound,
    /// Код не совпал; попытка засчитана.
    CodeMismatch {
        /// Сколько попыток осталось до гашения кода.
        attempts_left: i32,
    },
}

/// Хеш кода: `sha256("{account_id}:{code}")` в hex.
///
/// `account_id` в префиксе — чтобы одинаковые коды у разных аккаунтов давали
/// разные хеши (иначе один и тот же `000000` виден в таблице как одна строка
/// и сравнение по хешу становится глобальным).
#[must_use]
pub fn hash_code(account_id: Uuid, code: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(account_id.as_bytes());
    hasher.update(b":");
    hasher.update(code.trim().as_bytes());
    hex::encode(hasher.finalize())
}

/// Репозиторий кодов подтверждения.
#[derive(Clone)]
pub struct GuestVerifyRepo {
    pool: PgPool,
}

impl GuestVerifyRepo {
    /// Создать репозиторий на пуле.
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Пул соединений (для тестов и админ-запросов).
    #[must_use]
    pub fn pool(&self) -> PgPool {
        self.pool.clone()
    }

    /// Выдать новый код аккаунту, заменив предыдущий.
    ///
    /// `code_hash` — результат [`hash_code`]. Возвращает момент истечения,
    /// чтобы вызывающий положил его в письмо (шаблон `guest_verify`).
    pub async fn issue_code(
        &self,
        account_id: Uuid,
        code_hash: &str,
    ) -> Result<DateTime<Utc>, GuestVerifyError> {
        let expires_at = Utc::now() + chrono::Duration::minutes(VERIFICATION_TTL_MINUTES);
        sqlx::query(
            "INSERT INTO guest_email_verification (account_id, code_hash, expires_at, attempts) \
             VALUES ($1, $2, $3, 0) \
             ON CONFLICT (account_id) DO UPDATE \
               SET code_hash = EXCLUDED.code_hash, \
                   expires_at = EXCLUDED.expires_at, \
                   attempts = 0, \
                   created_at = now()",
        )
        .bind(account_id)
        .bind(code_hash)
        .bind(expires_at)
        .execute(&self.pool)
        .await?;
        Ok(expires_at)
    }

    /// Проверить код и, если он верный, пометить адрес подтверждённым.
    ///
    /// Обе операции — в одной транзакции: код не может быть погашен без
    /// установки `verified`, и наоборот.
    pub async fn verify(
        &self,
        account_id: Uuid,
        code: &str,
    ) -> Result<VerifyOutcome, GuestVerifyError> {
        let expected = hash_code(account_id, code);
        let mut tx = self.pool.begin().await?;

        // Код берём с блокировкой строки: два параллельных запроса с верным
        // кодом не должны оба «съесть» попытки.
        let row: Option<(String, i32)> = sqlx::query_as(
            "SELECT code_hash, attempts FROM guest_email_verification \
             WHERE account_id = $1 AND expires_at > now() FOR UPDATE",
        )
        .bind(account_id)
        .fetch_optional(&mut *tx)
        .await?;

        let Some((stored, attempts)) = row else {
            tx.rollback().await?;
            return Ok(VerifyOutcome::CodeNotFound);
        };

        if stored != expected {
            let attempts = attempts + 1;
            if attempts >= VERIFICATION_MAX_ATTEMPTS {
                // Перебор: код гасится, нужен новый.
                sqlx::query("DELETE FROM guest_email_verification WHERE account_id = $1")
                    .bind(account_id)
                    .execute(&mut *tx)
                    .await?;
                tx.commit().await?;
                return Ok(VerifyOutcome::CodeNotFound);
            }
            sqlx::query("UPDATE guest_email_verification SET attempts = $2 WHERE account_id = $1")
                .bind(account_id)
                .bind(attempts)
                .execute(&mut *tx)
                .await?;
            tx.commit().await?;
            return Ok(VerifyOutcome::CodeMismatch {
                attempts_left: VERIFICATION_MAX_ATTEMPTS - attempts,
            });
        }

        // Верный код: подтверждаем адрес и гасим код.
        let updated = sqlx::query("UPDATE guest_account SET verified = TRUE WHERE id = $1")
            .bind(account_id)
            .execute(&mut *tx)
            .await?;
        if updated.rows_affected() == 0 {
            // Аккаунта нет (удалён/псевдонимизирован retention-джобой).
            tx.rollback().await?;
            return Ok(VerifyOutcome::CodeNotFound);
        }
        sqlx::query("DELETE FROM guest_email_verification WHERE account_id = $1")
            .bind(account_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(VerifyOutcome::Verified)
    }

    /// Погасить код без подтверждения (смена e-mail, повторный апгрейд).
    pub async fn clear_code(&self, account_id: Uuid) -> Result<bool, GuestVerifyError> {
        let res = sqlx::query("DELETE FROM guest_email_verification WHERE account_id = $1")
            .bind(account_id)
            .execute(&self.pool)
            .await?;
        Ok(res.rows_affected() > 0)
    }

    /// Удалить просроченные коды (retention-джоба).
    pub async fn sweep_expired(&self) -> Result<u64, GuestVerifyError> {
        let res = sqlx::query("DELETE FROM guest_email_verification WHERE expires_at <= now()")
            .execute(&self.pool)
            .await?;
        Ok(res.rows_affected())
    }

    /// Подтверждён ли адрес аккаунта.
    pub async fn is_verified(&self, account_id: Uuid) -> Result<bool, GuestVerifyError> {
        let row: Option<(bool,)> =
            sqlx::query_as("SELECT verified FROM guest_account WHERE id = $1")
                .bind(account_id)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row.map(|(v,)| v).unwrap_or(false))
    }
}

/// Сгенерировать шестизначный код (100000..=999999).
///
/// Не `rand`-крейт напрямую: код — не секрет уровня токена, важнее
/// равномерность и отсутствие ведущих нулей в письме. Диапазон сдвинут,
/// чтобы код никогда не начинался с нуля (в письме он читается вслух).
#[must_use]
pub fn generate_code() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    format!("{:06}", rng.gen_range(100_000..=999_999))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_hash_is_stable_and_account_scoped() {
        let a = Uuid::from_u128(1);
        let b = Uuid::from_u128(2);
        assert_eq!(hash_code(a, "123456"), hash_code(a, "123456"));
        assert_ne!(hash_code(a, "123456"), hash_code(b, "123456"));
        assert_ne!(hash_code(a, "123456"), hash_code(a, "123457"));
        // trim: пробелы вокруг кода не меняют хеш (гость мог скопировать).
        assert_eq!(hash_code(a, " 123456 "), hash_code(a, "123456"));
    }

    #[test]
    fn generated_code_is_six_digits() {
        for _ in 0..100 {
            let code = generate_code();
            assert_eq!(code.len(), 6, "{code}");
            assert!(code.chars().all(|c| c.is_ascii_digit()), "{code}");
            assert!(!code.starts_with('0'), "{code}");
        }
    }

    /// Константы проверяются на этапе компиляции: ноль в TTL или в пределе
    /// попыток сделал бы код неработающим (`expires_at = now()` / мгновенное
    /// гашение), и это должно ломать сборку, а не тест.
    const _: () = {
        assert!(VERIFICATION_TTL_MINUTES > 0);
        assert!(VERIFICATION_MAX_ATTEMPTS > 0);
    };
}
