//! iTunes Search API: построение URL и разбор ответов.
//!
//! Endpoints (без API-ключа):
//! * search — `https://itunes.apple.com/search?term=..&media=music&entity=song&limit=..`
//! * lookup — `https://itunes.apple.com/lookup?id=..`
//!
//! Парсеры [`parse_itunes_search`] / [`parse_itunes_lookup`] детерминированы и
//! работают на записанных JSON-фикстурах без сети.

use crate::error::CatalogError;
use crate::model::CatalogHit;
use crate::query::{clamp_limit, encode_query_component};

/// Базовый URL поиска iTunes Search API.
pub const ITUNES_SEARCH_URL: &str = "https://itunes.apple.com/search";

/// Базовый URL lookup iTunes Search API.
pub const ITUNES_LOOKUP_URL: &str = "https://itunes.apple.com/lookup";

/// Строит URL поиска iTunes для терма и лимита.
///
/// `limit` зажимается в `1..=25` через [`clamp_limit`](crate::clamp_limit);
/// `term` percent-кодируется через [`encode_query_component`](crate::encode_query_component).
pub fn itunes_search_url(term: &str, limit: u8) -> String {
    format!(
        "{}?term={}&media=music&entity=song&limit={}",
        ITUNES_SEARCH_URL,
        encode_query_component(term),
        clamp_limit(limit)
    )
}

/// Строит URL lookup iTunes по идентификатору трека.
///
/// `source_id` percent-кодируется; ожидается число, но кодирование безопасно
/// для любого значения — валидация числового формата выполняется на уровне
/// [`CatalogSearch`](crate::CatalogSearch)::lookup.
pub fn itunes_lookup_url(source_id: &str) -> String {
    format!(
        "{}?id={}",
        ITUNES_LOOKUP_URL,
        encode_query_component(source_id)
    )
}

/// Ответ iTunes Search API.
#[derive(Debug, serde::Deserialize)]
struct ItunesResponse {
    #[serde(default)]
    results: Vec<ItunesTrack>,
}

/// Одна запись результата iTunes (только поля, нужные каталогу).
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ItunesTrack {
    #[serde(default)]
    track_id: Option<i64>,
    #[serde(default)]
    artist_name: Option<String>,
    #[serde(default)]
    track_name: Option<String>,
    #[serde(default)]
    collection_name: Option<String>,
    #[serde(default)]
    track_time_millis: Option<i64>,
    #[serde(default)]
    isrc: Option<String>,
    #[serde(default)]
    artwork_url_30: Option<String>,
    #[serde(default)]
    artwork_url_60: Option<String>,
    #[serde(default)]
    artwork_url_100: Option<String>,
    #[serde(default)]
    artwork_url_600: Option<String>,
    #[serde(default)]
    preview_url: Option<String>,
    #[serde(default)]
    track_view_url: Option<String>,
    #[serde(default)]
    track_explicitness: Option<String>,
    #[serde(default)]
    collection_explicitness: Option<String>,
}

/// Разбирает тело ответа `/search` в список хитов.
///
/// Пропускает записи без `trackId`/`trackName`/`artistName` (включая
/// non-track wrappers: artist/collection). Возвращает
/// [`CatalogError::Decode`] на битый JSON.
pub fn parse_itunes_search(body: &str) -> Result<Vec<CatalogHit>, CatalogError> {
    let resp: ItunesResponse = serde_json::from_str(body)
        .map_err(|e| CatalogError::Decode(format!("itunes search: {e}")))?;
    Ok(resp.results.into_iter().filter_map(track_to_hit).collect())
}

/// Разбирает тело ответа `/lookup` в единственный хит (или `None`).
///
/// Берёт первую подходящую трек-запись; пустой `results` → `None`.
pub fn parse_itunes_lookup(body: &str) -> Result<Option<CatalogHit>, CatalogError> {
    let resp: ItunesResponse = serde_json::from_str(body)
        .map_err(|e| CatalogError::Decode(format!("itunes lookup: {e}")))?;
    Ok(resp.results.into_iter().find_map(track_to_hit))
}

/// Преобразует запись iTunes в [`CatalogHit`], если она валидна (есть
/// `trackId`, `trackName`, `artistName`).
fn track_to_hit(t: ItunesTrack) -> Option<CatalogHit> {
    let track_id = t.track_id?;
    let title = t.track_name?;
    let artist = t.artist_name?;
    if title.trim().is_empty() || artist.trim().is_empty() {
        return None;
    }
    let explicit = is_explicit(&t.track_explicitness) || is_explicit(&t.collection_explicitness);
    Some(CatalogHit {
        source: "itunes".to_owned(),
        source_id: track_id.to_string(),
        title,
        artist,
        album: t.collection_name.filter(|s| !s.is_empty()),
        duration_ms: t.track_time_millis.unwrap_or(0),
        isrc: t.isrc.filter(|s| !s.is_empty()),
        artwork_url: pick_artwork(
            t.artwork_url_600.as_deref(),
            t.artwork_url_100.as_deref(),
            t.artwork_url_60.as_deref(),
            t.artwork_url_30.as_deref(),
        ),
        preview_url: t.preview_url.filter(|s| !s.is_empty()),
        source_url: t.track_view_url.filter(|s| !s.is_empty()),
        explicit,
    })
}

/// Признак explicit по строке `*Explicitness` (`explicit`/`cleaned`/`notExplicit`).
fn is_explicit(value: &Option<String>) -> bool {
    match value {
        Some(s) => s.eq_ignore_ascii_case("explicit"),
        None => false,
    }
}

/// Выбирает обложку наибольшего доступного разрешения.
fn pick_artwork(
    a: Option<&str>,
    b: Option<&str>,
    c: Option<&str>,
    d: Option<&str>,
) -> Option<String> {
    a.or(b)
        .or(c)
        .or(d)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_owned())
}
