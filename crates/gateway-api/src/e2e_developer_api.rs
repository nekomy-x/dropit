//! E2e Developer API v1 (ADR-010): скоупы, аренда клуба, идемпотентность,
//! лимит и журнал вызовов.
//!
//! Требует живой Postgres (`DATABASE_URL`): при недоступной БД тесты
//! пропускаются, как весь остальной e2e-набор. Прогон серийный
//! (`--test-threads=1`), потому что `test_repo` чистит фикстуры.
//!
//! Что доказываем (это же — критерии приёмки §9 ADR-010):
//!   * ключ только с `docs:read` читает документацию и получает
//!     `403 insufficient_scope` на КАЖДОМ маршруте данных — целой матрицей,
//!     а не одним примером;
//!   * ключ клуба A не видит сессию/заявку клуба B (`404`, не `403`);
//!   * полный ключ проходит счастливый путь: сессия → QR → заявка
//!     (с replay по `Idempotency-Key`) → accept/played → список/карточка →
//!     каталог → статистика;
//!   * отозванный и просроченный ключ — `401` на следующем же запросе;
//!   * allowlist IP закрывается при чужом адресе (`403 ip_not_allowed`);
//!   * лимит per-key отвечает `429` с `X-RateLimit-*` и `Retry-After`;
//!   * в ответах нет гостевой идентичности и e-mail'ов, секрет не в журнале.

use axum::{
    body::Body,
    http::{HeaderMap, Request, StatusCode},
};
use dropit_common::{Environment, Role};
use dropit_domain::SessionTransition;
use dropit_infra_catalog::{CatalogHit, MockCatalog};
use dropit_infra_db::{ApiKeyRepo, NewApiKeyInput, RequestRepo, SessionRepo};
use serde_json::{json, Value};
use tower::ServiceExt;
use uuid::Uuid;

use crate::e2e::{app_env, test_repo, EnvGuard};
use crate::routes::{router, ApiEnv};

// ============================================================
// Фикстуры
// ============================================================

/// Клуб + DJ + трек в кэше каталога + сессия (по умолчанию `created`).
struct Tenant {
    club_id: Uuid,
    dj_id: Uuid,
    session_id: Uuid,
    track_id: Uuid,
}

/// Создаёт изолированный клуб с DJ, треком и сессией.
/// `open` — сразу перевести сессию в `active` (нужно для создания заявки).
async fn seed_tenant(pool: &sqlx::PgPool, tag: &str, open: bool) -> Tenant {
    let club_id = Uuid::now_v7();
    let dj_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO club (id, name, slug, email, password_hash, status, timezone) \
         VALUES ($1, $2, $3, $4, 'x', 'active', 'Europe/Madrid')",
    )
    .bind(club_id)
    .bind(format!("Club {tag}"))
    .bind(format!("club-{tag}"))
    .bind(format!("club-{tag}@dev.test"))
    .execute(pool)
    .await
    .expect("club insert");
    sqlx::query(
        "INSERT INTO dj (id, club_id, email, password_hash, display_name) \
         VALUES ($1, $2, $3, 'x', $4)",
    )
    .bind(dj_id)
    .bind(club_id)
    .bind(format!("dj-{tag}@dev.test"))
    .bind(format!("DJ {tag}"))
    .execute(pool)
    .await
    .expect("dj insert");

    let sessions = SessionRepo::new(pool.clone());
    let session = sessions
        .create_session(club_id, Some(dj_id), None, "Europe/Madrid")
        .await
        .expect("session insert");
    if open {
        sessions
            .apply_transition(session.id, SessionTransition::Open)
            .await
            .expect("session open");
    }

    let track_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO track_cache (id, source, source_id, canonical_key, title, artist) \
         VALUES ($1, 'itunes', $2, $3, 'Vivir Mi Vida', 'Marc Anthony')",
    )
    .bind(track_id)
    .bind(format!("track-{tag}"))
    .bind(format!("itunes:track-{tag}"))
    .execute(pool)
    .await
    .expect("track insert");

    Tenant {
        club_id,
        dj_id,
        session_id: session.id,
        track_id,
    }
}

/// Все девять скоупов (пресет «Full automation»).
const ALL_SCOPES: [&str; 9] = [
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

/// Создаёт ключ напрямую через репозиторий (панельный CRUD — тесты другого
/// пакета; здесь нужен только сам ключ).
async fn create_key(
    pool: &sqlx::PgPool,
    club_id: Uuid,
    name: &str,
    scopes: &[&str],
) -> (Uuid, String) {
    let repo = ApiKeyRepo::new(pool.clone());
    let created = repo
        .create(NewApiKeyInput {
            club_id,
            name: name.to_string(),
            environment: Environment::Live,
            scopes: scopes.iter().map(|scope| scope.to_string()).collect(),
            rate_limit_per_min: None,
            allowed_cidrs: Vec::new(),
            expires_in_days: None,
            created_by: None,
        })
        .await
        .expect("api key create");
    (created.key.id, created.revealed)
}

/// Гость + живой грант сессии — домену нужен гостевой грант даже в тесте.
async fn synthetic_guest(pool: &sqlx::PgPool, session_id: Uuid) -> Uuid {
    let guest_id = Uuid::now_v7();
    sqlx::query("INSERT INTO guest_device (id, platform) VALUES ($1, 'ios')")
        .bind(guest_id)
        .execute(pool)
        .await
        .expect("guest insert");
    sqlx::query("INSERT INTO session_guest (id, session_id, guest_device_id) VALUES ($1, $2, $3)")
        .bind(Uuid::now_v7())
        .bind(session_id)
        .bind(guest_id)
        .execute(pool)
        .await
        .expect("grant insert");
    guest_id
}

// ============================================================
// Вызовы
// ============================================================

/// Вызов Developer API с ключом; возвращает статус, заголовки и тело.
/// Заголовки нужны для проверки `X-RateLimit-*` / `Retry-After`.
async fn dev_call(
    env: &ApiEnv,
    method: &str,
    uri: &str,
    key: &str,
    body: Option<Value>,
    headers: &[(&str, &str)],
) -> (StatusCode, HeaderMap, Value) {
    let app = router(env.clone());
    let mut req = Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {key}"));
    for (name, value) in headers {
        req = req.header(*name, *value);
    }
    let req = match body {
        Some(value) => req
            .header("content-type", "application/json")
            .body(Body::from(value.to_string()))
            .expect("request body"),
        None => req.body(Body::empty()).expect("empty body"),
    };
    let response = app.oneshot(req).await.expect("oneshot");
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap_or_default();
    let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, headers, value)
}

