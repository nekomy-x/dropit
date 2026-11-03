//! API-ключи публичного Developer API v1 (ADR-010).
//!
//! Здесь только чистая логика без БД и HTTP: формат ключа, хеширование,
//! скоупы (что именно ключ разрешает), allowlist IP и лимиты. Проверка
//! секрета — в постоянном времени; в базе хранится только
//! `sha256(plaintext)`, поэтому утечка БД не даёт рабочих ключей.
//!
//! Формат ключа: `dit_<live|test>_<prefix>_<secret>`, где prefix — 10
//! hex-символов (публичный идентификатор для поиска и логов), secret —
//! 32 случайных байта в hex (256 бит энтропии).

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Строковый префикс всех ключей DROP IT.
pub const KEY_SCHEME: &str = "dit";
/// Длина публичного prefix в hex-символах (40 бит).
pub const PREFIX_LEN: usize = 10;
/// Длина секрета в hex-символах (32 байта = 256 бит).
pub const SECRET_HEX_LEN: usize = 64;
/// Сколько активных ключей может быть у одного клуба.
pub const MAX_ACTIVE_KEYS_PER_CLUB: i64 = 25;
/// Дефолтный лимит запросов на ключ в минуту.
pub const DEFAULT_RATE_LIMIT_PER_MIN: i32 = 120;
/// Нижняя граница настраиваемого лимита.
pub const MIN_RATE_LIMIT_PER_MIN: i32 = 30;
/// Верхняя граница настраиваемого лимита.
pub const MAX_RATE_LIMIT_PER_MIN: i32 = 600;
/// Сколько дней хранится журнал вызовов (`api_key_usage`).
pub const USAGE_RETENTION_DAYS: i64 = 30;
/// Дефолтное «окно перекрытия» при ротации ключа, часов.
pub const DEFAULT_ROTATION_GRACE_HOURS: i64 = 24;
/// Максимальное окно перекрытия при ротации.
pub const MAX_ROTATION_GRACE_HOURS: i64 = 72;

/// Окружение ключа: боевой или тестовый.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Environment {
    Live,
    Test,
}

impl Environment {
    pub fn as_str(&self) -> &'static str {
        match self {
            Environment::Live => "live",
            Environment::Test => "test",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "live" => Some(Environment::Live),
            "test" => Some(Environment::Test),
            _ => None,
        }
    }
}

/// Разрешение ключа. Никакого wildcard: всё, что не перечислено, запрещено.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Scope {
    #[serde(rename = "docs:read")]
    DocsRead,
    #[serde(rename = "profile:read")]
    ProfileRead,
    #[serde(rename = "sessions:read")]
    SessionsRead,
    #[serde(rename = "sessions:write")]
    SessionsWrite,
    #[serde(rename = "qr:write")]
    QrWrite,
    #[serde(rename = "requests:read")]
    RequestsRead,
    #[serde(rename = "requests:write")]
    RequestsWrite,
    #[serde(rename = "catalog:read")]
    CatalogRead,
    #[serde(rename = "stats:read")]
    StatsRead,
}

impl Scope {
    /// Полный каталог в стабильном порядке (он же порядок в UI и в ответах).
    pub const ALL: [Scope; 9] = [
        Scope::DocsRead,
        Scope::ProfileRead,
        Scope::SessionsRead,
        Scope::SessionsWrite,
        Scope::QrWrite,
        Scope::RequestsRead,
        Scope::RequestsWrite,
        Scope::CatalogRead,
        Scope::StatsRead,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            Scope::DocsRead => "docs:read",
            Scope::ProfileRead => "profile:read",
            Scope::SessionsRead => "sessions:read",
            Scope::SessionsWrite => "sessions:write",
            Scope::QrWrite => "qr:write",
            Scope::RequestsRead => "requests:read",
            Scope::RequestsWrite => "requests:write",
            Scope::CatalogRead => "catalog:read",
            Scope::StatsRead => "stats:read",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Scope::ALL.into_iter().find(|s| s.as_str() == value)
    }
}

