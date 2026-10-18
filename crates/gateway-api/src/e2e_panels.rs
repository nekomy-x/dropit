//! E2e-тест роутов панелей (F7-бэкенд).
//!
//! Критерии:
//!   * platform-settings roundtrip: PATCH значений → GET возвращает;
//!     NULL-семантика (clear → NULL = «не задано»);
//!   * pricing: история Q6 — новая строка закрывает старую (valid_to);
//!   * admin dashboard/now, clubs (блокировка + аудит);
//!   * club dashboard/profile/earnings/djs;
//!   * dj shift/stats;
//!   * payments + splits;
//!   * RBAC: клуб не может в админ-роуты.
//!
//! W2-C дополнения: инвайт DJ, Stripe-онбординг (status/link), создание
//! клуба админом, список гостей клуба (пагинация), история сверок.

#![cfg(test)]

use axum::http::StatusCode;
use dropit_common::Role;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::e2e::{app_env, call, test_repo};
use dropit_infra_db::SessionRepo;

/// Процент-кодирование курсора для query-string (в курсоре есть ':' и
/// разделитель `\x1f`, не URL-safe).
fn urlenc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Берёт строковое поле из JSON-ответа.
fn sfield(v: &Value, k: &str) -> String {
    v[k].as_str().unwrap_or_default().to_string()
}

