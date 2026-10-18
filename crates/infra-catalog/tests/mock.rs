//! Тесты MockCatalog: фильтрация по q, lookup, пустой двойник.

use dropit_infra_catalog::{CatalogHit, CatalogSearch, MockCatalog, SearchQuery};

fn sample_hits() -> Vec<CatalogHit> {
    vec![
        CatalogHit {
            source: "itunes".to_owned(),
            source_id: "1".to_owned(),
            title: "Vivir Mi Vida".to_owned(),
            artist: "Marc Anthony".to_owned(),
            album: Some("3.0".to_owned()),
            duration_ms: 264_000,
            isrc: Some("USUM71302116".to_owned()),
            artwork_url: None,
            preview_url: None,
            source_url: None,
            explicit: false,
        },
        CatalogHit {
            source: "deezer".to_owned(),
            source_id: "2".to_owned(),
            title: "Get Lucky".to_owned(),
            artist: "Daft Punk".to_owned(),
            album: None,
            duration_ms: 369_000,
            isrc: None,
            artwork_url: None,
            preview_url: None,
            source_url: None,
            explicit: false,
        },
        CatalogHit {
            source: "itunes".to_owned(),
            source_id: "3".to_owned(),
            title: "Café Bleu".to_owned(),
            artist: "The Style Council".to_owned(),
            album: None,
            duration_ms: 0,
            isrc: None,
            artwork_url: None,
            preview_url: None,
            source_url: None,
            explicit: true,
        },
    ]
}

#[tokio::test]
async fn mock_search_matches_token() {
    let cat = MockCatalog::with_hits(sample_hits());
    let hits = cat
        .search(SearchQuery::new("Marc Anthony"))
        .await
        .expect("ok");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].source_id, "1");
}

#[tokio::test]
async fn mock_search_matches_partial_token() {
    let cat = MockCatalog::with_hits(sample_hits());
    let hits = cat.search(SearchQuery::new("daft")).await.expect("ok");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].source_id, "2");
}

#[tokio::test]
async fn mock_search_diacritic_insensitive() {
    // «cafe» находит «Café».
    let cat = MockCatalog::with_hits(sample_hits());
    let hits = cat.search(SearchQuery::new("cafe")).await.expect("ok");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].source_id, "3");
}

#[tokio::test]
async fn mock_search_multiple_tokens_and() {
    let cat = MockCatalog::with_hits(sample_hits());
    // Оба токена должны присутствовать.
    let hits = cat
        .search(SearchQuery::new("marc vivir"))
        .await
        .expect("ok");
    assert_eq!(hits.len(), 1);
    let hits = cat.search(SearchQuery::new("marc daft")).await.expect("ok");
    assert!(hits.is_empty());
}

#[tokio::test]
async fn mock_search_limit_truncates() {
    let cat = MockCatalog::with_hits(sample_hits());
    let hits = cat
        .search(SearchQuery::new("a").with_limit(2))
        .await
        .expect("ok");
    // «a» содержится во всех трёх — лимит 2.
    assert_eq!(hits.len(), 2);
}

#[tokio::test]
async fn mock_search_empty_query_returns_empty() {
    let cat = MockCatalog::with_hits(sample_hits());
    let hits = cat.search(SearchQuery::new("   ")).await.expect("ok");
    assert!(hits.is_empty());
}

#[tokio::test]
async fn mock_empty_catalog_search() {
    let cat = MockCatalog::empty();
    let hits = cat.search(SearchQuery::new("anything")).await.expect("ok");
    assert!(hits.is_empty());
}

#[tokio::test]
async fn mock_lookup_found() {
    let cat = MockCatalog::with_hits(sample_hits());
    let hit = cat.lookup("itunes", "1").await.expect("ok");
    assert!(hit.is_some());
    assert_eq!(hit.unwrap().title, "Vivir Mi Vida");
}

#[tokio::test]
async fn mock_lookup_case_insensitive_source() {
    let cat = MockCatalog::with_hits(sample_hits());
    let hit = cat.lookup("ITUNES", "1").await.expect("ok");
    assert!(hit.is_some());
}

#[tokio::test]
async fn mock_lookup_not_found() {
    let cat = MockCatalog::with_hits(sample_hits());
    let hit = cat.lookup("itunes", "999").await.expect("ok");
    assert!(hit.is_none());
    // Несовпадение источника.
    let hit = cat.lookup("deezer", "1").await.expect("ok");
    assert!(hit.is_none());
}

#[tokio::test]
async fn mock_lookup_trims_source_id() {
    let cat = MockCatalog::with_hits(sample_hits());
    let hit = cat.lookup("itunes", "  1  ").await.expect("ok");
    assert!(hit.is_some());
}
