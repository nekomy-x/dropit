//! Сборка инфраструктурных адаптеров воркера из переменных окружения
//! с деградацией (in-memory/noop), аналогично `gateway-api/src/infra.rs`.
//!
//! Правило: отсутствие сервиса не валит воркер. БД-задачи без `DATABASE_URL`
//! пропускаются (статус `skipped`); NATS без `NATS_URL` → [`NoopBus`];
//! push без ключей → [`NoopPush`].

use std::sync::Arc;

use dropit_infra_db::outbox::PgOutboxStore;
use dropit_infra_db::{JobsRepo, PayoutRepo};
use dropit_infra_nats::{MessageBus, NatsBus, NoopBus};
use dropit_infra_push::{NoopPush, PushSender, RoutingPush};

use crate::config::{database_url, nats_url, redis_url, Config};

/// Имя JetStream-стрима для outbox-событий (`dropit.outbox.>`).
const NATS_STREAM: &str = "DROPIT";

/// Собранный набор адаптеров для задач воркера.
pub struct WorkerEnv {
    /// Конфигурация интервалов/параметров.
    pub config: Config,
    /// NATS реально подключён (не NoopBus).
    pub nats_connected: bool,
    /// Шина сообщений (NatsBus или NoopBus).
    pub bus: Arc<dyn MessageBus>,
    /// Outbox-стор (None — без БД).
    pub outbox_store: Option<Arc<PgOutboxStore>>,
    /// Push-отправитель.
    pub push_sender: Arc<dyn PushSender>,
    /// Репозиторий джоб (автозакрытие, retention, push-токены).
    pub jobs: Option<JobsRepo>,
    /// Репозиторий выплат/сверки.
    pub payouts: Option<PayoutRepo>,
}

impl WorkerEnv {
    /// Собрать окружение из переменных с деградацией.
    pub async fn build(config: Config) -> WorkerEnv {
        let pool = build_pool(&config).await;
        let (bus, nats_connected) = build_bus().await;
        let outbox_store = pool
            .as_ref()
            .map(|p| Arc::new(PgOutboxStore::with_max_attempts(p.clone(), config.outbox_max_attempts)));
        let push_sender = build_push();
        let jobs = pool.as_ref().map(|p| JobsRepo::new(p.clone()));
        let payouts = pool.as_ref().map(|p| PayoutRepo::new(p.clone()));

        if let Some(r) = redis_url() {
            tracing::info!(redis_url = %r, "redis url detected (workers use it indirectly via shared infra)");
        } else {
            tracing::info!("redis: not configured (REDIS_URL not set)");
        }

        WorkerEnv {
            config,
            nats_connected,
            bus,
            outbox_store,
            push_sender,
            jobs,
            payouts,
        }
    }
}

/// Подключиться к Postgres (опционально — с прогоном миграций).
async fn build_pool(config: &Config) -> Option<sqlx::PgPool> {
    let url = database_url()?;
    match dropit_infra_db::connect(&url).await {
        Ok(pool) => {
            if config.run_migrations {
                match dropit_infra_db::migrations::run(&pool).await {
                    Ok(_) => tracing::info!("database: migrations applied"),
                    Err(e) => tracing::warn!(error = %e, "database: migrations failed"),
                }
            }
            tracing::info!("database: connected");
            Some(pool)
        }
        Err(e) => {
            tracing::warn!(error = %e, "database: unavailable, DB jobs will be skipped");
            None
        }
    }
}

/// Собрать шину: NatsBus при `NATS_URL`, иначе NoopBus.
async fn build_bus() -> (Arc<dyn MessageBus>, bool) {
    let Some(url) = nats_url() else {
        tracing::warn!("nats: NATS_URL не задан — используется NoopBus, outbox-реле будет пропущен");
        return (Arc::new(NoopBus), false);
    };
    match NatsBus::connect(&url, NATS_STREAM).await {
        Ok(bus) => {
            tracing::info!("nats: подключён (stream {NATS_STREAM})");
            (Arc::new(bus), true)
        }
        Err(e) => {
            tracing::warn!(error = %e, "nats: недоступен, используется NoopBus; outbox-реле пропущен, push продолжит работу");
            (Arc::new(NoopBus), false)
        }
    }
}

/// Собрать push-отправитель: RoutingPush при наличии ключей, иначе NoopPush.
fn build_push() -> Arc<dyn PushSender> {
    match dropit_infra_push::from_env() {
        Some(cfg) => match RoutingPush::from_config(&cfg) {
            Ok(Some(sender)) => {
                tracing::info!("push: routing (fcm/apns)");
                Arc::new(sender)
            }
            Ok(None) => {
                tracing::warn!("push: ни один транспорт не настроен — NoopPush");
                Arc::new(NoopPush)
            }
            Err(e) => {
                tracing::warn!(error = %e, "push: конфигурация невалидна — NoopPush");
                Arc::new(NoopPush)
            }
        },
        None => {
            tracing::info!("push: NoopPush (ключи не заданы)");
            Arc::new(NoopPush)
        }
    }
}
