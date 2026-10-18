//! Ошибки транспорта push-уведомлений.
//!
//! Все ошибки транспорта выражаются одним перечислением [`PushError`].
//! Секреты (токены, JWT, приватные ключи) в сообщения ошибок не попадают.

use thiserror::Error;

/// Ошибка отправки push-уведомления.
#[derive(Debug, Error)]
pub enum PushError {
    /// Сервис недоступен: сетевая ошибка, 5xx, 429 или исчерпаны повторы.
    #[error("push transport unavailable: {0}")]
    Unavailable(String),
    /// Ошибка аутентификации: 401/403, невалидный JWT или сервисный аккаунт.
    #[error("push transport unauthorized: {0}")]
    Unauthorized(String),
    /// Не удалось декодировать ответ сервиса (битый JSON, отсутствие полей).
    #[error("push transport decode error: {0}")]
    Decode(String),
    /// Ошибка конфигурации: пустые поля, невалидный ключ, нет проекта.
    #[error("push config error: {0}")]
    Config(String),
    /// Превышен таймаут одиночного запроса к транспорту.
    #[error("push request timed out")]
    Timeout,
}
