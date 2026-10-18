//! Push-уведомления: FCM (Android) и APNs (iOS) за единым трейтом [`PushSender`].
//!
//! Крейт реализует транспорт push-уведомлений для проекта DROP IT:
//! запрос трека принят, трек играет, отклонён. Секреты (токены, JWT, приватные
//! ключи) не логируются и не попадают в сообщения ошибок.
//!
//! # Типы
//! * Модели: [`Platform`], [`PushToken`], [`PushMessage`], [`PushOutcome`],
//!   [`PushReport`].
//! * Транспорт-трейт: [`PushSender`].
//! * Реализации: [`NoopPush`], [`FcmSender`], [`ApnsSender`], [`RoutingPush`].
//! * Конфигурация: [`PushConfig`], [`from_env`].
//!
//! # Поведение
//! * Ошибка одного токена не валит весь вызов — она попадает в [`PushOutcome`].
//! * Транспортная ошибка (401/503) прерывает вызов как [`PushError`].
//! * Невалидные токены (`UNREGISTERED`/`BadDeviceToken`) помечаются
//!   `invalid_token = true` и попадают в [`PushReport::invalid_tokens`].

#![deny(missing_docs)]

mod apns;
mod error;
mod fcm;
mod model;
mod routing;
mod sender;
mod transport;

#[cfg(test)]
mod tests;

pub use apns::ApnsSender;
pub use error::PushError;
pub use fcm::FcmSender;
pub use model::{Platform, PushMessage, PushOutcome, PushReport, PushToken};
pub use routing::{from_env, ApnsConfig, FcmConfig, PushConfig, RoutingPush};
pub use sender::{NoopPush, PushSender};