/// Описание скоупа для панели и документации.
#[derive(Debug, Clone, Copy)]
pub struct ScopeInfo {
    pub scope: Scope,
    pub title: &'static str,
    pub description: &'static str,
    /// Группа в чек-листе: Documentation / Club / Live room / Orders / Catalog / Analytics.
    pub group: &'static str,
    /// `read`, `write` или `sensitive`.
    pub risk: &'static str,
    /// Секция документации, где скоуп описан.
    pub docs_section: &'static str,
}

/// Каталог скоупов — единственный источник правды для UI, доков и тестов.
pub const SCOPE_CATALOGUE: [ScopeInfo; 9] = [
    ScopeInfo {
        scope: Scope::DocsRead,
        title: "Documentation",
        description: "Read the developer documentation, guides and this API reference.",
        group: "Documentation",
        risk: "read",
        docs_section: "scopes",
    },
    ScopeInfo {
        scope: Scope::ProfileRead,
        title: "Club profile",
        description: "Read the club profile: name, address, contact, timezone, status.",
        group: "Club",
        risk: "read",
        docs_section: "profile",
    },
    ScopeInfo {
        scope: Scope::SessionsRead,
        title: "Sessions — read",
        description: "List and inspect the club's sessions and their state.",
        group: "Live room",
        risk: "read",
        docs_section: "sessions",
    },
    ScopeInfo {
        scope: Scope::SessionsWrite,
        title: "Sessions — write",
        description: "Create a session and close it. Opening for requests stays with the DJ.",
        group: "Live room",
        risk: "write",
        docs_section: "sessions",
    },
    ScopeInfo {
        scope: Scope::QrWrite,
        title: "Guest entry QR",
        description: "Issue or rotate the session QR token that lets guests join. Sensitive: it opens the room.",
        group: "Live room",
        risk: "sensitive",
        docs_section: "sessions",
    },
    ScopeInfo {
        scope: Scope::RequestsRead,
        title: "Requests — read",
        description: "Read the session queue, individual requests and club request history.",
        group: "Orders",
        risk: "read",
        docs_section: "requests",
    },
    ScopeInfo {
        scope: Scope::RequestsWrite,
        title: "Requests — write",
        description: "Place a music request and accept, reject or mark it played.",
        group: "Orders",
        risk: "write",
        docs_section: "requests",
    },
    ScopeInfo {
        scope: Scope::CatalogRead,
        title: "Catalog search",
        description: "Search the track catalog used by the request flow.",
        group: "Catalog",
        risk: "read",
        docs_section: "catalog",
    },
    ScopeInfo {
        scope: Scope::StatsRead,
        title: "Nightly numbers",
        description: "Aggregated requests, revenue and top tracks for a period.",
        group: "Analytics",
        risk: "read",
        docs_section: "stats",
    },
];

impl ScopeInfo {
    pub fn for_scope(scope: Scope) -> Option<&'static ScopeInfo> {
        SCOPE_CATALOGUE.iter().find(|info| info.scope == scope)
    }
}

/// Пресет выбора скоупов в панели.
#[derive(Debug, Clone, Copy)]
pub struct ScopePreset {
    pub id: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub scopes: &'static [Scope],
}

pub const SCOPE_PRESETS: [ScopePreset; 4] = [
    ScopePreset {
        id: "docs-only",
        title: "Documentation only",
        description: "Read the guides and reference. No club data at all.",
        scopes: &[Scope::DocsRead],
    },
    ScopePreset {
        id: "read-only",
        title: "Read-only integration",
        description: "Read profile, sessions, requests, catalog and nightly numbers.",
        scopes: &[
            Scope::DocsRead,
            Scope::ProfileRead,
            Scope::SessionsRead,
            Scope::RequestsRead,
            Scope::CatalogRead,
            Scope::StatsRead,
        ],
    },
    ScopePreset {
        id: "queue-automation",
        title: "Queue automation",
        description: "Read-only plus placing and moderating music requests.",
        scopes: &[
            Scope::DocsRead,
            Scope::ProfileRead,
            Scope::SessionsRead,
            Scope::RequestsRead,
            Scope::CatalogRead,
            Scope::StatsRead,
            Scope::RequestsWrite,
        ],
    },
    ScopePreset {
        id: "full-automation",
        title: "Full automation",
        description: "Everything: sessions, QR, requests, catalog, numbers.",
        scopes: &[
            Scope::DocsRead,
            Scope::ProfileRead,
            Scope::SessionsRead,
            Scope::SessionsWrite,
            Scope::QrWrite,
            Scope::RequestsRead,
            Scope::RequestsWrite,
            Scope::CatalogRead,
            Scope::StatsRead,
        ],
    },
];

