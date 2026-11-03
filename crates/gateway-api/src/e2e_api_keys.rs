//! E2e-тесты управления API-ключами Developer API v1 (ADR-010).
//!
//! Покрывают панельные роуты `/v1/club/api-keys/**` и
//! `/v1/admin/api-keys/**`, а также инварианты репозитория, которые нельзя
//! проверить на уровне HTTP, пока Developer API ещё не наполнен:
//!   * секрет возвращается ровно один раз (create/rotate), в списках и
//!     карточке его нет, в БД лежит только `sha256(plaintext)`;
//!   * ключ одного клуба не виден другому (404, не 403);
//!   * сужение скоупов работает, расширение — 422 `scope_widening_forbidden`;
//!   * ротация с `grace_hours = 0` гасит старый секрет, новый работает;
//!   * отзыв возвращает 204, повторный — 404, аутентификация — `Revoked`;
//!   * лимит 25 активных ключей на клуб — 409 `key_limit_reached`;
//!   * RBAC: DJ/чужая роль → 403, без токена → 401;
//!   * админ видит все ключи с названием клуба и отзывает любой ключ;
//!   * журнал вызовов отдаёт сводку и ленту, а аудит не содержит секрета.
//!
//! БД-тесты идут серийно через `scripts/db-test.sh ... --test-threads=1`:
//! `test_repo()` чистит фикстуры, поэтому параллельный прогон двух
//! DB-тестов ложно падает.

#![cfg(test)]

use axum::{
    body::Body,
    http::{HeaderMap, Request, StatusCode},
};
use dropit_common::{Environment, Role, Scope};
use dropit_infra_db::{ApiKeyError, ApiKeyRepo};
use serde_json::{json, Value};
use tower::ServiceExt;
use uuid::Uuid;

use crate::e2e::{app_env, call, test_repo};
use crate::routes::{router, ApiEnv};

/// `call` из `e2e.rs` не отдаёт заголовки, а нам нужен `Cache-Control`.
/// Тело всё так же разбирается в JSON; пустой ответ — `Value::Null`.
async fn call_full(
    env: &ApiEnv,
    method: &str,
    uri: &str,
    body: Option<Value>,
    token: Option<&str>,
) -> (StatusCode, HeaderMap, Value) {
    let app = router(env.clone());
    let mut req = Request::builder().method(method).uri(uri);
    if let Some(token) = token {
        req = req.header("authorization", format!("Bearer {token}"));
    }
    let req = match body {
        Some(value) => req
            .header("content-type", "application/json")
            .body(Body::from(value.to_string()))
            .unwrap(),
        None => req.body(Body::empty()).unwrap(),
    };
    let response = app.oneshot(req).await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap_or_default();
    let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, headers, value)
}

fn sfield(value: &Value, key: &str) -> String {
    value[key].as_str().unwrap_or_default().to_string()
}

async fn insert_club(pool: &sqlx::PgPool, id: Uuid, name: &str, slug: &str, email: &str) {
    sqlx::query(
        "INSERT INTO club (id, name, slug, email, password_hash, status) \
         VALUES ($1, $2, $3, $4, 'x', 'active')",
    )
    .bind(id)
    .bind(name)
    .bind(slug)
    .bind(email)
    .execute(pool)
    .await
    .expect("club insert");
}

/// `POST /v1/club/api-keys` с токеном клуба.
async fn create_key(env: &ApiEnv, token: &str, body: Value) -> (StatusCode, HeaderMap, Value) {
    call_full(env, "POST", "/v1/club/api-keys", Some(body), Some(token)).await
}

/// Запрос Developer API с ключом в `X-API-Key` (без JWT).
async fn call_with_api_key(
    env: &ApiEnv,
    method: &str,
    uri: &str,
    key: &str,
) -> (StatusCode, Value) {
    let app = router(env.clone());
    let req = Request::builder()
        .method(method)
        .uri(uri)
        .header("x-api-key", key)
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(req).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap_or_default();
    let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, value)
}

