//! Deezer API: построение URL и разбор ответов.
//!
//! Endpoints (без API-ключа):
//! * search — `https://api.deezer.com/search?q=..&limit=..`
//! * track — `https://api.deezer.com/track/{id}`
//!
//! Парсеры [`parse_deezer_search`] / [`parse_deezer_track`] детерминированы и
//! работают на записанных JSON-фикстурах без сети. Обрабатывают квотную
//! ошибку Deezer (`{"error":{"code":4,...}}` → [`CatalogError::RateLimited`])
//! и «нет данных» (`code:800` → пустой результат/`None`).

use crate::error::CatalogError;
use crate::model::CatalogHit;
use crate::query::{clamp_limit, encode_query_component};

/// Базовый URL поиска Deezer API.
pub const DEEZER_SEARCH_URL: &str = "https://api.deezer.com/search";

/// Базовый URL трека Deezer API.
pub const DEEZER_TRACK_URL: &str = "https://api.deezer.com/track";

/// Код ошибки Deezer «превышена квота» (rate-limit).
pub const DEEZER_ERROR_QUOTA: i32 = 4;

/// Код ошибки Deezer «нет данных» (not found).
pub const DEEZER_ERROR_NO_DATA: i32 = 800;

/// Строит URL поиска Deezer для терма и лимита.
///
/// `limit` зажимается в `1..=25` через [`clamp_limit`](crate::clamp_limit);
/// `q` percent-кодируется.
pub fn deezer_search_url(term: &str, limit: u8) -> String {
    format!(
        "{}?q={}&limit={}",
        DEEZER_SEARCH_URL,
        encode_query_component(term),
        clamp_limit(limit)
    )
}

/// Строит URL трека Deezer по идентификатору.
///
/// `source_id` percent-кодируется; валидация числового формата выполняется
/// на уровне [`CatalogSearch`](crate::CatalogSearch)::lookup.
pub fn deezer_track_url(source_id: &str) -> String {
    format!("{}/{}", DEEZER_TRACK_URL, encode_query_component(source_id))
}

/// Ошибка Deezer в теле ответа (возвращается с HTTP 200).
#[derive(Debug, serde::Deserialize)]
struct DeezerErrorBody {
    #[serde(default)]
    code: Option<i32>,
    #[serde(default)]
    message: Option<String>,
}

/// Обёртка поиска Deezer: `{"data":[...],"total":N,"error":{...}?}`.
#[derive(Debug, serde::Deserialize)]
struct DeezerSearchResponse {
    #[serde(default)]
    data: Vec<DeezerTrack>,
    #[serde(default)]
    error: Option<DeezerErrorBody>,
}

/// Bare-объект трека Deezer (ответ `/track/{id}`); может содержать `error`.
#[derive(Debug, serde::Deserialize)]
struct DeezerTrackResponse {
    #[serde(flatten)]
    track: DeezerTrack,
    #[serde(default)]
    error: Option<DeezerErrorBody>,
}

/// Запись трека Deezer (поля, нужные каталогу).
#[derive(Debug, Default, serde::Deserialize)]
struct DeezerTrack {
    #[serde(default)]
    id: Option<i64>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    artist: Option<DeezerArtist>,
    #[serde(default)]
    album: Option<DeezerAlbum>,
    #[serde(default)]
    duration: Option<i64>,
    #[serde(default)]
    isrc: Option<String>,
    #[serde(default)]
    link: Option<String>,
    #[serde(default)]
    preview: Option<String>,
    #[serde(default)]
    explicit_lyrics: Option<bool>,
    #[serde(default)]
    explicit_content_lyrics: Option<i32>,
}

