//! E2e-тесты пакета W2-D: поиск каталога, push-токены, стили DJ.
//!
//! Требует живой Postgres (см. `scripts/db-test.sh`). При недоступной БД —
//! пропуск. Использует хелперы `app_env`/`call`/`test_repo` из `e2e.rs` и
//! собственный конструктор окружения с `MockCatalog::with_hits` (не ломая
//! `e2e.rs`).

use axum::http::StatusCode;
use dropit_common::Role;
use dropit_infra_catalog::{CatalogHit, MockCatalog};
use dropit_infra_db::IdentityRepo;
use serde_json::json;
use uuid::Uuid;

use crate::e2e::{app_env, call, test_repo};

/// Поддерживаемые фикстуры треков для поиска: Marc Anthony — Vivir Mi Vida.
fn mock_hits() -> Vec<CatalogHit> {
    vec![
        CatalogHit {
            source: "itunes".into(),
            source_id: "711404302".into(),
            title: "Vivir Mi Vida".into(),
            artist: "Marc Anthony".into(),
            album: Some("3.0".into()),
            duration_ms: 257_000,
            isrc: Some("USUM71302116".into()),
            artwork_url: Some("https://is1-ssl.mzstatic.com/image/x.jpg".into()),
            preview_url: Some("https://preview.example.com/p.m4a".into()),
            source_url: Some("https://music.apple.com/album/711404302".into()),
            explicit: false,
        },
        CatalogHit {
            source: "itunes".into(),
            source_id: "711404303".into(),
            title: "Flor Pálida".into(),
            artist: "Marc Anthony".into(),
            album: Some("3.0".into()),
            duration_ms: 0,
            isrc: None,
            artwork_url: None,
            preview_url: None,
            source_url: None,
            explicit: false,
        },
    ]
}

/// Окружение с каталогом-двойником, содержащим фикстуры.
async fn catalog_env(repo: IdentityRepo) -> crate::routes::ApiEnv {
    let mut env = app_env(Some(repo.clone()), None, None).await;
    env.catalog = Some(std::sync::Arc::new(MockCatalog::with_hits(mock_hits())));
    env
}

/// Чистит таблицы пакета W2-D, не затронутые `test_repo`.
async fn clean_w2d(pool: &sqlx::PgPool) {
    let _ = sqlx::query("TRUNCATE TABLE guest_push_token, catalog_query_cache")
        .execute(pool)
        .await;
    // Сбрасываем стили DJ, добавленные миграцией 0009.
    let _ = sqlx::query("UPDATE dj SET styles = '{}'")
        .execute(pool)
        .await;
}

/// Создаёт строку DJ с заданным id и возвращает JWT-токен роли dj.
async fn make_dj(env: &crate::routes::ApiEnv, pool: &sqlx::PgPool) -> (Uuid, String) {
    let dj_id = Uuid::now_v7();
    let club_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO club (id, name, slug, email, password_hash) \
         VALUES ($1, 'W2D Club', 'w2d-club', $2, 'x')",
    )
    .bind(club_id)
    .bind(format!("w2d-club-{}@club.test", dj_id))
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO dj (id, club_id, email, password_hash, display_name) \
         VALUES ($1, $2, $3, 'x', 'DJ W2D')",
    )
    .bind(dj_id)
    .bind(club_id)
    .bind(format!("w2d-{}@dj.test", dj_id))
    .execute(pool)
    .await
    .unwrap();
    let token = env
        .keys
        .issue_access(dj_id, Role::Dj, Uuid::now_v7())
        .unwrap();
    (dj_id, token)
}

/// Создаёт JWT гостя с заданным (или случайным) subject_id.
fn guest_token(env: &crate::routes::ApiEnv) -> (Uuid, String) {
    let sub = Uuid::now_v7();
    let token = env
        .keys
        .issue_access(sub, Role::Guest, Uuid::now_v7())
        .unwrap();
    (sub, token)
}