#[tokio::test]
async fn e2e_panel_routes() {
    let repo = match test_repo().await {
        Some(r) => r,
        None => {
            eprintln!("DATABASE_URL недоступен — e2e панелей пропущен");
            return;
        }
    };
    let sessions = SessionRepo::new(repo.pool());
    let env = app_env(Some(repo.clone()), Some(sessions.clone()), None).await;
    let pool = repo.pool();

    // --- Токены ------------------------------------------------------
    let admin_id = uuid::Uuid::now_v7();
    let club_id = uuid::Uuid::now_v7();
    let dj_id = uuid::Uuid::now_v7();
    let admin = env
        .keys
        .issue_access(admin_id, dropit_common::Role::Admin, uuid::Uuid::now_v7())
        .unwrap();
    let club = env
        .keys
        .issue_access(club_id, dropit_common::Role::Club, uuid::Uuid::now_v7())
        .unwrap();
    let dj = env
        .keys
        .issue_access(dj_id, dropit_common::Role::Dj, uuid::Uuid::now_v7())
        .unwrap();

    sqlx::query("INSERT INTO club (id, name, slug, email, password_hash) VALUES ($1, 'Panel Club', 'panel-club', 'panel@club.test', 'x')")
        .bind(club_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO dj (id, club_id, email, password_hash, display_name) VALUES ($1, $2, 'panel@dj.test', 'x', 'DJ Panel')")
        .bind(dj_id)
        .bind(club_id)
        .execute(&pool)
        .await
        .unwrap();

    // --- platform-settings: NULL-семантика + roundtrip --------------
    let (s, body) = call(
        &env,
        "GET",
        "/v1/admin/platform-settings",
        None,
        Some(&admin),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "settings get: {body}");
    assert_eq!(body["commission_mode"], "unset"); // стартовое значение

    // PATCH: задаём (валидные диапазоны; значения выбирает владелец — Q11).
    let (s, body) = call(
        &env,
        "PATCH",
        "/v1/admin/platform-settings",
        Some(json!({
            "commission_mode": "percent",
            "commission_percent_bp": 3000,
            "min_price_minor": 100,
            "max_price_minor": 1000
        })),
        Some(&admin),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "settings patch: {body}");
    assert_eq!(body["commission_mode"], "percent");
    assert_eq!(body["commission_percent_bp"], 3000);
    assert_eq!(body["min_price_minor"], 100);

    // Некорректный диапазон отклоняется.
    let (s, _) = call(
        &env,
        "PATCH",
        "/v1/admin/platform-settings",
        Some(json!({ "commission_percent_bp": 99999 })),
        Some(&admin),
    )
    .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "bp out of range");

    // clear → NULL («не задано»).
    let (s, body) = call(
        &env,
        "PATCH",
        "/v1/admin/platform-settings",
        Some(json!({ "clear_commission_percent_bp": true })),
        Some(&admin),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert!(
        body["commission_percent_bp"].is_null(),
        "cleared to NULL: {body}"
    );

    // Клуб НЕ может в настройки платформы.
    let (s, _) = call(
        &env,
        "GET",
        "/v1/admin/platform-settings",
        None,
        Some(&club),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "club forbidden on admin route");

    // --- pricing: история Q6 ------------------------------------------
    let (s, body) = call(
        &env,
        "POST",
        "/v1/club/pricing",
        Some(json!({ "price_minor": 300 })),
        Some(&club),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "pricing create: {body}");
    let (s, _) = call(
        &env,
        "POST",
        "/v1/club/pricing",
        Some(json!({ "price_minor": 500, "dj_share_bp": 6000 })),
        Some(&club),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED);

    let (s, body) = call(&env, "GET", "/v1/club/pricing", None, Some(&club)).await;
    assert_eq!(s, StatusCode::OK);
    let items = body["items"].as_array().unwrap();
    assert_eq!(items.len(), 2, "pricing history: {body}");
    // Q6: старая закрыта (valid_to NOT NULL), новая действует.
    // Порядок при равных микросекундах не гарантирован — ищем
    // по значению политики.
    let old = items
        .iter()
        .find(|i| i["price_minor"] == 300)
        .expect("old 300 policy");
    let new = items
        .iter()
        .find(|i| i["price_minor"] == 500)
        .expect("new 500 policy");
    assert!(!old["valid_to"].is_null(), "old policy closed (Q6)");
    assert!(new["valid_to"].is_null(), "new policy active");

    // Некорректная цена отклоняется.
    let (s, _) = call(
        &env,
        "POST",
        "/v1/club/pricing",
        Some(json!({ "price_minor": 0 })),
        Some(&club),
    )
    .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);

    // --- клуб: dashboard/profile/djs -----------------------------------
    let (s, body) = call(&env, "GET", "/v1/club/dashboard", None, Some(&club)).await;
    assert_eq!(s, StatusCode::OK, "club dashboard: {body}");
    assert_eq!(body["requests_today"], 0);

    let (s, body) = call(&env, "GET", "/v1/club/profile", None, Some(&club)).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["name"], "Panel Club");

    let (s, body) = call(
        &env,
        "PATCH",
        "/v1/club/profile",
        Some(json!({ "address": "Calle Nueva 1", "phone": "+34 600 000 000" })),
        Some(&club),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["address"], "Calle Nueva 1");

    let (s, body) = call(&env, "GET", "/v1/club/djs", None, Some(&club)).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["items"].as_array().map(Vec::len), Some(1));

    let (s, body) = call(
        &env,
        "GET",
        "/v1/club/earnings?period=week",
        None,
        Some(&club),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "earnings: {body}");
    assert_eq!(body["period"], "week");

    let (s, body) = call(&env, "GET", "/v1/club/payouts", None, Some(&club)).await;
    assert_eq!(s, StatusCode::OK, "club payouts: {body}");

    // --- админ: dashboard, clubs, блокировка + аудит -------------------
    let (s, body) = call(&env, "GET", "/v1/admin/dashboard/now", None, Some(&admin)).await;
    assert_eq!(s, StatusCode::OK, "admin dashboard: {body}");
    assert!(body["active_sessions"].is_i64());

    let (s, body) = call(&env, "GET", "/v1/admin/clubs", None, Some(&admin)).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["items"].as_array().map(Vec::len), Some(1));

    // Блокировка клуба (статус, не delete) + запись в audit_log.
    let (s, body) = call(
        &env,
        "PATCH",
        &format!("/v1/admin/clubs/{club_id}"),
        Some(json!({ "status": "blocked" })),
        Some(&admin),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "club block: {body}");
    assert_eq!(body["status"], "blocked");

    let (s, body) = call(&env, "GET", "/v1/admin/audit-log", None, Some(&admin)).await;
    assert_eq!(s, StatusCode::OK);
    let events = body["items"].as_array().unwrap();
    assert!(
        events.iter().any(|e| e["action"] == "club.status_change"),
        "audit entry written: {events:?}"
    );

    // --- DJ: shift + stats ----------------------------------------------
    // Сессия для DJ.
    let session = sessions
        .create_session(club_id, Some(dj_id), None, "Europe/Madrid")
        .await
        .unwrap();
    let (s, body) = call(&env, "GET", "/v1/dj/shift", None, Some(&dj)).await;
    assert_eq!(s, StatusCode::OK, "dj shift: {body}");
    assert_eq!(
        body["session"]["id"].as_str().unwrap(),
        session.id.to_string()
    );

    let (s, body) = call(&env, "GET", "/v1/dj/stats/today", None, Some(&dj)).await;
    assert_eq!(s, StatusCode::OK, "dj stats: {body}");
    assert_eq!(body["requests_today"], 0);

    // --- платежи + сплиты (админ) ----------------------------------------
    let (s, body) = call(&env, "GET", "/v1/admin/payments", None, Some(&admin)).await;
    assert_eq!(s, StatusCode::OK, "admin payments: {body}");
    // Сплиты пустого платежа — пустой список (404 если платежа нет).
    let fake = uuid::Uuid::now_v7();
    let (s, _) = call(
        &env,
        "GET",
        &format!("/v1/admin/payments/{fake}/splits"),
        None,
        Some(&admin),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "empty splits ok");

    // --- RBAC: гость/клуб/DJ в админ-роуты → 403 ------------------------
    for token in [&club, &dj] {
        let (s, _) = call(&env, "GET", "/v1/admin/clubs", None, Some(token)).await;
        assert_eq!(s, StatusCode::FORBIDDEN);
    }
    let (s, _) = call(&env, "GET", "/v1/club/dashboard", None, Some(&dj)).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "dj forbidden on club route");
}

