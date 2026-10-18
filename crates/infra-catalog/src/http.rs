//! Транспорт HTTP и классификация сетевых ошибок.
//!
//! Внутренний трейт [`Transport`] изолирует reqwest от оркестрации поиска,
//! чтобы [`HttpCatalog`](crate::HttpCatalog) можно было тестировать на
//! фейковом транспорте без сети. Функция [`is_transient`] определяет, какие
//! ошибки [`CatalogError`] дают право на резервный источник (iTunes → Deezer),
//! а [`check_status`] (pub(crate)) сопоставляет HTTP-статус с ошибкой.

use crate::error::CatalogError;

/// Простой HTTP-ответ: статус и тело.
#[derive(Debug, Clone)]
pub(crate) struct Response {
    /// HTTP-статус.
    pub status: u16,
    /// Тело ответа как текст.
    pub body: String,
}

/// Транспорт: асинхронный GET, возвращающий [`Response`] или ошибку.
#[async_trait::async_trait]
pub(crate) trait Transport: Send + Sync {
    /// Выполняет GET-запрос и возвращает статус + тело.
    async fn get(&self, url: &str) -> Result<Response, CatalogError>;
}

/// Реальный транспорт на reqwest.
pub(crate) struct ReqwestTransport {
    /// HTTP-клиент с настроенным таймаутом.
    pub client: reqwest::Client,
}

#[async_trait::async_trait]
impl Transport for ReqwestTransport {
    async fn get(&self, url: &str) -> Result<Response, CatalogError> {
        let resp = self
            .client
            .get(url)
            .send()
            .await
            .map_err(map_reqwest_error)?;
        let status = resp.status().as_u16();
        let body = resp.text().await.map_err(|e| {
            if e.is_timeout() {
                CatalogError::Timeout
            } else {
                CatalogError::Decode(format!("body read: {e}"))
            }
        })?;
        Ok(Response { status, body })
    }
}

/// Транспорт, который всегда падает с [`CatalogError::Config`] — используется,
/// когда построить reqwest-клиент не удалось.
pub(crate) struct FailedTransport {
    /// Причина отказа построения клиента.
    pub reason: String,
}

#[async_trait::async_trait]
impl Transport for FailedTransport {
    async fn get(&self, _url: &str) -> Result<Response, CatalogError> {
        Err(CatalogError::Config(self.reason.clone()))
    }
}

/// Сопоставляет HTTP-статус с ошибкой каталога.
///
/// `2xx` → `Ok`; `429` → `RateLimited`; `408` → `Timeout`;
/// `5xx` → `Unavailable`; прочее (включая 404 на search) → `Unavailable`.
/// На уровне `lookup` статус 404 обрабатывается отдельно как «не найдено».
pub(crate) fn check_status(source: &str, status: u16) -> Result<(), CatalogError> {
    match status {
        200..=299 => Ok(()),
        429 => Err(CatalogError::RateLimited),
        408 => Err(CatalogError::Timeout),
        500..=599 => Err(CatalogError::Unavailable(format!(
            "{source} upstream {status}"
        ))),
        other => Err(CatalogError::Unavailable(format!(
            "{source} unexpected status {other}"
        ))),
    }
}

/// Признак транзиентности ошибки: даёт право на резервный источник.
///
/// Транзиентны: `RateLimited`, `Timeout`, `Unavailable`. Фатальны (не
/// маскируются резервом): `Decode`, `EmptyQuery`, `Config`.
pub fn is_transient(err: &CatalogError) -> bool {
    matches!(
        err,
        CatalogError::RateLimited | CatalogError::Timeout | CatalogError::Unavailable(_)
    )
}

/// Классифицирует ошибку reqwest в [`CatalogError`].
fn map_reqwest_error(err: reqwest::Error) -> CatalogError {
    if err.is_timeout() {
        CatalogError::Timeout
    } else if err.is_connect() {
        CatalogError::Unavailable(format!("connection error: {err}"))
    } else if err.is_decode() {
        CatalogError::Decode(format!("response decode: {err}"))
    } else {
        CatalogError::Unavailable(format!("http error: {err}"))
    }
}
