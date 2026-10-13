//! Каталог треков: серверная whitelist-валидация клиентских результатов
//! (техкарта, Часть II «КАТАЛОГ» и Часть VI «Каталог»; ADR-006).
//!
//! Поиск выполняется клиентом (Apple блокирует серверные IP), поэтому
//! сервер не доверяет клиентскому ответу: домены cover/preview, scheme,
//! длины полей и ISRC проверяются по whitelist, а canonical-ключ
//! `source:source_id` пересчитывается заново — защита от отравления кэша
//! модифицированным клиентом. Реальные внешние вызовы iTunes/Deezer/
//! MusicBrainz заблокированы лицензионным вопросом B5 — здесь только
//! чистая доменная логика, без HTTP и БД (ADR-001).
//!
//! Счётчик «той же песни» и дедупликация идут по canonical-ключу
//! `(source, source_id)`, НЕ по `id` строки track_cache.

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

use thiserror::Error;

/// Лимиты полей track_cache (техкарта, Часть VI «Каталог»).
pub const MAX_TITLE_LEN: usize = 500;
pub const MAX_ARTIST_LEN: usize = 500;
pub const MAX_ISRC_LEN: usize = 12;
pub const MAX_SOURCE_ID_LEN: usize = 128;

/// Источник каталога. MusicBrainz — только ISRC-обогащение,
/// без media-URL (техкарта, Часть VI «Каталог», п. 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Itunes,
    Deezer,
    Musicbrainz,
}

impl Source {
    /// Строковое имя в нижнем регистре — часть canonical-ключа.
    pub fn as_str(&self) -> &'static str {
        match self {
            Source::Itunes => "itunes",
            Source::Deezer => "deezer",
            Source::Musicbrainz => "musicbrainz",
        }
    }

    /// Whitelist доменов cover/preview для источника (ADR-006, п. 2).
    /// Пустой список = media-URL запрещены (MusicBrainz).
    fn media_domains(&self) -> &'static [&'static str] {
        match self {
            Source::Itunes => &["mzstatic.com"],
            // cdns-preview-*.deezer.com покрывается суффиксом deezer.com.
            Source::Deezer => &["deezer.com"],
            Source::Musicbrainz => &[],
        }
    }
}

impl fmt::Display for Source {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Неизвестный источник каталога (ошибка разбора строки).
#[derive(Debug, Clone, Error, PartialEq)]
#[error("unknown catalog source: {0}")]
pub struct ParseSourceError(String);

impl FromStr for Source {
    type Err = ParseSourceError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "itunes" => Ok(Source::Itunes),
            "deezer" => Ok(Source::Deezer),
            "musicbrainz" => Ok(Source::Musicbrainz),
            other => Err(ParseSourceError(other.to_owned())),
        }
    }
}

/// Клиентский результат поиска из каталога (сырой, до валидации).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CatalogTrack {
    pub source: Source,
    pub source_id: String,
    pub isrc: Option<String>,
    pub title: String,
    pub artist: String,
    pub cover_url: Option<String>,
    pub preview_url: Option<String>,
}

/// Результат серверной whitelist-валидации: трек, признанный безопасным
/// для upsert в track_cache, плюс пересчитанный на сервере canonical-ключ.
#[derive(Debug, Clone, PartialEq)]
pub struct ValidatedTrack {
    pub track: CatalogTrack,
    pub canonical_key: String,
}