/// Короткая форма: статус и тело (заголовки не нужны).
async fn call(
    env: &ApiEnv,
    method: &str,
    uri: &str,
    key: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let (status, _headers, value) = dev_call(env, method, uri, key, body, &[]).await;
    (status, value)
}

// ============================================================
// 1. Матрица скоупов: docs-only ключ
// ============================================================

/// Ключ с одним `docs:read`: документация доступна, каждый маршрут данных —
/// `403 insufficient_scope` с указанием недостающего скоупа.
///
/// Матрица перечисляет ВСЕ маршруты данных ADR-010 §4: новый маршрут без
/// скоупа обязан уронить этот тест, а не тихо открыться.
#[tokio::test]
async fn e2e_developer_docs_only_key_denied_on_every_data_route() {
    let Some(repo) = test_repo().await else {
        eprintln!("DATABASE_URL недоступен — e2e developer scope-матрица пропущена");
        return;
    };
    let pool = repo.pool();
    let tenant = seed_tenant(&pool, "scope-matrix", true).await;
    let (_key_id, key) = create_key(&pool, tenant.club_id, "docs only", &["docs:read"]).await;
    let env = app_env(Some(repo), None, None).await;

    // /me — без скоупа, работает любому валидному ключу.
    let (status, body) = call(&env, "GET", "/v1/developer/me", &key, None).await;
    assert_eq!(status, StatusCode::OK, "docs-only key on /me: {body}");
    assert_eq!(body["scopes"], json!(["docs:read"]));
    assert_eq!(body["environment"], "live");
    assert_eq!(body["club_id"], json!(tenant.club_id));
    assert!(
        body.get("key_id").is_some() && body.get("prefix").is_some(),
        "идентичность ключа без секрета: {body}"
    );
    assert!(
        !body.to_string().contains("secret") && !body.to_string().contains("hash"),
        "секрет/хеш не должны попадать в /me: {body}"
    );

    // Документация — доступна (скоуп есть).
    let (status, body) = call(&env, "GET", "/v1/developer/docs", &key, None).await;
    assert_eq!(status, StatusCode::OK, "docs manifest: {body}");
    assert_eq!(body["api_version"], "1.0.0");

    let (status, body) = call(&env, "GET", "/v1/developer/openapi.json", &key, None).await;
    assert_eq!(status, StatusCode::OK, "openapi filtered: {body}");
    assert!(
        body["paths"].get("/v1/developer/docs").is_some(),
        "docs-операция видна ключу с docs:read"
    );
    assert!(
        body["paths"].get("/v1/developer/profile").is_none(),
        "чужая операция не должна утекать в отфильтрованный OpenAPI"
    );
    assert!(
        body["paths"].get("/v1/developer/me").is_some(),
        "/me помечен x-no-scope и виден любому ключу"
    );
    assert_eq!(body["x-granted-scopes"], json!(["docs:read"]));

    // Неизвестная секция — 404; существующая, но недоступная — 403.
    let (status, body) = call(&env, "GET", "/v1/developer/docs/nope", &key, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "unknown section: {body}");
    assert_eq!(body["code"], "not_found");
    // Каждая секция: без требований — 200, с требованием чужого скоупа — 403
    // с первым недостающим скоупом (тело не отдаётся частично).
    let sections = crate::developer_docs::sections();
    assert!(!sections.is_empty(), "документация собрана в бинарник");
    for section in sections {
        let (status, body) = call(
            &env,
            "GET",
            &format!("/v1/developer/docs/{}", section.id),
            &key,
            None,
        )
        .await;
        if section.requires.is_empty() {
            assert_eq!(
                status,
                StatusCode::OK,
                "секция {} без требований: {body}",
                section.id
            );
            assert!(body["markdown"].is_string(), "тело секции — markdown");
        } else {
            assert_eq!(
                status,
                StatusCode::FORBIDDEN,
                "секция {} должна быть закрыта docs-only ключу: {body}",
                section.id
            );
            assert_eq!(body["code"], "insufficient_scope");
            assert_eq!(
                body["details"]["required_scope"], section.requires[0],
                "первый недостающий скоуп секции {}",
                section.id
            );
        }
    }
    // Манифест показывает ключу ровно те секции, что ему доступны.
    let (_, manifest) = call(&env, "GET", "/v1/developer/docs", &key, None).await;
    let listed = manifest["sections"]
        .as_array()
        .expect("sections")
        .iter()
        .filter_map(|section| section["id"].as_str())
        .count();
    let accessible = sections
        .iter()
        .filter(|section| section.requires.is_empty())
        .count();
    assert_eq!(
        listed, accessible,
        "манифест фильтрует секции по скоупам ключа"
    );

    // Матрица маршрутов данных: (метод, путь, тело, ожидаемый скоуп).
    let sid = tenant.session_id;
    let rid = Uuid::now_v7();
    let matrix: Vec<(&str, String, Option<Value>, &str)> = vec![
        ("GET", "/v1/developer/profile".into(), None, "profile:read"),
        (
            "GET",
            "/v1/developer/sessions".into(),
            None,
            "sessions:read",
        ),
        (
            "POST",
            "/v1/developer/sessions".into(),
            Some(json!({ "timezone": "Europe/Madrid" })),
            "sessions:write",
        ),
        (
            "GET",
            format!("/v1/developer/sessions/{sid}"),
            None,
            "sessions:read",
        ),
        (
            "POST",
            format!("/v1/developer/sessions/{sid}/close"),
            None,
            "sessions:write",
        ),
        (
            "POST",
            format!("/v1/developer/sessions/{sid}/qr"),
            None,
            "qr:write",
        ),
        (
            "GET",
            format!("/v1/developer/sessions/{sid}/queue"),
            None,
            "requests:read",
        ),
        (
            "POST",
            format!("/v1/developer/sessions/{sid}/requests"),
            Some(json!({ "track_id": tenant.track_id })),
            "requests:write",
        ),
        (
            "GET",
            "/v1/developer/requests".into(),
            None,
            "requests:read",
        ),
        (
            "GET",
            format!("/v1/developer/requests/{rid}"),
            None,
            "requests:read",
        ),
        (
            "POST",
            format!("/v1/developer/requests/{rid}/accept"),
            None,
            "requests:write",
        ),
        (
            "POST",
            format!("/v1/developer/requests/{rid}/reject"),
            Some(json!({ "reason": "no" })),
            "requests:write",
        ),
        (
            "POST",
            format!("/v1/developer/requests/{rid}/played"),
            None,
            "requests:write",
        ),
        (
            "GET",
            "/v1/developer/catalog/search?q=vivir".into(),
            None,
            "catalog:read",
        ),
        (
            "GET",
            "/v1/developer/stats/overview".into(),
            None,
            "stats:read",
        ),
    ];
    for (method, uri, body, scope) in matrix {
        let (status, response) = call(&env, method, &uri, &key, body).await;
        assert_eq!(
            status,
            StatusCode::FORBIDDEN,
            "{method} {uri} must be denied for docs-only key, got {status}: {response}"
        );
        assert_eq!(
            response["code"], "insufficient_scope",
            "{method} {uri}: {response}"
        );
        assert_eq!(
            response["details"]["required_scope"], scope,
            "{method} {uri}: недостающий скоуп должен быть назван"
        );
        assert_eq!(
            response["details"]["granted_scopes"],
            json!(["docs:read"]),
            "{method} {uri}: выданные скоупы перечислены"
        );
    }
}