/// Имеет ли набор скоупов нужное разрешение.
pub fn scopes_allow(granted: &[Scope], required: Scope) -> bool {
    granted.contains(&required)
}

/// Ошибка разбора набора скоупов (неизвестный скоуп / пустой список).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ScopeError {
    #[error("unknown scope: {0}")]
    Unknown(String),
    #[error("at least one scope is required")]
    Empty,
}

/// Разбирает строковые скоупы, убирает дубли и упорядочивает по каталогу.
pub fn normalize_scopes(values: &[String]) -> Result<Vec<Scope>, ScopeError> {
    if values.is_empty() {
        return Err(ScopeError::Empty);
    }
    let mut parsed = Vec::with_capacity(values.len());
    for value in values {
        let scope = Scope::parse(value).ok_or_else(|| ScopeError::Unknown(value.clone()))?;
        if !parsed.contains(&scope) {
            parsed.push(scope);
        }
    }
    parsed.sort();
    Ok(parsed)
}

/// Строковое представление набора скоупов (для БД и JSON).
pub fn scopes_to_strings(scopes: &[Scope]) -> Vec<String> {
    scopes.iter().map(|s| s.as_str().to_string()).collect()
}

/// Разобранный ключ: окружение и публичный prefix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedApiKey {
    pub environment: Environment,
    pub prefix: String,
}

/// Сгенерированный ключ: plaintext показывается клиенту один раз.
#[derive(Debug, Clone)]
pub struct GeneratedApiKey {
    pub plaintext: String,
    pub prefix: String,
    pub hash: String,
    pub environment: Environment,
}

/// Генерирует новый ключ: 40-битный prefix + 256-битный секрет.
pub fn generate_api_key(environment: Environment) -> GeneratedApiKey {
    use rand::RngCore;
    let mut prefix_bytes = [0u8; 5];
    rand::thread_rng().fill_bytes(&mut prefix_bytes);
    let mut secret_bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut secret_bytes);
    let prefix = hex::encode(prefix_bytes);
    let secret = hex::encode(secret_bytes);
    let plaintext = format!(
        "{KEY_SCHEME}_{}_{}_{}",
        environment.as_str(),
        prefix,
        secret
    );
    let hash = hash_api_key(&plaintext);
    GeneratedApiKey {
        plaintext,
        prefix,
        hash,
        environment,
    }
}

/// Разбирает формат ключа без обращения к БД.
pub fn parse_api_key(key: &str) -> Option<ParsedApiKey> {
    let key = key.trim();
    let mut parts = key.split('_');
    let scheme = parts.next()?;
    let env = parts.next()?;
    let prefix = parts.next()?;
    let secret = parts.next()?;
    if parts.next().is_some() || scheme != KEY_SCHEME {
        return None;
    }
    let environment = Environment::parse(env)?;
    if prefix.len() != PREFIX_LEN || !is_lower_hex(prefix) {
        return None;
    }
    if secret.len() != SECRET_HEX_LEN || !is_lower_hex(secret) {
        return None;
    }
    Some(ParsedApiKey {
        environment,
        prefix: prefix.to_string(),
    })
}

/// sha256(plaintext) в hex — единственное, что хранится в БД.
pub fn hash_api_key(key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(key.trim().as_bytes());
    hex::encode(hasher.finalize())
}

