//! E2e админской аналитики (`admin_insights_routes`): обзор платформы,
//! фича-флаги и выгрузки.
//!
//! Покрывает §7.7/§7.10/§7.11 `docs/audit/web-admin-gaps.md`:
//!   * RBAC-гейт: 401 без токена, 403 чужим ролям и readonly-админу;
//!   * overview: пустая БД → нули, серии ровно 24/7/30 точек с шагом
//!     час/день и нулями в пропусках, мусорный `period` → 400;
//!     после фикстур — счётчики, суммы в minor units и ненулевые бакеты;
//!   * feature flags: засеянный набор, PATCH включает и пишет аудит,
//!     повторный GET отражает состояние, неизвестный ключ → 404,
//!     readonly-админ → 403, невалидное тело → 422;
//!   * export: CSV с шапкой и RFC 4180-экранированием, JSON с `items`,
//!     неизвестный resource/format → 400.
//!
//! Запуск (нужна живая Postgres, миграция 0020 применена):
//! `./scripts/db-test.sh cargo test -p dropit-gateway-api --bin dropit-api
//! e2e_admin_insights -- --test-threads=1`.
//!
//! `test_repo()` чистит `feature_flag` (общий харнесс), поэтому каждый тест
//! сам засевает контрактный набор ключей — ровно те же четыре строки, что
//! кладёт миграция `0020_feature_flags.sql`.

#![cfg(test)]

use axum::body::Body;
use axum::http::{HeaderMap, Request, StatusCode};
use chrono::Duration;
use dropit_common::Role;
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

use crate::e2e::{app_env, call, test_repo};
use crate::routes::ApiEnv;

/// Контрактный набор флагов (миграция 0020): ключ → описание.
const SEEDED_FLAGS: [(&str, &str); 4] = [
    (
        "payments_live",
        "Accept real payments through the live provider instead of the mock",
    ),
    (
        "guest_signup_open",
        "Allow guests to create an account with an e-mail address",
    ),
    (
        "dj_self_signup",
        "Allow DJs to register themselves without a club invitation",
    ),
    (
        "maintenance_banner",
        "Show the maintenance banner to club staff and DJs",
    ),
];

// ============================================================
// Фикстуры
// ============================================================

/// Строка `admin_user` + access-JWT (как в `e2e_admin_users.rs`).
async fn admin_fixture(env: &ApiEnv, pool: &PgPool, role: &str, email: &str) -> (Uuid, String) {
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO admin_user \
         (id, email, password_hash, totp_secret_enc, totp_confirmed_at, role, status) \
         VALUES ($1, $2, 'x', $3, now(), $4, 'active')",
    )
    .bind(id)
    .bind(email)
    .bind(b"JBSWY3DPEHPK3PXP".to_vec())
    .bind(role)
    .execute(pool)
    .await
    .expect("insert admin_user");
    let token = env
        .keys
        .issue_access(id, Role::Admin, Uuid::now_v7())
        .expect("issue admin token");
    (id, token)
}

/// Засевает контрактный набор флагов (как миграция 0020), все выключены.
async fn seed_flags(pool: &PgPool) {
    for (key, description) in SEEDED_FLAGS {
        sqlx::query(
            "INSERT INTO feature_flag (key, enabled, description) VALUES ($1, FALSE, $2) \
             ON CONFLICT (key) DO UPDATE \
             SET enabled = FALSE, description = EXCLUDED.description, \
                 updated_at = now(), updated_by = NULL",
        )
        .bind(key)
        .bind(description)
        .execute(pool)
        .await
        .expect("seed feature_flag");
    }
}

async fn create_club(pool: &PgPool, name: &str, email: &str, status: &str) -> Uuid {
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO club (id, name, slug, email, password_hash, status) \
         VALUES ($1, $2, $3, $4, 'x', $5)",
    )
    .bind(id)
    .bind(name)
    .bind(format!("club-{id}"))
    .bind(email)
    .bind(status)
    .execute(pool)
    .await
    .expect("insert club");
    id
}

