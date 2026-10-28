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
    let pool = dropit_infra_db::connect(&url).await.ok()?;
    // Чистим транзакционные данные от прошлых прогонов — тесты
    // создают клубы с фиксированными email (UNIQUE). Схему (миграции)
    // НЕ трогаем; это тестовая БД, финансы чистим осознанно.
    let tables = [
        "audit_log",
        "gdpr_action",
        "incident",
        "reconciliation_run",
        "payout",
        "split",
        "ledger_entry",
        "payment",
        "idempotency_key",
        "idempotency_response",
        "device_token_issue",
        "signup_attempt",
        "outbox_event",
        "request",
        "session_guest",
        "qr_token",
        "session",
        "dj",
        "club",
        "admin_user",
        "guest_device",
        "refresh_token",
        "login_attempt",
        "pricing_policy",
        "track_cache",
        "webhook_event",
    ];
    for t in tables {
        let _ = sqlx::query(&format!("TRUNCATE TABLE {t} CASCADE"))
            .execute(&pool)
            .await;
    }
    let _ = sqlx::query(
        "UPDATE platform_settings SET commission_mode='unset', commission_percent_bp=NULL, \
         commission_fixed_minor=NULL, min_price_minor=NULL, max_price_minor=NULL, \
         dj_share_of_remainder_bp=NULL, runtime_mode='auto' WHERE id=1",
    )
    .execute(&pool)
    .await;
    Some(IdentityRepo::new(pool))
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
    let mail = pool
        .as_ref()
        .map(|p| dropit_infra_db::mail::MailRepo::new(p.clone()))
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
        mail,
        ws: Some(std::sync::Arc::new(crate::ws::WsHub::new())),
        tickets: Some(std::sync::Arc::new(
            dropit_infra_redis::InMemoryTicketStore::new(),
        )),
        bus: Some(std::sync::Arc::new(
            dropit_infra_redis::InMemoryEventBus::new(),
        )),
        stripe: Some(std::sync::Arc::new(dropit_infra_stripe::MockStripe)),
        catalog: Some(std::sync::Arc::new(
            dropit_infra_catalog::MockCatalog::empty(),
        )),
        // Тесты по умолчанию работают при включённой открытой регистрации;
        // тест выключенного флага гасит его в своей копии окружения.
        open_signup: true,
        signup_rate_per_hour: crate::infra::DEFAULT_SIGNUP_RATE_PER_HOUR,
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
    // Idempotency-Key обязателен на POST /v1/requests (правило
    // денег): подставляем уникальный, если тест не задал свой.
    if method == "POST" && uri == "/v1/requests" {
        req = req.header("idempotency-key", uuid::Uuid::now_v7().to_string());
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

/// `call` с произвольными доп. заголовками (для тестов антифрода по
/// `X-Forwarded-For` и лимита master-входа по IP). Сигнатура — как у
/// [`call`], плюс `&[(header, value)]`.
pub(crate) async fn call_with_headers(
    env: &ApiEnv,
    method: &str,
    uri: &str,
    body: Option<Value>,
    token: Option<&str>,
    headers: &[(&str, &str)],
) -> (StatusCode, Value) {
    let router = router(env.clone());
    let mut req = Request::builder().method(method).uri(uri);
    if let Some(t) = token {
        req = req.header("authorization", format!("Bearer {t}"));
    }
    for (k, v) in headers {
        req = req.header(*k, *v);
    }
    if method == "POST" && uri == "/v1/requests" {
        req = req.header("idempotency-key", uuid::Uuid::now_v7().to_string());
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

// ============================================================
// W5-C: антифрод device-token — лимит именно по IP (не глобальный)
// ============================================================

/// RAII-гард для env-переменной: сохраняет прежнее значение, выставляет
/// новое и восстанавливает при `drop`. Тесты лимита по IP манипулируют
/// `TRUST_PROXY`; чтобы не влиять на другие тесты при параллельном беге,
/// восстановление гарантировано. БД-тесты всё равно идут через
/// `db-test.sh -- --test-threads=1` (серийно).
pub(crate) struct EnvGuard {
    key: &'static str,
    prev: Option<std::string::String>,
}

impl EnvGuard {
    pub(crate) fn set(key: &'static str, value: &str) -> Self {
        let prev = std::env::var(key).ok();
        std::env::set_var(key, value);
        EnvGuard { key, prev }
    }
    /// Удаляет переменную (имитирует «не задано») и восстанавливает при drop.
    pub(crate) fn unset(key: &'static str) -> Self {
        let prev = std::env::var(key).ok();
        std::env::remove_var(key);
        EnvGuard { key, prev }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        match &self.prev {
            Some(v) => std::env::set_var(self.key, v),
            None => std::env::remove_var(self.key),
        }
    }
}

/// `POST /v1/guest/device` — два сценария доверия прокси-заголовкам:
///
/// 1. `TRUST_PROXY=1`: 6 разных `X-Forwarded-For` → разные бакеты → все
///    200 (по 1 выдаче на IP); 6 запросов с одним XFF → 1..5 ок, 6-й 429.
///    Доказывает, что при доверии лимит считается per-IP из заголовка.
/// 2. `TRUST_PROXY` не задан (безопасный дефолт — доверие ВЫКЛ): 6 разных
///    XFF → все сворачиваются в один бакет (заголовок игнорируется,
///    ConnectInfo в oneshot недоступен → `0.0.0.0`), 6-й → 429.
///    Доказывает, что при прямом подключении подмена XFF НЕ обходят лимит.
///
/// Без части (2) тест зелёный БЫЛ бы на уязвимом коде (доверие по
/// умолчанию) — поэтому регрессия ловится именно этим сценарием.
#[tokio::test]
async fn e2e_device_token_ip_limit() {
    let repo = match test_repo().await {
        Some(r) => r,
        None => {
            eprintln!("DATABASE_URL недоступен — e2e device-ip пропущен");
            return;
        }
    };
    let env = app_env(Some(repo.clone()), None, None).await;

    // --- (1) TRUST_PROXY=1: per-IP изоляция по XFF ---
    let _g = EnvGuard::set("TRUST_PROXY", "1");
    // 6 разных IP: все 200 (по 1 выдаче на IP, лимит 5 не достигнут).
    for i in 1..=6u8 {
        let xff = format!("203.0.113.{i}");
        let (s, body) = call_with_headers(
            &env,
            "POST",
            "/v1/guest/device",
            Some(json!({ "platform": "ios", "app_version": "1.0.0" })),
            None,
            &[("x-forwarded-for", xff.as_str())],
        )
        .await;
        assert_eq!(
            s,
            StatusCode::OK,
            "trust on, different ip #{i} must pass: {body}"
        );
    }
    // 6 запросов с одним IP: 1..5 → 200, 6-й → 429.
    let same = "203.0.113.50";
    for i in 1..=6u8 {
        let (s, body) = call_with_headers(
            &env,
            "POST",
            "/v1/guest/device",
            Some(json!({ "platform": "ios", "app_version": "1.0.0" })),
            None,
            &[("x-forwarded-for", same)],
        )
        .await;
        if i <= 5 {
            assert_eq!(
                s,
                StatusCode::OK,
                "trust on, same ip #{i} must pass: {body}"
            );
        } else {
            assert_eq!(
                s,
                StatusCode::TOO_MANY_REQUESTS,
                "trust on, same ip #{i} must be 429: {body}"
            );
            assert_eq!(body["code"], "rate_limited");
        }
    }
    drop(_g); // восстановить TRUST_PROXY до части (2)

    // --- (2) TRUST_PROXY не задан: заголовки игнорируются, один бакет ---
    let _g2 = EnvGuard::unset("TRUST_PROXY");
    // 6 разных XFF: заголовок игнорируется → все в бакет 0.0.0.0;
    // 1..5 → 200, 6-й → 429 (доказывает, что XFF не создаёт новых бакетов).
    for i in 1..=6u8 {
        let xff = format!("198.51.100.{i}");
        let (s, body) = call_with_headers(
            &env,
            "POST",
            "/v1/guest/device",
            Some(json!({ "platform": "ios", "app_version": "1.0.0" })),
            None,
            &[("x-forwarded-for", xff.as_str())],
        )
        .await;
        if i <= 5 {
            assert_eq!(
                s,
                StatusCode::OK,
                "trust off, different xff #{i} must pass (shared bucket): {body}"
            );
        } else {
            assert_eq!(
                s,
                StatusCode::TOO_MANY_REQUESTS,
                "trust off, different xff #{i} must be 429 (XFF ignored): {body}"
            );
            assert_eq!(body["code"], "rate_limited");
        }
    }
}

// ============================================================
// W5-C: лимит попыток 2FA (5 неудач / 15 мин) + сброс окна
// ============================================================

/// `POST /v1/auth/admin/2fa`: 5 неверных кодов → 401, шестой → 429;
/// после «сброса окна» (бэкдейт неудач за 15 мин) верный код → 200 и
/// рабочий JWT. Доказывает, что 2FA получила rate-limit (иначе 6-значный
/// TOTP брутфорсится), а успех выдаёт валидную пару токенов.
#[tokio::test]
async fn e2e_2fa_rate_limit_and_success() {
    let repo = match test_repo().await {
        Some(r) => r,
        None => {
            eprintln!("DATABASE_URL недоступен — e2e 2fa пропущен");
            return;
        }
    };
    let env = app_env(Some(repo.clone()), None, None).await;
    let pool = repo.pool();

    // Админ с легаси-plain TOTP-секретом (читается без DROPIT_TOTP_KEY).
    let admin_id = uuid::Uuid::now_v7();
    let email = format!("2fa-{admin_id}@dropit.test");
    let secret_b32 = "JBSWY3DPEHPK3PXP";
    sqlx::query(
        "INSERT INTO admin_user (id, email, password_hash, totp_secret_enc, totp_confirmed_at, status) \
         VALUES ($1, $2, 'x', $3, now(), 'active')",
    )
    .bind(admin_id)
    .bind(&email)
    .bind(secret_b32.as_bytes())
    .execute(&pool)
    .await
    .unwrap();

    // --- 5 неверных кодов → 401 (каждый пишет неудачу) ---
    for i in 1..=5u8 {
        let code = format!("{i:06}");
        let (s, body) = call(
            &env,
            "POST",
            "/v1/auth/admin/2fa",
            Some(json!({ "email": email, "totp_code": code })),
            None,
        )
        .await;
        assert_eq!(s, StatusCode::UNAUTHORIZED, "2fa wrong #{i}: {body}");
    }

    // --- 6-й неверный → 429 (лимит 5/15 мин) ---
    let (s, body) = call(
        &env,
        "POST",
        "/v1/auth/admin/2fa",
        Some(json!({ "email": email, "totp_code": "999999" })),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::TOO_MANY_REQUESTS, "2fa 6th must 429: {body}");
    assert_eq!(body["code"], "rate_limited");

    // --- сброс окна: бэкдейтим неудачи за пределы 15 мин ---
    sqlx::query(
        "UPDATE login_attempt SET at = now() - interval '16 minutes' \
         WHERE subject_type = 'admin' AND email = $1",
    )
    .bind(&email)
    .execute(&pool)
    .await
    .unwrap();

    // --- верный код → 200 + рабочий JWT ---
    use totp_rs::{Algorithm, Secret, TOTP};
    let bytes = Secret::Encoded(secret_b32.to_string()).to_bytes().unwrap();
    let totp = TOTP::new_unchecked(Algorithm::SHA1, 6, 1, 30, bytes);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let good_code = totp.generate(now);

    let (s, body) = call(
        &env,
        "POST",
        "/v1/auth/admin/2fa",
        Some(json!({ "email": email, "totp_code": good_code })),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK, "2fa success after reset: {body}");
    let access = body["access_token"].as_str().unwrap().to_string();
    assert!(!access.is_empty());

    // JWT рабочего: admin может на свой echo → 200.
    let (s, _) = call(&env, "GET", "/v1/admin/echo", None, Some(&access)).await;
    assert_eq!(s, StatusCode::OK, "2fa-issued jwt works on admin echo");
}
