//! Тесты построения URL: экранирование query, лимиты 1..=25, encode.

use dropit_infra_catalog::{
    clamp_limit, deezer_search_url, deezer_track_url, encode_query_component, itunes_lookup_url,
    itunes_search_url,
};

#[test]
fn itunes_search_url_escapes_and_clamps() {
    let url = itunes_search_url("Marc Anthony Vivir Mi Vida", 10);
    assert!(url.starts_with("https://itunes.apple.com/search?term="));
    assert!(url.contains("media=music"));
    assert!(url.contains("entity=song"));
    assert!(url.contains("limit=10"));
    // Пробел → %20.
    assert!(url.contains("Marc%20Anthony"));
    assert!(!url.contains('+'));
}

#[test]
fn itunes_search_url_clamps_high_limit() {
    let url = itunes_search_url("x", 100);
    assert!(url.contains("limit=25"));
}

#[test]
fn itunes_search_url_clamps_zero() {
    let url = itunes_search_url("x", 0);
    assert!(url.contains("limit=1"));
}

#[test]
fn itunes_search_url_encodes_specials() {
    let url = itunes_search_url("a&b=c/dé", 5);
    assert!(url.contains("a%26b%3Dc%2Fd%C3%A9"));
    assert!(url.contains("limit=5"));
}

#[test]
fn itunes_lookup_url_numeric() {
    let url = itunes_lookup_url("1440857786");
    assert_eq!(url, "https://itunes.apple.com/lookup?id=1440857786");
}

#[test]
fn itunes_lookup_url_escapes_non_numeric() {
    let url = itunes_lookup_url("a b");
    assert_eq!(url, "https://itunes.apple.com/lookup?id=a%20b");
}

#[test]
fn deezer_search_url_escapes_and_clamps() {
    let url = deezer_search_url("Daft Punk Get Lucky", 3);
    assert!(url.starts_with("https://api.deezer.com/search?q="));
    assert!(url.contains("Daft%20Punk"));
    assert!(url.contains("limit=3"));
}

#[test]
fn deezer_search_url_clamps() {
    assert!(deezer_search_url("x", 0).contains("limit=1"));
    assert!(deezer_search_url("x", 99).contains("limit=25"));
}

#[test]
fn deezer_track_url_numeric() {
    assert_eq!(
        deezer_track_url("3135556"),
        "https://api.deezer.com/track/3135556"
    );
}

#[test]
fn deezer_track_url_escapes() {
    assert_eq!(
        deezer_track_url("a b"),
        "https://api.deezer.com/track/a%20b"
    );
}

#[test]
fn encode_query_component_corner_cases() {
    assert_eq!(encode_query_component(""), "");
    assert_eq!(encode_query_component("A1-_."), "A1-_.");
    assert_eq!(encode_query_component("100%"), "100%25");
}

#[test]
fn clamp_limit_bounds() {
    assert_eq!(clamp_limit(0), 1);
    assert_eq!(clamp_limit(1), 1);
    assert_eq!(clamp_limit(25), 25);
    assert_eq!(clamp_limit(26), 25);
}