#[tokio::test]
async fn e2e_catalog_search_success_and_cache() {
    let repo = match test_repo().await {
        Some(r) => r,
        None => {
            eprintln!("DATABASE_URL недоступен — e2e W2D (search) пропущен");
            return;
        }
    };
    let pool = repo.pool();
    clean_w2d(&pool).await;
    let env = catalog_env(repo.clone()).await;
    let (_, guest_jwt) = guest_token(&env);

    // 1. Первый поиск — промах кеша, обращение к MockCatalog, cached=false.
    let (s, body) = call(
        &env,
        "GET",
        "/v1/catalog/search?q=Marc%20Anthony&limit=5",
        None,
        Some(&guest_jwt),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "search ok: {body}");
    assert_eq!(body["cached"], false, "first search not cached: {body}");
    let items = body["items"].as_array().expect("items array");
    assert!(!items.is_empty(), "has results");
    assert_eq!(items[0]["artist"], "Marc Anthony");
    assert_eq!(items[0]["source"], "itunes");
    assert_eq!(items[0]["duration_ms"], 257000);

    // 2. Повторный поиск того же трека — отдаётся из кеша, cached=true,
    //    без обращения к внешнему каталогу.
    let (s, body) = call(
        &env,
        "GET",
        "/v1/catalog/search?q=Marc%20Anthony&limit=5",
        None,
        Some(&guest_jwt),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "cached search ok: {body}");
    assert_eq!(body["cached"], true, "second search cached: {body}");
    let items2 = body["items"].as_array().expect("items array");
    assert_eq!(items2.len(), items.len(), "same result count from cache");
    assert_eq!(items2[0]["title"], "Vivir Mi Vida");
}

#[tokio::test]
async fn e2e_catalog_search_validation_and_rbac() {
    let repo = match test_repo().await {
        Some(r) => r,
        None => {
            eprintln!("DATABASE_URL недоступен — e2e W2D (validation) пропущен");
            return;
        }
    };
    let pool = repo.pool();
    clean_w2d(&pool).await;
    let env = catalog_env(repo.clone()).await;
    let (_, guest_jwt) = guest_token(&env);

    // Короткий q → 400.
    let (s, body) = call(
        &env,
        "GET",
        "/v1/catalog/search?q=a",
        None,
        Some(&guest_jwt),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "short q: {body}");
    assert_eq!(body["code"], "invalid_query");

    // limit=0 → 400.
    let (s, body) = call(
        &env,
        "GET",
        "/v1/catalog/search?q=test&limit=0",
        None,
        Some(&guest_jwt),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "limit=0: {body}");
    assert_eq!(body["code"], "invalid_query");

    // limit=99 → 400.
    let (s, body) = call(
        &env,
        "GET",
        "/v1/catalog/search?q=test&limit=99",
        None,
        Some(&guest_jwt),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "limit=99: {body}");
    assert_eq!(body["code"], "invalid_query");

    // Без токена → 401.
    let (s, _) = call(&env, "GET", "/v1/catalog/search?q=test", None, None).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED, "no token");

    // club_owner (Role::Club) → 403.
    let club_sub = Uuid::now_v7();
    let club_jwt = env
        .keys
        .issue_access(club_sub, Role::Club, Uuid::now_v7())
        .unwrap();
    let (s, _) = call(
        &env,
        "GET",
        "/v1/catalog/search?q=test",
        None,
        Some(&club_jwt),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "club on guest route");
}

