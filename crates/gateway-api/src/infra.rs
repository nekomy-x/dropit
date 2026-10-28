//! Сборка инфраструктурных адаптеров из переменных окружения.
//!
//! Правило деградации: если сервис не настроен (нет переменной) или недоступен,
//! поднимается локальный in-memory/noop-аналог и в лог пишется предупреждение.
//! Прод-путь включается только явной конфигурацией — приложение остаётся
//! работоспособным без Redis, NATS и Stripe. Push-доставка вынесена в
//! воркер (`dropit-workers`) и здесь не собирается.

use std::sync::Arc;
use std::time::Duration;

use dropit_infra_catalog::CatalogSearch;
use dropit_infra_redis::{EventBus, TicketStore};
use dropit_infra_stripe::StripeApi;

/// Хранилище одноразовых WS-тикетов: Redis, если задан `REDIS_URL`, иначе in-memory.
pub(crate) async fn ticket_store() -> Arc<dyn TicketStore> {
    match dropit_infra_redis::from_env() {
        Some(config) => match dropit_infra_redis::RedisTicketStore::connect_with_prefix(
            &config.url,
            &config.prefix,
        )
        .await
        {
            Ok(store) => {
                tracing::info!("ws tickets: redis");
                return Arc::new(store);
            }
            Err(e) => {
                tracing::warn!(error = %e, "ws tickets: redis unavailable, falling back to in-memory");
            }
        },
        None => tracing::info!("ws tickets: in-memory (REDIS_URL не задан)"),
    }
    Arc::new(dropit_infra_redis::InMemoryTicketStore::new())
}

/// Шина live-событий между узлами: Redis Pub/Sub или in-memory.
pub(crate) async fn event_bus() -> Arc<dyn EventBus> {
    match dropit_infra_redis::from_env() {
        Some(config) => match dropit_infra_redis::RedisEventBus::connect_with_prefix(
            &config.url,
            &config.prefix,
        )
        .await
        {
            Ok(bus) => {
                tracing::info!("event bus: redis");
                return Arc::new(bus);
            }
            Err(e) => {
                tracing::warn!(error = %e, "event bus: redis unavailable, falling back to in-memory");
            }
        },
        None => tracing::info!("event bus: in-memory (REDIS_URL не задан)"),
    }
    Arc::new(dropit_infra_redis::InMemoryEventBus::new())
}

/// Платёжный провайдер: `LiveStripe` при наличии ключей, иначе `MockStripe`.
pub(crate) fn stripe() -> Arc<dyn StripeApi> {
    if let Some((api_key, webhook_secret)) = dropit_infra_stripe::from_env() {
        match dropit_infra_stripe::LiveStripe::new(api_key, webhook_secret, None) {
            Ok(live) => {
                tracing::info!("stripe: live");
                return Arc::new(live);
            }
            Err(e) => tracing::warn!(error = %e, "stripe: live недоступен, используем mock"),
        }
    } else {
        tracing::info!("stripe: mock (STRIPE_API_KEY не задан)");
    }
    Arc::new(dropit_infra_stripe::MockStripe)
}

/// Каталог треков: внешний HTTP-каталог (iTunes/Deezer).
pub(crate) fn catalog() -> Arc<dyn CatalogSearch> {
    Arc::new(dropit_infra_catalog::HttpCatalog::new(Duration::from_secs(
        8,
    )))
}

/// Адрес отправителя писем (`MAIL_FROM`).
///
/// Формат — `Name <a@b>` или просто `a@b`. Переменная читается на каждом
/// письме, а не один раз при старте: админ может поменять отправителя
/// перезапуском воркера, не трогая код, а значение всё равно не секрет.
/// Пусто → дефолт `DROP IT <no-reply@dropit.local>`: письмо в очереди должно
/// быть валидным, даже если окружение не настроено (иначе строка не создастся
/// и след потеряется).
#[must_use]
pub(crate) fn mail_from() -> String {
    std::env::var("MAIL_FROM")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| DEFAULT_MAIL_FROM.to_string())
}

/// Дефолтный отправитель, если `MAIL_FROM` не задан.
pub(crate) const DEFAULT_MAIL_FROM: &str = "DROP IT <no-reply@dropit.local>";

