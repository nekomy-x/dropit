//! Детерминированный двойник каталога для тестов.
//!
//! [`MockCatalog`] реализует [`CatalogSearch`](crate::CatalogSearch) на
//! фиксированном наборе хитов: `search` фильтрует их по токенам `q` (fold по
//! диакритике/регистру/пунктуации), `lookup` находит точное совпадение по
//! `(source, source_id)`. Стиль не фильтрует (хиты не несут жанр).

use crate::error::CatalogError;
use crate::model::{CatalogHit, SearchQuery};
use crate::normalize::normalize_token;
use crate::search::CatalogSearch;

/// Фиксированный каталог для тестов: возвращает хиты из памяти.
pub struct MockCatalog {
    /// Хиты, которыми оперирует двойник.
    hits: Vec<CatalogHit>,
}

impl MockCatalog {
    /// Создаёт двойника с заданным набором хитов.
    pub fn with_hits(hits: Vec<CatalogHit>) -> Self {
        Self { hits }
    }

    /// Создаёт пустого двойника (все поиски возвращают пусто).
    pub fn empty() -> Self {
        Self { hits: Vec::new() }
    }
}

#[async_trait::async_trait]
impl CatalogSearch for MockCatalog {
    async fn search(&self, query: SearchQuery) -> Result<Vec<CatalogHit>, CatalogError> {
        let q = query.q.trim();
        if q.is_empty() {
            return Ok(Vec::new());
        }
        let needles: Vec<String> = q
            .split_whitespace()
            .map(normalize_token)
            .filter(|t| !t.is_empty())
            .collect();
        if needles.is_empty() {
            return Ok(Vec::new());
        }
        let limit = query.normalized_limit() as usize;
        let mut out: Vec<CatalogHit> = self
            .hits
            .iter()
            .filter(|h| {
                let hay = format!(
                    "{} {} {}",
                    h.artist,
                    h.title,
                    h.album.as_deref().unwrap_or("")
                );
                let hay = normalize_token(&hay);
                needles.iter().all(|n| hay.contains(n))
            })
            .cloned()
            .collect();
        out.truncate(limit);
        Ok(out)
    }

    async fn lookup(
        &self,
        source: &str,
        source_id: &str,
    ) -> Result<Option<CatalogHit>, CatalogError> {
        let id = source_id.trim();
        Ok(self
            .hits
            .iter()
            .find(|h| h.source.eq_ignore_ascii_case(source) && h.source_id == id)
            .cloned())
    }
}