/// Ошибка серверной whitelist-валидации клиентского результата каталога.
#[derive(Debug, Clone, Error, PartialEq)]
pub enum CatalogError {
    #[error("host `{host}` is not in the media-URL whitelist for source {catalog_source}")]
    InvalidDomain {
        field: &'static str,
        host: String,
        catalog_source: Source,
    },
    #[error("field `{field}`: expected https scheme, found `{found}`")]
    InvalidScheme { field: &'static str, found: String },
    #[error("field `{field}` is {actual} chars, max is {max}")]
    FieldTooLong {
        field: &'static str,
        actual: usize,
        max: usize,
    },
    #[error("field `{field}` is empty")]
    EmptyTitle { field: &'static str },
    #[error("invalid ISRC `{value}`: expected 12 uppercase alphanumeric chars")]
    InvalidIsrc { value: String },
    #[error("invalid source_id: {reason}")]
    InvalidSourceId { reason: &'static str },
    #[error("field `{field}` contains control characters")]
    ControlChars { field: &'static str },
}

/// Канонический ключ трека вида `itunes:12345` — source в нижнем
/// регистре, source_id обрезан по краям и провалидирован.
///
/// Единственный ключ дедупликации и счётчика «той же песни»:
/// должен совпадать с UNIQUE(source, source_id) в track_cache
/// и пересчитываться на сервере, а не браться у клиента (ADR-006).
pub fn canonical_key(source: Source, source_id: &str) -> Result<String, CatalogError> {
    let trimmed = source_id.trim();
    if trimmed.is_empty() {
        return Err(CatalogError::InvalidSourceId {
            reason: "empty or whitespace-only",
        });
    }
    if source_id.chars().count() > MAX_SOURCE_ID_LEN {
        return Err(CatalogError::InvalidSourceId {
            reason: "longer than 128 chars",
        });
    }
    if source_id.chars().any(char::is_control) {
        return Err(CatalogError::ControlChars { field: "source_id" });
    }
    Ok(format!("{}:{}", source.as_str(), trimmed))
}

/// Серверная whitelist-валидация клиентского результата каталога
/// (ADR-006, п. 2). Возвращает [`ValidatedTrack`] с пересчитанным
/// canonical-ключом — его сервер сверяет с ключом из ответа клиента.
pub fn validate_track(track: &CatalogTrack) -> Result<ValidatedTrack, CatalogError> {
    let key = canonical_key(track.source, &track.source_id)?;

    validate_text("title", &track.title, MAX_TITLE_LEN)?;
    validate_text("artist", &track.artist, MAX_ARTIST_LEN)?;

    if let Some(isrc) = &track.isrc {
        validate_isrc(isrc)?;
    }

    if let Some(url) = &track.cover_url {
        check_media_url("cover_url", url, track.source)?;
    }
    if let Some(url) = &track.preview_url {
        check_media_url("preview_url", url, track.source)?;
    }

    Ok(ValidatedTrack {
        track: track.clone(),
        canonical_key: key,
    })
}

/// Текстовое поле: не пустое, без управляющих символов (кроме пробела),
/// в пределах лимита длины.
fn validate_text(field: &'static str, value: &str, max: usize) -> Result<(), CatalogError> {
    if value.trim().is_empty() {
        return Err(CatalogError::EmptyTitle { field });
    }
    if value.chars().any(char::is_control) {
        return Err(CatalogError::ControlChars { field });
    }
    if value.chars().count() > max {
        return Err(CatalogError::FieldTooLong {
            field,
            actual: value.chars().count(),
            max,
        });
    }
    Ok(())
}

/// ISRC: ровно 12 буквенно-цифровых символов верхнего регистра
/// (перепроверяется MusicBrainz на infra-уровне, техкарта Часть VI).
fn validate_isrc(isrc: &str) -> Result<(), CatalogError> {
    let valid = isrc.chars().count() == MAX_ISRC_LEN
        && isrc
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit());
    if valid {
        Ok(())
    } else {
        Err(CatalogError::InvalidIsrc {
            value: isrc.to_owned(),
        })
    }
}

/// Media-URL: scheme строго https, домен — в whitelist источника.
/// Host выделяется вручную, без url-крейта: whitelist-проверке достаточно
/// префикса до первого `/`, `?`, `#`; userinfo через `@` отклоняется
/// как попытка подмены host.
fn check_media_url(field: &'static str, url: &str, source: Source) -> Result<(), CatalogError> {
    let (scheme, rest) = url.split_once("://").ok_or(CatalogError::InvalidScheme {
        field,
        found: "missing".to_owned(),
    })?;
    if !scheme.eq_ignore_ascii_case("https") {
        return Err(CatalogError::InvalidScheme {
            field,
            found: scheme.to_owned(),
        });
    }

    let authority = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    if authority.contains('@') {
        return Err(CatalogError::InvalidDomain {
            field,
            host: authority.to_owned(),
            catalog_source: source,
        });
    }
    let host = authority
        .split(':')
        .next()
        .unwrap_or(authority)
        .to_lowercase();

    let allowed = source
        .media_domains()
        .iter()
        .any(|base| domain_allowed(&host, base));
    if !allowed {
        return Err(CatalogError::InvalidDomain {
            field,
            host,
            catalog_source: source,
        });
    }
    Ok(())
}

/// `host` — это `base` или его поддомен (не далее одного уровня до
/// произвольной глубины, но без пустых меток вроде `a..base`).
fn domain_allowed(host: &str, base: &str) -> bool {
    if host == base {
        return true;
    }
    let suffix = format!(".{base}");
    match host.strip_suffix(&suffix) {
        Some(label) => !label.is_empty() && !label.ends_with('.'),
        None => false,
    }
}

/// Счётчик «той же песни»: инкремент и дедупликация по canonical-ключу
/// `source:source_id`, НЕ по `id` строки track_cache (техкарта,
/// Часть II «КАТАЛОГ»). BTreeMap даёт стабильный порядок ключей —
/// удобно для сортировки hot и интеграционных тестов.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TrackCounter {
    counts: BTreeMap<String, u64>,
}

impl TrackCounter {
    /// Инкрементирует счётчик трека с данным canonical-ключом,
    /// возвращает новое значение (1 = первая песня).
    pub fn increment(&mut self, key: &str) -> u64 {
        let entry = self.counts.entry(key.to_owned()).or_insert(0);
        *entry += 1;
        *entry
    }

    /// Текущий счётчик трека с данным canonical-ключом (0 = не встречался).
    pub fn count(&self, key: &str) -> u64 {
        self.counts.get(key).copied().unwrap_or(0)
    }

    /// Число различных песен (различных canonical-ключей).
    pub fn distinct_tracks(&self) -> usize {
        self.counts.len()
    }

    /// Суммарное число инкрементов (всего «песен» с повторами).
    pub fn total(&self) -> u64 {
        self.counts.values().sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn itunes_track() -> CatalogTrack {
        CatalogTrack {
            source: Source::Itunes,
            source_id: "12345".to_owned(),
            isrc: Some("USUM71302116".to_owned()),
            title: "Vivir Mi Vida".to_owned(),
            artist: "Marc Anthony".to_owned(),
            cover_url: Some("https://is1-ssl.mzstatic.com/image/thumb.jpg".to_owned()),
            preview_url: Some("https://audio-ssl.mzstatic.com/preview.m4a".to_owned()),
        }
    }

    #[test]
    fn valid_itunes_track() {
        let validated = validate_track(&itunes_track()).expect("valid itunes track");
        assert_eq!(validated.canonical_key, "itunes:12345");
        assert_eq!(validated.track.title, "Vivir Mi Vida");
    }

    #[test]
    fn valid_deezer_track() {
        let track = CatalogTrack {
            source: Source::Deezer,
            source_id: "98765".to_owned(),
            isrc: None,
            title: "Todo Cambió".to_owned(),
            artist: "Camila".to_owned(),
            cover_url: Some("https://api.deezer.com/cover/98765.jpg".to_owned()),
            preview_url: Some("https://cdns-preview-7.dzcdn.net/stream.mp3".to_owned()),
        };
        // Спецификация техкарты: для Deezer разрешены только *.deezer.com
        // (включая cdns-preview-*.deezer.com) — dzcdn.net не в whitelist.
        assert!(matches!(
            validate_track(&track),
            Err(CatalogError::InvalidDomain { .. })
        ));

        let track = CatalogTrack {
            preview_url: Some("https://cdns-preview-7.deezer.com/stream.mp3".to_owned()),
            ..track
        };
        let validated = validate_track(&track).expect("valid deezer track");
        assert_eq!(validated.canonical_key, "deezer:98765");
    }

    #[test]
    fn rejects_foreign_preview_domain() {
        let mut track = itunes_track();
        track.preview_url = Some("https://evil.example.com/preview.mp3".to_owned());
        assert_eq!(
            validate_track(&track),
            Err(CatalogError::InvalidDomain {
                field: "preview_url",
                host: "evil.example.com".to_owned(),
                catalog_source: Source::Itunes,
            })
        );
    }

    #[test]
    fn rejects_cross_source_domain() {
        // iTunes-трек не может ссылаться на домен Deezer и наоборот.
        let mut track = itunes_track();
        track.preview_url = Some("https://cdns-preview-7.deezer.com/stream.mp3".to_owned());
        assert!(matches!(
            validate_track(&track),
            Err(CatalogError::InvalidDomain { .. })
        ));
    }

    #[test]
    fn rejects_http_scheme() {
        let mut track = itunes_track();
        track.cover_url = Some("http://is1-ssl.mzstatic.com/image/thumb.jpg".to_owned());
        assert_eq!(
            validate_track(&track),
            Err(CatalogError::InvalidScheme {
                field: "cover_url",
                found: "http".to_owned(),
            })
        );

        // URL вообще без scheme тоже отклоняется.
        track.cover_url = Some("is1-ssl.mzstatic.com/image/thumb.jpg".to_owned());
        assert!(matches!(
            validate_track(&track),
            Err(CatalogError::InvalidScheme { .. })
        ));
    }

    #[test]
    fn rejects_too_long_title() {
        let mut track = itunes_track();
        track.title = "a".repeat(MAX_TITLE_LEN + 1);
        assert_eq!(
            validate_track(&track),
            Err(CatalogError::FieldTooLong {
                field: "title",
                actual: MAX_TITLE_LEN + 1,
                max: MAX_TITLE_LEN,
            })
        );
    }

    #[test]
    fn rejects_empty_title() {
        let mut track = itunes_track();
        track.title = "   ".to_owned();
        assert_eq!(
            validate_track(&track),
            Err(CatalogError::EmptyTitle { field: "title" })
        );

        let mut track = itunes_track();
        track.artist = String::new();
        assert_eq!(
            validate_track(&track),
            Err(CatalogError::EmptyTitle { field: "artist" })
        );
    }

    #[test]
    fn rejects_control_chars() {
        let mut track = itunes_track();
        track.title = "Vivir\u{0007} Mi Vida".to_owned();
        assert_eq!(
            validate_track(&track),
            Err(CatalogError::ControlChars { field: "title" })
        );

        let mut track = itunes_track();
        track.artist = "Marc\nAnthony".to_owned();
        assert_eq!(
            validate_track(&track),
            Err(CatalogError::ControlChars { field: "artist" })
        );
    }

    #[test]
    fn rejects_invalid_isrc() {
        let mut track = itunes_track();
        track.isrc = Some("usum71302116".to_owned()); // нижний регистр
        assert_eq!(
            validate_track(&track),
            Err(CatalogError::InvalidIsrc {
                value: "usum71302116".to_owned()
            })
        );

        let mut track = itunes_track();
        track.isrc = Some("USUM7130211".to_owned()); // 11 символов
        assert!(matches!(
            validate_track(&track),
            Err(CatalogError::InvalidIsrc { .. })
        ));

        // None — валидно: ISRC опционален, обогащает MusicBrainz позже.
        let mut track = itunes_track();
        track.isrc = None;
        assert!(validate_track(&track).is_ok());
    }

    #[test]
    fn accepts_valid_isrc() {
        let track = CatalogTrack {
            isrc: Some("USUM71302116".to_owned()),
            ..itunes_track()
        };
        let validated = validate_track(&track).expect("valid isrc");
        assert_eq!(validated.track.isrc.as_deref(), Some("USUM71302116"));
    }

    #[test]
    fn canonical_key_is_stable_across_variants() {
        // Пробелы по краям source_id и регистр имени источника
        // не меняют ключ — дедупликация не зависит от оформления.
        let a = canonical_key(Source::Itunes, "12345").unwrap();
        let b = canonical_key(Source::Itunes, "  12345  ").unwrap();
        let c: Source = "ITUNES".parse().unwrap();
        let d = canonical_key(c, "12345").unwrap();
        assert_eq!(a, "itunes:12345");
        assert_eq!(a, b);
        assert_eq!(a, d);

        assert_eq!("musicbrainz".parse::<Source>(), Ok(Source::Musicbrainz));
        assert_eq!(Source::Deezer.to_string(), "deezer");
    }

    #[test]
    fn rejects_bad_source_id() {
        assert!(matches!(
            canonical_key(Source::Itunes, "   "),
            Err(CatalogError::InvalidSourceId { .. })
        ));
        assert!(matches!(
            canonical_key(Source::Itunes, &"x".repeat(MAX_SOURCE_ID_LEN + 1)),
            Err(CatalogError::InvalidSourceId { .. })
        ));
        assert!(matches!(
            canonical_key(Source::Itunes, "12\u{0000}45"),
            Err(CatalogError::ControlChars { field: "source_id" })
        ));
    }

    #[test]
    fn musicbrainz_has_no_media_urls() {
        let track = CatalogTrack {
            source: Source::Musicbrainz,
            source_id: "b10bbbfc-cf9e-42e0-be17-e2c3e1d2600d".to_owned(),
            isrc: Some("USUM71302116".to_owned()),
            title: "Vivir Mi Vida".to_owned(),
            artist: "Marc Anthony".to_owned(),
            cover_url: None,
            preview_url: None,
        };
        let validated = validate_track(&track).expect("musicbrainz without media urls");
        assert!(validated.canonical_key.starts_with("musicbrainz:"));

        let track = CatalogTrack {
            cover_url: Some("https://coverart.example.com/1.jpg".to_owned()),
            ..track
        };
        assert!(matches!(
            validate_track(&track),
            Err(CatalogError::InvalidDomain { .. })
        ));
    }

    #[test]
    fn track_counter_counts_by_canonical_key() {
        // Одна и та же песня — один canonical-ключ, счётчик растёт;
        // другой id строки (или другой источник) — другой ключ.
        let mut counter = TrackCounter::default();
        assert_eq!(counter.increment("itunes:12345"), 1);
        assert_eq!(counter.increment("itunes:12345"), 2);
        assert_eq!(counter.increment("deezer:98765"), 1);
        assert_eq!(counter.count("itunes:12345"), 2);
        assert_eq!(counter.count("itunes:99999"), 0);
        assert_eq!(counter.distinct_tracks(), 2);
        assert_eq!(counter.total(), 3);
    }

    #[test]
    fn serializes_snake_case() {
        let json = serde_json::to_string(&itunes_track()).unwrap();
        for field in [
            "\"source_id\"",
            "\"cover_url\"",
            "\"preview_url\"",
            "\"title\"",
        ] {
            assert!(json.contains(field), "missing field {field} in {json}");
        }
        assert!(json.contains("\"source\":\"itunes\""));

        let roundtrip: CatalogTrack = serde_json::from_str(&json).unwrap();
        assert_eq!(roundtrip, itunes_track());
    }
}