#[tokio::test]
async fn e2e_push_token_register_idempotent_transfer() {
    let repo = match test_repo().await {
        Some(r) => r,
        None => {
            eprintln!("DATABASE_URL недоступен — e2e W2D (push) пропущен");
            return;
        }
    };
    let pool = repo.pool();
    clean_w2d(&pool).await;
    let env = app_env(Some(repo.clone()), None, None).await;

    let (_guest_a, token_a) = guest_token(&env);
    let (guest_b, token_b) = guest_token(&env);

    let push_token = "fcm-token-0123456789abcdef"; // 23 символа, ≥16.

    // 1. Регистрация гостем A → 201.
    let (s, body) = call(
        &env,
        "POST",
        "/v1/guest/push-token",
        Some(json!({ "token": push_token, "platform": "android" })),
        Some(&token_a),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "register: {body}");
    assert_eq!(body["registered"], true);

    // 2. Повторная регистрация того же токена тем же гостем → 201 (идемпотентно).
    let (s, body) = call(
        &env,
        "POST",
        "/v1/guest/push-token",
        Some(json!({ "token": push_token, "platform": "android" })),
        Some(&token_a),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "re-register: {body}");
    assert_eq!(body["registered"], true);

    // В БД ровно одна строка токена.
    let (cnt,): (i64,) = sqlx::query_as(
        "SELECT count(*) FROM guest_push_token WHERE token = $1",
    )
    .bind(push_token)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(cnt, 1, "single token row after re-register");

    // 3. Перевод токена другому гостю (B) → 201, guest_id обновлён.
    let (s, body) = call(
        &env,
        "POST",
        "/v1/guest/push-token",
        Some(json!({ "token": push_token, "platform": "ios" })),
        Some(&token_b),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "transfer: {body}");
    let (owner,): (Uuid,) = sqlx::query_as(
        "SELECT guest_id FROM guest_push_token WHERE token = $1",
    )
    .bind(push_token)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(owner, guest_b, "token transferred to guest B");

    // 4. Мусор: короткий токен → 400; невалидная платформа → 400.
    let (s, body) = call(
        &env,
        "POST",
        "/v1/guest/push-token",
        Some(json!({ "token": "short", "platform": "android" })),
        Some(&token_a),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "short token: {body}");
    assert_eq!(body["code"], "invalid_token");

    let (s, body) = call(
        &env,
        "POST",
        "/v1/guest/push-token",
        Some(json!({ "token": push_token, "platform": "windows" })),
        Some(&token_a),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "bad platform: {body}");
    assert_eq!(body["code"], "invalid_platform");

    // 401 без токена.
    let (s, _) = call(
        &env,
        "POST",
        "/v1/guest/push-token",
        Some(json!({ "token": push_token, "platform": "android" })),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED, "no token");
}

#[tokio::test]
async fn e2e_dj_styles_set_replace_dedup_rbac() {
    let repo = match test_repo().await {
        Some(r) => r,
        None => {
            eprintln!("DATABASE_URL недоступен — e2e W2D (dj styles) пропущен");
            return;
        }
    };
    let pool = repo.pool();
    clean_w2d(&pool).await;
    let env = app_env(Some(repo.clone()), None, None).await;
    let (_, dj_jwt) = make_dj(&env, &pool).await;

    // 1. Установка стилей → 200.
    let (s, body) = call(
        &env,
        "PATCH",
        "/v1/dj/styles",
        Some(json!({ "styles": ["Salsa", "Bachata"] })),
        Some(&dj_jwt),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "set styles: {body}");
    assert_eq!(body["styles"], json!(["salsa", "bachata"]));

    // В БД сохранено.
    let (db_styles,): (Vec<String>,) =
        sqlx::query_as("SELECT styles FROM dj WHERE email LIKE 'w2d-%@dj.test'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(db_styles, vec!["salsa", "bachata"]);

    // 2. Замена → 200.
    let (s, body) = call(
        &env,
        "PATCH",
        "/v1/dj/styles",
        Some(json!({ "styles": ["Merengue"] })),
        Some(&dj_jwt),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "replace styles: {body}");
    assert_eq!(body["styles"], json!(["merengue"]));

    // 3. Дедуп (регистр + точный дубль) → 200, стабильный порядок.
    let (s, body) = call(
        &env,
        "PATCH",
        "/v1/dj/styles",
        Some(json!({ "styles": ["Salsa", "salsa", "Bachata", "BACHATA"] })),
        Some(&dj_jwt),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "dedup styles: {body}");
    assert_eq!(body["styles"], json!(["salsa", "bachata"]));

    // 4. >8 стилей → 400.
    let (s, body) = call(
        &env,
        "PATCH",
        "/v1/dj/styles",
        Some(json!({ "styles": ["aa","bb","cc","dd","ee","ff","gg","hh","ii"] })),
        Some(&dj_jwt),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "too many styles: {body}");
    assert_eq!(body["code"], "invalid_styles");

    // Короткий стиль → 400.
    let (s, body) = call(
        &env,
        "PATCH",
        "/v1/dj/styles",
        Some(json!({ "styles": ["x"] })),
        Some(&dj_jwt),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "short style: {body}");

    // 5. Гость → 403 (dj-гейт).
    let (_, guest_jwt) = guest_token(&env);
    let (s, _) = call(
        &env,
        "PATCH",
        "/v1/dj/styles",
        Some(json!({ "styles": ["salsa"] })),
        Some(&guest_jwt),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "guest on dj route");

    // 6. Без токена → 401.
    let (s, _) = call(
        &env,
        "PATCH",
        "/v1/dj/styles",
        Some(json!({ "styles": ["salsa"] })),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED, "no token");
}
