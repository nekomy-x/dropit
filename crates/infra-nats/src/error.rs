//! Ошибки шины и outbox-реля.
//!
//! Единый enum [`NatsError`] покрывает и хранилище outbox, и транспорт NATS,
//! чтобы реле могло единообразно решать, повторять ли попытку. В публичном API
//! `anyhow` не используется — только `thiserror`.

use thiserror::Error;

/// Ошибка публикации/обработки outbox-событий.
///
/// Варианты разделены по тому, как реле должно реагировать:
/// [`NatsError::Store`] и [`NatsError::Poisoned`] — не повторяемые в рамках
/// того же цикла (проблема данных/БД); [`NatsError::Transport`] и
/// [`NatsError::Timeout`] — повторяемые (сеть/NATS); [`NatsError::Unavailable`]
/// — соединение/стрим отсутствуют, реле должен дождаться поднятия инфраструктуры.
#[derive(Debug, Error)]
pub enum NatsError {
    /// NATS/JetStream недоступен: нет соединения или стрим отсутствует.
    #[error("nats unavailable: {0}")]
    Unavailable(String),
    /// Транспортная ошибка публикации: сбой отправки, битый ack и т. п.
    #[error("nats transport error: {0}")]
    Transport(String),
    /// Ошибка хранилища outbox (БД): claim/mark_published/mark_failed.
    #[error("outbox store error: {0}")]
    Store(String),
    /// Превышен таймаут операции (connect, ack).
    #[error("operation timed out")]
    Timeout,
    /// «Отравленное» сообщение: не подлежит повторной попытке
    /// (битый payload, невалидный конфиг, нечитаемая строка).
    /// Реле не должно бесконечно крутить такую строку.
    #[error("poisoned: {0}")]
    Poisoned(String),
}
