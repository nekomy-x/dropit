//! Событийная шина: трейт [`MessageBus`] и реализации [`NatsBus`] / [`NoopBus`].

use std::time::Duration;

use async_nats::jetstream::context::{PublishError, PublishErrorKind};
use async_nats::jetstream::stream::Config;
use async_nats::{ConnectError, ConnectErrorKind};
use async_trait::async_trait;
use bytes::Bytes;
use tokio::time::timeout;

use crate::error::NatsError;

/// Таймаут первоначального подключения к NATS.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// Таймаут обращения к JetStream API (создание/проверка стрима).
const STREAM_TIMEOUT: Duration = Duration::from_secs(10);

/// Шина сообщений для публикации outbox-событий.
///
/// Тонкая абстракция над NATS JetStream, чтобы реле [`crate::OutboxRelay`]
/// не зависело от конкретного клиента и тестировалось на фейках. Реализации:
/// [`NatsBus`] (продакшен) и [`NoopBus`] (тесты/локал по умолчанию).
#[async_trait]
pub trait MessageBus: Send + Sync {
    /// Опубликовать `payload` (сырые байты JSON) в `subject`.
    async fn publish(&self, subject: &str, payload: &[u8]) -> Result<(), NatsError>;
}

/// No-op шина: успех без отправки.
///
/// Дефолт для локальной разработки и unit-тестов, где NATS не поднят
/// (ADR-002: до F5 NATS не вводится). Ничего не делает и всегда возвращает
/// `Ok(())`.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopBus;

#[async_trait]
impl MessageBus for NoopBus {
    async fn publish(&self, _subject: &str, _payload: &[u8]) -> Result<(), NatsError> {
        Ok(())
    }
}

/// NATS JetStream-шина.
///
/// При [`NatsBus::connect`] устанавливается соединение с NATS и гарантируется
/// наличие стрима с фильтром по subject-шаблону `dropit.outbox.>` (все события
/// outbox). Публикация ждёт ack сервера (at-least-once на уровне JetStream).
#[derive(Clone)]
pub struct NatsBus {
    jetstream: async_nats::jetstream::Context,
}

impl NatsBus {
    /// Подключиться к NATS по `url` и гарантировать стрим `stream`.
    ///
    /// Стрим создаётся (через get-or-create), если отсутствует, с фильтром
    /// subjects `dropit.outbox.>`. Ошибки соединения →
    /// [`NatsError::Unavailable`] или [`NatsError::Timeout`].
    pub async fn connect(url: &str, stream: &str) -> Result<Self, NatsError> {
        let client = timeout(CONNECT_TIMEOUT, async_nats::connect(url))
            .await
            .map_err(|_| NatsError::Timeout)?
            .map_err(map_connect_error)?;

        let jetstream = async_nats::jetstream::new(client);

        // Гарантируем стрим для событий outbox (dropit.outbox.>).
        timeout(STREAM_TIMEOUT, {
            let js = jetstream.clone();
            async move {
                js.get_or_create_stream(Config {
                    name: stream.to_string(),
                    subjects: vec!["dropit.outbox.>".to_string()],
                    ..Default::default()
                })
                .await
            }
        })
        .await
        .map_err(|_| NatsError::Timeout)?
        .map_err(|err| NatsError::Unavailable(format!("stream {stream}: {err}")))?;

        Ok(Self { jetstream })
    }
}

#[async_trait]
impl MessageBus for NatsBus {
    async fn publish(&self, subject: &str, payload: &[u8]) -> Result<(), NatsError> {
        let bytes = Bytes::copy_from_slice(payload);
        // Шаг 1: отправка сообщения (возвращает ack-future).
        let ack_future = self
            .jetstream
            .publish(subject.to_string(), bytes)
            .await
            .map_err(map_publish_error)?;
        // Шаг 2: ожидание ack от JetStream.
        ack_future.await.map_err(map_publish_error)?;
        Ok(())
    }
}

/// Сопоставление ошибки подключения с [`NatsError`].
fn map_connect_error(err: ConnectError) -> NatsError {
    match err.kind() {
        ConnectErrorKind::TimedOut => NatsError::Timeout,
        other => NatsError::Unavailable(format!("{other}: {err}")),
    }
}

/// Сопоставление ошибки публикации с [`NatsError`].
fn map_publish_error(err: PublishError) -> NatsError {
    match err.kind() {
        PublishErrorKind::TimedOut => NatsError::Timeout,
        PublishErrorKind::StreamNotFound => NatsError::Unavailable(err.to_string()),
        _ => NatsError::Transport(err.to_string()),
    }
}