#[tokio::test]
async fn e2e_api_key_lifecycle() {
    let repo = match test_repo().await {
        Some(repo) => repo,
        None => {
            eprintln!("DATABASE_URL недоступен — e2e API-ключей пропущен");
            return;
        }
    };
    let env = app_env(Some(repo.clone()), None, None).await;
    let pool = repo.pool();
    let keys = ApiKeyRepo::new(pool.clone());

    // --- Фикстуры: два клуба и токены всех ролей --------------------
    let club_id = Uuid::now_v7();
    let other_club_id = Uuid::now_v7();
    insert_club(&pool, club_id, "Keys Club", "keys-club", "keys@club.test").await;
    insert_club(
        &pool,
        other_club_id,
        "Other Club",
        "other-club",
        "other@club.test",
    )
    .await;

    let club_token = env
        .keys
        .issue_access(club_id, Role::Club, Uuid::now_v7())
        .unwrap();
    let other_club_token = env
        .keys
        .issue_access(other_club_id, Role::Club, Uuid::now_v7())
        .unwrap();
    let dj_token = env
        .keys
        .issue_access(Uuid::now_v7(), Role::Dj, Uuid::now_v7())
        .unwrap();
    let admin_token = env
        .keys
        .issue_access(Uuid::now_v7(), Role::Admin, Uuid::now_v7())
        .unwrap();

    // --- RBAC: без токена 401, чужая роль 403 ----------------------
    let (status, _, body) = call_full(&env, "GET", "/v1/club/api-keys", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "no token: {body}");
    let (status, _, body) =
        call_full(&env, "GET", "/v1/club/api-keys", None, Some(&dj_token)).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "dj on club keys: {body}");
    let (status, _, _) =
        call_full(&env, "GET", "/v1/admin/api-keys", None, Some(&club_token)).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "club on admin keys");
    let (status, _, _) =
        call_full(&env, "GET", "/v1/club/api-keys", None, Some(&admin_token)).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "admin on club keys");

    // --- Создание: секрет один раз, no-store -----------------------
    let (status, headers, created) = create_key(
        &env,
        &club_token,
        json!({
            "name": "Night automation",
            "scopes": ["sessions:read", "docs:read"],
            "environment": "live",
            "expires_in_days": 30,
            "rate_limit_per_min": 60,
            "allowed_cidrs": ["203.0.113.0/24"],
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "create: {created}");
    assert_eq!(
        headers.get("cache-control").and_then(|v| v.to_str().ok()),
        Some("no-store"),
        "секрет не должен кэшироваться"
    );
    let key_id = Uuid::parse_str(&sfield(&created, "id")).unwrap();
    let plaintext = sfield(&created, "key");
    let prefix = sfield(&created, "prefix");
    assert!(
        plaintext.starts_with("dit_live_"),
        "формат ключа: {plaintext}"
    );
    assert_eq!(prefix.len(), 10);
    assert!(created["warning"].as_str().is_some_and(|w| !w.is_empty()));
    assert!(created.get("secret_hash").is_none());
    assert_eq!(created["environment"], "live");
    assert_eq!(created["status"], "active");
    assert_eq!(created["rate_limit_per_min"], 60);
    assert_eq!(created["allowed_cidrs"], json!(["203.0.113.0/24"]));
    assert!(created["expires_at"].as_str().is_some());
    assert_eq!(created["expired"], false);
    // Скоупы нормализованы в порядок каталога, дублей нет.
    assert_eq!(created["scopes"], json!(["docs:read", "sessions:read"]));

    // В БД только sha256(plaintext), секрета нет ни в одной колонке.
    let (stored_hash, stored_prefix): (String, String) =
        sqlx::query_as("SELECT secret_hash, prefix FROM api_key WHERE id = $1")
            .bind(key_id)
            .fetch_one(&pool)
            .await
            .expect("stored key row");
    assert_eq!(stored_hash, dropit_common::hash_api_key(&plaintext));
    assert_ne!(stored_hash, plaintext);
    assert_eq!(stored_prefix, prefix);
    let leaked: i64 =
        sqlx::query_scalar("SELECT count(*) FROM api_key WHERE secret_hash = $1 OR prefix = $1")
            .bind(&plaintext)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(leaked, 0, "plaintext-ключ не должен лежать в таблице");

    // --- Список и карточка не содержат секрета ---------------------
    let (status, _, listed) =
        call_full(&env, "GET", "/v1/club/api-keys", None, Some(&club_token)).await;
    assert_eq!(status, StatusCode::OK);
    let items = listed["items"].as_array().expect("items");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["id"], key_id.to_string());
    assert!(
        items[0].get("key").is_none(),
        "список не должен отдавать секрет"
    );
    assert!(items[0].get("secret_hash").is_none());

    let (status, _, fetched) = call_full(
        &env,
        "GET",
        &format!("/v1/club/api-keys/{key_id}"),
        None,
        Some(&club_token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "get: {fetched}");
    assert!(fetched.get("key").is_none());
    assert_eq!(fetched["prefix"], prefix);

    // Чужой клуб не видит ключ: 404, а не 403 (нельзя перебирать id).
    let (status, _, _) = call_full(
        &env,
        "GET",
        &format!("/v1/club/api-keys/{key_id}"),
        None,
        Some(&other_club_token),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "foreign get");
    let (status, _, _) = call_full(
        &env,
        "PATCH",
        &format!("/v1/club/api-keys/{key_id}"),
        Some(json!({ "name": "hijack" })),
        Some(&other_club_token),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "foreign patch");
    let (status, _, _) = call_full(
        &env,
        "DELETE",
        &format!("/v1/club/api-keys/{key_id}"),
        None,
        Some(&other_club_token),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "foreign delete");

    // --- Сужение скоупов, запрет расширения, валидация -------------
    let (status, _, narrowed) = call_full(
        &env,
        "PATCH",
        &format!("/v1/club/api-keys/{key_id}"),
        Some(json!({ "name": "Night automation v2", "scopes": ["docs:read"] })),
        Some(&club_token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "narrow: {narrowed}");
    assert_eq!(narrowed["scopes"], json!(["docs:read"]));
    assert_eq!(narrowed["name"], "Night automation v2");

    let (status, _, widened) = call_full(
        &env,
        "PATCH",
        &format!("/v1/club/api-keys/{key_id}"),
        Some(json!({ "scopes": ["docs:read", "stats:read"] })),
        Some(&club_token),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "widen: {widened}");
    assert_eq!(widened["code"], "scope_widening_forbidden");

    let (status, _, empty) = call_full(
        &env,
        "PATCH",
        &format!("/v1/club/api-keys/{key_id}"),
        Some(json!({ "scopes": [] })),
        Some(&club_token),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "empty: {empty}");
    assert_eq!(empty["code"], "validation_error");

    let (status, _, unknown) = call_full(
        &env,
        "PATCH",
        &format!("/v1/club/api-keys/{key_id}"),
        Some(json!({ "scopes": ["root:*"] })),
        Some(&club_token),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "unknown: {unknown}"
    );
    assert_eq!(unknown["code"], "validation_error");
    assert_eq!(unknown["details"]["unknown_scope"], "root:*");

    // --- Валидация входа при создании ------------------------------
    for (label, body) in [
        (
            "unknown scope",
            json!({ "name": "bad", "scopes": ["admin:*"] }),
        ),
        ("empty scopes", json!({ "name": "bad", "scopes": [] })),
        (
            "invalid cidr",
            json!({ "name": "bad", "scopes": ["docs:read"], "allowed_cidrs": ["not-an-ip"] }),
        ),
        (
            "rate out of range",
            json!({ "name": "bad", "scopes": ["docs:read"], "rate_limit_per_min": 10 }),
        ),
        (
            "expiry out of range",
            json!({ "name": "bad", "scopes": ["docs:read"], "expires_in_days": 0 }),
        ),
        (
            "invalid environment",
            json!({ "name": "bad", "scopes": ["docs:read"], "environment": "prod" }),
        ),
        (
            "empty name",
            json!({ "name": " ", "scopes": ["docs:read"] }),
        ),
    ] {
        let (status, _, body) = create_key(&env, &club_token, body).await;
        assert_eq!(
            status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{label} must be 422: {body}"
        );
        assert_eq!(body["code"], "validation_error", "{label}");
    }

    // --- Ротация: grace 0 гасит старый секрет ----------------------
    let (status, _, rotate_key) = create_key(
        &env,
        &club_token,
        json!({ "name": "Rotate me", "scopes": ["docs:read"], "environment": "test" }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "rotate fixture: {rotate_key}");
    let rotate_id = Uuid::parse_str(&sfield(&rotate_key, "id")).unwrap();
    let rotate_plaintext = sfield(&rotate_key, "key");
    let rotate_prefix = sfield(&rotate_key, "prefix");

    let auth = keys
        .authenticate(&rotate_plaintext, "127.0.0.1".parse().unwrap())
        .await
        .expect("fresh key authenticates");
    assert_eq!(auth.key_id, rotate_id);
    assert_eq!(auth.environment, Environment::Test);
    assert_eq!(auth.scopes, vec![Scope::DocsRead]);

    // Неверный секрет и неизвестный prefix одинаковы по коду.
    let wrong_secret = format!("dit_test_{rotate_prefix}_{}", "0".repeat(64));
    assert!(matches!(
        keys.authenticate(&wrong_secret, "127.0.0.1".parse().unwrap())
            .await,
        Err(ApiKeyError::InvalidKey)
    ));
    assert!(matches!(
        keys.authenticate("not-a-key", "127.0.0.1".parse().unwrap())
            .await,
        Err(ApiKeyError::InvalidKey)
    ));

    let (status, headers, rotated) = call_full(
        &env,
        "POST",
        &format!("/v1/club/api-keys/{rotate_id}/rotate"),
        Some(json!({ "grace_hours": 0 })),
        Some(&club_token),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "rotate: {rotated}");
    assert_eq!(
        headers.get("cache-control").and_then(|v| v.to_str().ok()),
        Some("no-store")
    );
    let new_rotate_id = Uuid::parse_str(&sfield(&rotated, "id")).unwrap();
    let new_rotate_plaintext = sfield(&rotated, "key");
    assert_ne!(new_rotate_id, rotate_id);
    assert_eq!(rotated["replaces"], rotate_id.to_string());
    assert!(new_rotate_plaintext.starts_with("dit_test_"));
    assert_eq!(rotated["environment"], "test");
    // Старый секрет погашен сразу (grace 0 ≈ expires_at = now).
    assert!(matches!(
        keys.authenticate(&rotate_plaintext, "127.0.0.1".parse().unwrap())
            .await,
        Err(ApiKeyError::Expired)
    ));
    let rotated_auth = keys
        .authenticate(&new_rotate_plaintext, "127.0.0.1".parse().unwrap())
        .await
        .expect("rotated key authenticates");
    assert_eq!(rotated_auth.key_id, new_rotate_id);

    let (status, _, bad_grace) = call_full(
        &env,
        "POST",
        &format!("/v1/club/api-keys/{new_rotate_id}/rotate"),
        Some(json!({ "grace_hours": 73 })),
        Some(&club_token),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "grace: {bad_grace}"
    );
    assert_eq!(bad_grace["code"], "validation_error");

    let (status, _, missing) = call_full(
        &env,
        "POST",
        &format!("/v1/club/api-keys/{}/rotate", Uuid::now_v7()),
        Some(json!({})),
        Some(&club_token),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "rotate missing: {missing}");

    // --- Отзыв: 204, повторный 404, аутентификация Revoked ---------
    let (status, _, body) = call_full(
        &env,
        "DELETE",
        &format!("/v1/club/api-keys/{new_rotate_id}"),
        None,
        Some(&club_token),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "revoke: {body}");
    let (status, _, body) = call_full(
        &env,
        "DELETE",
        &format!("/v1/club/api-keys/{new_rotate_id}"),
        None,
        Some(&club_token),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "second revoke: {body}");
    assert_eq!(body["code"], "not_found");
    assert!(matches!(
        keys.authenticate(&new_rotate_plaintext, "127.0.0.1".parse().unwrap())
            .await,
        Err(ApiKeyError::Revoked)
    ));
    // Карточку отозванного ключа менять нельзя — 409.
    let (status, _, body) = call_full(
        &env,
        "PATCH",
        &format!("/v1/club/api-keys/{new_rotate_id}"),
        Some(json!({ "name": "nope" })),
        Some(&club_token),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "patch revoked: {body}");
    assert_eq!(body["code"], "api_key_revoked");

    // --- IP allowlist проверяется на реальной аутентификации -------
    assert!(matches!(
        keys.authenticate(&plaintext, "127.0.0.1".parse().unwrap())
            .await,
        Err(ApiKeyError::IpNotAllowed)
    ));
    let allowed = keys
        .authenticate(&plaintext, "203.0.113.9".parse().unwrap())
        .await
        .expect("allowed ip");
    assert_eq!(allowed.key_id, key_id);
    assert_eq!(allowed.scopes, vec![Scope::DocsRead]);

    // --- HTTP-слой: ротация и отзыв действуют на /v1/developer/me --
    // Клуб здесь уже active, ключ live без allowlist: проверяем сам gate
    // (middleware), а не только ApiKeyRepo::authenticate.
    let (status, _, http_key) = create_key(
        &env,
        &club_token,
        json!({ "name": "HTTP auth key", "scopes": ["docs:read"] }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "http key: {http_key}");
    let http_key_id = Uuid::parse_str(&sfield(&http_key, "id")).unwrap();
    let http_plaintext = sfield(&http_key, "key");
    let (status, me) = call_with_api_key(&env, "GET", "/v1/developer/me", &http_plaintext).await;
    assert_eq!(status, StatusCode::OK, "developer me: {me}");
    assert_eq!(me["key_id"], http_key_id.to_string());

    let (status, _, http_rotated) = call_full(
        &env,
        "POST",
        &format!("/v1/club/api-keys/{http_key_id}/rotate"),
        Some(json!({ "grace_hours": 0 })),
        Some(&club_token),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "http rotate: {http_rotated}");
    let http_new_id = Uuid::parse_str(&sfield(&http_rotated, "id")).unwrap();
    let http_new_plaintext = sfield(&http_rotated, "key");

    let (status, body) = call_with_api_key(&env, "GET", "/v1/developer/me", &http_plaintext).await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "old key after rotate: {body}"
    );
    assert_eq!(body["code"], "api_key_expired");
    let (status, me_new) =
        call_with_api_key(&env, "GET", "/v1/developer/me", &http_new_plaintext).await;
    assert_eq!(status, StatusCode::OK, "new key after rotate: {me_new}");
    assert_eq!(me_new["key_id"], http_new_id.to_string());

    let (status, _, _) = call_full(
        &env,
        "DELETE",
        &format!("/v1/club/api-keys/{http_new_id}"),
        None,
        Some(&club_token),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, body) =
        call_with_api_key(&env, "GET", "/v1/developer/me", &http_new_plaintext).await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "revoked key on me: {body}"
    );
    assert_eq!(body["code"], "api_key_revoked");
    // Неизвестный/мусорный ключ — тот же 401 без подсказок.
    let (status, body) = call_with_api_key(&env, "GET", "/v1/developer/me", "dit_live_x").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "garbage key: {body}");
    assert_eq!(body["code"], "invalid_api_key");

    // --- Журнал вызовов: сводка + лента ----------------------------
    for (method, path, status) in [
        ("GET", "/v1/developer/me", 200u16),
        ("GET", "/v1/developer/docs", 200u16),
        ("GET", "/v1/developer/profile", 403u16),
    ] {
        keys.record_usage(key_id, club_id, method, path, status)
            .await
            .expect("record usage");
    }
    let (status, _, usage) = call_full(
        &env,
        "GET",
        &format!("/v1/club/api-keys/{key_id}/usage"),
        None,
        Some(&club_token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "usage: {usage}");
    assert!(usage["summary"]["calls_24h"].as_i64().unwrap_or(0) >= 3);
    assert!(usage["summary"]["errors_24h"].as_i64().unwrap_or(0) >= 1);
    assert!(usage["summary"]["calls_30d"].as_i64().unwrap_or(0) >= 3);
    assert!(usage["summary"]["last_call_at"].as_str().is_some());
    let feed = usage["items"].as_array().expect("usage items");
    assert_eq!(feed.len(), 3);
    for item in feed {
        assert!(item["at"].as_str().is_some());
        assert!(item["method"].as_str().is_some());
        assert!(item["path"].as_str().is_some());
        assert!(item["status"].as_i64().is_some());
    }
    let (status, _, _) = call_full(
        &env,
        "GET",
        &format!("/v1/club/api-keys/{key_id}/usage"),
        None,
        Some(&other_club_token),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "foreign usage");
    let (status, _, _) = call_full(
        &env,
        "GET",
        &format!("/v1/club/api-keys/{}/usage", Uuid::now_v7()),
        None,
        Some(&club_token),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "missing usage");

    // --- Каталог скоупов для панели --------------------------------
    let (status, _, catalogue) = call_full(
        &env,
        "GET",
        "/v1/club/api-keys/scopes",
        None,
        Some(&club_token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "scopes: {catalogue}");
    assert_eq!(catalogue["scopes"].as_array().unwrap().len(), 9);
    assert_eq!(catalogue["presets"].as_array().unwrap().len(), 4);

    // --- Админ: список с названием клуба, скоупы, отзыв ------------
    let (status, _, other_key) = create_key(
        &env,
        &other_club_token,
        json!({ "name": "Other club key", "scopes": ["docs:read"] }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "other key: {other_key}");
    let other_key_id = Uuid::parse_str(&sfield(&other_key, "id")).unwrap();
    let other_plaintext = sfield(&other_key, "key");

    let (status, _, admin_list) = call_full(
        &env,
        "GET",
        "/v1/admin/api-keys?limit=200",
        None,
        Some(&admin_token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "admin list: {admin_list}");
    let admin_items = admin_list["items"].as_array().expect("admin items");
    assert!(admin_items.len() >= 2);
    let names: Vec<&str> = admin_items
        .iter()
        .filter_map(|item| item["club_name"].as_str())
        .collect();
    assert!(names.contains(&"Keys Club"));
    assert!(names.contains(&"Other Club"));
    assert!(
        admin_items.iter().all(|item| item.get("key").is_none()),
        "админ-список не отдаёт секреты"
    );
    let (status, _, limited) = call_full(
        &env,
        "GET",
        "/v1/admin/api-keys?limit=1",
        None,
        Some(&admin_token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(limited["items"].as_array().unwrap().len(), 1);

    let (status, _, admin_catalogue) = call_full(
        &env,
        "GET",
        "/v1/admin/api-keys/scopes",
        None,
        Some(&admin_token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(admin_catalogue["scopes"].as_array().unwrap().len(), 9);

    // Админ отзывает ключ ЧУЖОГО клуба — 204, повтор — 404.
    let (status, _, _) = call_full(
        &env,
        "POST",
        &format!("/v1/admin/api-keys/{other_key_id}/revoke"),
        Some(json!({})),
        Some(&admin_token),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "admin revoke");
    assert!(matches!(
        keys.authenticate(&other_plaintext, "127.0.0.1".parse().unwrap())
            .await,
        Err(ApiKeyError::Revoked)
    ));
    let (status, _, _) = call_full(
        &env,
        "POST",
        &format!("/v1/admin/api-keys/{other_key_id}/revoke"),
        Some(json!({})),
        Some(&admin_token),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "admin second revoke");

    // Админ читает usage любого ключа.
    let (status, _, admin_usage) = call_full(
        &env,
        "GET",
        &format!("/v1/admin/api-keys/{key_id}/usage"),
        None,
        Some(&admin_token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "admin usage: {admin_usage}");
    assert!(admin_usage["summary"]["calls_24h"].as_i64().unwrap_or(0) >= 3);
    assert_eq!(admin_usage["items"].as_array().unwrap().len(), 3);

    // --- Лимит 25 активных ключей на клуб --------------------------
    let active: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM api_key WHERE club_id = $1 AND status = 'active' \
         AND (expires_at IS NULL OR expires_at > now())",
    )
    .bind(club_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    for index in active..dropit_common::MAX_ACTIVE_KEYS_PER_CLUB {
        let (status, _, body) = create_key(
            &env,
            &club_token,
            json!({ "name": format!("bulk {index}"), "scopes": ["docs:read"] }),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "bulk #{index}: {body}");
    }
    let (status, _, limit_body) = create_key(
        &env,
        &club_token,
        json!({ "name": "one too many", "scopes": ["docs:read"] }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "limit: {limit_body}");
    assert_eq!(limit_body["code"], "key_limit_reached");
    assert_eq!(
        limit_body["details"]["limit"],
        dropit_common::MAX_ACTIVE_KEYS_PER_CLUB
    );

    // --- Аудит: все действия, без секрета --------------------------
    let audit_rows: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT actor_type, action, COALESCE(diff::text, '') FROM audit_log \
         WHERE action LIKE 'api_key.%' ORDER BY at",
    )
    .fetch_all(&pool)
    .await
    .expect("audit rows");
    assert!(!audit_rows.is_empty(), "аудит ключей должен писаться");
    let actions: Vec<&str> = audit_rows
        .iter()
        .map(|(_, action, _)| action.as_str())
        .collect();
    for expected in [
        "api_key.created",
        "api_key.updated",
        "api_key.rotated",
        "api_key.revoked",
    ] {
        assert!(
            actions.contains(&expected),
            "нет аудита {expected}: {actions:?}"
        );
    }
    let actor_types: Vec<&str> = audit_rows
        .iter()
        .map(|(actor_type, _, _)| actor_type.as_str())
        .collect();
    assert!(actor_types.contains(&"club"));
    assert!(actor_types.contains(&"admin"));
    for (_, action, diff) in &audit_rows {
        assert!(!diff.contains(&plaintext), "{action}: секрет попал в аудит");
        assert!(
            !diff.contains(&rotate_plaintext),
            "{action}: секрет попал в аудит"
        );
        assert!(!diff.contains("secret_hash"), "{action}: хеш в аудите");
    }
    // entity — id затронутого ключа строкой: та же конвенция, что у dj.*
    // в panel_routes.rs (не имя таблицы).
    let entity_rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT action, entity FROM audit_log WHERE action LIKE 'api_key.%' ORDER BY at",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    for (action, entity) in &entity_rows {
        assert!(
            Uuid::parse_str(entity).is_ok(),
            "{action}: entity должен быть id ключа, получено {entity:?}"
        );
    }
    let entities_for = |action: &str| -> Vec<Uuid> {
        entity_rows
            .iter()
            .filter(|(row_action, _)| row_action == action)
            .filter_map(|(_, entity)| Uuid::parse_str(entity).ok())
            .collect()
    };
    assert!(entities_for("api_key.created").contains(&key_id));
    assert!(entities_for("api_key.updated").contains(&key_id));
    assert!(entities_for("api_key.rotated").contains(&new_rotate_id));
    assert!(entities_for("api_key.revoked").contains(&new_rotate_id));
    assert!(entities_for("api_key.revoked").contains(&other_key_id));
    let admin_revokes: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_log WHERE action = 'api_key.revoked' AND actor_type = 'admin'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(admin_revokes >= 1, "админ-отзыв должен быть в аудите");
}

/// Без БД роуты отвечают 503, а RBAC-гейт всё равно работает (JWT
/// проверяется до обращения к базе). Тест не трогает test_repo и не чистит
/// таблицы, поэтому безопасен при параллельном запуске.
#[tokio::test]
async fn e2e_api_key_routes_without_db() {
    let env = app_env(None, None, None).await;
    let (status, _) = call(&env, "GET", "/v1/club/api-keys", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let dj_token = env
        .keys
        .issue_access(Uuid::now_v7(), Role::Dj, Uuid::now_v7())
        .unwrap();
    let (status, _) = call(&env, "GET", "/v1/club/api-keys", None, Some(&dj_token)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let club_token = env
        .keys
        .issue_access(Uuid::now_v7(), Role::Club, Uuid::now_v7())
        .unwrap();
    let (status, body) = call(&env, "GET", "/v1/club/api-keys", None, Some(&club_token)).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "no db: {body}");

    let admin_token = env
        .keys
        .issue_access(Uuid::now_v7(), Role::Admin, Uuid::now_v7())
        .unwrap();
    let (status, _) = call(&env, "GET", "/v1/admin/api-keys", None, Some(&admin_token)).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
}
