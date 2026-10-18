//! Трейт поиска каталога и HTTP-реализация.
//!
//! [`CatalogSearch`] — асинхронный порт поиска/lookup; [`HttpCatalog`] —
//! реализация на iTunes Search + Deezer с автоматическим резервом:
//! iTunes опрашивается первым, при пустом результате или транзиентной ошибке
//! подключается Deezer. Фатальные ошибки (`Decode`/`Config`) не маскируются
//! резервом.

use std::sync::Arc;
use std::time::Duration;

use crate::deezer::{deezer_search_url, deezer_track_url, parse_deezer_search, parse_deezer_track};
use crate::error::CatalogError;
use crate::http::{check_status, is_transient, FailedTransport, ReqwestTransport, Transport};
use crate::itunes::{
    itunes_lookup_url, itunes_search_url, parse_itunes_lookup, parse_itunes_search,
};
use crate::model::{CatalogHit, SearchQuery};
use crate::normalize::build_search_term;

/// Асинхронный порт поиска треков во внешнем каталоге.
///
/// `search` — текстовый поиск по `q` (с опциональным стилем); `lookup` —
/// точечное получение одного хита по `(source, source_id)`. Реализации:
/// [`HttpCatalog`] (iTunes + Deezer) и
/// [`MockCatalog`](crate::MockCatalog) (детерминированный двойник для тестов).
#[async_trait::async_trait]
pub trait CatalogSearch: Send + Sync {
    /// Ищет треки по запросу `query`.
    async fn search(&self, query: SearchQuery) -> Result<Vec<CatalogHit>, CatalogError>;

    /// Возвращает один хит по источнику и идентификатору или `None`.
    async fn lookup(
        &self,
        source: &str,
        source_id: &str,
    ) -> Result<Option<CatalogHit>, CatalogError>;
}

/// HTTP-клиент каталога: iTunes Search + Deezer (без API-ключей).
///
/// `search` опрашивает iTunes первым; при пустом результате или транзиентной
/// ошибке (rate-limit/timeout/5xx) подключается Deezer как резерв. Битый JSON
/// или ошибка конфигурации (`Decode`/`Config`) пробрасываются сразу, без
/// резерва — это не транзиентные сбои. `lookup` маршрутизируется по `source`.
pub struct HttpCatalog {
    /// Транспорт, изолирующий reqwest (реальный или фейковый в тестах).
    transport: Arc<dyn Transport + Send + Sync>,
}

impl HttpCatalog {
    /// Создаёт клиент с заданным таймаутом HTTP.
    ///
    /// Если построить reqwest-клиент не удалось (например, невалидный
    /// таймаут), все запросы будут возвращать [`CatalogError::Config`].
    pub fn new(timeout: Duration) -> Self {
        let client = reqwest::Client::builder().timeout(timeout).build();
        match client {
            Ok(c) => Self {
                transport: Arc::new(ReqwestTransport { client: c }),
            },
            Err(e) => Self {
                transport: Arc::new(FailedTransport {
                    reason: format!("failed to build HTTP client: {e}"),
                }),
            },
        }
    }

    /// Конструктор для тестов на фейковом транспорте.
    #[cfg(test)]
    pub(crate) fn with_transport(transport: Arc<dyn Transport + Send + Sync>) -> Self {
        Self { transport }
    }

    /// Опрашивает iTunes Search и возвращает хиты.
    async fn fetch_itunes(&self, url: &str) -> Result<Vec<CatalogHit>, CatalogError> {
        let resp = self.transport.get(url).await?;
        if resp.status == 404 {
            return Ok(Vec::new());
        }
        check_status("itunes", resp.status)?;
        parse_itunes_search(&resp.body)
    }

    /// Опрашивает Deezer search и возвращает хиты.
    async fn fetch_deezer(&self, url: &str) -> Result<Vec<CatalogHit>, CatalogError> {
        let resp = self.transport.get(url).await?;
        if resp.status == 404 {
            return Ok(Vec::new());
        }
        check_status("deezer", resp.status)?;
        parse_deezer_search(&resp.body)
    }
}