/// Сравнение хешей в постоянном времени (одинаковая длина → без раннего выхода).
pub fn secret_eq(left: &str, right: &str) -> bool {
    let a = left.as_bytes();
    let b = right.as_bytes();
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

fn is_lower_hex(value: &str) -> bool {
    value
        .bytes()
        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Ошибка allowlist IP (невалидное правило или пустая строка).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IpRuleError {
    #[error("invalid IP or CIDR rule: {0}")]
    Invalid(String),
}

/// Проверяет правила allowlist на этапе создания/изменения ключа.
/// Поддерживаются точные IP и CIDR для IPv4 (0–32) и IPv6 (0–128).
pub fn validate_ip_rules(rules: &[String]) -> Result<(), IpRuleError> {
    for rule in rules {
        if parse_ip_rule(rule).is_none() {
            return Err(IpRuleError::Invalid(rule.clone()));
        }
    }
    Ok(())
}

/// Разрешён ли IP. Пустой allowlist — разрешено всё; невалидное правило —
/// запрет (fail closed), потому что правило могло быть испорчено в БД.
pub fn ip_allowed(rules: &[String], ip: IpAddr) -> bool {
    if rules.is_empty() {
        return true;
    }
    for rule in rules {
        let Some((network, prefix)) = parse_ip_rule(rule) else {
            return false;
        };
        if ip_in_network(network, prefix, ip) {
            return true;
        }
    }
    false
}

fn parse_ip_rule(rule: &str) -> Option<(IpAddr, u8)> {
    let rule = rule.trim();
    if rule.is_empty() {
        return None;
    }
    let (addr, prefix) = match rule.split_once('/') {
        Some((addr, prefix)) => (addr.trim(), Some(prefix.trim())),
        None => (rule, None),
    };
    let ip: IpAddr = addr.parse().ok()?;
    let max = match ip {
        IpAddr::V4(_) => 32,
        IpAddr::V6(_) => 128,
    };
    let prefix = match prefix {
        Some(value) => value.parse::<u8>().ok()?,
        None => max,
    };
    if prefix > max {
        return None;
    }
    Some((ip, prefix))
}

fn ip_in_network(network: IpAddr, prefix: u8, candidate: IpAddr) -> bool {
    match (network, candidate) {
        (IpAddr::V4(net), IpAddr::V4(ip)) => {
            mask_v4(u32::from(net), prefix) == mask_v4(u32::from(ip), prefix)
        }
        (IpAddr::V6(net), IpAddr::V6(ip)) => {
            mask_v6(u128::from(net), prefix) == mask_v6(u128::from(ip), prefix)
        }
        _ => false,
    }
}

fn mask_v4(value: u32, prefix: u8) -> u32 {
    if prefix == 0 {
        0
    } else {
        value & (u32::MAX << (32 - prefix))
    }
}

fn mask_v6(value: u128, prefix: u8) -> u128 {
    if prefix == 0 {
        0
    } else if prefix == 128 {
        value
    } else {
        value & (u128::MAX << (128 - prefix))
    }
}

/// Человекочитаемое имя сети для документации (например, `203.0.113.0/24`).
pub fn describe_network(rule: &str) -> Option<String> {
    parse_ip_rule(rule).map(|(ip, prefix)| format!("{ip}/{prefix}"))
}

/// Сеть как `Ipv4Addr` (только для тестов документации).
#[allow(dead_code)]
pub(crate) fn network_v4(rule: &str) -> Option<(Ipv4Addr, u8)> {
    match parse_ip_rule(rule)? {
        (IpAddr::V4(ip), prefix) => Some((ip, prefix)),
        _ => None,
    }
}

/// Сеть как `Ipv6Addr` (только для тестов документации).
#[allow(dead_code)]
pub(crate) fn network_v6(rule: &str) -> Option<(Ipv6Addr, u8)> {
    match parse_ip_rule(rule)? {
        (IpAddr::V6(ip), prefix) => Some((ip, prefix)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_roundtrip() {
        for scope in Scope::ALL {
            assert_eq!(Scope::parse(scope.as_str()), Some(scope));
            let json = serde_json::to_string(&scope).unwrap();
            assert_eq!(json, format!("\"{}\"", scope.as_str()));
            let back: Scope = serde_json::from_str(&json).unwrap();
            assert_eq!(back, scope);
        }
    }

    #[test]
    fn catalogue_covers_every_scope_once() {
        assert_eq!(SCOPE_CATALOGUE.len(), Scope::ALL.len());
        for scope in Scope::ALL {
            let count = SCOPE_CATALOGUE.iter().filter(|i| i.scope == scope).count();
            assert_eq!(count, 1, "{} must be described once", scope.as_str());
        }
        for preset in SCOPE_PRESETS {
            for scope in preset.scopes {
                assert!(Scope::ALL.contains(scope), "preset has unknown scope");
            }
        }
    }

    #[test]
    fn key_format_roundtrip() {
        let generated = generate_api_key(Environment::Live);
        assert!(generated.plaintext.starts_with("dit_live_"));
        let parsed = parse_api_key(&generated.plaintext).expect("parse generated key");
        assert_eq!(parsed.environment, Environment::Live);
        assert_eq!(parsed.prefix, generated.prefix);
        assert_eq!(generated.prefix.len(), PREFIX_LEN);
        assert_eq!(generated.hash, hash_api_key(&generated.plaintext));
        assert_eq!(generated.hash.len(), 64);
    }

    #[test]
    fn malformed_keys_are_rejected() {
        for bad in [
            "",
            "dit_live",
            "dit_live_abc_",
            "dit_live_zzzzzzzzzz_0000000000000000000000000000000000000000000000000000000000000000",
            "dit_prod_0123456789_0000000000000000000000000000000000000000000000000000000000000000",
            "xyz_live_0123456789_0000000000000000000000000000000000000000000000000000000000000000",
            "dit_live_0123456789_00000000000000000000000000000000000000000000000000000000000000G0",
        ] {
            assert!(parse_api_key(bad).is_none(), "must reject {bad:?}");
        }
    }

    #[test]
    fn hashes_and_comparison() {
        let key =
            "dit_live_0123456789_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let hash = hash_api_key(key);
        assert_eq!(hash, hash_api_key(key));
        assert!(secret_eq(&hash, &hash));
        assert!(!secret_eq(&hash, &hash_api_key("other")));
        assert!(!secret_eq("abc", "ab"));
    }

    #[test]
    fn normalize_scopes_dedupes_and_rejects_unknown() {
        let scopes = normalize_scopes(&[
            "requests:read".into(),
            "docs:read".into(),
            "requests:read".into(),
        ])
        .unwrap();
        assert_eq!(scopes, vec![Scope::DocsRead, Scope::RequestsRead]);
        assert_eq!(
            normalize_scopes(&["admin:*".into()]),
            Err(ScopeError::Unknown("admin:*".into()))
        );
        assert_eq!(normalize_scopes(&[]), Err(ScopeError::Empty));
    }

    #[test]
    fn ip_allowlist_matches_v4_and_v6() {
        let rules = vec!["203.0.113.0/24".to_string(), "198.51.100.7".to_string()];
        assert!(ip_allowed(&rules, "203.0.113.9".parse().unwrap()));
        assert!(ip_allowed(&rules, "198.51.100.7".parse().unwrap()));
        assert!(!ip_allowed(&rules, "198.51.100.8".parse().unwrap()));
        assert!(!ip_allowed(&rules, "203.0.114.1".parse().unwrap()));
        // IPv6 network and exact host.
        let v6 = vec!["2001:db8::/32".to_string()];
        assert!(ip_allowed(&v6, "2001:db8:1::5".parse().unwrap()));
        assert!(!ip_allowed(&v6, "2001:db9::5".parse().unwrap()));
        // Empty allowlist = everything.
        assert!(ip_allowed(&[], "10.0.0.1".parse().unwrap()));
        // Fail closed on a corrupt rule.
        assert!(!ip_allowed(
            &["not-an-ip".to_string()],
            "10.0.0.1".parse().unwrap()
        ));
    }

    #[test]
    fn ip_rule_validation() {
        assert!(validate_ip_rules(&["203.0.113.0/24".into(), "::1".into()]).is_ok());
        assert!(validate_ip_rules(&["203.0.113.0/33".into()]).is_err());
        assert!(validate_ip_rules(&["".into()]).is_err());
    }
}
