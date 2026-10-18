//! Внутренний HTTP-транспорт.
//!
//! Тонкая абстракция над [`reqwest`], позволяющая подменять транспорт в
//! unit-тестах фиктивной реализацией без сетевых вызовов. Всё публичное API
//! крейта использует [`ReqwestTransport`]; тесты внедряют фейк через
//! конструкторы `with_transport` (видны только внутри крейта).

use async_trait::async_trait;
use std::sync::Arc;
use std::time::Duration;

use crate::PushError;

/// Одиночный HTTP-запрос к транспорту.
#[derive(Debug, Clone)]
pub(crate) struct HttpRequest {
    /// Целевой URL.
    pub url: String,
    /// Заголовки парами «имя-значение».
    pub headers: Vec<(String, String)>,
    /// Тело запроса (байты).
    pub body: Vec<u8>,
    /// Требовать HTTP/2 (для APNs).
    pub http2: bool,
}

/// Ответ транспорта.
#[derive(Debug, Clone)]
pub(crate) struct HttpResponse {
    /// HTTP-статус.
    pub status: u16,
    /// Тело ответа (байты).
    pub body: Vec<u8>,
}

/// Асинхронный HTTP-транспорт, абстрагирующий `reqwest`.
#[async_trait]
pub(crate) trait HttpTransport: Send + Sync {
    /// Выполнить `req` и вернуть ответ либо ошибку транспорта.
    async fn execute(&self, req: HttpRequest) -> Result<HttpResponse, PushError>;
}

/// Реальная реализация поверх `reqwest` с rustls TLS.
pub(crate) struct ReqwestTransport {
    client: reqwest::Client,
}

impl ReqwestTransport {
    /// Создать клиент с заданным таймаутом одиночного запроса.
    pub(crate) fn new(timeout: Duration) -> Result<Self, PushError> {
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .user_agent("dropit-infra-push")
            .build()
            .map_err(|e| PushError::Unavailable(format!("http client init: {e}")))?;
        Ok(Self { client })
    }
}

#[async_trait]
impl HttpTransport for ReqwestTransport {
    async fn execute(&self, req: HttpRequest) -> Result<HttpResponse, PushError> {
        let mut builder = self.client.post(&req.url);
        if req.http2 {
            builder = builder.version(reqwest::Version::HTTP_2);
        }
        for (name, value) in &req.headers {
            builder = builder.header(name.as_str(), value.as_str());
        }
        let response = builder
            .body(req.body)
            .send()
            .await
            .map_err(map_reqwest_error)?;
        let status = response.status().as_u16();
        let bytes = response
            .bytes()
            .await
            .map_err(|e| PushError::Decode(format!("read response body: {e}")))?;
        Ok(HttpResponse {
            status,
            body: bytes.to_vec(),
        })
    }
}

/// Привести ошибку `reqwest` к [`PushError`].
fn map_reqwest_error(err: reqwest::Error) -> PushError {
    if err.is_timeout() {
        PushError::Timeout
    } else if err.is_connect() || err.is_request() {
        PushError::Unavailable(err.to_string())
    } else if err.is_decode() {
        PushError::Decode(err.to_string())
    } else {
        PushError::Unavailable(err.to_string())
    }
}

/// Сократить тело ответа для безопасного включения в сообщение ошибки.
///
/// Гарантирует, что в ошибку не попадёт длинный дамп и потенциально секретные
/// данные — только краткий префикс ответа сервера.
pub(crate) fn sanitize_body(prefix: &str, body: &[u8]) -> String {
    let text = String::from_utf8_lossy(body);
    let trimmed = text.trim();
    let max = 160;
    let snippet = if trimmed.len() > max {
        &trimmed[..max]
    } else {
        trimmed
    };
    format!("{prefix}: {snippet}")
}

/// Тип-псевдоним для разделяемого транспорта.
pub(crate) type SharedTransport = Arc<dyn HttpTransport>;