#[async_trait::async_trait]
impl CatalogSearch for HttpCatalog {
    async fn search(&self, query: SearchQuery) -> Result<Vec<CatalogHit>, CatalogError> {
        let q = query.q.trim();
        if q.is_empty() {
            return Ok(Vec::new());
        }
        let limit = query.normalized_limit();
        let term = build_search_term(q, query.style.as_deref());

        let itunes_url = itunes_search_url(&term, limit);
        let itunes = self.fetch_itunes(&itunes_url).await;

        match itunes {
            Ok(hits) if !hits.is_empty() => Ok(hits),
            Ok(_) => {
                // iTunes пуст — резерв Deezer.
                let deezer_url = deezer_search_url(&term, limit);
                match self.fetch_deezer(&deezer_url).await {
                    Ok(hits) => Ok(hits),
                    Err(e) if is_transient(&e) => Ok(Vec::new()),
                    Err(e) => Err(e),
                }
            }
            Err(e) if is_transient(&e) => {
                // iTunes транзиентно недоступен — резерв Deezer.
                let deezer_url = deezer_search_url(&term, limit);
                match self.fetch_deezer(&deezer_url).await {
                    Ok(hits) => Ok(hits),
                    Err(de) if is_transient(&de) => Err(e),
                    Err(de) => Err(de),
                }
            }
            Err(e) => Err(e),
        }
    }

