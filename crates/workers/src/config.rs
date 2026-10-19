//! Конфигурация воркера из переменных окружения.
//!
//! Все переменные опциональны кроме `DATABASE_URL` (нужна для БД-задач).
//! При отсутствии БД БД-задачи безопасно пропускаются (статус `skipped`,
//! код 0). Интервалы — секунды; разумные значения по умолчанию подобраны
//! под расписание техкарты (выплаты — еженедельно, сверка — ежедневно,
//! push — часто).

use std::env;
use std::time::Duration;

/// Конфигурация интервалов и параметров супервизора.
#[derive(Debug, Clone)]
pub struct Config {
    /// Интервал outbox-реля (с). По умолчанию 2 — часто, чтобы события
    /// не копились при поднятом NATS.
    pub outbox_interval: Duration,
    /// Интервал цикла выплат (с). По умолчанию 3600 — раз в час
    /// (прод-расписание «пн 10:00» задаётся кроном вне воркера; воркер
    /// выполняет цикл, когда до него доходит очередь).
    pub payout_interval: Duration,
    /// Интервал сверки ledger vs провайдер (с). По умолчанию 3600.
    pub reconcile_interval: Duration,
    /// Интервал GDPR/retention-чистки (с). По умолчанию 86400 — раз в сутки.
    pub retention_interval: Duration,
    /// Интервал автозакрытия сессий (с). По умолчанию 1800 — раз в полчаса
    /// (проверка планового времени 07:00 IANA + неактивности ≥ 2 ч).
    pub close_sessions_interval: Duration,
    /// Интервал диспетчера push (с). По умолчанию 5 — часто, чтобы
    /// уведомления уходили оперативно.
    pub push_interval: Duration,
    /// Размер батча outbox (claim_batch limit). По умолчанию 100.
    pub outbox_batch_size: i64,
    /// Порог попыток outbox (attempts >= max → skip). По умолчанию 5.
    pub outbox_max_attempts: i32,
    /// Размер батча push-диспетчера. По умолчанию 100.
    pub push_batch_size: i64,
    /// Запустить миграции при старте (`DROPIT_RUN_MIGRATIONS=true`).
    pub run_migrations: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            outbox_interval: Duration::from_secs(env_secs("OUTBOX_INTERVAL_SECS", 2)),
            payout_interval: Duration::from_secs(env_secs("PAYOUT_INTERVAL_SECS", 3600)),
            reconcile_interval: Duration::from_secs(env_secs("RECONCILE_INTERVAL_SECS", 3600)),
            retention_interval: Duration::from_secs(env_secs("RETENTION_INTERVAL_SECS", 86400)),
            close_sessions_interval: Duration::from_secs(env_secs(
                "CLOSE_SESSIONS_INTERVAL_SECS",
                1800,
            )),
            push_interval: Duration::from_secs(env_secs("PUSH_INTERVAL_SECS", 5)),
            outbox_batch_size: env_i64("OUTBOX_BATCH_SIZE", 100),
            outbox_max_attempts: env_i32("OUTBOX_MAX_ATTEMPTS", 5),
            push_batch_size: env_i64("PUSH_BATCH_SIZE", 100),
            run_migrations: env_bool("DROPIT_RUN_MIGRATIONS"),
        }
    }
}

impl Config {
    /// Загрузить конфигурацию из окружения (с дефолтами).
    #[must_use]
    pub fn from_env() -> Self {
        Self::default()
    }
}

/// Прочитать `DATABASE_URL` (или дефолт локального compose).
#[must_use]
pub fn database_url() -> Option<String> {
    env::var("DATABASE_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
}

/// Прочитать `NATS_URL`.
#[must_use]
pub fn nats_url() -> Option<String> {
    env::var("NATS_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
}

/// Прочитать `REDIS_URL` (логируется, воркером напрямую не используется).
#[must_use]
pub fn redis_url() -> Option<String> {
    env::var("REDIS_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
}

fn env_secs(key: &str, default: u64) -> u64 {
    match env::var(key).ok().filter(|s| !s.trim().is_empty()) {
        Some(v) => v.trim().parse().unwrap_or(default),
        None => default,
    }
}

fn env_i64(key: &str, default: i64) -> i64 {
    match env::var(key).ok().filter(|s| !s.trim().is_empty()) {
        Some(v) => v.trim().parse().unwrap_or(default),
        None => default,
    }
}

fn env_i32(key: &str, default: i32) -> i32 {
    match env::var(key).ok().filter(|s| !s.trim().is_empty()) {
        Some(v) => v.trim().parse().unwrap_or(default),
        None => default,
    }
}

fn env_bool(key: &str) -> bool {
    matches!(
        env::var(key)
            .ok()
            .map(|s| s.trim().to_ascii_lowercase())
            .as_deref(),
        Some("1" | "true" | "yes" | "on")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_reasonable() {
        let c = Config::from_env();
        assert!(c.outbox_interval.as_secs() >= 1);
        assert!(c.push_interval.as_secs() >= 1);
        assert!(c.outbox_batch_size > 0);
        assert!(c.outbox_max_attempts > 0);
    }

    #[test]
    fn env_secs_parses_or_default() {
        std::env::set_var("DROPIT_TEST_SECS", "42");
        assert_eq!(env_secs("DROPIT_TEST_SECS", 1), 42);
        std::env::set_var("DROPIT_TEST_SECS", "not-a-number");
        assert_eq!(env_secs("DROPIT_TEST_SECS", 7), 7);
        std::env::remove_var("DROPIT_TEST_SECS");
        assert_eq!(env_secs("DROPIT_TEST_SECS", 9), 9);
    }

    #[test]
    fn env_bool_truthy_variants() {
        for v in ["1", "true", "TRUE", "yes", "on"] {
            std::env::set_var("DROPIT_TEST_BOOL", v);
            assert!(env_bool("DROPIT_TEST_BOOL"), "{v} should be true");
        }
        for v in ["0", "false", "no", "off", ""] {
            std::env::set_var("DROPIT_TEST_BOOL", v);
            assert!(!env_bool("DROPIT_TEST_BOOL"), "{v:?} should be false");
        }
        std::env::remove_var("DROPIT_TEST_BOOL");
        assert!(!env_bool("DROPIT_TEST_BOOL"));
    }

    #[test]
    fn intervals_from_env_override() {
        std::env::set_var("OUTBOX_INTERVAL_SECS", "13");
        std::env::set_var("PUSH_INTERVAL_SECS", "17");
        let c = Config::from_env();
        assert_eq!(c.outbox_interval.as_secs(), 13);
        assert_eq!(c.push_interval.as_secs(), 17);
        std::env::remove_var("OUTBOX_INTERVAL_SECS");
        std::env::remove_var("PUSH_INTERVAL_SECS");
    }
}