async fn create_dj(pool: &PgPool, display_name: &str, email: &str, status: &str) -> Uuid {
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO dj (id, email, password_hash, display_name, status) \
         VALUES ($1, $2, 'x', $3, $4)",
    )
    .bind(id)
    .bind(email)
    .bind(display_name)
    .bind(status)
    .execute(pool)
    .await
    .expect("insert dj");
    id
}

async fn create_session(pool: &PgPool, club_id: Uuid, dj_id: Uuid, status: &str) -> Uuid {
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO session (id, club_id, dj_id, status, timezone) \
         VALUES ($1, $2, $3, $4, 'Europe/Madrid')",
    )
    .bind(id)
    .bind(club_id)
    .bind(dj_id)
    .bind(status)
    .execute(pool)
    .await
    .expect("insert session");
    id
}

async fn create_guest_device(pool: &PgPool) -> Uuid {
    let id = Uuid::now_v7();
    sqlx::query("INSERT INTO guest_device (id, platform) VALUES ($1, 'ios')")
        .bind(id)
        .execute(pool)
        .await
        .expect("insert guest_device");
    id
}

/// Запрос гостя: создаёт и трек (у `track_cache` уникален `(source, source_id)`).
async fn create_request(pool: &PgPool, session_id: Uuid, device_id: Uuid, status: &str) -> Uuid {
    let tag = Uuid::now_v7().to_string();
    let track_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO track_cache (id, source, source_id, canonical_key, title, artist) \
         VALUES ($1, 'itunes', $2, $3, 'Track', 'Artist')",
    )
    .bind(track_id)
    .bind(format!("src-{tag}"))
    .bind(format!("itunes:src-{tag}"))
    .execute(pool)
    .await
    .expect("insert track_cache");
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO request (id, session_id, guest_device_id, track_cache_id, status) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(id)
    .bind(session_id)
    .bind(device_id)
    .bind(track_id)
    .bind(status)
    .execute(pool)
    .await
    .expect("insert request");
    id
}

/// Платёж: `captured_at` — сейчас (попадает в текущий бакет серии).
async fn create_payment(
    pool: &PgPool,
    request_id: Uuid,
    status: &str,
    amount_minor: i64,
    refunded_minor: i64,
) -> Uuid {
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO payment \
         (id, request_id, idempotency_key, amount_minor, status, refunded_minor, captured_at) \
         VALUES ($1, $2, $3, $4, $5, $6, now())",
    )
    .bind(id)
    .bind(request_id)
    .bind(format!("idem-{id}"))
    .bind(amount_minor)
    .bind(status)
    .bind(refunded_minor)
    .execute(pool)
    .await
    .expect("insert payment");
    id
}

/// GET с сырым телом (CSV не парсится в JSON, `e2e::call` его не отдаст).
async fn call_raw(
    env: &ApiEnv,
    uri: &str,
    token: Option<&str>,
) -> (StatusCode, HeaderMap, Vec<u8>) {
    let router = crate::routes::router(env.clone());
    let mut request = Request::builder().method("GET").uri(uri);
    if let Some(token) = token {
        request = request.header("authorization", format!("Bearer {token}"));
    }
    let response = router
        .oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap_or_default();
    (status, headers, bytes.to_vec())
}

fn bucket_time(point: &Value) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339(
        point["bucket"]
            .as_str()
            .unwrap_or_else(|| panic!("bucket is not a string: {point}")),
    )
    .expect("bucket is RFC3339")
    .with_timezone(&chrono::Utc)
}

fn series_values(body: &Value, metric: &str) -> Vec<i64> {
    body["series"][metric]
        .as_array()
        .unwrap_or_else(|| panic!("series.{metric} missing in {body}"))
        .iter()
        .map(|point| point["value"].as_i64().expect("series value is int"))
        .collect()
}

// ============================================================
// Доступ
// ============================================================

