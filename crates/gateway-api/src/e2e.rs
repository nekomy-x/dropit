//! E2e-тест F1: 4 роли, wrong-role → 403 (критерий приёмки фазы).
//!
//! Требует живой Postgres (compose): `DATABASE_URL` из env
//! (по умолчанию local-compose URL). При недоступной БД тест
//! помечается ignored — CI без сервиса БД не падает.
//!
//! Покрывает: выдача device-token, JWT по ролям, RBAC-гейт
//! (каждая роль на свой echo → 200; на чужой → 403; без токена → 401),
//! refresh-ротацию и reuse-детекцию.

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use dropit_common::Role;
use dropit_infra_db::IdentityRepo;
use serde_json::{json, Value};
use tower::ServiceExt; // oneshot

use crate::auth::JwtKeys;
use crate::routes::{router, ApiEnv};

pub(crate) async fn test_repo() -> Option<IdentityRepo> {
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://dropit:dropit_local@localhost:5432/dropit".into());
    dropit_infra_db::connect(&url)
        .await
        .ok()
        .map(IdentityRepo::new)
}

pub(crate) async fn app_env(
    repo: Option<IdentityRepo>,
    sessions: Option<dropit_infra_db::SessionRepo>,
    requests: Option<dropit_infra_db::RequestRepo>,
) -> ApiEnv {
    let pool = repo.as_ref().map(|r| r.pool());
    let billing = pool
        .as_ref()
        .map(|p| dropit_infra_db::BillingRepo::new(p.clone()))
        .map(std::sync::Arc::new);
    let payouts = pool
        .as_ref()
        .map(|p| dropit_infra_db::PayoutRepo::new(p.clone()))
        .map(std::sync::Arc::new);
    let jobs = pool
        .as_ref()
        .map(|p| dropit_infra_db::JobsRepo::new(p.clone()))
        .map(std::sync::Arc::new);
    ApiEnv {
        keys: JwtKeys::from_secret(b"e2e-test-secret-key-32-bytes-xxxxx"),
        repo: repo.map(std::sync::Arc::new),
        sessions: sessions.map(std::sync::Arc::new),
        requests: requests
            .or_else(|| pool.clone().map(dropit_infra_db::RequestRepo::new))
            .map(std::sync::Arc::new),
        billing,
        payouts,
        jobs,
    }
}

pub(crate) async fn call(
    env: &ApiEnv,
    method: &str,
    uri: &str,
    body: Option<Value>,
    token: Option<&str>,
) -> (StatusCode, Value) {
    let router = router(env.clone());
    let mut req = Request::builder().method(method).uri(uri);
    if let Some(t) = token {
        req = req.header("authorization", format!("Bearer {t}"));
    }
    let req = match body {
        Some(b) => req
            .header("content-type", "application/json")
            .body(Body::from(b.to_string()))
            .unwrap(),
        None => req.body(Body::empty()).unwrap(),
    };
    let resp = router.oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap_or_default();
    let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, value)
}

