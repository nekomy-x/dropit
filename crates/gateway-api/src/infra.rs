//! Сборка инфраструктурных адаптеров из переменных окружения.
//!
//! Правило деградации: если сервис не настроен (нет переменной) или недоступен,
//! поднимается локальный in-memory/noop-аналог и в лог пишется предупреждение.
//! Прод-путь включается только явной конфигурацией — приложение остаётся
//! работоспособным без Redis, NATS, Stripe и push-ключей.

use std::sync::Arc;
use std::time::Duration;

use dropit_infra_catalog::CatalogSearch;
use dropit_infra_push::PushSender;
use dropit_infra_redis::{EventBus, TicketStore};
use dropit_infra_stripe::StripeApi;

/// Хранилище одноразовых WS-тикетов: Redis, если задан `REDIS_URL`, иначе in-memory.
pub(crate) async fn ticket_store() -> Arc<dyn TicketStore> {
    match dropit_infra_redis::from_env() {
        Some(config) => match dropit_infra_redis::RedisTicketStore::connect_with_prefix(&config.url, &config.prefix).await
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
        Some(config) => match dropit_infra_redis::RedisEventBus::connect_with_prefix(&config.url, &config.prefix).await
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
    Arc::new(dropit_infra_catalog::HttpCatalog::new(Duration::from_secs(8)))
}

/// Push-транспорт: FCM/APNs при наличии ключей, иначе `NoopPush`.
pub(crate) fn push() -> Arc<dyn PushSender> {
    match dropit_infra_push::from_env() {
        Some(config) => match dropit_infra_push::RoutingPush::from_config(&config) {
            Ok(Some(sender)) => {
                tracing::info!("push: routing (fcm/apns)");
                return Arc::new(sender);
            }
            Ok(None) => tracing::warn!("push: ни один транспорт не настроен"),
            Err(e) => tracing::warn!(error = %e, "push: конфигурация невалидна"),
        },
        None => tracing::info!("push: noop (ключи не заданы)"),
    }
    Arc::new(dropit_infra_push::NoopPush)
}
