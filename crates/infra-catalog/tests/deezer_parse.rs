//! Тесты разбора ответов Deezer на записанных JSON-фикстурах (без сети).

use dropit_infra_catalog::{parse_deezer_search, parse_deezer_track, CatalogError};

const SEARCH: &str = include_str!("fixtures/deezer_search.json");
const TRACK: &str = include_str!("fixtures/deezer_track.json");
const EMPTY: &str = include_str!("fixtures/deezer_empty.json");
const QUOTA: &str = include_str!("fixtures/deezer_quota.json");
const NOTFOUND: &str = include_str!("fixtures/deezer_notfound.json");
const BROKEN: &str = "{ broken ";

#[test]
fn parse_search_two_hits() {
    let hits = parse_deezer_search(SEARCH).expect("valid fixture");
    assert_eq!(hits.len(), 2);

    let first = &hits[0];
    assert_eq!(first.source, "deezer");
    assert_eq!(first.source_id, "3135556");
    assert_eq!(first.title, "Harder, Better, Faster, Stronger");
    assert_eq!(first.artist, "Daft Punk");
    assert_eq!(first.album.as_deref(), Some("Discovery"));
    // duration в секундах → мс.
    assert_eq!(first.duration_ms, 224_000);
    assert_eq!(first.isrc.as_deref(), Some("USUM71024900"));
    // Наибольшее разрешение обложки (cover_xl).
    assert_eq!(
        first.artwork_url.as_deref(),
        Some("https://e-cdns-images.dzcdn.net/images/cover/2a018e4c6f/1000x1000.jpg")
    );
    assert!(first.preview_url.is_some());
    assert_eq!(
        first.source_url.as_deref(),
        Some("https://www.deezer.com/track/3135556")
    );
    assert!(!first.explicit);
}

#[test]
fn parse_search_explicit_flag() {
    let hits = parse_deezer_search(SEARCH).expect("valid fixture");
    let bb = hits.iter().find(|h| h.source_id == "916424").expect("bb");
    assert!(bb.explicit);
    assert_eq!(bb.duration_ms, 196_000);
}

#[test]
fn parse_track_single_hit() {
    let hit = parse_deezer_track(TRACK)
        .expect("valid fixture")
        .expect("present");
    assert_eq!(hit.source_id, "3135556");
    assert_eq!(hit.artist, "Daft Punk");
    assert_eq!(hit.duration_ms, 224_000);
    assert_eq!(hit.isrc.as_deref(), Some("USUM71024900"));
}

#[test]
fn parse_search_empty() {
    let hits = parse_deezer_search(EMPTY).expect("valid empty");
    assert!(hits.is_empty());
}

#[test]
fn parse_track_empty_is_none_for_no_data() {
    // code 800 → Unavailable (для search), но lookup возвращает Err.
    let err = parse_deezer_track(NOTFOUND).unwrap_err();
    assert!(matches!(err, CatalogError::Unavailable(_)), "got {err:?}");
}

#[test]
fn parse_search_quota_is_rate_limited() {
    let err = parse_deezer_search(QUOTA).unwrap_err();
    assert_eq!(err, CatalogError::RateLimited);
}

#[test]
fn parse_track_quota_is_rate_limited() {
    let err = parse_deezer_track(QUOTA).unwrap_err();
    assert_eq!(err, CatalogError::RateLimited);
}

#[test]
fn parse_search_broken_json_is_decode() {
    let err = parse_deezer_search(BROKEN).unwrap_err();
    assert!(matches!(err, CatalogError::Decode(_)), "got {err:?}");
}

#[test]
fn parse_track_broken_json_is_decode() {
    let err = parse_deezer_track(BROKEN).unwrap_err();
    assert!(matches!(err, CatalogError::Decode(_)), "got {err:?}");
}