    async fn lookup(
        &self,
        source: &str,
        source_id: &str,
    ) -> Result<Option<CatalogHit>, CatalogError> {
        let src = source.trim().to_ascii_lowercase();
        let id = source_id.trim();
        if id.is_empty() {
            return Err(CatalogError::Config("empty source_id".to_owned()));
        }
        match src.as_str() {
            "itunes" => {
                if !id.chars().all(|c| c.is_ascii_digit()) {
                    return Err(CatalogError::Config(format!(
                        "itunes source_id must be numeric: {id}"
                    )));
                }
                let url = itunes_lookup_url(id);
                let resp = self.transport.get(&url).await?;
                if resp.status == 404 {
                    return Ok(None);
                }
                check_status("itunes", resp.status)?;
                parse_itunes_lookup(&resp.body)
            }
            "deezer" => {
                if !id.chars().all(|c| c.is_ascii_digit()) {
                    return Err(CatalogError::Config(format!(
                        "deezer source_id must be numeric: {id}"
                    )));
                }
                let url = deezer_track_url(id);
                let resp = self.transport.get(&url).await?;
                if resp.status == 404 {
                    return Ok(None);
                }
                check_status("deezer", resp.status)?;
                parse_deezer_track(&resp.body)
            }
            other => Err(CatalogError::Config(format!(
                "unsupported catalog source: {other}"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::Response;
    use std::collections::VecDeque;
    use std::sync::Mutex;

    /// Фейковый транспорт: выдаёт подготовленные ответы по порядку и
    /// запоминает URL последнего запроса для проверок построения запроса.
    struct FakeTransport {
        responses: Mutex<VecDeque<Result<Response, CatalogError>>>,
        last_url: Mutex<Option<String>>,
    }

    impl FakeTransport {
        fn new() -> Self {
            Self {
                responses: Mutex::new(VecDeque::new()),
                last_url: Mutex::new(None),
            }
        }
        fn push_ok(&self, status: u16, body: &str) {
            self.responses.lock().unwrap().push_back(Ok(Response {
                status,
                body: body.to_owned(),
            }));
        }
        fn push_err(&self, err: CatalogError) {
            self.responses.lock().unwrap().push_back(Err(err));
        }
        fn last_url(&self) -> Option<String> {
            self.last_url.lock().unwrap().clone()
        }
    }

    #[async_trait::async_trait]
    impl Transport for FakeTransport {
        async fn get(&self, url: &str) -> Result<Response, CatalogError> {
            *self.last_url.lock().unwrap() = Some(url.to_owned());
            self.responses
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or_else(|| Err(CatalogError::Unavailable("no fake response".to_owned())))
        }
    }

    fn itunes_hits_body() -> &'static str {
        include_str!("../tests/fixtures/itunes_search.json")
    }
    fn deezer_hits_body() -> &'static str {
        include_str!("../tests/fixtures/deezer_search.json")
    }
    fn itunes_empty_body() -> &'static str {
        include_str!("../tests/fixtures/itunes_empty.json")
    }
    fn deezer_empty_body() -> &'static str {
        include_str!("../tests/fixtures/deezer_empty.json")
    }
    fn itunes_lookup_body() -> &'static str {
        include_str!("../tests/fixtures/itunes_lookup.json")
    }
    fn deezer_track_body() -> &'static str {
        include_str!("../tests/fixtures/deezer_track.json")
    }

    fn http(fake: Arc<FakeTransport>) -> HttpCatalog {
        HttpCatalog::with_transport(fake as Arc<dyn Transport + Send + Sync>)
    }

    #[tokio::test]
    async fn search_empty_query_returns_empty() {
        let fake = Arc::new(FakeTransport::new());
        let cat = http(fake.clone());
        let hits = cat.search(SearchQuery::new("   ")).await.expect("ok");
        assert!(hits.is_empty());
        assert!(fake.last_url().is_none(), "no transport call expected");
    }

    #[tokio::test]
    async fn search_builds_itunes_url_with_style_and_limit() {
        let fake = Arc::new(FakeTransport::new());
        fake.push_ok(200, itunes_hits_body());
        let cat = http(fake.clone());
        let hits = cat
            .search(
                SearchQuery::new("Marc Anthony")
                    .with_limit(5)
                    .with_style("salsa"),
            )
            .await
            .expect("ok");
        assert_eq!(hits.len(), 3);
        let url = fake.last_url().expect("url captured");
        assert!(url.starts_with("https://itunes.apple.com/search?term="));
        assert!(url.contains("Marc%20Anthony%20salsa"));
        assert!(url.contains("limit=5"));
    }

    #[tokio::test]
    async fn search_itunes_hits_returned() {
        let fake = Arc::new(FakeTransport::new());
        fake.push_ok(200, itunes_hits_body());
        let cat = http(fake);
        let hits = cat.search(SearchQuery::new("vivir")).await.expect("ok");
        assert_eq!(hits.len(), 3);
        assert_eq!(hits[0].source, "itunes");
    }

    #[tokio::test]
    async fn search_falls_back_to_deezer_when_itunes_empty() {
        let fake = Arc::new(FakeTransport::new());
        fake.push_ok(200, itunes_empty_body());
        fake.push_ok(200, deezer_hits_body());
        let cat = http(fake);
        let hits = cat.search(SearchQuery::new("daft")).await.expect("ok");
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].source, "deezer");
    }

    #[tokio::test]
    async fn search_itunes_empty_and_deezer_5xx_returns_empty() {
        let fake = Arc::new(FakeTransport::new());
        fake.push_ok(200, itunes_empty_body());
        fake.push_ok(503, "");
        let cat = http(fake);
        let hits = cat.search(SearchQuery::new("daft")).await.expect("ok");
        assert!(hits.is_empty());
    }

    #[tokio::test]
    async fn search_itunes_429_falls_back_to_deezer() {
        let fake = Arc::new(FakeTransport::new());
        fake.push_ok(429, "");
        fake.push_ok(200, deezer_hits_body());
        let cat = http(fake);
        let hits = cat.search(SearchQuery::new("daft")).await.expect("ok");
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].source, "deezer");
    }

    #[tokio::test]
    async fn search_both_rate_limited_returns_rate_limited() {
        let fake = Arc::new(FakeTransport::new());
        fake.push_ok(429, "");
        fake.push_ok(429, "");
        let cat = http(fake);
        let err = cat.search(SearchQuery::new("daft")).await.unwrap_err();
        assert_eq!(err, CatalogError::RateLimited);
    }

    #[tokio::test]
    async fn search_broken_itunes_json_is_decode_without_fallback() {
        let fake = Arc::new(FakeTransport::new());
        fake.push_ok(200, "{ broken ");
        let cat = http(fake);
        let err = cat.search(SearchQuery::new("daft")).await.unwrap_err();
        assert!(matches!(err, CatalogError::Decode(_)), "got {err:?}");
    }

    #[tokio::test]
    async fn search_itunes_timeout_falls_back_to_deezer() {
        let fake = Arc::new(FakeTransport::new());
        fake.push_err(CatalogError::Timeout);
        fake.push_ok(200, deezer_hits_body());
        let cat = http(fake);
        let hits = cat.search(SearchQuery::new("daft")).await.expect("ok");
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].source, "deezer");
    }

    #[tokio::test]
    async fn search_both_timeout_returns_timeout() {
        let fake = Arc::new(FakeTransport::new());
        fake.push_err(CatalogError::Timeout);
        fake.push_err(CatalogError::Timeout);
        let cat = http(fake);
        let err = cat.search(SearchQuery::new("daft")).await.unwrap_err();
        assert_eq!(err, CatalogError::Timeout);
    }

    #[tokio::test]
    async fn search_itunes_5xx_and_deezer_empty_returns_empty() {
        let fake = Arc::new(FakeTransport::new());
        fake.push_ok(500, "");
        fake.push_ok(200, deezer_empty_body());
        let cat = http(fake);
        let hits = cat.search(SearchQuery::new("daft")).await.expect("ok");
        assert!(hits.is_empty());
    }

    #[tokio::test]
    async fn lookup_itunes_found() {
        let fake = Arc::new(FakeTransport::new());
        fake.push_ok(200, itunes_lookup_body());
        let cat = http(fake.clone());
        let hit = cat
            .lookup("itunes", "711404302")
            .await
            .expect("ok")
            .expect("present");
        assert_eq!(hit.source_id, "711404302");
        let url = fake.last_url().expect("url");
        assert_eq!(url, "https://itunes.apple.com/lookup?id=711404302");
    }

    #[tokio::test]
    async fn lookup_itunes_404_is_none() {
        let fake = Arc::new(FakeTransport::new());
        fake.push_ok(404, "");
        let cat = http(fake);
        let hit = cat.lookup("itunes", "1").await.expect("ok");
        assert!(hit.is_none());
    }

    #[tokio::test]
    async fn lookup_itunes_429_is_rate_limited() {
        let fake = Arc::new(FakeTransport::new());
        fake.push_ok(429, "");
        let cat = http(fake);
        let err = cat.lookup("itunes", "1").await.unwrap_err();
        assert_eq!(err, CatalogError::RateLimited);
    }

    #[tokio::test]
    async fn lookup_deezer_found() {
        let fake = Arc::new(FakeTransport::new());
        fake.push_ok(200, deezer_track_body());
        let cat = http(fake.clone());
        let hit = cat
            .lookup("deezer", "3135556")
            .await
            .expect("ok")
            .expect("present");
        assert_eq!(hit.source_id, "3135556");
        let url = fake.last_url().expect("url");
        assert_eq!(url, "https://api.deezer.com/track/3135556");
    }

    #[tokio::test]
    async fn lookup_deezer_quota_is_rate_limited() {
        let fake = Arc::new(FakeTransport::new());
        fake.push_ok(200, include_str!("../tests/fixtures/deezer_quota.json"));
        let cat = http(fake);
        let err = cat.lookup("deezer", "1").await.unwrap_err();
        assert_eq!(err, CatalogError::RateLimited);
    }

    #[tokio::test]
    async fn lookup_unknown_source_is_config() {
        let fake = Arc::new(FakeTransport::new());
        let cat = http(fake.clone());
        let err = cat.lookup("spotify", "1").await.unwrap_err();
        assert!(matches!(err, CatalogError::Config(_)), "got {err:?}");
        assert!(
            fake.last_url().is_none(),
            "no transport call for unknown source"
        );
    }

    #[tokio::test]
    async fn lookup_non_numeric_id_is_config() {
        let fake = Arc::new(FakeTransport::new());
        let cat = http(fake.clone());
        let err = cat.lookup("itunes", "abc").await.unwrap_err();
        assert!(matches!(err, CatalogError::Config(_)), "got {err:?}");
        let err = cat.lookup("deezer", "1a").await.unwrap_err();
        assert!(matches!(err, CatalogError::Config(_)), "got {err:?}");
        assert!(fake.last_url().is_none());
    }

    #[tokio::test]
    async fn lookup_empty_id_is_config() {
        let fake = Arc::new(FakeTransport::new());
        let cat = http(fake.clone());
        let err = cat.lookup("itunes", "   ").await.unwrap_err();
        assert!(matches!(err, CatalogError::Config(_)), "got {err:?}");
    }

    #[tokio::test]
    async fn lookup_trims_source_and_id() {
        let fake = Arc::new(FakeTransport::new());
        fake.push_ok(200, itunes_lookup_body());
        let cat = http(fake);
        let hit = cat.lookup("  ITUNES  ", "  711404302  ").await.expect("ok");
        assert!(hit.is_some());
    }

    #[tokio::test]
    async fn new_builds_real_client() {
        // Конструктор не паникует и создаёт рабочий клиент.
        let cat = HttpCatalog::new(Duration::from_secs(5));
        // Пустой запрос не доходит до транспорта — клиент не нужен.
        let hits = cat.search(SearchQuery::new("   ")).await.expect("ok");
        assert!(hits.is_empty());
    }
}
