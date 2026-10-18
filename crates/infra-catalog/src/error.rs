//! Ошибки транспортного слоя каталога.
//!
//! Один перечисление [`CatalogError`] покрывает все классы отказов внешних
//! источников (iTunes, Deezer): сетевые сбои, rate-limit, разбор ответа и
//! конфигурацию запроса. В публичном API крейта `anyhow` не используется —
//! только `thiserror`.

use thiserror::Error;

/// Ошибка внешнего источника каталога треков.
///
/// Служит единым возвращаемым типом для [`CatalogSearch`](crate::CatalogSearch)
/// и всех парсеров. Реализует [`Clone`] и [`PartialEq`], чтобы детерминированно
/// проверяться в тестах без сравнения нечётких сетевых сообщений.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CatalogError {
    /// Источник недоступен: 5xx upstream, обрыв соединения, нечитаемый ответ
    /// транспорта. Содержит человекочитаемую детализацию.
    #[error("catalog source unavailable: {0}")]
    Unavailable(String),

    /// Пустой поисковый запрос (`q` состоит только из пробелов).
    #[error("catalog query is empty")]
    EmptyQuery,

    /// Источник отклонил запрос по лимиту (HTTP 429 или эквивалент API).
    #[error("catalog source rate-limited the request")]
    RateLimited,

    /// Ответ не удалось разобрать как JSON или структура не соответствует
    /// ожидаемой схеме источника. Содержит детализацию.
    #[error("catalog response decode error: {0}")]
    Decode(String),

    /// Превышен таймаут HTTP-запроса или источник вернул 408.
    #[error("catalog request timed out")]
    Timeout,

    /// Ошибка конфигурации запроса: неподдерживаемый источник, нечисловой
    /// идентификатор для источника, требующего чисел, и т. п.
    #[error("catalog config error: {0}")]
    Config(String),
}
