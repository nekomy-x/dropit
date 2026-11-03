//! Контрактный тест: синхронизация `docs/openapi.yaml` с кодом роутов.
//!
//! Integration-тест библиотеки `dropit-gateway-api`. Не требует БД и сети:
//! читает спецификацию и исходники как текст, нормализует axum-синтаксис
//! путей (`:id` → `{id}`) и сверяет множество путей/методов.
//!
//! Проверки:
//!   (а) каждый путь из кода описан в OpenAPI;
//!   (б) каждый путь из OpenAPI существует в коде (иначе мёртвый контракт);
//!   (в) для `/v1/**` размечены `security` и хотя бы один `tags`;
//!   (г) ни одна операция не осталась без `responses`.
//!
//! health-эндпоинты (`/healthz`, `/readyz`) явно допущены без security.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;

use serde_json::Value;

/// HTTP-методы, которые axum регистрирует в `.route(...)`.
const METHODS: &[&str] = &["get", "post", "patch", "put", "delete", "head", "options"];

/// Пути, для которых `security` не требуется (публичные health- и webhook-роуты).
/// healthz/readyz — liveness/readness без auth; вебхуки Stripe подписываются
/// сами (Stripe-Signature), а не bearer-токеном; WS-апгрейд ходит по билету.
const ALLOW_NO_SECURITY: &[&str] = &[
    "/healthz",
    "/readyz",
    // Режим эксплуатации стенда для нижнего статус-бара панелей:
    // публичный, без авторизации (те же факты, что у /readyz).
    "/v1/status",
    // Публичная статус-страница сервисов: без авторизации по определению.
    "/v1/status/services",
    "/v1/stripe/webhook",
    "/v1/stripe/transfer-webhook",
    "/v1/ws",
    // Resend подписывает события Svix-подписью (`svix-*`), а не bearer-токеном:
    // доверие даёт HMAC над сырым телом, поэтому security здесь не нужен.
    "/webhooks/resend",
];

/// Абсолютный путь к `docs/openapi.yaml` относительно `CARGO_MANIFEST_DIR`
/// (`crates/gateway-api` → `../../docs/openapi.yaml`).
///
/// Второй контракт — публичный Developer API v1 (`/v1/developer/**`,
/// ADR-010). Он живёт отдельным файлом, потому что это другая аудитория
/// и другой способ аутентификации; тест сводит оба множества путей, чтобы
/// ни один не мог отстать от кода.
fn openapi_path() -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    dir.join("../../docs/openapi.yaml")
}

fn developer_openapi_path() -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    dir.join("../../docs/api/openapi-developer.yaml")
}

/// Каталог `src` крейта (`crates/gateway-api/src`).
fn src_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src")
}

/// Разбор OpenAPI-документа из текста в `serde_json::Value`.
fn load_openapi() -> Value {
    let raw = fs::read_to_string(openapi_path())
        .unwrap_or_else(|e| panic!("не удалось прочитать {}: {e}", openapi_path().display()));
    serde_yaml::from_str::<Value>(&raw)
        .unwrap_or_else(|e| panic!("docs/openapi.yaml не валидный YAML: {e}"))
}

/// Разбор публичного контракта Developer API v1.
fn load_developer_openapi() -> Value {
    let path = developer_openapi_path();
    let raw = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("не удалось прочитать {}: {e}", path.display()));
    serde_yaml::from_str::<Value>(&raw)
        .unwrap_or_else(|e| panic!("docs/api/openapi-developer.yaml не валидный YAML: {e}"))
}

/// Извлекает из текста исходников все регистрации `.route("path", method(...))`
/// (включая многострочные) и нормализует axum-синтаксис `:param` → `{param}`.
///
/// Возвращает множество (path, method) — по одной записи на каждый метод.
///
/// Разбор ведётся ручным сканером без внешних regex-крейтов: ищем `.route(`,
/// затем первую строку в кавычках (путь), затем цепочку методов до закрывающей
/// скобки `.route(...)`.
fn collect_code_routes() -> BTreeSet<(String, String)> {
    let mut routes = BTreeSet::new();
    let src = src_dir();
    let entries = fs::read_dir(&src)
        .unwrap_or_else(|e| panic!("не удалось прочитать каталог {}: {e}", src.display()));
    // Один общий текст с разделителями — многострочные `.route(` могут
    // захватывать перевод строки между `.route(` и путём.
    let mut joined = String::new();
    for entry in entries {
        let path = entry.expect("read_dir entry").path();
        if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            joined.push_str(
                &fs::read_to_string(&path)
                    .unwrap_or_else(|e| panic!("не удалось прочитать {}: {e}", path.display())),
            );
            joined.push('\n');
        }
    }

    let bytes = joined.as_bytes();
    let mut i = 0;
    while i + 7 < bytes.len() {
        // Искать вхождение ".route(".
        if &bytes[i..i + 7] == b".route(" {
            // Пропустить пробелы/переводы строк после открывающей скобки.
            let mut j = i + 7;
            while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                j += 1;
            }
            // Ожидаем открывающую кавычку пути.
            if j >= bytes.len() || bytes[j] != b'"' {
                i += 7;
                continue;
            }
            let path_start = j + 1;
            let mut k = path_start;
            while k < bytes.len() && bytes[k] != b'"' {
                k += 1;
            }
            if k >= bytes.len() {
                i += 7;
                continue;
            }
            let raw_path = &joined[path_start..k];
            // После закрывающей кавычки пути — запятая, затем цепочка методов.
            let mut m = k + 1;
            while m < bytes.len() && (bytes[m].is_ascii_whitespace() || bytes[m] == b',') {
                m += 1;
            }
            // Цепочка методов до закрывающей `)` этой `.route(...)`-группы.
            // Глубина скобок: начинаем с 1 (уже внутри `.route(`).
            let chain_start = m;
            let mut depth: i32 = 1;
            while m < bytes.len() && depth > 0 {
                match bytes[m] {
                    b'(' => depth += 1,
                    b')' => depth -= 1,
                    _ => {}
                }
                if depth > 0 {
                    m += 1;
                }
            }
            let chain = &joined[chain_start..m];

            let norm = normalize_path(raw_path);
            for method in METHODS {
                let needle = format!("{method}(");
                if chain.contains(&needle) {
                    routes.insert((norm.clone(), method.to_string()));
                }
            }
            i = m;
        } else {
            i += 1;
        }
    }
    routes
}

