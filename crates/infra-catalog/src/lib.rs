//! Каталог треков DROP IT: внешние источники (iTunes Search, Deezer),
//! нормализация, ISRC-валидация и политика кеша.
//!
//! Крейт реализует транспортный слой каталога (ADR-006, техкарта
//! Часть VI «Каталог»): асинхронный трейт [`CatalogSearch`] с двумя
//! реализациями — реальный HTTP-клиент [`HttpCatalog`] (iTunes Search +
//! Deezer, без API-ключей) и детерминированный [`MockCatalog`] для тестов.
//!
//! Все сетевые ответы разбираются детерминированными парсерами
//! (`parse_itunes_search`, `parse_deezer_search` и т. д.), которые работают
//! на записанных JSON-фикстурах без сети. Сетевые ошибки классифицируются
//! в [`CatalogError`]: `RateLimited` (HTTP 429), `Timeout`,
//! `Unavailable` (5xx/связь), `Decode` (битый JSON), `Config`
//! (неподдерживаемый источник / невалидный идентификатор).
//!
//! Нормализация [`canonical_key`] даёт стабильный ключ дедупликации
//! `(artist|title)` без учёта регистра, диакритики, пунктуации и
//! feat./prod.-суффиксов; [`validate_isrc`] проверяет структуру ISRC
//! `CC-XXX-YY-NNNNN`; [`cache_ttl`] возвращает политику TTL кеша в
//! зависимости от наличия ISRC.

#![deny(missing_docs)]

mod deezer;
mod error;
mod http;
mod itunes;
mod mock;
mod model;
mod normalize;
mod query;
mod search;

pub use deezer::{deezer_search_url, deezer_track_url, parse_deezer_search, parse_deezer_track};
pub use error::CatalogError;
pub use http::is_transient;
pub use itunes::{itunes_lookup_url, itunes_search_url, parse_itunes_lookup, parse_itunes_search};
pub use mock::MockCatalog;
pub use model::{CatalogHit, SearchQuery};
pub use normalize::{build_search_term, cache_ttl, canonical_key, fold_diacritics, validate_isrc};
pub use query::{clamp_limit, encode_query_component};
pub use search::{CatalogSearch, HttpCatalog};

/// Поддерживаемые внешние источники каталога (нижний регистр).
///
/// Используется трейтом [`CatalogSearch`](crate::CatalogSearch) для
/// маршрутизации `lookup` и как значение поля
/// [`CatalogHit::source`](crate::CatalogHit::source).
pub const SUPPORTED_SOURCES: &[&str] = &["itunes", "deezer"];
