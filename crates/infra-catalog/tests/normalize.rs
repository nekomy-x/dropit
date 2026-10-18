//! Тесты нормализации: canonical_key, validate_isrc, cache_ttl, style mapping.

use std::time::Duration;

use dropit_infra_catalog::{
    build_search_term, cache_ttl, canonical_key, fold_diacritics, validate_isrc,
};

#[test]
fn canonical_key_basic() {
    assert_eq!(
        canonical_key("Marc Anthony", "Vivir Mi Vida"),
        "marc anthony|vivir mi vida"
    );
}

#[test]
fn canonical_key_case_insensitive() {
    assert_eq!(
        canonical_key("MARC ANTHONY", "VIVIR MI VIDA"),
        "marc anthony|vivir mi vida"
    );
    assert_eq!(
        canonical_key("marc anthony", "vivir mi vida"),
        canonical_key("MaRc AnThOnY", "ViViR mI vIdA")
    );
}

#[test]
fn canonical_key_diacritics_fold() {
    // «Café» и «cafe» — один ключ.
    assert_eq!(
        canonical_key("Café del Mar", "Song"),
        canonical_key("cafe del mar", "song")
    );
    // «Beyoncé»/«Beyonce», «Niño»/«Nino», «Müller»/«Muller».
    assert_eq!(
        canonical_key("Beyoncé", "Halo"),
        canonical_key("Beyonce", "Halo")
    );
    assert_eq!(
        canonical_key("Niño de Elche", "X"),
        canonical_key("Nino de Elche", "X")
    );
    assert_eq!(canonical_key("Müller", "Y"), canonical_key("Muller", "Y"));
}

#[test]
fn canonical_key_strips_feat_suffix() {
    assert_eq!(
        canonical_key("Daft Punk feat. Pharrell Williams", "Get Lucky"),
        canonical_key("Daft Punk", "Get Lucky")
    );
    assert_eq!(canonical_key("A ft B", "T"), canonical_key("A", "T"));
    assert_eq!(canonical_key("A featuring B", "T"), canonical_key("A", "T"));
    assert_eq!(canonical_key("A prod B", "T"), canonical_key("A", "T"));
    assert_eq!(
        canonical_key("A produced by B", "T"),
        canonical_key("A", "T")
    );
}

#[test]
fn canonical_key_strips_punctuation() {
    assert_eq!(
        canonical_key("AC/DC", "Back in Black"),
        "ac dc|back in black"
    );
    assert_eq!(canonical_key("U2", "One"), "u2|one");
    assert_eq!(canonical_key("Hip-Hop", "X"), "hip hop|x");
    assert_eq!(canonical_key("don't stop", "Y"), "dont stop|y");
}

#[test]
fn canonical_key_collapses_whitespace() {
    assert_eq!(
        canonical_key("  Marc   Anthony  ", "  Vivir   Mi  Vida  "),
        "marc anthony|vivir mi vida"
    );
    assert_eq!(
        canonical_key("Marc\tAnthony", "Vivir\nMi"),
        "marc anthony|vivir mi"
    );
}

#[test]
fn canonical_key_empty_strings() {
    assert_eq!(canonical_key("", ""), "");
    assert_eq!(canonical_key("   ", "   "), "");
    assert_eq!(canonical_key("", "Song"), "|song");
    assert_eq!(canonical_key("Artist", ""), "artist|");
}

#[test]
fn canonical_key_only_feat_is_dropped() {
    // Поле из одного маркера feat обнуляется.
    assert_eq!(canonical_key("feat. Someone", "Title"), "|title");
}

#[test]
fn canonical_key_deteministic_order() {
    // Порядок artist|title фиксирован — swap даёт другой ключ.
    assert_ne!(canonical_key("A", "B"), canonical_key("B", "A"));
    assert_eq!(canonical_key("A", "B"), "a|b");
}

#[test]
fn fold_diacritics_keeps_non_latin() {
    // Кириллица остаётся как есть.
    assert_eq!(fold_diacritics("Привét"), "Привet");
    // Лигатуры разворачиваются.
    assert_eq!(fold_diacritics("ÆON ß œuf"), "AEON ss oeuf");
}

#[test]
fn validate_isrc_valid_compact() {
    assert!(validate_isrc("USUM71302116"));
}

#[test]
fn validate_isrc_valid_hyphenated() {
    assert!(validate_isrc("US-UM7-13-02116"));
}

#[test]
fn validate_isrc_case_insensitive() {
    assert!(validate_isrc("usum71302116"));
    assert!(validate_isrc("us-um7-13-02116"));
}

#[test]
fn validate_isrc_wrong_length() {
    assert!(!validate_isrc("USUM7130211")); // 11
    assert!(!validate_isrc("USUM713021166")); // 13
}

#[test]
fn validate_isrc_bad_structure() {
    // Код страны должен быть буквами.
    assert!(!validate_isrc("12UM71302116"));
    // Год должен быть цифрами.
    assert!(!validate_isrc("USUM7CA02116"));
    // Назначение должно быть цифрами.
    assert!(!validate_isrc("USUM7130211A"));
}

#[test]
fn validate_isrc_empty_and_garbage() {
    assert!(!validate_isrc(""));
    assert!(!validate_isrc("not-an-isrc"));
    assert!(!validate_isrc("   "));
}

#[test]
fn cache_ttl_policy() {
    assert_eq!(cache_ttl(true), Duration::from_secs(86_400));
    assert_eq!(cache_ttl(false), Duration::from_secs(3_600));
}

#[test]
fn build_search_term_appends_style() {
    assert_eq!(
        build_search_term("Marc Anthony", Some("salsa")),
        "Marc Anthony salsa"
    );
    assert_eq!(build_search_term("hip hop", Some("salsa")), "hip hop salsa");
}

#[test]
fn build_search_term_no_duplicate_keyword() {
    // Ключевое слово уже в запросе — не дублируем.
    assert_eq!(build_search_term("salsa mix", Some("salsa")), "salsa mix");
    assert_eq!(
        build_search_term("Hip Hop song", Some("hip-hop")),
        "Hip Hop song"
    );
}

#[test]
fn build_search_term_collapses_whitespace() {
    assert_eq!(
        build_search_term("  Marc   Anthony  ", None),
        "Marc Anthony"
    );
}

#[test]
fn build_search_term_style_only() {
    assert_eq!(build_search_term("", Some("salsa")), "salsa");
    assert_eq!(build_search_term("   ", Some("bachata")), "bachata");
}

#[test]
fn build_search_term_unknown_style_ignored() {
    assert_eq!(build_search_term("song", Some("k-pop")), "song");
}

#[test]
fn build_search_term_reggaeton_diacritic() {
    // «reggaetón» fold-ится в ключевое слово reggaeton.
    assert_eq!(
        build_search_term("Bad Bunny", Some("reggaetón")),
        "Bad Bunny reggaeton"
    );
}