#[derive(Debug, serde::Deserialize)]
struct DeezerArtist {
    #[serde(default)]
    name: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct DeezerAlbum {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    cover_xl: Option<String>,
    #[serde(default)]
    cover_big: Option<String>,
    #[serde(default)]
    cover_medium: Option<String>,
    #[serde(default)]
    cover_small: Option<String>,
    #[serde(default)]
    cover: Option<String>,
}

/// Разбирает тело ответа `/search` в список хитов.
///
/// Квота (`error.code == 4`) → [`CatalogError::RateLimited`]; «нет данных»
/// (`code == 800`) → пустой список; иная ошибка →
/// [`CatalogError::Unavailable`]. Битый JSON → [`CatalogError::Decode`].
pub fn parse_deezer_search(body: &str) -> Result<Vec<CatalogHit>, CatalogError> {
    let resp: DeezerSearchResponse = serde_json::from_str(body)
        .map_err(|e| CatalogError::Decode(format!("deezer search: {e}")))?;
    if let Some(err) = resp.error {
        return Err(map_deezer_error(err));
    }
    Ok(resp.data.into_iter().filter_map(track_to_hit).collect())
}

/// Разбирает тело ответа `/track/{id}` в хит (или `None`).
///
/// Квота → [`CatalogError::RateLimited`]; «нет данных»/иная ошибка → `None`.
/// Битый JSON → [`CatalogError::Decode`].
pub fn parse_deezer_track(body: &str) -> Result<Option<CatalogHit>, CatalogError> {
    let resp: DeezerTrackResponse = serde_json::from_str(body)
        .map_err(|e| CatalogError::Decode(format!("deezer track: {e}")))?;
    if let Some(err) = resp.error {
        return Err(map_deezer_error(err));
    }
    Ok(track_to_hit(resp.track))
}

/// Классифицирует тело-ошибку Deezer в `Result`: квота → `RateLimited`,
/// «нет данных» → `Ok(пусто)`-семантика здесь не нужна (вызов в search/track
/// возвращает Err), прочее → `Unavailable`.
fn map_deezer_error(err: DeezerErrorBody) -> CatalogError {
    match err.code {
        Some(DEEZER_ERROR_QUOTA) => CatalogError::RateLimited,
        Some(DEEZER_ERROR_NO_DATA) => {
            CatalogError::Unavailable(err.message.unwrap_or_else(|| "deezer: no data".to_owned()))
        }
        Some(code) => CatalogError::Unavailable(format!(
            "deezer error code {code}: {}",
            err.message.unwrap_or_default()
        )),
        None => CatalogError::Unavailable(
            err.message
                .unwrap_or_else(|| "deezer: unknown error".to_owned()),
        ),
    }
}

/// Преобразует запись Deezer в [`CatalogHit`], если она валидна (есть `id`,
/// `title`, `artist.name`).
fn track_to_hit(t: DeezerTrack) -> Option<CatalogHit> {
    let id = t.id?;
    let title = t.title?;
    let artist = t.artist.and_then(|a| a.name)?;
    if title.trim().is_empty() || artist.trim().is_empty() {
        return None;
    }
    let explicit = t.explicit_lyrics.unwrap_or(false) || t.explicit_content_lyrics == Some(1);
    let album = t.album;
    let album_title = album
        .as_ref()
        .and_then(|a| a.title.clone())
        .filter(|s| !s.is_empty());
    let artwork = album
        .as_ref()
        .and_then(pick_album_cover)
        .filter(|s| !s.is_empty());
    Some(CatalogHit {
        source: "deezer".to_owned(),
        source_id: id.to_string(),
        title,
        artist,
        album: album_title,
        duration_ms: t.duration.unwrap_or(0).saturating_mul(1000),
        isrc: t.isrc.filter(|s| !s.is_empty()),
        artwork_url: artwork,
        preview_url: t.preview.filter(|s| !s.is_empty()),
        source_url: t.link.filter(|s| !s.is_empty()),
        explicit,
    })
}

/// Выбирает обложку альбома наибольшего доступного разрешения.
fn pick_album_cover(album: &DeezerAlbum) -> Option<String> {
    album
        .cover_xl
        .as_deref()
        .or(album.cover_big.as_deref())
        .or(album.cover_medium.as_deref())
        .or(album.cover_small.as_deref())
        .or(album.cover.as_deref())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_owned())
}