/// Адрес для ответа (`MAIL_REPLY_TO`), если задан.
#[must_use]
pub(crate) fn mail_reply_to() -> Option<String> {
    std::env::var("MAIL_REPLY_TO")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// База для ссылок в письмах (`MAIL_APP_BASE_URL`), без завершающего `/`.
///
/// Ссылки собирает вызывающий слой и передаёт в переменных шаблона, поэтому
/// рендерер не читает окружение и остаётся чистой функцией.
#[must_use]
pub(crate) fn mail_app_base_url() -> String {
    std::env::var("MAIL_APP_BASE_URL")
        .ok()
        .map(|v| v.trim().trim_end_matches('/').to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "http://localhost:5173".to_string())
}

/// Дефолтный лимит открытой регистрации: 10 регистраций с одного IP в час.
pub(crate) const DEFAULT_SIGNUP_RATE_PER_HOUR: i64 = 10;

/// Открытая регистрация (`DROPIT_OPEN_SIGNUP`) — Test Mode stand.
///
/// Включающие значения (без учёта регистра, после `trim`): `1`, `true`,
/// `yes`, `on`. Всё остальное — включая отсутствие переменной, `0`,
/// `false` и мусор — ВЫКЛЮЧАЕТ регистрацию: безопасный дефолт (в проде
/// открытая регистрация включается только осознанно).
///
/// Флаг читается один раз при старте: включение/выключение — перезапуск
/// процесса, поведение на живом процессе не «мигает».
pub(crate) fn open_signup() -> bool {
    let enabled = match std::env::var("DROPIT_OPEN_SIGNUP") {
        Ok(v) => {
            let s = v.trim().to_ascii_lowercase();
            matches!(s.as_str(), "1" | "true" | "yes" | "on")
        }
        Err(_) => false,
    };
    if enabled {
        tracing::info!("open signup: ENABLED (DROPIT_OPEN_SIGNUP) — Test Mode");
    } else {
        tracing::info!("open signup: disabled (DROPIT_OPEN_SIGNUP не включён)");
    }
    enabled
}

/// Лимит открытой регистрации: регистраций с одного IP в час
/// (`DROPIT_SIGNUP_RATE_PER_HOUR`, дефолт [`DEFAULT_SIGNUP_RATE_PER_HOUR`]).
///
/// Зачем настраивается: демо-сценарий — «регистрация в один клик», и
/// несколько посетителей за одним NAT/офисным IP выжигают жёсткий бюджет
/// 10/час за минуты и получают `429` на стенде. На Test Mode стенде
/// выставляется 100; дефолт 10 остаётся для обычного режима.
///
/// Некорректное/отрицательное/нулевое значение → дефолт (лимит нельзя
/// случайно отключить пустой переменной: 0 трактовался бы как «всё
/// заблокировано», а не «без лимита»).
pub(crate) fn signup_rate_per_hour() -> i64 {
    let parsed = std::env::var("DROPIT_SIGNUP_RATE_PER_HOUR")
        .ok()
        .and_then(|v| v.trim().parse::<i64>().ok())
        .filter(|n| *n > 0);
    match parsed {
        Some(n) => {
            tracing::info!(
                limit = n,
                "open signup: rate limit per hour (DROPIT_SIGNUP_RATE_PER_HOUR)"
            );
            n
        }
        None => DEFAULT_SIGNUP_RATE_PER_HOUR,
    }
}

#[cfg(test)]
mod tests {
    use super::open_signup;

    /// Флаг читается из env; тест выставляет и снимает переменную сам.
    /// `serial` не нужен: значение читается сразу после установки в том же
    /// потоке, а другие тесты к `DROPIT_OPEN_SIGNUP` не обращаются.
    #[test]
    fn open_signup_flag_values() {
        let prev = std::env::var("DROPIT_OPEN_SIGNUP").ok();

        for v in ["1", "true", "TRUE", " yes ", "on"] {
            std::env::set_var("DROPIT_OPEN_SIGNUP", v);
            assert!(open_signup(), "{v:?} должен включать открытую регистрацию");
        }
        for v in ["0", "false", "no", "off", "", "garbage"] {
            std::env::set_var("DROPIT_OPEN_SIGNUP", v);
            assert!(!open_signup(), "{v:?} должен выключать регистрацию");
        }
        std::env::remove_var("DROPIT_OPEN_SIGNUP");
        assert!(!open_signup(), "без переменной регистрация выключена");

        match prev {
            Some(v) => std::env::set_var("DROPIT_OPEN_SIGNUP", v),
            None => std::env::remove_var("DROPIT_OPEN_SIGNUP"),
        }
    }

    /// `DROPIT_SIGNUP_RATE_PER_HOUR`: валидное значение применяется,
    /// пустое/мусорное/нулевое/отрицательное — дефолт 10.
    #[test]
    fn signup_rate_per_hour_values() {
        let prev = std::env::var("DROPIT_SIGNUP_RATE_PER_HOUR").ok();

        std::env::set_var("DROPIT_SIGNUP_RATE_PER_HOUR", "100");
        assert_eq!(super::signup_rate_per_hour(), 100);
        std::env::set_var("DROPIT_SIGNUP_RATE_PER_HOUR", " 7 ");
        assert_eq!(super::signup_rate_per_hour(), 7);
        for bad in ["", "0", "-5", "abc", "10.5"] {
            std::env::set_var("DROPIT_SIGNUP_RATE_PER_HOUR", bad);
            assert_eq!(
                super::signup_rate_per_hour(),
                super::DEFAULT_SIGNUP_RATE_PER_HOUR,
                "{bad:?} должен давать дефолт"
            );
        }
        std::env::remove_var("DROPIT_SIGNUP_RATE_PER_HOUR");
        assert_eq!(
            super::signup_rate_per_hour(),
            super::DEFAULT_SIGNUP_RATE_PER_HOUR
        );

        match prev {
            Some(v) => std::env::set_var("DROPIT_SIGNUP_RATE_PER_HOUR", v),
            None => std::env::remove_var("DROPIT_SIGNUP_RATE_PER_HOUR"),
        }
    }
}