// ============================================================
// 2. Счастливый путь полного ключа
// ============================================================

/// Полный ключ проходит вечер целиком: профиль → сессия → QR → очередь →
/// заявка (с replay) → accept/played → список/карточка → каталог → статистика.
#[tokio::test]
async fn e2e_developer_full_automation_happy_path() {
    let Some(repo) = test_repo().await else {
        eprintln!("DATABASE_URL недоступен — e2e developer happy path пропущен");
        return;
    };
    let pool = repo.pool();
    let tenant = seed_tenant(&pool, "happy", false).await;
    let (_key_id, key) = create_key(&pool, tenant.club_id, "full", &ALL_SCOPES).await;
    let mut env = app_env(Some(repo), None, None).await;
    // Каталог с одним хитом: проверяем нормализованный ответ и что id годится
    // для создания заявки.
    env.catalog = Some(std::sync::Arc::new(MockCatalog::with_hits(vec![
        CatalogHit {
            source: "itunes".into(),
            source_id: "12345".into(),
            title: "Vivir Mi Vida".into(),
            artist: "Marc Anthony".into(),
            album: Some("3.0".into()),
            duration_ms: 252_000,
            isrc: Some("USSD11300001".into()),
            artwork_url: Some("https://cdn.example/cover.jpg".into()),
            preview_url: Some("https://cdn.example/preview.m4a".into()),
            source_url: None,
            explicit: false,
        },
    ])));

    // --- профиль ---------------------------------------------------
    let (status, body) = call(&env, "GET", "/v1/developer/profile", &key, None).await;
    assert_eq!(status, StatusCode::OK, "profile: {body}");
    assert_eq!(body["club"]["id"], json!(tenant.club_id));
    assert_eq!(body["club"]["timezone"], "Europe/Madrid");
    assert_eq!(body["club"]["status"], "active");
    let profile_json = body.to_string();
    assert!(
        !profile_json.contains("stripe") && !profile_json.contains("password"),
        "в профиле нет Stripe/пароля: {profile_json}"
    );

    // Секция гайда, требующая скоуп, доступна полному ключу.
    let gated = crate::developer_docs::sections()
        .iter()
        .find(|section| !section.requires.is_empty())
        .expect("в документации есть секции, требующие скоуп");
    let (status, body) = call(
        &env,
        "GET",
        &format!("/v1/developer/docs/{}", gated.id),
        &key,
        None,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "полный ключ читает секцию {}: {body}",
        gated.id
    );
    assert!(body["markdown"].is_string());

    // --- создание сессии ------------------------------------------
    let (status, body) = call(
        &env,
        "POST",
        "/v1/developer/sessions",
        &key,
        Some(json!({ "dj_id": tenant.dj_id, "timezone": "Europe/Madrid" })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "create session: {body}");
    let session_id = body["id"].as_str().expect("session id").to_string();
    assert_eq!(body["status"], "created");
    assert_eq!(body["dj_id"], json!(tenant.dj_id));

    // Чужой DJ в теле → 404 (не подтверждаем существование).
    let other_dj = Uuid::now_v7();
    let (status, body) = call(
        &env,
        "POST",
        "/v1/developer/sessions",
        &key,
        Some(json!({ "dj_id": other_dj, "timezone": "UTC" })),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "foreign dj: {body}");

    // Кривая таймзона → 422.
    let (status, body) = call(
        &env,
        "POST",
        "/v1/developer/sessions",
        &key,
        Some(json!({ "timezone": "Mars/Olympus" })),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "bad tz: {body}");
    assert_eq!(body["code"], "validation_error");

    // --- список и карточка ----------------------------------------
    let (status, body) = call(&env, "GET", "/v1/developer/sessions", &key, None).await;
    assert_eq!(status, StatusCode::OK, "list sessions: {body}");
    assert!(
        body["items"]
            .as_array()
            .expect("items")
            .iter()
            .any(|item| item["id"] == json!(session_id)),
        "новая сессия в списке: {body}"
    );
    let (status, body) = call(
        &env,
        "GET",
        &format!("/v1/developer/sessions/{session_id}"),
        &key,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "get session: {body}");
    assert_eq!(body["counts"]["total"], 0);

    // --- QR -------------------------------------------------------
    let (status, body) = call(
        &env,
        "POST",
        &format!("/v1/developer/sessions/{session_id}/qr"),
        &key,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "qr issue: {body}");
    let qr_token = body["qr_token"].as_str().expect("qr token").to_string();
    assert_eq!(qr_token.len(), 64, "256-битный токен в hex");
    assert!(
        body["expires_at"].is_string(),
        "срок жизни QR = плановое закрытие: {body}"
    );
    assert!(
        body.get("join_url").is_none(),
        "без DROPIT_GUEST_JOIN_BASE_URL join_url не выдумывается: {body}"
    );

    // --- активируем сессию (это делает DJ в консоли) и проверяем очередь ---
    SessionRepo::new(pool.clone())
        .apply_transition(
            Uuid::parse_str(&session_id).unwrap(),
            SessionTransition::Open,
        )
        .await
        .expect("open session");
    let (status, body) = call(
        &env,
        "GET",
        &format!("/v1/developer/sessions/{session_id}/queue"),
        &key,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "empty queue: {body}");
    assert_eq!(body["items"], json!([]));

    // --- каталог --------------------------------------------------
    let (status, body) = call(
        &env,
        "GET",
        "/v1/developer/catalog/search?q=vivir",
        &key,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "catalog: {body}");
    let items = body["items"].as_array().expect("catalog items");
    assert_eq!(items.len(), 1, "один хит из мока: {body}");
    let catalog_track_id = items[0]["id"].as_str().expect("track id").to_string();
    assert_eq!(items[0]["title"], "Vivir Mi Vida");
    assert!(
        items[0].get("source_id").is_none() && items[0].get("source_url").is_none(),
        "сырой провайдерский payload не отдаём: {}",
        items[0]
    );

    // --- заявка: Idempotency-Key обязателен ------------------------
    let (status, body) = call(
        &env,
        "POST",
        &format!("/v1/developer/sessions/{session_id}/requests"),
        &key,
        Some(json!({ "track_id": catalog_track_id })),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "без Idempotency-Key: {body}"
    );
    assert_eq!(body["code"], "validation_error");
    assert_eq!(body["details"]["field"], "Idempotency-Key");

    // Неизвестный трек → 422.
    let (status, _headers, body) = dev_call(
        &env,
        "POST",
        &format!("/v1/developer/sessions/{session_id}/requests"),
        &key,
        Some(json!({ "track_id": Uuid::now_v7() })),
        &[("idempotency-key", "unknown-track")],
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "unknown track: {body}"
    );

    // Создание: 201.
    let idem = format!("order-{}", Uuid::now_v7());
    let (status, _headers, body) = dev_call(
        &env,
        "POST",
        &format!("/v1/developer/sessions/{session_id}/requests"),
        &key,
        Some(json!({ "track_id": catalog_track_id, "style": "salsa" })),
        &[("idempotency-key", idem.as_str())],
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "create request: {body}");
    let request_id = body["id"].as_str().expect("request id").to_string();
    assert_eq!(body["status"], "pending");
    assert_eq!(body["style"], "salsa");
    assert_eq!(body["track"]["title"], "Vivir Mi Vida");
    let created_json = body.to_string();
    assert!(
        !created_json.contains("guest_device_id"),
        "гостевая идентичность не отдаётся: {created_json}"
    );

    // Replay того же ключа: 201 и тот же id, второй заявки нет.
    let (status, _headers, replay) = dev_call(
        &env,
        "POST",
        &format!("/v1/developer/sessions/{session_id}/requests"),
        &key,
        Some(json!({ "track_id": catalog_track_id, "style": "salsa" })),
        &[("idempotency-key", idem.as_str())],
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "replay: {replay}");
    assert_eq!(
        replay["id"],
        json!(request_id),
        "replay отдаёт ту же заявку"
    );
    let (count,): (i64,) = sqlx::query_as("SELECT count(*) FROM request WHERE session_id = $1")
        .bind(Uuid::parse_str(&session_id).unwrap())
        .fetch_one(&pool)
        .await
        .expect("count requests");
    assert_eq!(count, 1, "replay не создаёт вторую заявку");

    // --- очередь с заявкой ----------------------------------------
    let (status, body) = call(
        &env,
        "GET",
        &format!("/v1/developer/sessions/{session_id}/queue?sort=hot"),
        &key,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "queue: {body}");
    assert_eq!(body["items"][0]["id"], json!(request_id));
    assert_eq!(body["items"][0]["same_track_count"], 1);
    assert!(
        body["items"][0].get("guest_device_id").is_none(),
        "в очереди нет гостя: {}",
        body["items"][0]
    );

    // --- accept → played, no-op повтор -----------------------------
    let (status, body) = call(
        &env,
        "POST",
        &format!("/v1/developer/requests/{request_id}/accept"),
        &key,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "accept: {body}");
    assert_eq!(body["status"], "accepted");
    assert_eq!(body["noop"], false);
    let (status, body) = call(
        &env,
        "POST",
        &format!("/v1/developer/requests/{request_id}/accept"),
        &key,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "повтор accept: {body}");
    assert_eq!(body["noop"], true, "дубль действия — no-op, не ошибка");

    let (status, body) = call(
        &env,
        "POST",
        &format!("/v1/developer/requests/{request_id}/played"),
        &key,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "played: {body}");
    assert_eq!(body["status"], "played");
    assert!(
        body["played_at"].is_string(),
        "played_at проставлен: {body}"
    );

    // Запрещённый переход: accept после played → 409.
    let (status, body) = call(
        &env,
        "POST",
        &format!("/v1/developer/requests/{request_id}/accept"),
        &key,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "accept after played: {body}");
    assert_eq!(body["code"], "invalid_request_transition");

    // --- список и карточка заявки ---------------------------------
    let (status, body) = call(
        &env,
        "GET",
        &format!("/v1/developer/requests?session_id={session_id}&status=played"),
        &key,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "requests list: {body}");
    assert_eq!(body["items"][0]["id"], json!(request_id));
    assert!(body["next_cursor"].is_null(), "одна страница: {body}");

    let (status, body) = call(
        &env,
        "GET",
        &format!("/v1/developer/requests/{request_id}"),
        &key,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "request detail: {body}");
    assert_eq!(body["track"]["artist"], "Marc Anthony");
    let detail_json = body.to_string();
    assert!(
        !detail_json.contains("guest_device_id")
            && !detail_json.contains('@')
            && !detail_json.contains("stripe"),
        "карточка заявки без PII/платежей: {detail_json}"
    );

    // --- статистика ------------------------------------------------
    let (status, body) = call(
        &env,
        "GET",
        "/v1/developer/stats/overview?period=day",
        &key,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "stats: {body}");
    assert_eq!(body["period"], "day");
    assert!(
        body["sessions"].as_i64().unwrap_or(0) >= 2,
        "две сессии: {body}"
    );
    assert_eq!(body["requests"]["total"], 1);
    assert_eq!(body["requests"]["played"], 1);
    assert_eq!(body["revenue"]["currency"], "EUR");
    assert!(body["revenue"]["captured_minor"].is_i64());
    assert_eq!(body["active_session"]["id"], json!(session_id));
    let top = body["top_tracks"].as_array().expect("top tracks");
    assert_eq!(top[0]["title"], "Vivir Mi Vida");
    assert_eq!(top[0]["count"], 1);

    // --- закрытие сессии ------------------------------------------
    let (status, body) = call(
        &env,
        "POST",
        &format!("/v1/developer/sessions/{session_id}/close"),
        &key,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "close: {body}");
    assert_eq!(body["status"], "closed");
    // Повторное закрытие — 409, а не 200.
    let (status, body) = call(
        &env,
        "POST",
        &format!("/v1/developer/sessions/{session_id}/close"),
        &key,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "second close: {body}");
    assert_eq!(body["code"], "session_closed");

    // Заявка в закрытую сессию — 409.
    let (status, _headers, body) = dev_call(
        &env,
        "POST",
        &format!("/v1/developer/sessions/{session_id}/requests"),
        &key,
        Some(json!({ "track_id": catalog_track_id })),
        &[("idempotency-key", "closed-session")],
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "request to closed: {body}");
    assert_eq!(body["code"], "session_not_active");
}

// ============================================================
// 2b. QR join_url и сессия не в active
// ============================================================

/// `join_url` появляется только когда стенд задал `DROPIT_GUEST_JOIN_BASE_URL`
/// (и содержит тот же одноразовый токен), а заявка в сессию `created`
/// отвечает `409 session_not_active` — не 500 и не «тихое» создание.
#[tokio::test]
async fn e2e_developer_qr_join_url_and_inactive_session() {
    let Some(repo) = test_repo().await else {
        eprintln!("DATABASE_URL недоступен — e2e developer QR/неактивная сессия пропущена");
        return;
    };
    let pool = repo.pool();
    let tenant = seed_tenant(&pool, "qr-join", false).await;
    let (_key_id, key) = create_key(&pool, tenant.club_id, "qr", &ALL_SCOPES).await;
    let env = app_env(Some(repo), None, None).await;
    let sid = tenant.session_id;

    // Заявка в сессию created — 409 (приём открывает DJ, а не создание).
    let (status, _headers, body) = dev_call(
        &env,
        "POST",
        &format!("/v1/developer/sessions/{sid}/requests"),
        &key,
        Some(json!({ "track_id": tenant.track_id })),
        &[("idempotency-key", "inactive-session")],
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "сессия created: {body}");
    assert_eq!(body["code"], "session_not_active");

    // join_url: включаем стендовую базу ссылки и проверяем сборку.
    {
        let _guard = EnvGuard::set("DROPIT_GUEST_JOIN_BASE_URL", "https://app.example/join");
        let (status, body) = call(
            &env,
            "POST",
            &format!("/v1/developer/sessions/{sid}/qr"),
            &key,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "qr с join_url: {body}");
        let token = body["qr_token"].as_str().expect("qr token");
        assert_eq!(
            body["join_url"],
            json!(format!("https://app.example/join?token={token}")),
            "join_url содержит тот же токен"
        );
        // Токен в базе только хешем: сырое значение не сохранено.
        let (plain_rows,): (i64,) =
            sqlx::query_as("SELECT count(*) FROM qr_token WHERE token_hash = $1")
                .bind(token)
                .fetch_one(&pool)
                .await
                .expect("qr token lookup");
        assert_eq!(plain_rows, 0, "plain-токен не хранится в БД");
    }

    // Ротация: новый токен отличается, старый деактивирован.
    let (_, first) = call(
        &env,
        "POST",
        &format!("/v1/developer/sessions/{sid}/qr"),
        &key,
        None,
    )
    .await;
    let (_, second) = call(
        &env,
        "POST",
        &format!("/v1/developer/sessions/{sid}/qr"),
        &key,
        None,
    )
    .await;
    assert_ne!(first["qr_token"], second["qr_token"], "токен ротируется");
    let (active,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM qr_token WHERE session_id = $1 AND active")
            .bind(sid)
            .fetch_one(&pool)
            .await
            .expect("active qr count");
    assert_eq!(active, 1, "у сессии ровно один активный QR");

    // Закрытая сессия: выпуск QR — 409, а не «токен в никуда».
    SessionRepo::new(pool.clone())
        .apply_transition(sid, SessionTransition::Close)
        .await
        .expect("close");
    let (status, body) = call(
        &env,
        "POST",
        &format!("/v1/developer/sessions/{sid}/qr"),
        &key,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "qr для closed: {body}");
    assert_eq!(body["code"], "session_closed");
}

// ============================================================
// 3. Аренда: чужой клуб → 404
// ============================================================

/// Ключ клуба B не видит сессию и заявку клуба A: `404` на всех обращениях,
/// включая фильтр `session_id` в списке — id нельзя перебрать.
#[tokio::test]
async fn e2e_developer_cross_tenant_is_404() {
    let Some(repo) = test_repo().await else {
        eprintln!("DATABASE_URL недоступен — e2e developer аренда пропущена");
        return;
    };
    let pool = repo.pool();
    let club_a = seed_tenant(&pool, "tenant-a", true).await;
    let club_b = seed_tenant(&pool, "tenant-b", true).await;
    let (_key_a, key_a) = create_key(&pool, club_a.club_id, "A", &ALL_SCOPES).await;
    let (_key_b, key_b) = create_key(&pool, club_b.club_id, "B", &ALL_SCOPES).await;
    let env = app_env(Some(repo), None, None).await;

    // Заявка клуба A (нужна, чтобы проверить чужую карточку заявки).
    let requests = RequestRepo::new(pool.clone());
    let guest = synthetic_guest(&pool, club_a.session_id).await;
    requests
        .create_request(
            club_a.session_id,
            guest,
            club_a.track_id,
            None,
            &format!("tenant-{}", Uuid::now_v7()),
        )
        .await
        .expect("create")
        .expect("created");
    let (status, body) = call(
        &env,
        "GET",
        &format!("/v1/developer/sessions/{}", club_a.session_id),
        &key_a,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "своя сессия: {body}");
    let (_, body) = call(&env, "GET", "/v1/developer/requests", &key_a, None).await;
    let request_id = body["items"][0]["id"]
        .as_str()
        .expect("request id")
        .to_string();

    // Сессия A глазами B.
    for (method, uri) in [
        (
            "GET",
            format!("/v1/developer/sessions/{}", club_a.session_id),
        ),
        (
            "POST",
            format!("/v1/developer/sessions/{}/close", club_a.session_id),
        ),
        (
            "POST",
            format!("/v1/developer/sessions/{}/qr", club_a.session_id),
        ),
        (
            "GET",
            format!("/v1/developer/sessions/{}/queue", club_a.session_id),
        ),
    ] {
        let (status, body) = call(&env, method, &uri, &key_b, None).await;
        assert_eq!(
            status,
            StatusCode::NOT_FOUND,
            "{method} {uri} для чужого: {body}"
        );
        assert_eq!(body["code"], "not_found", "{method} {uri}: {body}");
    }

    // Фильтр по чужой сессии в списке — тоже 404, а не пустой список.
    let (status, body) = call(
        &env,
        "GET",
        &format!("/v1/developer/requests?session_id={}", club_a.session_id),
        &key_b,
        None,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "чужой session_id в фильтре: {body}"
    );

    // Заявка A глазами B: карточка и действия.
    let (status, body) = call(
        &env,
        "GET",
        &format!("/v1/developer/requests/{request_id}"),
        &key_b,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "чужая карточка: {body}");
    for suffix in ["accept", "played", "reject"] {
        let (status, body) = call(
            &env,
            "POST",
            &format!("/v1/developer/requests/{request_id}/{suffix}"),
            &key_b,
            None,
        )
        .await;
        assert_eq!(
            status,
            StatusCode::NOT_FOUND,
            "чужое действие {suffix}: {body}"
        );
    }

    // Заявка от B в сессию A — 404 (и ничего не создаётся).
    let (status, _headers, body) = dev_call(
        &env,
        "POST",
        &format!("/v1/developer/sessions/{}/requests", club_a.session_id),
        &key_b,
        Some(json!({ "track_id": club_a.track_id })),
        &[("idempotency-key", "cross-tenant")],
    )
    .await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "чужая сессия для заявки: {body}"
    );
    let (count,): (i64,) = sqlx::query_as("SELECT count(*) FROM request WHERE session_id = $1")
        .bind(club_a.session_id)
        .fetch_one(&pool)
        .await
        .expect("count");
    assert_eq!(count, 1, "чужая заявка не создана");

    // Ключ B видит только свою сессию.
    let (status, body) = call(&env, "GET", "/v1/developer/sessions", &key_b, None).await;
    assert_eq!(status, StatusCode::OK, "list B: {body}");
    let ids: Vec<String> = body["items"]
        .as_array()
        .expect("items")
        .iter()
        .filter_map(|item| item["id"].as_str().map(str::to_string))
        .collect();
    assert_eq!(ids, vec![club_b.session_id.to_string()]);
}

// ============================================================
// 4. Отзыв, истечение, статус клуба
// ============================================================

/// Отозванный ключ — `401 api_key_revoked`, просроченный — `401
/// api_key_expired`; оба отвечают на следующем же запросе (без кэша).
/// Ключ клуба в `suspended` — `403 club_suspended`.
#[tokio::test]
async fn e2e_developer_revoked_expired_and_suspended() {
    let Some(repo) = test_repo().await else {
        eprintln!("DATABASE_URL недоступен — e2e developer отзыв/срок пропущен");
        return;
    };
    let pool = repo.pool();
    let tenant = seed_tenant(&pool, "revoke", true).await;
    let env = app_env(Some(repo), None, None).await;
    let keys = ApiKeyRepo::new(pool.clone());

    let (key_id, key) = create_key(&pool, tenant.club_id, "revoked", &ALL_SCOPES).await;
    let (status, _) = call(&env, "GET", "/v1/developer/me", &key, None).await;
    assert_eq!(status, StatusCode::OK, "ключ работает до отзыва");

    keys.revoke(tenant.club_id, key_id).await.expect("revoke");
    let (status, body) = call(&env, "GET", "/v1/developer/me", &key, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "после отзыва: {body}");
    assert_eq!(body["code"], "api_key_revoked");
    // Данные тоже закрыты — не только /me.
    let (status, _) = call(&env, "GET", "/v1/developer/profile", &key, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // Просроченный ключ: выпускаем и бэкдейтим срок.
    let (expired_id, expired_key) = create_key(&pool, tenant.club_id, "expired", &ALL_SCOPES).await;
    let (status, _) = call(&env, "GET", "/v1/developer/me", &expired_key, None).await;
    assert_eq!(status, StatusCode::OK, "ключ жив до истечения");
    sqlx::query("UPDATE api_key SET expires_at = now() - interval '1 hour' WHERE id = $1")
        .bind(expired_id)
        .execute(&pool)
        .await
        .expect("expire");
    let (status, body) = call(&env, "GET", "/v1/developer/me", &expired_key, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "просроченный: {body}");
    assert_eq!(body["code"], "api_key_expired");

    // Мусор вместо ключа — 401 invalid_api_key (без подсказок).
    let (status, body) = call(
        &env,
        "GET",
        "/v1/developer/me",
        "dit_live_0000000000_dead",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "мусорный ключ: {body}");
    assert_eq!(body["code"], "invalid_api_key");

    // Клуб в статусе suspended: ключ валиден, но данные закрыты.
    let (_suspended_key_id, suspended_key) =
        create_key(&pool, tenant.club_id, "suspended", &ALL_SCOPES).await;
    sqlx::query("UPDATE club SET status = 'suspended' WHERE id = $1")
        .bind(tenant.club_id)
        .execute(&pool)
        .await
        .expect("suspend club");
    let (status, body) = call(&env, "GET", "/v1/developer/me", &suspended_key, None).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "suspended club: {body}");
    assert_eq!(body["code"], "club_suspended");
}

// ============================================================
// 5. Allowlist IP
// ============================================================

/// Allowlist IP закрывает ключ для чужого адреса (`403 ip_not_allowed`) и
/// пускает разрешённый. IP берётся из `X-Forwarded-For` при `TRUST_PROXY=1`
/// — то же правило, что у остального API.
#[tokio::test]
async fn e2e_developer_ip_allowlist_fails_closed() {
    let Some(repo) = test_repo().await else {
        eprintln!("DATABASE_URL недоступен — e2e developer allowlist пропущен");
        return;
    };
    let pool = repo.pool();
    let tenant = seed_tenant(&pool, "ip", true).await;
    let env = app_env(Some(repo), None, None).await;

    let created = ApiKeyRepo::new(pool.clone())
        .create(NewApiKeyInput {
            club_id: tenant.club_id,
            name: "office".into(),
            environment: Environment::Live,
            scopes: ALL_SCOPES.iter().map(|s| s.to_string()).collect(),
            rate_limit_per_min: None,
            allowed_cidrs: vec!["203.0.113.0/24".into()],
            expires_in_days: None,
            created_by: None,
        })
        .await
        .expect("key with allowlist");
    let key = created.revealed;

    let _guard = EnvGuard::set("TRUST_PROXY", "1");
    let (status, _headers, body) = dev_call(
        &env,
        "GET",
        "/v1/developer/me",
        &key,
        None,
        &[("x-forwarded-for", "203.0.113.7")],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "разрешённый IP: {body}");

    let (status, _headers, body) = dev_call(
        &env,
        "GET",
        "/v1/developer/me",
        &key,
        None,
        &[("x-forwarded-for", "198.51.100.9")],
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "чужой IP: {body}");
    assert_eq!(body["code"], "ip_not_allowed");

    // Отказ не «съедает» валидный ключ: снова разрешённый IP → 200.
    let (status, _headers, _body) = dev_call(
        &env,
        "GET",
        "/v1/developer/me",
        &key,
        None,
        &[("x-forwarded-for", "203.0.113.7")],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "после отказа ключ жив");
}

// ============================================================
// 6. Лимит per-key: 429 + заголовки
// ============================================================

/// Лимит считается по ключу (не по клубу): второй ключ того же клуба
/// продолжает работать, а исчерпавший отвечает `429 rate_limited` с
/// `X-RateLimit-*` и `Retry-After`. Журнал вызовов не содержит секрета.
#[tokio::test]
async fn e2e_developer_rate_limit_is_per_key_with_headers() {
    let Some(repo) = test_repo().await else {
        eprintln!("DATABASE_URL недоступен — e2e developer лимит пропущен");
        return;
    };
    let pool = repo.pool();
    let tenant = seed_tenant(&pool, "rate", true).await;
    let env = app_env(Some(repo), None, None).await;
    let keys = ApiKeyRepo::new(pool.clone());

    // Минимальный лимит (30/мин), чтобы не гонять сотни запросов.
    let limited = keys
        .create(NewApiKeyInput {
            club_id: tenant.club_id,
            name: "limited".into(),
            environment: Environment::Live,
            scopes: vec!["docs:read".into(), "profile:read".into()],
            rate_limit_per_min: Some(30),
            allowed_cidrs: Vec::new(),
            expires_in_days: None,
            created_by: None,
        })
        .await
        .expect("limited key");
    let other = keys
        .create(NewApiKeyInput {
            club_id: tenant.club_id,
            name: "other".into(),
            environment: Environment::Live,
            scopes: vec!["docs:read".into()],
            rate_limit_per_min: Some(30),
            allowed_cidrs: Vec::new(),
            expires_in_days: None,
            created_by: None,
        })
        .await
        .expect("second key");

    for i in 1..=30 {
        let (status, headers, body) = dev_call(
            &env,
            "GET",
            "/v1/developer/me",
            &limited.revealed,
            None,
            &[],
        )
        .await;
        assert_eq!(
            status,
            StatusCode::OK,
            "вызов {i} в пределах лимита: {body}"
        );
        assert_eq!(headers.get("x-ratelimit-limit").unwrap(), "30");
        assert_eq!(
            headers.get("x-ratelimit-remaining").unwrap(),
            &(30 - i).to_string(),
            "remaining уменьшается на каждом вызове"
        );
    }

    let (status, headers, body) = dev_call(
        &env,
        "GET",
        "/v1/developer/me",
        &limited.revealed,
        None,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS, "31-й вызов: {body}");
    assert_eq!(body["code"], "rate_limited");
    assert_eq!(body["details"]["limit"], 30);
    assert_eq!(headers.get("retry-after").unwrap(), "60");
    assert_eq!(headers.get("x-ratelimit-remaining").unwrap(), "0");
    assert_eq!(headers.get("x-ratelimit-limit").unwrap(), "30");
    assert_eq!(headers.get("x-ratelimit-reset").unwrap(), "60");

    // Лимит именно per-key: второй ключ того же клуба не затронут.
    let (status, _headers, body) =
        dev_call(&env, "GET", "/v1/developer/me", &other.revealed, None, &[]).await;
    assert_eq!(status, StatusCode::OK, "второй ключ не ограничен: {body}");

    // Журнал вызовов записан: 30 успешных строк у исчерпанного ключа.
    let (calls,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM api_key_usage WHERE api_key_id = $1")
            .bind(limited.key.id)
            .fetch_one(&pool)
            .await
            .expect("usage count");
    assert_eq!(calls, 30, "журнал вызовов: по строке на успешный запрос");
    // Сырой ключ в журнал не попадает.
    let (leaked,): (i64,) = sqlx::query_as(
        "SELECT count(*) FROM api_key_usage WHERE path LIKE '%dit_%' OR method LIKE '%dit_%'",
    )
    .fetch_one(&pool)
    .await
    .expect("usage leak check");
    assert_eq!(leaked, 0, "в журнале нет секрета");
}

// ============================================================
// 7. Пагинация курсором
// ============================================================

/// Курсор непрозрачен, вторая страница не повторяет первую, а испорченный
/// курсор — `422` (а не молчаливая первая страница).
#[tokio::test]
async fn e2e_developer_cursor_pagination_is_stable() {
    let Some(repo) = test_repo().await else {
        eprintln!("DATABASE_URL недоступен — e2e developer пагинация пропущена");
        return;
    };
    let pool = repo.pool();
    let tenant = seed_tenant(&pool, "paging", true).await;
    let (_key_id, key) = create_key(&pool, tenant.club_id, "paging", &ALL_SCOPES).await;
    let env = app_env(Some(repo), None, None).await;

    let requests = RequestRepo::new(pool.clone());
    let guest = synthetic_guest(&pool, tenant.session_id).await;
    for i in 0..3 {
        requests
            .create_request(
                tenant.session_id,
                guest,
                tenant.track_id,
                None,
                &format!("paging-{i}-{}", Uuid::now_v7()),
            )
            .await
            .expect("create")
            .expect("created");
    }

    let (status, page1) = call(&env, "GET", "/v1/developer/requests?limit=2", &key, None).await;
    assert_eq!(status, StatusCode::OK, "page1: {page1}");
    assert_eq!(page1["items"].as_array().expect("items").len(), 2);
    let cursor = page1["next_cursor"]
        .as_str()
        .expect("курсор второй страницы")
        .to_string();
    assert!(!cursor.contains('|'), "курсор непрозрачен: {cursor}");

    let uri = format!("/v1/developer/requests?limit=2&cursor={cursor}");
    let (status, page2) = call(&env, "GET", &uri, &key, None).await;
    assert_eq!(status, StatusCode::OK, "page2: {page2}");
    let ids1: Vec<&str> = page1["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|item| item["id"].as_str())
        .collect();
    let ids2: Vec<&str> = page2["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|item| item["id"].as_str())
        .collect();
    assert_eq!(ids2.len(), 1, "на второй странице остаток");
    assert!(
        ids1.iter().all(|id| !ids2.contains(id)),
        "страницы не пересекаются"
    );
    assert!(
        page2["next_cursor"].is_null(),
        "последняя страница: {page2}"
    );

    // Испорченный курсор — 422, а не «первая страница».
    let (status, body) = call(
        &env,
        "GET",
        "/v1/developer/requests?cursor=not-a-cursor",
        &key,
        None,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "битый курсор: {body}"
    );
    assert_eq!(body["details"]["field"], "cursor");

    // Лимит вне диапазона — 422.
    let (status, body) = call(&env, "GET", "/v1/developer/requests?limit=101", &key, None).await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "limit 101: {body}"
    );
    let (status, _) = call(
        &env,
        "GET",
        "/v1/developer/requests?status=weird",
        &key,
        None,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "неизвестный статус"
    );
}

// ============================================================
// W-8: документация панели (клуб) — отдельный контур от Developer API
// ============================================================

/// `GET /v1/club/developers/docs`, `/docs/:section`, `/openapi.json`:
/// RBAC `Role::Club`, тот же источник документации, что у Developer API,
/// но без скоупов (панельный доступ). До этого теста у маршрутов не было
/// ни одного e2e (`docs/audit/web-club-gaps.md` §6.9).
#[tokio::test]
async fn e2e_developer_panel_docs_are_club_only() {
    let Some(repo) = test_repo().await else {
        eprintln!("DATABASE_URL недоступен — e2e panel-docs пропущен");
        return;
    };
    let env = app_env(Some(repo), None, None).await;
    let club_token = env
        .keys
        .issue_access(Uuid::now_v7(), Role::Club, Uuid::now_v7())
        .unwrap();
    let dj_token = env
        .keys
        .issue_access(Uuid::now_v7(), Role::Dj, Uuid::now_v7())
        .unwrap();

    // --- RBAC: без токена 401, DJ 403 ---
    let (status, body) = call(&env, "GET", "/v1/club/developers/docs", "", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "no token: {body}");
    let (status, body) = call(&env, "GET", "/v1/club/developers/docs", &dj_token, None).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "dj: {body}");
    assert_eq!(body["code"], "forbidden");

    // --- манифест ---
    let (status, body) = call(&env, "GET", "/v1/club/developers/docs", &club_token, None).await;
    assert_eq!(status, StatusCode::OK, "manifest: {body}");
    assert_eq!(body["api_version"], "1.0.0");
    assert_eq!(body["base_url"], "/v1/developer");
    assert_eq!(body["openapi_url"], "/v1/developer/openapi.json");
    let sections = body["sections"].as_array().expect("sections").clone();
    assert!(!sections.is_empty(), "секции не пустые: {body}");
    for section in &sections {
        assert!(section["id"].is_string(), "id: {section}");
        assert!(section["title"].is_string(), "title: {section}");
        assert!(section["summary"].is_string(), "summary: {section}");
        assert!(section["requires"].is_array(), "requires: {section}");
        assert!(section["order"].is_i64(), "order: {section}");
    }
    assert!(
        !body["scopes"].as_array().unwrap().is_empty(),
        "каталог скоупов: {body}"
    );
    assert!(
        !body["presets"].as_array().unwrap().is_empty(),
        "пресеты: {body}"
    );

    // --- секция по id ---
    let section_id = sections[0]["id"].as_str().expect("section id").to_string();
    let (status, section) = call(
        &env,
        "GET",
        &format!("/v1/club/developers/docs/{section_id}"),
        &club_token,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "section: {section}");
    assert_eq!(section["id"], section_id);
    assert!(
        section["markdown"]
            .as_str()
            .map(|m| !m.is_empty())
            .unwrap_or(false),
        "markdown секции: {section}"
    );
    let (status, body) = call(
        &env,
        "GET",
        "/v1/club/developers/docs/does-not-exist",
        &club_token,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "unknown section: {body}");
    assert_eq!(body["code"], "not_found");

    // --- openapi.json: полный спек Developer API ---
    let (status, spec) = call(
        &env,
        "GET",
        "/v1/club/developers/openapi.json",
        &club_token,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "openapi: {spec}");
    assert!(
        spec["openapi"]
            .as_str()
            .map(|v| v.starts_with("3."))
            .unwrap_or(false),
        "openapi version: {spec}"
    );
    assert!(
        spec["paths"]["/v1/developer/me"].is_object(),
        "спек содержит ключевой путь: {spec}"
    );
}

// ============================================================
// W-8: calls_30d — реальное окно 30 дней
// ============================================================

/// `GET /v1/club/api-keys/:id/usage`: `calls_30d` считает только вызовы за
/// последние 30 дней, а не «все за всё время» (`docs/audit/web-club-gaps.md`
/// §4.1, §6.9). Старый вызов не попадает ни в `calls_24h`, ни в `calls_30d`;
/// свежий — попадает в оба окна.
#[tokio::test]
async fn e2e_developer_usage_calls_30d_is_a_real_window() {
    let Some(repo) = test_repo().await else {
        eprintln!("DATABASE_URL недоступен — e2e usage-30d пропущен");
        return;
    };
    let pool = repo.pool();
    let club_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO club (id, name, slug, email, password_hash) \
         VALUES ($1, 'Usage Club', 'usage-club', 'usage@club.test', 'x')",
    )
    .bind(club_id)
    .execute(&pool)
    .await
    .unwrap();
    let (key_id, _secret) = create_key(&pool, club_id, "usage key", &["docs:read"]).await;
    let env = app_env(Some(repo), None, None).await;
    let club_token = env
        .keys
        .issue_access(club_id, Role::Club, Uuid::now_v7())
        .unwrap();

    // Старый вызов — 40 дней назад, вне обоих окон.
    sqlx::query(
        "INSERT INTO api_key_usage (api_key_id, club_id, method, path, status, at) \
         VALUES ($1, $2, 'GET', '/v1/developer/me', 200, now() - interval '40 days')",
    )
    .bind(key_id)
    .bind(club_id)
    .execute(&pool)
    .await
    .unwrap();

    let uri = format!("/v1/club/api-keys/{key_id}/usage");
    let (status, usage) = call(&env, "GET", &uri, &club_token, None).await;
    assert_eq!(status, StatusCode::OK, "usage: {usage}");
    assert_eq!(
        usage["summary"]["calls_30d"], 0,
        "вызов старше 30 дней не должен считаться: {usage}"
    );
    assert_eq!(usage["summary"]["calls_24h"], 0);
    assert!(
        usage["summary"]["last_call_at"].as_str().is_some(),
        "max(at) всё ещё виден (вне окна, но факт вызова есть): {usage}"
    );

    // Свежий вызов — входит в 24h и 30d.
    sqlx::query(
        "INSERT INTO api_key_usage (api_key_id, club_id, method, path, status) \
         VALUES ($1, $2, 'GET', '/v1/developer/docs', 200)",
    )
    .bind(key_id)
    .bind(club_id)
    .execute(&pool)
    .await
    .unwrap();
    let (status, usage) = call(&env, "GET", &uri, &club_token, None).await;
    assert_eq!(status, StatusCode::OK, "usage fresh: {usage}");
    assert_eq!(usage["summary"]["calls_30d"], 1, "свежий вызов: {usage}");
    assert_eq!(usage["summary"]["calls_24h"], 1);
}