/// `:param` → `{param}` (повторно, для путей с несколькими параметрами).
fn normalize_path(p: &str) -> String {
    let mut out = String::with_capacity(p.len());
    let bytes = p.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b':' {
            // Имя параметра — последовательность букв/цифр/_.
            let mut j = i + 1;
            while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                j += 1;
            }
            if j > i + 1 {
                out.push('{');
                out.push_str(&p[i + 1..j]);
                out.push('}');
                i = j;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

/// Объединение маршрутов обоих контрактов (панель + Developer API v1).
fn collect_all_spec_routes() -> BTreeSet<(String, String)> {
    let mut routes = collect_spec_routes(&load_openapi());
    routes.extend(collect_spec_routes(&load_developer_openapi()));
    routes
}

/// Извлекает множество (path, method) из OpenAPI-документа.
fn collect_spec_routes(doc: &Value) -> BTreeSet<(String, String)> {
    let mut routes = BTreeSet::new();
    let Some(paths) = doc.get("paths").and_then(|v| v.as_object()) else {
        panic!("openapi: отсутствует paths");
    };
    for (path, item) in paths {
        let Some(obj) = item.as_object() else {
            continue;
        };
        for method in METHODS {
            if obj.contains_key(*method) {
                routes.insert((path.clone(), method.to_string()));
            }
        }
    }
    routes
}

#[test]
fn openapi_yaml_is_valid() {
    let doc = load_openapi();
    assert!(
        doc.get("openapi").is_some(),
        "openapi: отсутствует поле openapi"
    );
    assert!(
        doc.get("paths")
            .and_then(|v| v.as_object())
            .map(|o| !o.is_empty())
            .unwrap_or(false),
        "openapi: paths пуст"
    );
}

#[test]
fn every_code_route_is_described() {
    let spec = collect_all_spec_routes();
    let code = collect_code_routes();

    let mut missing: Vec<_> = code.difference(&spec).collect();
    missing.sort();
    assert!(
        missing.is_empty(),
        "пути/методы из кода не описаны в OpenAPI:\n{}",
        missing
            .iter()
            .map(|(p, m)| format!("  {} {}", m.to_uppercase(), p))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn no_dead_contract_paths() {
    let spec = collect_all_spec_routes();
    let code = collect_code_routes();

    let mut dead: Vec<_> = spec.difference(&code).collect();
    dead.sort();
    assert!(
        dead.is_empty(),
        "мёртвый контракт: пути/методы в OpenAPI отсутствуют в коде:\n{}",
        dead.iter()
            .map(|(p, m)| format!("  {} {}", m.to_uppercase(), p))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn v1_ops_have_security_and_tags() {
    let doc = load_openapi();
    let allow: BTreeSet<&str> = ALLOW_NO_SECURITY.iter().copied().collect();
    let paths = doc.get("paths").and_then(|v| v.as_object()).expect("paths");
    let mut problems = Vec::new();

    for (path, item) in paths {
        if !path.starts_with("/v1/") {
            continue;
        }
        let Some(obj) = item.as_object() else {
            continue;
        };
        for method in METHODS {
            let Some(op) = obj.get(*method) else { continue };
            let upper = method.to_uppercase();
            // security: либо ключ присутствует (даже пустой []), либо путь в allow-списке.
            let has_security = op.get("security").is_some();
            if !has_security && !allow.contains(path.as_str()) {
                problems.push(format!("{upper} {path}: нет security"));
            }
            // Хотя бы один тег.
            let tags_ok = op
                .get("tags")
                .and_then(|t| t.as_array())
                .is_some_and(|arr| !arr.is_empty());
            if !tags_ok {
                problems.push(format!("{upper} {path}: нет tags"));
            }
            // responses обязателен.
            if op
                .get("responses")
                .and_then(|r| r.as_object())
                .map(|o| o.is_empty())
                .unwrap_or(true)
            {
                problems.push(format!("{upper} {path}: нет responses"));
            }
        }
    }

    assert!(
        problems.is_empty(),
        "контрактные нарушения:\n{}",
        problems.join("\n")
    );
}

/// Каталог скоупов Developer API v1 (зеркало `crates/common/src/api_key.rs`
/// `Scope::ALL`). Держим список здесь, чтобы контрактный тест читал его как
/// текст и не тянул зависимость ради девяти строк; расхождение ловит
/// `node scripts/api-docs-check.mjs` и `cargo test -p dropit-common api_key`.
const DEVELOPER_SCOPES: &[&str] = &[
    "docs:read",
    "profile:read",
    "sessions:read",
    "sessions:write",
    "qr:write",
    "requests:read",
    "requests:write",
    "catalog:read",
    "stats:read",
];

/// Публичный контракт обязан объявлять доступ для КАЖДОЙ операции.
///
/// Правило (ADR-010): операция либо требует конкретный скоуп
/// (`x-required-scope: <scope>` из каталога), либо явно объявлена
/// доступной любому валидному ключу (`x-no-scope: true`). Операция без
/// того и другого — дыра в контракте: клиент не знает, что ему нужно,
/// а ревьюер не видит, чем она защищена.
#[test]
fn developer_operations_declare_scope() {
    let doc = load_developer_openapi();
    let paths = doc
        .get("paths")
        .and_then(|v| v.as_object())
        .expect("developer openapi: paths");
    let mut problems = Vec::new();
    for (path, item) in paths {
        let Some(ops) = item.as_object() else {
            continue;
        };
        for method in METHODS {
            let Some(op) = ops.get(*method) else { continue };
            let upper = method.to_uppercase();
            let declared = op.get("x-required-scope");
            let no_scope = op
                .get("x-no-scope")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            match declared {
                Some(Value::String(scope)) => {
                    if !DEVELOPER_SCOPES.contains(&scope.as_str()) {
                        problems.push(format!("{upper} {path}: неизвестный скоуп `{scope}`"));
                    }
                    if no_scope {
                        problems.push(format!(
                            "{upper} {path}: x-no-scope и x-required-scope одновременно"
                        ));
                    }
                }
                // `x-required-scope: null` допустим только вместе с x-no-scope.
                Some(Value::Null) => {
                    if !no_scope {
                        problems.push(format!(
                            "{upper} {path}: `x-required-scope: null` без `x-no-scope: true`"
                        ));
                    }
                }
                Some(_) => problems.push(format!(
                    "{upper} {path}: x-required-scope должен быть строкой или null"
                )),
                None => {
                    if !no_scope {
                        problems.push(format!(
                            "{upper} {path}: нет ни x-required-scope, ни x-no-scope"
                        ));
                    }
                }
            }
            // Скоуп с токеном аутентификации: обе схемы объявлены.
            if op.get("security").is_none() {
                problems.push(format!("{upper} {path}: нет security"));
            }
        }
    }
    assert!(
        problems.is_empty(),
        "контракт Developer API нарушен:\n{}",
        problems.join("\n")
    );
}

#[test]
fn path_sets_match_exactly() {
    // Сводная сверка множеств путей (без методов) — удобная диагностическая точка.
    let spec_paths: BTreeSet<String> = collect_all_spec_routes()
        .into_iter()
        .map(|(p, _)| p)
        .collect();
    let code_paths: BTreeSet<String> = collect_code_routes().into_iter().map(|(p, _)| p).collect();
    assert_eq!(
        spec_paths, code_paths,
        "множества путей кода и спецификации не совпадают"
    );
}

/// Быстрая сводка покрытия — печатается при `--nocapture`, помогает ревьюеру.
#[test]
fn print_coverage_summary() {
    let spec = collect_all_spec_routes();
    let code = collect_code_routes();
    let spec_paths: BTreeSet<&str> = spec.iter().map(|(p, _)| p.as_str()).collect();
    let code_paths: BTreeSet<&str> = code.iter().map(|(p, _)| p.as_str()).collect();
    eprintln!(
        "openapi coverage: {} путей в коде, {} в спецификации, {} операций",
        code_paths.len(),
        spec_paths.len(),
        spec.len()
    );
    // Бонус: сводка тегов — сколько операций на каждый тег (оба контракта).
    let mut by_tag: BTreeMap<String, usize> = BTreeMap::new();
    for doc in [load_openapi(), load_developer_openapi()] {
        if let Some(paths) = doc.get("paths").and_then(|v| v.as_object()) {
            for item in paths.values() {
                for method in METHODS {
                    if let Some(tags) = item
                        .get(*method)
                        .and_then(|o| o.get("tags"))
                        .and_then(|t| t.as_array())
                    {
                        for t in tags {
                            if let Some(name) = t.as_str() {
                                *by_tag.entry(name.to_string()).or_insert(0) += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    eprintln!("теги: {:?}", by_tag);
    assert!(spec == code, "покрытие не совпадает");
}