#[tokio::test]
async fn e2e_four_roles_rbac_matrix() {
    let repo = match test_repo().await {
        Some(r) => r,
        None => {
            eprintln!("DATABASE_URL недоступен — e2e пропущен (см. README CI)");
            return;
        }
    };
    let sessions_repo = dropit_infra_db::SessionRepo::new(repo.pool());
    let env = app_env(Some(repo.clone()), Some(sessions_repo), None).await;

    // --- Гость: device-token --------------------------------------
    let (status, body) = call(
        &env,
        "POST",
        "/v1/guest/device",
        Some(json!({ "platform": "ios", "app_version": "1.0.0" })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "device issue: {body}");
    let device_token = body["device_token"].as_str().unwrap().to_string();
    assert!(!device_token.is_empty());

    // --- Токены для трёх парольных ролей напрямую (логин-флоу
    // покрывается юнит-тестами auth; здесь — RBAC-матрица) ----------
    let sub = uuid::Uuid::now_v7();
    let fam = uuid::Uuid::now_v7();
    let dj_token = env.keys.issue_access(sub, Role::Dj, fam).unwrap();
    let club_token = env
        .keys
        .issue_access(sub, Role::Club, uuid::Uuid::now_v7())
        .unwrap();
    let admin_token = env
        .keys
        .issue_access(sub, Role::Admin, uuid::Uuid::now_v7())
        .unwrap();
    let guest_token = env
        .keys
        .issue_access(uuid::Uuid::now_v7(), Role::Guest, uuid::Uuid::now_v7())
        .unwrap();

    // --- RBAC-матрица: своя роль → 200 -----------------------------
    let (s, _) = call(&env, "GET", "/v1/dj/echo", None, Some(&dj_token)).await;
    assert_eq!(s, StatusCode::OK, "dj on dj route");
    let (s, _) = call(&env, "GET", "/v1/club/echo", None, Some(&club_token)).await;
    assert_eq!(s, StatusCode::OK, "club on club route");
    let (s, _) = call(&env, "GET", "/v1/admin/echo", None, Some(&admin_token)).await;
    assert_eq!(s, StatusCode::OK, "admin on admin route");
    let (s, _) = call(&env, "GET", "/v1/guest/echo", None, Some(&guest_token)).await;
    assert_eq!(s, StatusCode::OK, "guest on guest route");

    // --- wrong-role → 403 (критерий приёмки F1) --------------------
    let (s, body) = call(&env, "GET", "/v1/admin/echo", None, Some(&dj_token)).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "dj on admin: {body}");
    assert_eq!(body["code"], "forbidden");

    let (s, _) = call(&env, "GET", "/v1/dj/echo", None, Some(&guest_token)).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "guest on dj");
    let (s, _) = call(&env, "GET", "/v1/club/echo", None, Some(&admin_token)).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "admin on club");
    let (s, _) = call(&env, "GET", "/v1/guest/echo", None, Some(&club_token)).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "club on guest");

    // --- без токена → 401 ------------------------------------------
    let (s, body) = call(&env, "GET", "/v1/dj/echo", None, None).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED, "no token: {body}");
    assert_eq!(body["code"], "unauthorized");

    // --- мусорный токен → 401 --------------------------------------
    let (s, _) = call(&env, "GET", "/v1/dj/echo", None, Some("garbage.token")).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED, "bad token");

    // --- refresh: ротация + reuse-детекция --------------------------
    let role = Role::Dj;
    let refresh_plain = crate::auth::generate_refresh_token();
    let refresh_hash = crate::auth::hash_refresh_token(&refresh_plain);
    let family = uuid::Uuid::now_v7();
    let sub2 = uuid::Uuid::now_v7();
    repo.insert_refresh_token(
        uuid::Uuid::now_v7(),
        family,
        role,
        sub2,
        &refresh_hash,
        crate::auth::refresh_expiry(),
    )
    .await
    .unwrap();

    // 1-я ротация: 200 + новая пара.
    let (s, body) = call(
        &env,
        "POST",
        "/v1/auth/refresh",
        Some(json!({ "refresh_token": refresh_plain })),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK, "refresh rotate: {body}");
    let new_refresh = body["refresh_token"].as_str().unwrap().to_string();

    // 2-е использование СТАРОГО токена → reuse: 401, семья убита.
    let (s, body) = call(
        &env,
        "POST",
        "/v1/auth/refresh",
        Some(json!({ "refresh_token": refresh_plain })),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED, "reuse must fail: {body}");
    assert_eq!(body["code"], "unauthorized");

    // После ревока семьи даже новый токен мёртв.
    let (s, _) = call(
        &env,
        "POST",
        "/v1/auth/refresh",
        Some(json!({ "refresh_token": new_refresh })),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED, "family revoked after reuse");
}

#[tokio::test]
async fn e2e_login_requires_database() {
    // Без БД (repo = None) логины отвечают 503, а не паникой.
    let env = app_env(None, None, None).await;
    let (s, body) = call(
        &env,
        "POST",
        "/v1/auth/dj/login",
        Some(json!({ "email": "x@y.z", "password": "wrong" })),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::SERVICE_UNAVAILABLE, "no db: {body}");
}
