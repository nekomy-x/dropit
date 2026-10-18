//! NATS/JetStream-адаптеры DROP IT: публикация outbox-событий и событийная шина.
//!
//! Реализует ADR-002 (transactional outbox + NATS JetStream):
//!
//! * [`OutboxStore`] — чтение/пометка строк таблицы `outbox`
//!   (поля `id, aggregate_id, event_type, payload, published_at, attempts`);
//! * [`MessageBus`] — публикация в NATS JetStream ([`NatsBus`]) или no-op
//!   ([`NoopBus`], дефолт для тестов/локала, пока NATS не поднят);
//! * [`OutboxRelay`] — цикл `claim → publish → mark`: захватывает батч,
//!   публикует успешные и помечает их, упавшие отправляет в `mark_failed`,
//!   исчерпавшие попытки — в `skipped`.
//!
//! До F5 NATS не поднимается (ADR-002); шина подключается только при наличии
//! `NATS_URL` (см. [`from_env`]), иначе используется [`NoopBus`].

#![deny(missing_docs)]

mod bus;
mod env;
mod error;
mod outbox;
mod relay;

pub use bus::{MessageBus, NatsBus, NoopBus};
pub use env::from_env;
pub use error::NatsError;
pub use outbox::{subject_for, InMemoryOutboxStore, OutboxRow, OutboxStore};
pub use relay::{OutboxRelay, RelayReport};
