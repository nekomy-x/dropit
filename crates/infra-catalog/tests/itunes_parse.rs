//! Тесты разбора ответов iTunes на записанных JSON-фикстурах (без сети).

use dropit_infra_catalog::{parse_itunes_lookup, parse_itunes_search, CatalogError};

const SEARCH: &str = include_str!("fixtures/itunes_search.json");
const LOOKUP: &str = include_str!("fixtures/itunes_lookup.json");
const EMPTY: &str = include_str!("fixtures/itunes_empty.json");
const BROKEN: &str = "{ not valid json ";

#[test]
fn parse_search_three_hits() {
    let hits = parse_itunes_search(SEARCH).expect("valid fixture");
    assert_eq!(hits.len(), 3);

    let first = &hits[0];
    assert_eq!(first.source, "itunes");
    assert_eq!(first.source_id, "1440857786");
    assert_eq!(first.title, "Better Together");
    assert_eq!(first.artist, "Jack Johnson");
    assert_eq!(first.album.as_deref(), Some("In Between Dreams"));
    assert_eq!(first.duration_ms, 201_000);
    assert!(first.isrc.is_none());
    // Наибольшее разрешение обложки.
    assert_eq!(
        first.artwork_url.as_deref(),
        Some("https://is1-ssl.mzstatic.com/image/thumb/600x600bb.jpg")
    );
    assert!(first.preview_url.is_some());
    assert!(first.source_url.is_some());
    assert!(!first.explicit);
}

#[test]
fn parse_search_isrc_and_explicit() {
    let hits = parse_itunes_search(SEARCH).expect("valid fixture");
    // Второй хит — Marc Anthony с ISRC, не explicit.
    let marc = hits
        .iter()
        .find(|h| h.source_id == "1440888792")
        .expect("marc");
    assert_eq!(marc.isrc.as_deref(), Some("USUM71302116"));
    assert!(!marc.explicit);

    // Третий хит — explicit.
    let bb = hits
        .iter()
        .find(|h| h.source_id == "10987654321")
        .expect("bb");
    assert!(bb.explicit);
}

#[test]
fn parse_search_normalizes_source_name() {
    let hits = parse_itunes_search(SEARCH).expect("valid fixture");
    assert!(hits.iter().all(|h| h.source == "itunes"));
    assert!(hits.iter().all(|h| !h.source_id.is_empty()));
}

#[test]
fn parse_lookup_single_hit() {
    let hit = parse_itunes_lookup(LOOKUP)
        .expect("valid fixture")
        .expect("present");
    assert_eq!(hit.source_id, "711404302");
    assert_eq!(hit.artist, "Daft Punk");
    assert_eq!(hit.title, "Get Lucky (feat. Pharrell Williams)");
    assert_eq!(hit.duration_ms, 369_000);
    assert_eq!(hit.isrc.as_deref(), Some("USUM71302117"));
}

#[test]
fn parse_lookup_empty_is_none() {
    let hit = parse_itunes_lookup(EMPTY).expect("valid empty");
    assert!(hit.is_none());
}

#[test]
fn parse_search_empty_is_vec() {
    let hits = parse_itunes_search(EMPTY).expect("valid empty");
    assert!(hits.is_empty());
}

#[test]
fn parse_search_broken_json_is_decode() {
    let err = parse_itunes_search(BROKEN).unwrap_err();
    assert!(matches!(err, CatalogError::Decode(_)), "got {err:?}");
}

#[test]
fn parse_lookup_broken_json_is_decode() {
    let err = parse_itunes_lookup(BROKEN).unwrap_err();
    assert!(matches!(err, CatalogError::Decode(_)), "got {err:?}");
}