// ============================================================
// W2-C: недостающие эндпоинты панелей
// ============================================================

/// Помощник: пропустить тест, если БД недоступна.
macro_rules! maybe_skip {
    ($repo:expr) => {
        match $repo {
            Some(r) => r,
            None => {
                eprintln!("DATABASE_URL недоступен — e2e панелей (W2-C) пропущен");
                return;
            }
        }
    };
}

/// `POST /v1/club/djs/invite` — успех, RBAC (401/403), изоляция клубов,
/// 409 на повтор, invite_token хранится хешем.
#[tokio::test]
async fn e2e_club_djs_invite() {
    let repo = maybe_skip!(test_repo().await);
    let sessions = SessionRepo::new(repo.pool());
    let env = app_env(Some(repo.clone()), Some(sessions), None).await;
    let pool = repo.pool();

    let club_a = Uuid::now_v7();
    let club_b = Uuid::now_v7();
    for (id, name, slug, email) in [
        (club_a, "Club A", "club-a", "a@club.test"),
        (club_b, "Club B", "club-b", "b@club.test"),
    ] {
        sqlx::query(
            "INSERT INTO club (id, name, slug, email, password_hash) \
             VALUES ($1, $2, $3, $4, 'x')",
        )
        .bind(id)
        .bind(name)
        .bind(slug)
        .bind(email)
        .execute(&pool)
        .await
        .unwrap();
    }
    let token_a = env.keys.issue_access(club_a, Role::Club, Uuid::now_v7()).unwrap();
    let token_b = env.keys.issue_access(club_b, Role::Club, Uuid::now_v7()).unwrap();
    let dj_tok = env.keys.issue_access(Uuid::now_v7(), Role::Dj, Uuid::now_v7()).unwrap();
    let admin_tok = env
        .keys
        .issue_access(Uuid::now_v7(), Role::Admin, Uuid::now_v7())
        .unwrap();

    // Успех.
    let (s, body) = call(
        &env,
        "POST",
        "/v1/club/djs/invite",
        Some(json!({ "email": "dj1@a.test", "display_name": "DJ One" })),
        Some(&token_a),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "invite: {body}");
    let dj_id: Uuid = sfield(&body, "dj_id").parse().unwrap();
    let invite_token = sfield(&body, "invite_token");
    assert!(!invite_token.is_empty());
    assert!(body["invite_expires_at"].is_string());

    // 401 без токена.
    let (s, _) = call(
        &env,
        "POST",
        "/v1/club/djs/invite",
        Some(json!({ "email": "x1@a.test", "display_name": "X" })),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);

    // 403 с чужой ролью (DJ и админ — не клуб).
    for tok in [&dj_tok, &admin_tok] {
        let (s, _) = call(
            &env,
            "POST",
            "/v1/club/djs/invite",
            Some(json!({ "email": "x2@a.test", "display_name": "X" })),
            Some(tok),
        )
        .await;
        assert_eq!(s, StatusCode::FORBIDDEN, "wrong role on invite");
    }

    // Изоляция: club_id в теле игнорируется — DJ принадлежит клубу A.
    let (s, body) = call(
        &env,
        "POST",
        "/v1/club/djs/invite",
        Some(json!({ "email": "dj2@a.test", "display_name": "DJ Two", "club_id": club_b })),
        Some(&token_a),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "invite body club_id ignored: {body}");
    let dj2: Uuid = sfield(&body, "dj_id").parse().unwrap();
    let (club_of_dj2,): (Uuid,) = sqlx::query_as("SELECT club_id FROM dj WHERE id = $1")
        .bind(dj2)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(club_of_dj2, club_a, "club_id в теле игнорируется (изоляция)");

    // 409 на повторную попытку того же e-mail в этот клуб.
    let (s, body) = call(
        &env,
        "POST",
        "/v1/club/djs/invite",
        Some(json!({ "email": "dj1@a.test", "display_name": "DJ One" })),
        Some(&token_a),
    )
    .await;
    assert_eq!(s, StatusCode::CONFLICT, "repeat invite: {body}");
    assert_eq!(body["code"], "dj_already_invited");

    // invite_token не хранится в открытом виде.
    let (hash,): (Option<String>,) =
        sqlx::query_as("SELECT invite_token_hash FROM dj WHERE id = $1")
            .bind(dj_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(hash.is_some(), "invite_token_hash хранится");
    assert_ne!(
        hash.as_deref().unwrap(),
        invite_token,
        "plain-токен не хранится"
    );
    let (count,): (i64,) = sqlx::query_as(
        "SELECT count(*) FROM dj WHERE invite_token_hash = $1",
    )
    .bind(&invite_token)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 0, "plain-токен не находится по колонке хеша");

    // DJ в статусе invited.
    let (status,): (String,) = sqlx::query_as("SELECT status FROM dj WHERE id = $1")
        .bind(dj_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(status, "invited");

    // Изоляция: клуб B не видит DJ клуба A (и наоборот).
    let (_, body) = call(&env, "GET", "/v1/club/djs", None, Some(&token_b)).await;
    assert_eq!(
        body["items"].as_array().unwrap().len(),
        0,
        "клуб B не видит DJ клуба A"
    );
    let (_, body) = call(&env, "GET", "/v1/club/djs", None, Some(&token_a)).await;
    assert_eq!(
        body["items"].as_array().unwrap().len(),
        2,
        "клуб A видит своих DJ"
    );
}

/// `GET /v1/club/stripe/onboarding-status` + `POST .../onboarding-link`:
/// none → enabled, аккаунт создаётся один раз, деградация по сохранённому.
#[tokio::test]
async fn e2e_club_stripe_onboarding() {
    let repo = maybe_skip!(test_repo().await);
    let sessions = SessionRepo::new(repo.pool());
    let env = app_env(Some(repo.clone()), Some(sessions), None).await;
    let pool = repo.pool();

    let club_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO club (id, name, slug, email, password_hash, country) \
         VALUES ($1, 'Stripe Club', 'stripe-club', 'stripe@club.test', 'x', 'ES')",
    )
    .bind(club_id)
    .execute(&pool)
    .await
    .unwrap();
    let club_tok = env.keys.issue_access(club_id, Role::Club, Uuid::now_v7()).unwrap();
    let dj_tok = env.keys.issue_access(Uuid::now_v7(), Role::Dj, Uuid::now_v7()).unwrap();
    let admin_tok = env
        .keys
        .issue_access(Uuid::now_v7(), Role::Admin, Uuid::now_v7())
        .unwrap();

    // Без аккаунта — status "none".
    let (s, body) = call(
        &env,
        "GET",
        "/v1/club/stripe/onboarding-status",
        None,
        Some(&club_tok),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "status none: {body}");
    assert_eq!(body["status"], "none");
    assert!(body["stripe_account_id"].is_null());
    assert_eq!(body["charges_enabled"], false);

    // 401/403.
    let (s, _) = call(&env, "GET", "/v1/club/stripe/onboarding-status", None, None).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let (s, _) = call(
        &env,
        "GET",
        "/v1/club/stripe/onboarding-status",
        None,
        Some(&dj_tok),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN);

    // onboarding-link: создаёт аккаунт при первом вызове.
    let (s, body) = call(
        &env,
        "POST",
        "/v1/club/stripe/onboarding-link",
        Some(json!({ "refresh_url": "https://app/refresh", "return_url": "https://app/return" })),
        Some(&club_tok),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "onboarding-link: {body}");
    assert!(body["url"].as_str().unwrap().starts_with("https://"));
    let (acct,): (Option<String>,) =
        sqlx::query_as("SELECT stripe_account_id FROM club WHERE id = $1")
            .bind(club_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(acct.is_some(), "аккаунт создан и сохранён");
    let acct_before = acct.unwrap();

    // Повторный вызов не создаёт второй аккаунт.
    let (s, body) = call(
        &env,
        "POST",
        "/v1/club/stripe/onboarding-link",
        Some(json!({ "refresh_url": "https://app/refresh", "return_url": "https://app/return" })),
        Some(&club_tok),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "onboarding-link 2: {body}");
    let (acct2,): (Option<String>,) =
        sqlx::query_as("SELECT stripe_account_id FROM club WHERE id = $1")
            .bind(club_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        acct2.as_deref(),
        Some(acct_before.as_str()),
        "второй аккаунт не создаётся"
    );

    // Некорректные URL → 422.
    let (s, _) = call(
        &env,
        "POST",
        "/v1/club/stripe/onboarding-link",
        Some(json!({ "refresh_url": "ftp://x", "return_url": "https://app/return" })),
        Some(&club_tok),
    )
    .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);

    // onboarding-status после: mock отдаёт charges/payouts/details=true →
    // status "enabled", метка сохранена как 'verified'.
    let (s, body) = call(
        &env,
        "GET",
        "/v1/club/stripe/onboarding-status",
        None,
        Some(&club_tok),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "status after: {body}");
    assert_eq!(body["status"], "enabled");
    assert_eq!(body["charges_enabled"], true);
    assert_eq!(body["payouts_enabled"], true);
    assert_eq!(body["details_submitted"], true);
    let (persisted,): (String,) =
        sqlx::query_as("SELECT stripe_onboarding_status FROM club WHERE id = $1")
            .bind(club_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(persisted, "verified", "статус сохранён в БД");

    // onboarding-link — только club_owner: админ/DJ → 403.
    for tok in [&admin_tok, &dj_tok] {
        let (s, _) = call(
            &env,
            "POST",
            "/v1/club/stripe/onboarding-link",
            Some(json!({ "refresh_url": "https://a", "return_url": "https://b" })),
            Some(tok),
        )
        .await;
        assert_eq!(s, StatusCode::FORBIDDEN, "non-club on onboarding-link");
    }
}

/// `POST /v1/admin/clubs` — успех, qr_master_token хешем, 400 на плохой
/// timezone, RBAC.
#[tokio::test]
async fn e2e_admin_clubs_create() {
    let repo = maybe_skip!(test_repo().await);
    let env = app_env(Some(repo.clone()), Some(SessionRepo::new(repo.pool())), None).await;
    let pool = repo.pool();
    let admin_tok = env
        .keys
        .issue_access(Uuid::now_v7(), Role::Admin, Uuid::now_v7())
        .unwrap();
    let club_tok = env
        .keys
        .issue_access(Uuid::now_v7(), Role::Club, Uuid::now_v7())
        .unwrap();

    // Успех.
    let (s, body) = call(
        &env,
        "POST",
        "/v1/admin/clubs",
        Some(json!({ "name": "New Club", "city": "Madrid", "timezone": "Europe/Madrid", "address": "Calle 1" })),
        Some(&admin_tok),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "create club: {body}");
    let club_id: Uuid = sfield(&body, "club_id").parse().unwrap();
    assert_eq!(body["name"], "New Club");
    let master = sfield(&body, "qr_master_token");
    assert!(!master.is_empty(), "master token выдан один раз");

    // Клуб в БД; qr_master_token хранится хешем, не открытым.
    let (name, city, tz, status, hash): (String, String, String, String, Option<String>) =
        sqlx::query_as(
            "SELECT name, city, timezone, status, qr_master_token_hash FROM club WHERE id = $1",
        )
        .bind(club_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(name, "New Club");
    assert_eq!(city, "Madrid");
    assert_eq!(tz, "Europe/Madrid");
    assert_eq!(status, "pending_verification");
    assert!(hash.is_some(), "хеш master-токена хранится");
    assert_ne!(hash.as_deref().unwrap(), master, "plain master не хранится");
    let (count,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM club WHERE qr_master_token_hash = $1")
            .bind(&master)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 0, "plain master не находится по колонке хеша");

    // 400 на плохой timezone.
    let (s, body) = call(
        &env,
        "POST",
        "/v1/admin/clubs",
        Some(json!({ "name": "TZ Club", "city": "Madrid", "timezone": "Bad/Zone" })),
        Some(&admin_tok),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "bad tz: {body}");
    assert_eq!(body["code"], "invalid_timezone");

    // 422 на слишком короткое имя.
    let (s, _) = call(
        &env,
        "POST",
        "/v1/admin/clubs",
        Some(json!({ "name": "A", "city": "Madrid", "timezone": "UTC" })),
        Some(&admin_tok),
    )
    .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);

    // 401/403.
    let (s, _) = call(
        &env,
        "POST",
        "/v1/admin/clubs",
        Some(json!({ "name": "Y Club", "city": "Madrid", "timezone": "UTC" })),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let (s, _) = call(
        &env,
        "POST",
        "/v1/admin/clubs",
        Some(json!({ "name": "Z Club", "city": "Madrid", "timezone": "UTC" })),
        Some(&club_tok),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
}

/// `GET /v1/admin/guests` — список гостей клуба, пагинация, изоляция,
/// 404 на неизвестный клуб, RBAC.
#[tokio::test]
async fn e2e_admin_guests() {
    let repo = maybe_skip!(test_repo().await);
    let sessions = SessionRepo::new(repo.pool());
    let env = app_env(Some(repo.clone()), Some(sessions.clone()), None).await;
    let pool = repo.pool();
    let admin_tok = env
        .keys
        .issue_access(Uuid::now_v7(), Role::Admin, Uuid::now_v7())
        .unwrap();
    let club_tok = env
        .keys
        .issue_access(Uuid::now_v7(), Role::Club, Uuid::now_v7())
        .unwrap();

    let club_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO club (id, name, slug, email, password_hash) \
         VALUES ($1, 'Guests Club', 'guests-club', 'g@club.test', 'x')",
    )
    .bind(club_id)
    .execute(&pool)
    .await
    .unwrap();
    let session = sessions
        .create_session(club_id, None, None, "Europe/Madrid")
        .await
        .unwrap();

    // Два гостя, один с профилем и запросом.
    let g1 = Uuid::now_v7();
    let g2 = Uuid::now_v7();
    for (id, disp, email) in [
        (g1, Some("Alice"), Some("alice@g.test")),
        (g2, None, None),
    ] {
        sqlx::query(
            "INSERT INTO guest_device (id, platform, display_name, email) \
             VALUES ($1, 'ios', $2, $3)",
        )
        .bind(id)
        .bind(disp)
        .bind(email)
        .execute(&pool)
        .await
        .unwrap();
        sessions.grant_session_guest(session.id, id).await.unwrap();
    }
    let track_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO track_cache (id, source, source_id, canonical_key, title, artist) \
         VALUES ($1, 'itunes', 't1', 'itunes:t1', 'Song', 'Artist')",
    )
    .bind(track_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO request (id, session_id, guest_device_id, track_cache_id, price_minor, status) \
         VALUES ($1, $2, $3, $4, 0, 'pending')",
    )
    .bind(Uuid::now_v7())
    .bind(session.id)
    .bind(g1)
    .bind(track_id)
    .execute(&pool)
    .await
    .unwrap();

    // Список.
    let (s, body) = call(
        &env,
        "GET",
        &format!("/v1/admin/guests?club_id={club_id}"),
        None,
        Some(&admin_tok),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "guests list: {body}");
    let items = body["items"].as_array().unwrap();
    assert_eq!(items.len(), 2, "два гостя клуба");
    let g1row = items
        .iter()
        .find(|i| i["id"].as_str().unwrap() == g1.to_string())
        .unwrap();
    assert_eq!(g1row["requests_count"], 1);
    assert_eq!(g1row["display_name"], "Alice");
    assert_eq!(g1row["email"], "alice@g.test");
    assert_eq!(g1row["banned"], false);
    assert!(g1row["created_at"].is_string());
    assert!(g1row["last_seen_at"].is_string());

    // 404 на неизвестный клуб.
    let fake = Uuid::now_v7();
    let (s, body) = call(
        &env,
        "GET",
        &format!("/v1/admin/guests?club_id={fake}"),
        None,
        Some(&admin_tok),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND, "unknown club: {body}");
    assert_eq!(body["code"], "club_not_found");

    // Отсутствие обязательного club_id → 400.
    let (s, _) = call(&env, "GET", "/v1/admin/guests", None, Some(&admin_tok)).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    // Пагинация: limit=1 → next_cursor непустой, второй запрос отдаёт след.
    let (s, body) = call(
        &env,
        "GET",
        &format!("/v1/admin/guests?club_id={club_id}&limit=1"),
        None,
        Some(&admin_tok),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["items"].as_array().unwrap().len(), 1, "первая страница = 1");
    let cursor = body["next_cursor"].as_str().unwrap().to_string();
    assert!(!cursor.is_empty(), "next_cursor непустой");
    let (s, body2) = call(
        &env,
        "GET",
        &format!(
            "/v1/admin/guests?club_id={club_id}&limit=1&cursor={}",
            urlenc(&cursor)
        ),
        None,
        Some(&admin_tok),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "вторая страница: {body2}");
    assert_eq!(
        body2["items"].as_array().unwrap().len(),
        1,
        "вторая страница = 1"
    );
    // На второй странице — другой гость.
    assert_ne!(
        body2["items"][0]["id"].as_str().unwrap(),
        body["items"][0]["id"].as_str().unwrap()
    );

    // 401/403.
    let (s, _) = call(
        &env,
        "GET",
        &format!("/v1/admin/guests?club_id={club_id}"),
        None,
        None,
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let (s, _) = call(
        &env,
        "GET",
        &format!("/v1/admin/guests?club_id={club_id}"),
        None,
        Some(&club_tok),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN);

    // Изоляция: гость другого клуба не попадает в список.
    let club2 = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO club (id, name, slug, email, password_hash) \
         VALUES ($1, 'Other Club', 'other-club', 'o@club.test', 'x')",
    )
    .bind(club2)
    .execute(&pool)
    .await
    .unwrap();
    let s2 = sessions
        .create_session(club2, None, None, "UTC")
        .await
        .unwrap();
    let g3 = Uuid::now_v7();
    sqlx::query("INSERT INTO guest_device (id, platform) VALUES ($1, 'ios')")
        .bind(g3)
        .execute(&pool)
        .await
        .unwrap();
    sessions.grant_session_guest(s2.id, g3).await.unwrap();
    let (s, body) = call(
        &env,
        "GET",
        &format!("/v1/admin/guests?club_id={club_id}"),
        None,
        Some(&admin_tok),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert!(
        body["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|i| i["id"].as_str().unwrap() != g3.to_string()),
        "гость чужого клуба не в списке"
    );
}

/// `GET /v1/admin/reconciliation-runs` — пусто → []; после вставки строки
/// приходит с корректным discrepancy_minor; RBAC.
#[tokio::test]
async fn e2e_admin_reconciliation_runs() {
    let repo = maybe_skip!(test_repo().await);
    let env = app_env(Some(repo.clone()), Some(SessionRepo::new(repo.pool())), None).await;
    let pool = repo.pool();
    let admin_tok = env
        .keys
        .issue_access(Uuid::now_v7(), Role::Admin, Uuid::now_v7())
        .unwrap();
    let club_tok = env
        .keys
        .issue_access(Uuid::now_v7(), Role::Club, Uuid::now_v7())
        .unwrap();

    // Пустая история — валидный ответ.
    let (s, body) = call(
        &env,
        "GET",
        "/v1/admin/reconciliation-runs",
        None,
        Some(&admin_tok),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "recon empty: {body}");
    assert_eq!(body["items"].as_array().unwrap().len(), 0);

    // Вставка строки напрямую SQL (status discrepancies → mismatch).
    let run_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO reconciliation_run \
         (id, period_start, period_end, status, expected_minor, actual_minor) \
         VALUES ($1, now(), now(), 'discrepancies', 100, 90)",
    )
    .bind(run_id)
    .execute(&pool)
    .await
    .unwrap();

    let (s, body) = call(
        &env,
        "GET",
        "/v1/admin/reconciliation-runs",
        None,
        Some(&admin_tok),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "recon with row: {body}");
    let items = body["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["status"], "mismatch");
    assert_eq!(items[0]["expected_minor"], 100);
    assert_eq!(items[0]["actual_minor"], 90);
    assert_eq!(items[0]["discrepancy_minor"], -10);
    assert!(items[0]["period_start"].is_string());
    assert!(items[0]["period_end"].is_string());
    assert!(items[0]["created_at"].is_string());

    // Пагинация: ещё две строки 'ok', limit=1 → next_cursor.
    for _ in 0..2 {
        let r = Uuid::now_v7();
        sqlx::query(
            "INSERT INTO reconciliation_run \
             (id, period_start, period_end, status, expected_minor, actual_minor) \
             VALUES ($1, now(), now(), 'ok', 50, 50)",
        )
        .bind(r)
        .execute(&pool)
        .await
        .unwrap();
    }
    let (s, body) = call(
        &env,
        "GET",
        "/v1/admin/reconciliation-runs?limit=1",
        None,
        Some(&admin_tok),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["items"].as_array().unwrap().len(), 1);
    assert!(
        body["next_cursor"].as_str().is_some(),
        "next_cursor присутствует"
    );
    // 'ok' статус маппится в 'ok'.
    assert_eq!(body["items"][0]["status"], "ok");
    assert_eq!(body["items"][0]["discrepancy_minor"], 0);

    // 401/403.
    let (s, _) = call(&env, "GET", "/v1/admin/reconciliation-runs", None, None).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let (s, _) = call(
        &env,
        "GET",
        "/v1/admin/reconciliation-runs",
        None,
        Some(&club_tok),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
}
