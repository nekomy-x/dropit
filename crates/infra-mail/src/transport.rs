//! Внутренний HTTP-транспорт.
//!
//! Тонкая абстракция над [`reqwest`], позволяющая подменять транспорт в
//! unit-тестах фиктивной реализацией без сетевых вызовов. Публичное API
//! крейта всегда использует [`ReqwestTransport`]; тесты внедряют фейк
//! через конструктор `ResendSender::with_transport` (виден только внутри
//! крейта). Форма скопирована с `dropit-infra-push::transport`, чтобы
//! отправители в проекте были устроены одинаково.
//!
//! Отличие от push: [`HttpResponse`] несёт заголовки. Без них нельзя
//! прочитать `Retry-After` у 429, а контракт требует вернуть подсказку
//! провайдера воркеру.

use async_trait::async_trait;
use std::sync::Arc;
use std::time::Duration;

use crate::MailError;

/// Таймаут одиночного запроса к Resend по умолчанию (секунды).
///
/// Письмо — не интерактивная операция: 15 с хватает, чтобы отличить
/// медленный ответ от мёртвого соединения, и не держит батч воркера
/// дольше разумного.
pub(crate) const DEFAULT_REQUEST_TIMEOUT_SECS: u64 = 15;

/// Одиночный HTTP-запрос к транспорту.
#[derive(Debug, Clone)]
pub(crate) struct HttpRequest {
    /// Целевой URL.
    pub url: String,
    /// Заголовки парами «имя-значение».
    pub headers: Vec<(String, String)>,
    /// Тело запроса (байты).
    pub body: Vec<u8>,
}

/// Ответ транспорта.
#[derive(Debug, Clone)]
pub(crate) struct HttpResponse {
    /// HTTP-статус.
    pub status: u16,
    /// Заголовки ответа парами «имя-значение» (нужны для `Retry-After`).
    pub headers: Vec<(String, String)>,
    /// Тело ответа (байты).
    pub body: Vec<u8>,
}

/// Асинхронный HTTP-транспорт, абстрагирующий `reqwest`.
#[async_trait]
pub(crate) trait HttpTransport: Send + Sync {
    /// Выполнить `req` и вернуть ответ либо ошибку транспорта.
    async fn execute(&self, req: HttpRequest) -> Result<HttpResponse, MailError>;
}

/// Реальная реализация поверх `reqwest` с rustls TLS.
pub(crate) struct ReqwestTransport {
    client: reqwest::Client,
}

impl ReqwestTransport {
    /// Создать клиент с заданным таймаутом одиночного запроса.
    ///
    /// `User-Agent` намеренно не задаётся на клиенте: контракт требует
    /// явный заголовок `User-Agent: dropit-infra-mail` в каждом запросе,
    /// а два значения в одном запросе сбивают провайдера с толку.
    pub(crate) fn new(timeout: Duration) -> Result<Self, MailError> {
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .map_err(|e| MailError::Unavailable(format!("http client init: {e}")))?;
        Ok(Self { client })
    }
}

#[async_trait]
impl HttpTransport for ReqwestTransport {
    async fn execute(&self, req: HttpRequest) -> Result<HttpResponse, MailError> {
        let mut builder = self.client.post(&req.url);
        for (name, value) in &req.headers {
            builder = builder.header(name.as_str(), value.as_str());
        }
        let response = builder
            .body(req.body)
            .send()
            .await
            .map_err(map_reqwest_error)?;
        let status = response.status().as_u16();
        let headers = response
            .headers()
            .iter()
            .map(|(name, value)| {
                (
                    name.as_str().to_string(),
                    value.to_str().unwrap_or_default().to_string(),
                )
            })
            .collect();
        let bytes = response
            .bytes()
            .await
            .map_err(|e| MailError::Decode(format!("read response body: {e}")))?;
        Ok(HttpResponse {
            status,
            headers,
            body: bytes.to_vec(),
        })
    }
}

/// Привести ошибку `reqwest` к [`MailError`].
fn map_reqwest_error(err: reqwest::Error) -> MailError {
    if err.is_timeout() {
        MailError::Timeout
    } else if err.is_connect() || err.is_request() {
        MailError::Unavailable(err.to_string())
    } else if err.is_decode() {
        MailError::Decode(err.to_string())
    } else {
        MailError::Unavailable(err.to_string())
    }
}

/// Сократить тело ответа для безопасного включения в сообщение ошибки.
///
/// Гарантирует, что в ошибку не попадёт длинный дамп ответа провайдера
/// (там бывает эхо запроса, включая адреса получателей). Обрезка идёт по
/// границе символа: тела ошибок бывают с кириллицей, а резать `String`
/// по произвольному байту — паника.
pub(crate) fn sanitize_body(prefix: &str, body: &[u8]) -> String {
    /// Сколько символов тела максимум оставляем в ошибке.
    const MAX_CHARS: usize = 160;

    let text = String::from_utf8_lossy(body);
    let trimmed = text.trim();
    let snippet: String = if trimmed.chars().count() > MAX_CHARS {
        let mut cut: String = trimmed.chars().take(MAX_CHARS).collect();
        cut.push('…');
        cut
    } else {
        trimmed.to_string()
    };
    format!("{prefix}: {snippet}")
}

/// Тип-псевдоним для разделяемого транспорта.
pub(crate) type SharedTransport = Arc<dyn HttpTransport>;
