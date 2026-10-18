//! Трейт отправителя [`PushSender`] и no-op реализация [`NoopPush`].

use crate::model::{PushMessage, PushOutcome, PushReport, PushToken};
use crate::PushError;

/// Асинхронный отправитель push-уведомлений.
///
/// Реализации: [`NoopPush`](crate::NoopPush), [`FcmSender`](crate::FcmSender),
/// [`ApnsSender`](crate::ApnsSender), [`RoutingPush`](crate::RoutingPush).
///
/// Контракт отправки:
/// * ошибка одного токена не валит весь вызов — она попадает в [`PushOutcome`];
/// * транспортная ошибка (401/503) прерывает весь вызов как [`PushError`];
/// * невалидные токены помечаются `invalid_token = true` и попадают в
///   [`PushReport::invalid_tokens`].
#[async_trait::async_trait]
pub trait PushSender: Send + Sync {
    /// Отправить `message` всем `tokens` и вернуть сводный отчёт.
    async fn send(
        &self,
        tokens: &[PushToken],
        message: PushMessage,
    ) -> Result<PushReport, PushError>;
}

/// Заглушка-отправитель: ничего не отправляет в сеть, возвращает успешный
/// отчёт по каждому токену.
///
/// Применяется в тестах и как безопасный деградированный транспорт, когда
/// push не настроен через переменные окружения.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopPush;

#[async_trait::async_trait]
impl PushSender for NoopPush {
    async fn send(
        &self,
        tokens: &[PushToken],
        _message: PushMessage,
    ) -> Result<PushReport, PushError> {
        let outcomes: Vec<PushOutcome> = tokens
            .iter()
            .map(|t| PushOutcome::success(&t.token))
            .collect();
        let sent = outcomes.len();
        Ok(PushReport {
            sent,
            failed: 0,
            invalid_tokens: Vec::new(),
            outcomes,
        })
    }
}