#[tokio::test]
async fn e2e_admin_insights_access_matrix() {
    let repo = match test_repo().await {
        Some(repo) => repo,
        None => {
            eprintln!("DATABASE_URL недоступен — e2e_admin_insights пропущен");
            return;
        }
    };
    let env = app_env(Some(repo.clone()), None, None).await;
    let pool = repo.pool();
    seed_flags(&pool).await;
    let (_admin_id, admin_token) =
        admin_fixture(&env, &pool, "superadmin", "insights-admin@dropit.test").await;

    let get_routes = [
        "/v1/admin/overview",
        "/v1/admin/feature-flags",
        "/v1/admin/export?resource=clubs",
    ];

    // --- 401 без токена ---
    for uri in get_routes {
        let (status, body) = call(&env, "GET", uri, None, None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{uri} без токена: {body}");
        assert_eq!(body["code"], "unauthorized");
    }
    let (status, body) = call(
        &env,
        "PATCH",
        "/v1/admin/feature-flags/payments_live",
        Some(json!({ "enabled": true })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "PATCH без токена: {body}");

    // --- 403 чужим ролям ---
    let club_token = env
        .keys
        .issue_access(Uuid::now_v7(), Role::Club, Uuid::now_v7())
        .unwrap();
    let dj_token = env
        .keys
        .issue_access(Uuid::now_v7(), Role::Dj, Uuid::now_v7())
        .unwrap();
    let guest_token = env
        .keys
        .issue_access(Uuid::now_v7(), Role::Guest, Uuid::now_v7())
        .unwrap();
    for (label, token) in [
        ("club", &club_token),
        ("dj", &dj_token),
        ("guest", &guest_token),
    ] {
        for uri in get_routes {
            let (status, body) = call(&env, "GET", uri, None, Some(token)).await;
            assert_eq!(status, StatusCode::FORBIDDEN, "{label} на {uri}: {body}");
            assert_eq!(body["code"], "forbidden");
        }
        let (status, body) = call(
            &env,
            "PATCH",
            "/v1/admin/feature-flags/payments_live",
            Some(json!({ "enabled": true })),
            Some(token),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{label} на PATCH: {body}");
    }

    // --- 200 админу ---
    for uri in get_routes {
        let (status, body) = call(&env, "GET", uri, None, Some(&admin_token)).await;
        assert_eq!(status, StatusCode::OK, "admin на {uri}: {body}");
    }

    // --- 503 без БД: читать нечего, но и притворяться пустыми нулями нельзя ---
    // (пустой обзор и «БД не подключена» — разные факты, оператор должен
    // видеть разницу: 503 — это «не смогли посчитать», а не «всё по нулям»).
    let no_db = app_env(None, None, None).await;
    let no_db_admin = no_db
        .keys
        .issue_access(Uuid::now_v7(), Role::Admin, Uuid::now_v7())
        .unwrap();
    for uri in get_routes {
        let (status, body) = call(&no_db, "GET", uri, None, Some(&no_db_admin)).await;
        assert_eq!(
            status,
            StatusCode::SERVICE_UNAVAILABLE,
            "{uri} без БД: {body}"
        );
        assert_eq!(body["code"], "external_unavailable", "{uri}: {body}");
    }
}

// ============================================================
// Overview
// ============================================================

#[tokio::test]
async fn e2e_admin_overview_empty_database_has_zeros_and_full_series() {
    let repo = match test_repo().await {
        Some(repo) => repo,
        None => {
            eprintln!("DATABASE_URL недоступен — e2e_admin_insights пропущен");
            return;
        }
    };
    let env = app_env(Some(repo.clone()), None, None).await;
    let pool = repo.pool();
    // `guest_account` общий харнесс не чистит (soft-reference из 0010).
    sqlx::query("TRUNCATE TABLE guest_account CASCADE")
        .execute(&pool)
        .await
        .expect("truncate guest_account");
    let (_admin_id, admin_token) =
        admin_fixture(&env, &pool, "superadmin", "overview-admin@dropit.test").await;

    let (status, body) = call(&env, "GET", "/v1/admin/overview", None, Some(&admin_token)).await;
    assert_eq!(status, StatusCode::OK, "overview: {body}");
    assert_eq!(body["period"], "24h", "дефолтный период: {body}");
    assert!(
        chrono::DateTime::parse_from_rfc3339(body["generated_at"].as_str().expect("generated_at"))
            .is_ok(),
        "generated_at must be RFC3339: {body}"
    );

    // Пустая БД: все счётчики — честные нули, а не отсутствующие поля.
    for (group, keys) in [
        (
            "clubs",
            vec!["total", "active", "pending_verification", "suspended"],
        ),
        ("djs", vec!["total", "active", "invited", "suspended"]),
        ("sessions", vec!["open", "created", "paused_acceptance"]),
        (
            "requests",
            vec!["total", "pending", "accepted", "played", "rejected"],
        ),
        (
            "payments",
            vec![
                "captured_count",
                "captured_minor",
                "refunded_minor",
                "failed_count",
            ],
        ),
        ("guests", vec!["total", "online_24h"]),
    ] {
        for key in keys {
            assert_eq!(
                body[group][key].as_i64(),
                Some(0),
                "empty DB: {group}.{key} must be 0: {body}"
            );
        }
    }

    // Серии: ровно 24/7/30 точек, шаг — час/день, значения — целые нули.
    for (period, points, step) in [
        ("24h", 24, Duration::hours(1)),
        ("7d", 7, Duration::days(1)),
        ("30d", 30, Duration::days(1)),
    ] {
        let (status, body) = call(
            &env,
            "GET",
            &format!("/v1/admin/overview?period={period}"),
            None,
            Some(&admin_token),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "period={period}: {body}");
        assert_eq!(body["period"], period);
        for metric in ["requests", "revenue_minor"] {
            let points_json = body["series"][metric]
                .as_array()
                .unwrap_or_else(|| panic!("series.{metric} missing: {body}"));
            assert_eq!(
                points_json.len(),
                points,
                "period={period}: series.{metric} must have {points} points"
            );
            assert!(
                series_values(&body, metric).iter().all(|value| *value == 0),
                "period={period}: empty DB must not invent traffic"
            );
            for pair in points_json.windows(2) {
                assert_eq!(
                    bucket_time(&pair[1]) - bucket_time(&pair[0]),
                    step,
                    "period={period}: buckets must be contiguous"
                );
            }
        }
    }

    // Мусорный период — 400 с подсказкой допустимых значений.
    let (status, body) = call(
        &env,
        "GET",
        "/v1/admin/overview?period=1y",
        None,
        Some(&admin_token),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "junk period: {body}");
    assert_eq!(body["code"], "validation_error");
    assert_eq!(body["details"]["parameter"], "period");
    assert_eq!(body["details"]["allowed"], json!(["24h", "7d", "30d"]));
}

#[tokio::test]
async fn e2e_admin_overview_counts_real_rows_and_series() {
    let repo = match test_repo().await {
        Some(repo) => repo,
        None => {
            eprintln!("DATABASE_URL недоступен — e2e_admin_insights пропущен");
            return;
        }
    };
    let env = app_env(Some(repo.clone()), None, None).await;
    let pool = repo.pool();
    sqlx::query("TRUNCATE TABLE guest_account CASCADE")
        .execute(&pool)
        .await
        .expect("truncate guest_account");
    let (_admin_id, admin_token) =
        admin_fixture(&env, &pool, "superadmin", "counts-admin@dropit.test").await;

    // Клубы: 2 (active + pending_verification), DJ: 2 (active + invited).
    let club_active = create_club(&pool, "Overview Club", "overview@club.test", "active").await;
    create_club(
        &pool,
        "Overview Pending",
        "pending@club.test",
        "pending_verification",
    )
    .await;
    let dj_active = create_dj(&pool, "Overview DJ", "overview@dj.test", "active").await;
    create_dj(&pool, "Invited DJ", "invited@dj.test", "invited").await;

    // Сессии: все три состояния из контракта.
    let session_active = create_session(&pool, club_active, dj_active, "active").await;
    create_session(&pool, club_active, dj_active, "created").await;
    create_session(&pool, club_active, dj_active, "paused_acceptance").await;

    // Запросы: pending + played.
    let device = create_guest_device(&pool).await;
    let request_pending = create_request(&pool, session_active, device, "pending").await;
    let request_played = create_request(&pool, session_active, device, "played").await;

    // Гость с аккаунтом (для guests.total) — устройство уже «онлайн».
    let guest_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO guest_account (id, email_normalized, email_display, verified) \
         VALUES ($1, 'counts@guest.test', 'counts@guest.test', TRUE)",
    )
    .bind(guest_id)
    .execute(&pool)
    .await
    .expect("insert guest_account");

    // Платежи: 500 captured, 200 частично возвращён (50), 100 failed.
    create_payment(&pool, request_pending, "captured", 500, 0).await;
    create_payment(&pool, request_played, "partially_refunded", 200, 50).await;
    create_payment(&pool, request_played, "failed", 100, 0).await;

    let (status, body) = call(&env, "GET", "/v1/admin/overview", None, Some(&admin_token)).await;
    assert_eq!(status, StatusCode::OK, "overview: {body}");

    assert_eq!(body["clubs"]["total"], 2, "{body}");
    assert_eq!(body["clubs"]["active"], 1);
    assert_eq!(body["clubs"]["pending_verification"], 1);
    assert_eq!(body["clubs"]["suspended"], 0);

    assert_eq!(body["djs"]["total"], 2);
    assert_eq!(body["djs"]["active"], 1);
    assert_eq!(body["djs"]["invited"], 1);
    assert_eq!(body["djs"]["suspended"], 0);

    assert_eq!(body["sessions"]["open"], 1);
    assert_eq!(body["sessions"]["created"], 1);
    assert_eq!(body["sessions"]["paused_acceptance"], 1);

    assert_eq!(body["requests"]["total"], 2);
    assert_eq!(body["requests"]["pending"], 1);
    assert_eq!(body["requests"]["played"], 1);
    assert_eq!(body["requests"]["accepted"], 0);
    assert_eq!(body["requests"]["rejected"], 0);

    // Деньги — minor units: 500 + 200 captured, 50 возвращено, 1 failed.
    assert_eq!(body["payments"]["captured_count"], 2, "{body}");
    assert_eq!(body["payments"]["captured_minor"], 700);
    assert_eq!(body["payments"]["refunded_minor"], 50);
    assert_eq!(body["payments"]["failed_count"], 1);

    assert_eq!(body["guests"]["total"], 1);
    assert_eq!(body["guests"]["online_24h"], 1);

    // Серии: запросы и чистая выручка попадают в текущий бакет.
    assert_eq!(
        series_values(&body, "requests").iter().sum::<i64>(),
        2,
        "series.requests must sum to the request count: {body}"
    );
    assert_eq!(
        series_values(&body, "revenue_minor").iter().sum::<i64>(),
        650,
        "revenue = (500 + 200) - 50, failed and refunds excluded: {body}"
    );
    assert!(
        series_values(&body, "requests")
            .iter()
            .any(|value| *value > 0),
        "fresh requests must land in a non-zero bucket: {body}"
    );

    // Тот же ответ за неделю: счётчики те же (это перепись), серия — 7 точек.
    let (status, week) = call(
        &env,
        "GET",
        "/v1/admin/overview?period=7d",
        None,
        Some(&admin_token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(week["period"], "7d");
    assert_eq!(week["requests"]["total"], 2, "{week}");
    assert_eq!(
        series_values(&week, "revenue_minor").iter().sum::<i64>(),
        650,
        "weekly revenue covers the same fixtures: {week}"
    );
    assert_eq!(series_values(&week, "revenue_minor").len(), 7);
}

// ============================================================
// Feature flags
// ============================================================

#[tokio::test]
async fn e2e_admin_feature_flags_list_patch_audit_and_guards() {
    let repo = match test_repo().await {
        Some(repo) => repo,
        None => {
            eprintln!("DATABASE_URL недоступен — e2e_admin_insights пропущен");
            return;
        }
    };
    let env = app_env(Some(repo.clone()), None, None).await;
    let pool = repo.pool();
    seed_flags(&pool).await;
    let (_admin_id, admin_token) =
        admin_fixture(&env, &pool, "superadmin", "flags-admin@dropit.test").await;
    let (_readonly_id, readonly_token) =
        admin_fixture(&env, &pool, "readonly", "flags-readonly@dropit.test").await;

    // --- список: контрактный набор, все выключены, описания непустые ---
    let (status, body) = call(
        &env,
        "GET",
        "/v1/admin/feature-flags",
        None,
        Some(&admin_token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let items = body["items"].as_array().expect("items array");
    let keys: Vec<&str> = items
        .iter()
        .filter_map(|item| item["key"].as_str())
        .collect();
    for (key, _) in SEEDED_FLAGS {
        assert!(keys.contains(&key), "flag {key} must be listed: {body}");
    }
    for item in items {
        assert_eq!(item["enabled"], false, "seed must be off: {item}");
        assert!(
            item["description"]
                .as_str()
                .is_some_and(|value| !value.is_empty()),
            "flag needs a human description: {item}"
        );
        assert!(item["updated_at"].is_string(), "{item}");
        assert!(item.get("updated_by").is_some(), "{item}");
    }

    // --- неизвестный ключ → 404 ---
    let (status, body) = call(
        &env,
        "PATCH",
        "/v1/admin/feature-flags/does_not_exist",
        Some(json!({ "enabled": true })),
        Some(&admin_token),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["code"], "not_found");

    // --- readonly-админ писать не может ---
    let (status, body) = call(
        &env,
        "PATCH",
        "/v1/admin/feature-flags/payments_live",
        Some(json!({ "enabled": true })),
        Some(&readonly_token),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "readonly PATCH: {body}");
    assert_eq!(body["code"], "forbidden");

    // --- невалидное тело → 422 (флаг не меняется) ---
    let (status, body) = call(
        &env,
        "PATCH",
        "/v1/admin/feature-flags/payments_live",
        Some(json!({ "enabled": "yes" })),
        Some(&admin_token),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["code"], "validation_error");
    assert_eq!(body["details"]["field"], "enabled");

    // --- включение суперадмином ---
    let (status, body) = call(
        &env,
        "PATCH",
        "/v1/admin/feature-flags/payments_live",
        Some(json!({ "enabled": true })),
        Some(&admin_token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "PATCH: {body}");
    assert_eq!(body["key"], "payments_live");
    assert_eq!(body["enabled"], true);
    assert!(
        chrono::DateTime::parse_from_rfc3339(body["updated_at"].as_str().expect("updated_at"))
            .is_ok(),
        "updated_at must be RFC3339: {body}"
    );

    // Повторный GET отражает новое состояние.
    let (status, body) = call(
        &env,
        "GET",
        "/v1/admin/feature-flags",
        None,
        Some(&admin_token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let flag = body["items"]
        .as_array()
        .expect("items")
        .iter()
        .find(|item| item["key"] == "payments_live")
        .expect("payments_live listed");
    assert_eq!(flag["enabled"], true, "GET must reflect PATCH: {body}");

    // --- аудит: одна запись, before/after в diff ---
    let (count,): (i64,) = sqlx::query_as(
        "SELECT count(*) FROM audit_log \
         WHERE action = 'admin.feature_flag.update' AND entity = 'payments_live'",
    )
    .fetch_one(&pool)
    .await
    .expect("audit count");
    assert_eq!(count, 1, "exactly one audit row per mutation");
    let (diff,): (Value,) = sqlx::query_as(
        "SELECT diff FROM audit_log \
         WHERE action = 'admin.feature_flag.update' AND entity = 'payments_live' \
         ORDER BY at DESC LIMIT 1",
    )
    .fetch_one(&pool)
    .await
    .expect("audit diff");
    assert_eq!(diff["before"], false, "{diff}");
    assert_eq!(diff["after"], true, "{diff}");

    // Возврат в исходное состояние — тест не оставляет включённый live-режим.
    let (status, _) = call(
        &env,
        "PATCH",
        "/v1/admin/feature-flags/payments_live",
        Some(json!({ "enabled": false })),
        Some(&admin_token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

// ============================================================
// Export
// ============================================================

#[tokio::test]
async fn e2e_admin_export_csv_and_json() {
    let repo = match test_repo().await {
        Some(repo) => repo,
        None => {
            eprintln!("DATABASE_URL недоступен — e2e_admin_insights пропущен");
            return;
        }
    };
    let env = app_env(Some(repo.clone()), None, None).await;
    let pool = repo.pool();
    let (_admin_id, admin_token) =
        admin_fixture(&env, &pool, "superadmin", "export-admin@dropit.test").await;

    // Имя с запятой и кавычкой — проверка RFC 4180-экранирования.
    let club_id = create_club(&pool, "Night, \"Club\" & Bar", "night@club.test", "active").await;
    let dj_id = create_dj(&pool, "Export DJ", "export@dj.test", "active").await;
    let session_id = create_session(&pool, club_id, dj_id, "active").await;
    let device = create_guest_device(&pool).await;
    let request_id = create_request(&pool, session_id, device, "pending").await;
    create_payment(&pool, request_id, "captured", 250, 0).await;

    // --- CSV: шапка, экранирование, заголовки ответа ---
    let (status, headers, bytes) = call_raw(
        &env,
        "/v1/admin/export?resource=clubs&format=csv",
        Some(&admin_token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers
            .get("content-type")
            .and_then(|value| value.to_str().ok()),
        Some("text/csv; charset=utf-8")
    );
    let disposition = headers
        .get("content-disposition")
        .and_then(|value| value.to_str().ok())
        .expect("content-disposition");
    assert!(
        disposition.starts_with("attachment; filename=\"dropit-clubs-")
            && disposition.ends_with(".csv\""),
        "filename must name the resource: {disposition}"
    );
    let csv = String::from_utf8(bytes).expect("csv is utf-8");
    let mut lines = csv.split("\r\n");
    assert_eq!(
        lines.next(),
        Some("id,name,slug,email,status,created_at"),
        "header row: {csv}"
    );
    let row = lines.next().expect("data row");
    assert!(
        row.contains("\"Night, \"\"Club\"\" & Bar\""),
        "comma and quote must be escaped per RFC 4180: {row}"
    );
    assert!(row.contains("night@club.test"), "{row}");

    // --- JSON: items с типизированными значениями ---
    let (status, body) = call(
        &env,
        "GET",
        "/v1/admin/export?resource=clubs&format=json",
        None,
        Some(&admin_token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["resource"], "clubs");
    assert!(body["generated_at"].is_string());
    let items = body["items"].as_array().expect("items array");
    assert_eq!(items.len(), 1, "{body}");
    assert_eq!(items[0]["name"], "Night, \"Club\" & Bar");
    assert_eq!(items[0]["status"], "active");

    // --- users: тот же UNION, что каталог §7.1 ---
    let (status, body) = call(
        &env,
        "GET",
        "/v1/admin/export?resource=users&format=json",
        None,
        Some(&admin_token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let roles: Vec<&str> = body["items"]
        .as_array()
        .expect("items")
        .iter()
        .filter_map(|item| item["role"].as_str())
        .collect();
    assert!(roles.contains(&"admin"), "admin row: {body}");
    assert!(roles.contains(&"club_staff"), "club row: {body}");
    assert!(roles.contains(&"dj"), "dj row: {body}");

    // --- payments: деньги в minor units, без пересчёта в валюту ---
    let (status, headers, bytes) = call_raw(
        &env,
        "/v1/admin/export?resource=payments&format=csv",
        Some(&admin_token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(headers.contains_key("content-disposition"));
    let csv = String::from_utf8(bytes).expect("csv is utf-8");
    assert!(
        csv.starts_with(
            "id,request_id,status,amount_minor,currency,refunded_minor,captured_at,created_at\r\n"
        ),
        "payments header: {csv}"
    );
    assert!(csv.contains(",250,EUR,0,"), "minor units: {csv}");

    // --- неизвестные параметры → 400 с подсказкой ---
    for uri in [
        "/v1/admin/export?resource=audit&format=csv",
        "/v1/admin/export?resource=clubs&format=xml",
        "/v1/admin/export?format=csv",
    ] {
        let (status, body) = call(&env, "GET", uri, None, Some(&admin_token)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}: {body}");
        assert_eq!(body["code"], "validation_error", "{uri}: {body}");
    }
}
