//! Конфигурация из окружения.

use std::env;

/// Прочитать `NATS_URL` из окружения.
///
/// Возвращает `Some(url)` только если переменная задана и непуста (после
/// `trim`), иначе `None` — реле/воркер должен тогда использовать [`NoopBus`]
/// или локальный адрес по умолчанию.
#[must_use]
pub fn from_env() -> Option<String> {
    env::var("NATS_URL").ok().filter(|s| !s.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_env_missing() {
        // Сохраняем и убираем, чтобы тест был детерминирован.
        let prev = env::var("NATS_URL").ok();
        env::remove_var("NATS_URL");
        assert_eq!(from_env(), None);
        if let Some(v) = prev {
            env::set_var("NATS_URL", v);
        }
    }

    #[test]
    fn from_env_present() {
        let prev = env::var("NATS_URL").ok();
        env::set_var("NATS_URL", "nats://example:4222");
        assert_eq!(from_env().as_deref(), Some("nats://example:4222"));
        // пустая/whitespace — None
        env::set_var("NATS_URL", "   ");
        assert_eq!(from_env(), None);
        env::remove_var("NATS_URL");
        assert_eq!(from_env(), None);
        if let Some(v) = prev {
            env::set_var("NATS_URL", v);
        }
    }
}
