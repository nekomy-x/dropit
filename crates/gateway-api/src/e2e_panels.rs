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
//!   * RBAC: клуб не может в админ-роуты;
//!   * режим эксплуатации стенда: `runtime_mode` в platform-settings и
//!     публичный `GET /v1/status` (test / maintenance / normal).
//!
//! W2-C дополнения: инвайт DJ, Stripe-онбординг (status/link), создание
//! клуба админом, список гостей клуба (пагинация), история сверок.

#![cfg(test)]

use axum::http::StatusCode;
use dropit_common::Role;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::e2e::{app_env, call, call_with_headers, test_repo};
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
        "PATCH",
        &format!("/v1/club/djs/{dj_id}"),
        Some(json!({ "status": "suspended" })),
        Some(&club),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "suspend DJ: {body}");
    assert_eq!(body["status"], "suspended");
    let (s, body) = call(
        &env,
        "PATCH",
        &format!("/v1/club/djs/{dj_id}"),
        Some(json!({ "status": "active" })),
        Some(&club),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "restore DJ: {body}");
    assert_eq!(body["status"], "active");
    let (s, _) = call(
        &env,
        "PATCH",
        &format!("/v1/club/djs/{dj_id}"),
        Some(json!({ "status": "blocked" })),
        Some(&club),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let foreign_dj = Uuid::now_v7();
    let (s, _) = call(
        &env,
        "PATCH",
        &format!("/v1/club/djs/{foreign_dj}"),
        Some(json!({ "status": "suspended" })),
        Some(&club),
    )
    .await;
    assert_eq!(
        s,
        StatusCode::NOT_FOUND,
        "club cannot mutate another club's DJ"
    );
    let (s, _) = call(
        &env,
        "PATCH",
        &format!("/v1/club/djs/{dj_id}"),
        Some(json!({ "status": "suspended" })),
        Some(&dj),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "DJ cannot manage club roster");

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
    let token_a = env
        .keys
        .issue_access(club_a, Role::Club, Uuid::now_v7())
        .unwrap();
    let token_b = env
        .keys
        .issue_access(club_b, Role::Club, Uuid::now_v7())
        .unwrap();
    let dj_tok = env
        .keys
        .issue_access(Uuid::now_v7(), Role::Dj, Uuid::now_v7())
        .unwrap();
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
    assert_eq!(
        s,
        StatusCode::CREATED,
        "invite body club_id ignored: {body}"
    );
    let dj2: Uuid = sfield(&body, "dj_id").parse().unwrap();
    let (club_of_dj2,): (Uuid,) = sqlx::query_as("SELECT club_id FROM dj WHERE id = $1")
        .bind(dj2)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        club_of_dj2, club_a,
        "club_id в теле игнорируется (изоляция)"
    );

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
    let (count,): (i64,) = sqlx::query_as("SELECT count(*) FROM dj WHERE invite_token_hash = $1")
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

    // Одноразовая активация задаёт пароль и очищает приглашение.
    let (s, body) = call(
        &env,
        "POST",
        "/v1/dj/invite/accept",
        Some(json!({ "invite_token": invite_token, "password": "dj-password-123" })),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK, "accept invite: {body}");
    assert_eq!(body["accepted"], true);
    let (status, invite_hash): (String, Option<String>) =
        sqlx::query_as("SELECT status, invite_token_hash FROM dj WHERE id = $1")
            .bind(dj_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(status, "active");
    assert!(invite_hash.is_none(), "accepted invite token is cleared");

    let (s, _) = call(
        &env,
        "POST",
        "/v1/dj/invite/accept",
        Some(json!({ "invite_token": invite_token, "password": "dj-password-123" })),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED, "invite cannot be reused");

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
    let club_tok = env
        .keys
        .issue_access(club_id, Role::Club, Uuid::now_v7())
        .unwrap();
    let dj_tok = env
        .keys
        .issue_access(Uuid::now_v7(), Role::Dj, Uuid::now_v7())
        .unwrap();
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
    let env = app_env(
        Some(repo.clone()),
        Some(SessionRepo::new(repo.pool())),
        None,
    )
    .await;
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
    for (id, disp, email) in [(g1, Some("Alice"), Some("alice@g.test")), (g2, None, None)] {
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
    assert_eq!(
        body["items"].as_array().unwrap().len(),
        1,
        "первая страница = 1"
    );
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
    let env = app_env(
        Some(repo.clone()),
        Some(SessionRepo::new(repo.pool())),
        None,
    )
    .await;
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

// ============================================================
// W5-C: вход клуба по master-токену (POST /v1/auth/club/master)
// ============================================================

/// `POST /v1/auth/club/master`: верный токен → 200 + рабочий JWT (доступ к
/// `/v1/club/dashboard`), неверный → 401 `invalid_credentials`, пустой → 400
/// `invalid_request`, 11 попыток с одного IP → 429 `too_many_attempts`.
/// Клуб создаётся через `POST /v1/admin/clubs` (password_hash='!' — обычный
/// логин ему недоступен); master-токен — единственный способ входа в панель.
#[tokio::test]
async fn e2e_club_master_login() {
    let repo = maybe_skip!(test_repo().await);
    let env = app_env(
        Some(repo.clone()),
        Some(SessionRepo::new(repo.pool())),
        None,
    )
    .await;
    let pool = repo.pool();
    // XFF-флуд-сценарий ниже считает лимит по IP из заголовка. После
    // смены безопасного дефолта (TRUST_PROXY по умолчанию ВЫКЛ) заголовок
    // игнорируется без явного включения — поэтому тест явно включает
    // доверие, чтобы бакет флуда (198.51.100.10) изолировался от
    // предшествующей неудачной попытки (без XFF → бакет 0.0.0.0). Гард
    // восстанавливает env после теста.
    let _trust = crate::e2e::EnvGuard::set("TRUST_PROXY", "1");
    let admin_tok = env
        .keys
        .issue_access(Uuid::now_v7(), Role::Admin, Uuid::now_v7())
        .unwrap();

    // --- Создаём клуб админом → получаем qr_master_token (один раз) ---
    let (s, body) = call(
        &env,
        "POST",
        "/v1/admin/clubs",
        Some(json!({ "name": "Master Club", "city": "Madrid", "timezone": "Europe/Madrid" })),
        Some(&admin_tok),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "create club: {body}");
    let club_id: Uuid = sfield(&body, "club_id").parse().unwrap();
    let master = sfield(&body, "qr_master_token");
    assert!(!master.is_empty());

    // --- Верный токен → 200 + пара JWT + club_id + role ---
    let (s, body) = call(
        &env,
        "POST",
        "/v1/auth/club/master",
        Some(json!({ "qr_master_token": master })),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK, "master login: {body}");
    let access = body["access_token"].as_str().unwrap().to_string();
    let refresh = body["refresh_token"].as_str().unwrap().to_string();
    assert!(!access.is_empty(), "access token issued");
    assert!(!refresh.is_empty(), "refresh token issued");
    assert_eq!(body["expires_in"], 900, "15 min access ttl");
    assert_eq!(body["club_id"], club_id.to_string());
    assert_eq!(body["role"], "club");

    // JWT рабочий: клуб попадает на свой dashboard → 200.
    let (s, body) = call(&env, "GET", "/v1/club/dashboard", None, Some(&access)).await;
    assert_eq!(s, StatusCode::OK, "master-issued jwt on dashboard: {body}");

    // В audit_log записан club_master_login (без токена).
    let (n,): (i64,) = sqlx::query_as(
        "SELECT count(*) FROM audit_log WHERE action = 'club_master_login' AND actor_id = $1",
    )
    .bind(club_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(n, 1, "club_master_login audit entry written");

    // --- Неверный токен → 401 invalid_credentials ---
    let (s, body) = call(
        &env,
        "POST",
        "/v1/auth/club/master",
        Some(json!({ "qr_master_token": "deadbeef".repeat(8) })),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED, "wrong token: {body}");
    assert_eq!(body["code"], "invalid_credentials");

    // --- Пустой токен → 400 invalid_request ---
    let (s, body) = call(
        &env,
        "POST",
        "/v1/auth/club/master",
        Some(json!({ "qr_master_token": "" })),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "empty token: {body}");
    assert_eq!(body["code"], "invalid_request");

    // --- 11 попыток с одного IP (валидный формат, неверные токены) → 429 ---
    // Первые 10: format ok → лимит не превышен → verify fail → 401 (record).
    // 11-й: count=10 → 429 too_many_attempts.
    let xff = "198.51.100.10";
    for i in 1..=11u32 {
        // 64 hex-символа: валидный формат, но не совпадает ни с одним клубом.
        let wrong = format!("deadbeef{i:058x}");
        let (s, body) = call_with_headers(
            &env,
            "POST",
            "/v1/auth/club/master",
            Some(json!({ "qr_master_token": wrong })),
            None,
            &[("x-forwarded-for", xff)],
        )
        .await;
        if i <= 10 {
            assert_eq!(
                s,
                StatusCode::UNAUTHORIZED,
                "flood #{i} must be 401: {body}"
            );
        } else {
            assert_eq!(
                s,
                StatusCode::TOO_MANY_REQUESTS,
                "flood #{i} must be 429: {body}"
            );
            assert_eq!(body["code"], "too_many_attempts");
        }
    }
}

// ============================================================
// W7-C дефект 2: фильтры status/club_id в GET /v1/admin/payments
// ============================================================

/// Сижим цепочку `club → session → guest_device → track_cache → request`
/// и возвращаем `(club_id, request_id)` для вставки платежей в тесте
/// фильтров. Уникальные `slug`/`email` через `tag` (UNIQUE-констрейнты).
async fn seed_club_request(pool: &sqlx::PgPool, tag: &str) -> (Uuid, Uuid) {
    let club_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO club (id, name, slug, email, password_hash) \
         VALUES ($1, $2, $2, $3, 'x')",
    )
    .bind(club_id)
    .bind(format!("club-{tag}"))
    .bind(format!("club-{tag}@dropit.test"))
    .execute(pool)
    .await
    .unwrap();
    let session_id = Uuid::now_v7();
    sqlx::query("INSERT INTO session (id, club_id, timezone) VALUES ($1, $2, 'Europe/Madrid')")
        .bind(session_id)
        .bind(club_id)
        .execute(pool)
        .await
        .unwrap();
    let guest_id = Uuid::now_v7();
    sqlx::query("INSERT INTO guest_device (id, platform) VALUES ($1, 'ios')")
        .bind(guest_id)
        .execute(pool)
        .await
        .unwrap();
    let track_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO track_cache (id, source, source_id, canonical_key, title, artist) \
         VALUES ($1, 'itunes', $2, $3, 'T', 'A')",
    )
    .bind(track_id)
    .bind(format!("tr-{tag}"))
    .bind(format!("itunes:tr-{tag}"))
    .execute(pool)
    .await
    .unwrap();
    let request_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO request (id, session_id, guest_device_id, track_cache_id) \
         VALUES ($1, $2, $3, $4)",
    )
    .bind(request_id)
    .bind(session_id)
    .bind(guest_id)
    .bind(track_id)
    .execute(pool)
    .await
    .unwrap();
    (club_id, request_id)
}

/// Вставляет `payment` с заданным статусом и суммой; возвращает его id.
/// `idempotency_key` уникален (свежий UUIDv7) — UNIQUE-констрейнт соблюдён.
async fn insert_payment(pool: &sqlx::PgPool, request_id: Uuid, status: &str, amount: i64) -> Uuid {
    let id = Uuid::now_v7();
    let idem = format!("idem-{}-{status}", Uuid::now_v7());
    sqlx::query(
        "INSERT INTO payment (id, request_id, idempotency_key, amount_minor, status) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(id)
    .bind(request_id)
    .bind(idem)
    .bind(amount)
    .bind(status)
    .execute(pool)
    .await
    .unwrap();
    id
}

/// `GET /v1/admin/payments`: фильтры `status` и `club_id` реально работают
/// (раньше параметры помечались `#[allow(dead_code)]` и молча игнорировались
/// — SQL без WHERE). Проверки:
///   * без параметров — все платежи (прежнее поведение сохранено);
///   * `?status=captured` → только captured (3 шт: 2 из клуба A + 1 из B);
///   * `?status=bogus` → `400 invalid_status` (валидация по списку статусов);
///   * `?club_id=A` → изоляция клуба (3 платежа клуба A);
///   * `?club_id=B&status=intent` → композиция (1);
///   * `?club_id=A&status=failed` → композиция (1);
///   * RBAC: клуб → 403.
#[tokio::test]
async fn e2e_admin_payments_filters() {
    let repo = maybe_skip!(test_repo().await);
    let env = app_env(
        Some(repo.clone()),
        Some(SessionRepo::new(repo.pool())),
        None,
    )
    .await;
    let pool = repo.pool();
    let admin = env
        .keys
        .issue_access(Uuid::now_v7(), Role::Admin, Uuid::now_v7())
        .unwrap();

    // RBAC: клуб не может в админ-платежи.
    let club_tok = env
        .keys
        .issue_access(Uuid::now_v7(), Role::Club, Uuid::now_v7())
        .unwrap();
    let (s, _) = call(&env, "GET", "/v1/admin/payments", None, Some(&club_tok)).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "club forbidden on admin payments");

    // --- Сижим два клуба с запросами ---
    let tag = Uuid::now_v7();
    let (club_a, req_a) = seed_club_request(&pool, &format!("a-{tag}")).await;
    let (club_b, req_b) = seed_club_request(&pool, &format!("b-{tag}")).await;

    // club_a: 2 captured + 1 failed; club_b: 1 captured + 1 intent.
    // Итого: 3 captured, 1 failed, 1 intent = 5 платежей.
    insert_payment(&pool, req_a, "captured", 300).await;
    insert_payment(&pool, req_a, "captured", 500).await;
    insert_payment(&pool, req_a, "failed", 700).await;
    insert_payment(&pool, req_b, "captured", 400).await;
    insert_payment(&pool, req_b, "intent", 200).await;

    // --- Без параметров: все 5 (преждеё поведение сохранено) ---
    let (s, body) = call(&env, "GET", "/v1/admin/payments", None, Some(&admin)).await;
    assert_eq!(s, StatusCode::OK, "no params: {body}");
    assert_eq!(
        body["items"].as_array().map(Vec::len),
        Some(5),
        "no filter → all 5 payments: {body}"
    );

    // --- status=captured: 3 (2 из A + 1 из B), все со статусом captured ---
    let (s, body) = call(
        &env,
        "GET",
        "/v1/admin/payments?status=captured",
        None,
        Some(&admin),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "status=captured: {body}");
    let caps = body["items"].as_array().expect("items array");
    assert_eq!(caps.len(), 3, "captured count: {body}");
    assert!(
        caps.iter().all(|p| p["status"] == "captured"),
        "all items are captured: {body}"
    );

    // --- status=bogus → 400 invalid_status (валидация по списку) ---
    let (s, body) = call(
        &env,
        "GET",
        "/v1/admin/payments?status=bogus",
        None,
        Some(&admin),
    )
    .await;
    assert_eq!(
        s,
        StatusCode::BAD_REQUEST,
        "invalid status must be 400: {body}"
    );
    assert_eq!(body["code"], "invalid_status", "error code: {body}");

    // --- club_id=club_a: 3 платежа (изоляция клуба через JOIN) ---
    let (s, body) = call(
        &env,
        "GET",
        &format!("/v1/admin/payments?club_id={club_a}"),
        None,
        Some(&admin),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "club filter: {body}");
    assert_eq!(
        body["items"].as_array().map(Vec::len),
        Some(3),
        "club_a isolated to its 3 payments: {body}"
    );

    // --- club_id=club_b&status=intent: 1 (композиция фильтров) ---
    let (s, body) = call(
        &env,
        "GET",
        &format!("/v1/admin/payments?club_id={club_b}&status=intent"),
        None,
        Some(&admin),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "club+status: {body}");
    let items = body["items"].as_array().expect("items array");
    assert_eq!(items.len(), 1, "club_b intent: {body}");
    assert_eq!(items[0]["status"], "intent");

    // --- club_id=club_a&status=failed: 1 ---
    let (s, body) = call(
        &env,
        "GET",
        &format!("/v1/admin/payments?club_id={club_a}&status=failed"),
        None,
        Some(&admin),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "club+status failed: {body}");
    assert_eq!(
        body["items"].as_array().map(Vec::len),
        Some(1),
        "club_a failed: {body}"
    );
}

/// Управление составом DJ: заметка, присутствие, мягкое удаление,
/// восстановление и жёсткое удаление непринятого приглашения.
///
/// Контракт, который проверяется:
///   * `note` — независимое от `status` поле; `null` очищает;
///   * `online` считается сервером по `last_seen_at` (heartbeat DJ), а не
///     присылается клиентом;
///   * `DELETE` не стирает историю: DJ с паролем остаётся строкой с
///     `removed_at`, исчезает из состава и возвращается `/restore`;
///   * непринятое приглашение удаляется физически, чтобы тот же e-mail
///     можно было пригласить снова (UNIQUE(email));
///   * DJ с открытой сменой снять нельзя (409 `dj_on_shift`);
///   * чужой DJ не меняется (404), DJ не управляет составом (403).
#[tokio::test]
async fn e2e_club_dj_management() {
    let repo = maybe_skip!(test_repo().await);
    let sessions = SessionRepo::new(repo.pool());
    let env = app_env(Some(repo.clone()), Some(sessions), None).await;
    let pool = repo.pool();

    let club_id = Uuid::now_v7();
    let other_club = Uuid::now_v7();
    for (id, name, slug, email) in [
        (club_id, "Roster Club", "roster-club", "roster@club.test"),
        (other_club, "Other Club", "other-club", "other@club.test"),
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
    let club_tok = env
        .keys
        .issue_access(club_id, Role::Club, Uuid::now_v7())
        .unwrap();
    let other_tok = env
        .keys
        .issue_access(other_club, Role::Club, Uuid::now_v7())
        .unwrap();

    // DJ, принявший приглашение (есть пароль) и непринятый DJ.
    let active_dj = Uuid::now_v7();
    let invited_dj = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO dj (id, club_id, email, password_hash, display_name, status) \
         VALUES ($1, $2, 'active@dj.test', 'x', 'Active DJ', 'active')",
    )
    .bind(active_dj)
    .bind(club_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO dj (id, club_id, email, password_hash, display_name, status) \
         VALUES ($1, $2, 'invited@dj.test', '!', 'Invited DJ', 'invited')",
    )
    .bind(invited_dj)
    .bind(club_id)
    .execute(&pool)
    .await
    .unwrap();

    // --- список: присутствие и смена считаются сервером ---------------
    let (s, body) = call(&env, "GET", "/v1/club/djs", None, Some(&club_tok)).await;
    assert_eq!(s, StatusCode::OK, "list: {body}");
    let items = body["items"].as_array().expect("items");
    assert_eq!(items.len(), 2, "two DJs: {body}");
    for item in items {
        assert_eq!(item["online"], false, "no heartbeat → offline: {item}");
        assert_eq!(item["on_shift"], false, "no session → off shift: {item}");
        assert!(item["note"].is_null(), "no note yet: {item}");
    }
    assert!(body["presence_ttl_seconds"].as_i64().is_some());

    // --- заметка: сохраняется, читается, очищается --------------------
    let (s, body) = call(
        &env,
        "PATCH",
        &format!("/v1/club/djs/{active_dj}"),
        Some(json!({ "note": "  vinyl only  " })),
        Some(&club_tok),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "set note: {body}");
    assert_eq!(body["note"], "vinyl only", "note is trimmed");
    assert_eq!(body["status"], "active", "note patch leaves status alone");

    let (_, body) = call(&env, "GET", "/v1/club/djs", None, Some(&club_tok)).await;
    let row = body["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"] == active_dj.to_string())
        .expect("active dj listed");
    assert_eq!(row["note"], "vinyl only");

    let (s, body) = call(
        &env,
        "PATCH",
        &format!("/v1/club/djs/{active_dj}"),
        Some(json!({ "note": null })),
        Some(&club_tok),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "clear note: {body}");
    assert!(body["note"].is_null(), "note cleared: {body}");

    // Пустое тело и слишком длинная заметка — 400.
    let (s, body) = call(
        &env,
        "PATCH",
        &format!("/v1/club/djs/{active_dj}"),
        Some(json!({})),
        Some(&club_tok),
    )
    .await;
    assert_eq!(
        s,
        StatusCode::BAD_REQUEST,
        "empty patch is rejected: {body}"
    );
    let too_long = "x".repeat(501);
    let (s, _) = call(
        &env,
        "PATCH",
        &format!("/v1/club/djs/{active_dj}"),
        Some(json!({ "note": too_long })),
        Some(&club_tok),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "note over 500 chars rejected");

    // --- статус приглашённого DJ не переключается ---------------------
    let (s, body) = call(
        &env,
        "PATCH",
        &format!("/v1/club/djs/{invited_dj}"),
        Some(json!({ "status": "suspended" })),
        Some(&club_tok),
    )
    .await;
    assert_eq!(s, StatusCode::CONFLICT, "invited dj status locked: {body}");
    assert_eq!(body["code"], "dj_status_locked");

    // --- присутствие: heartbeat DJ → online --------------------------
    let dj_tok = env
        .keys
        .issue_access(active_dj, Role::Dj, Uuid::now_v7())
        .unwrap();
    let (s, body) = call(&env, "POST", "/v1/dj/presence", None, Some(&dj_tok)).await;
    assert_eq!(s, StatusCode::OK, "presence: {body}");
    assert_eq!(body["online"], true);

    let (_, body) = call(&env, "GET", "/v1/club/djs", None, Some(&club_tok)).await;
    let row = body["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"] == active_dj.to_string())
        .expect("active dj listed");
    assert_eq!(row["online"], true, "heartbeat makes the DJ online: {row}");
    assert!(row["last_seen_at"].is_string(), "last_seen_at is set");
    assert!(
        row["presence_age_seconds"].as_i64().unwrap_or(99) <= 5,
        "fresh heartbeat: {row}"
    );

    // Старый heartbeat — снова offline, но last_seen остаётся известным.
    sqlx::query("UPDATE dj SET last_seen_at = now() - interval '10 minutes' WHERE id = $1")
        .bind(active_dj)
        .execute(&pool)
        .await
        .unwrap();
    let (_, body) = call(&env, "GET", "/v1/club/djs", None, Some(&club_tok)).await;
    let row = body["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"] == active_dj.to_string())
        .expect("active dj listed");
    assert_eq!(row["online"], false, "stale heartbeat is offline: {row}");
    assert!(
        row["presence_age_seconds"].as_i64().unwrap_or(0) >= 590,
        "age keeps growing for the 'last seen' line: {row}"
    );

    // RBAC: клуб не может писать чужой heartbeat, DJ не управляет составом.
    let (s, _) = call(&env, "POST", "/v1/dj/presence", None, Some(&club_tok)).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "club is not a DJ");
    let (s, _) = call(
        &env,
        "PATCH",
        &format!("/v1/club/djs/{active_dj}"),
        Some(json!({ "status": "suspended" })),
        Some(&dj_tok),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "dj cannot manage the roster");

    // --- изоляция: чужой клуб не видит и не меняет DJ ----------------
    let (_, body) = call(&env, "GET", "/v1/club/djs", None, Some(&other_tok)).await;
    assert_eq!(
        body["items"].as_array().map(Vec::len),
        Some(0),
        "other club sees an empty roster"
    );
    let (s, _) = call(
        &env,
        "PATCH",
        &format!("/v1/club/djs/{active_dj}"),
        Some(json!({ "note": "hijacked" })),
        Some(&other_tok),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND, "other club cannot patch the DJ");
    let (s, _) = call(
        &env,
        "DELETE",
        &format!("/v1/club/djs/{active_dj}"),
        None,
        Some(&other_tok),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND, "other club cannot remove the DJ");

    // --- удаление непринятого приглашения: жёсткое --------------------
    let (s, body) = call(
        &env,
        "DELETE",
        &format!("/v1/club/djs/{invited_dj}"),
        None,
        Some(&club_tok),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "remove invite: {body}");
    assert_eq!(body["deleted"], "hard");
    let (left,): (i64,) = sqlx::query_as("SELECT count(*) FROM dj WHERE id = $1")
        .bind(invited_dj)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(left, 0, "unaccepted invitation row is gone");
    // Тот же e-mail можно пригласить снова.
    let (s, body) = call(
        &env,
        "POST",
        "/v1/club/djs/invite",
        Some(json!({ "email": "invited@dj.test", "display_name": "Invited DJ" })),
        Some(&club_tok),
    )
    .await;
    assert_eq!(
        s,
        StatusCode::CREATED,
        "re-invite after hard delete: {body}"
    );

    // --- удаление DJ с историей: мягкое + восстановление -------------
    let (s, body) = call(
        &env,
        "DELETE",
        &format!("/v1/club/djs/{active_dj}"),
        None,
        Some(&club_tok),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "soft remove: {body}");
    assert_eq!(body["deleted"], "soft");
    assert_eq!(body["status"], "suspended", "access is revoked");

    let (removed_at, status): (Option<chrono::DateTime<chrono::Utc>>, String) =
        sqlx::query_as("SELECT removed_at, status FROM dj WHERE id = $1")
            .bind(active_dj)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(removed_at.is_some(), "row is kept with removed_at");
    assert_eq!(status, "suspended", "access revoked in the row");

    // Скрыт из состава по умолчанию, виден с include_removed=1.
    let (_, body) = call(&env, "GET", "/v1/club/djs", None, Some(&club_tok)).await;
    assert!(
        body["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|item| item["id"] != active_dj.to_string()),
        "removed DJ is hidden by default: {body}"
    );
    let (_, body) = call(
        &env,
        "GET",
        "/v1/club/djs?include_removed=1",
        None,
        Some(&club_tok),
    )
    .await;
    let removed_row = body["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"] == active_dj.to_string())
        .expect("removed DJ is listed on request");
    assert_eq!(removed_row["removed"], true);
    assert!(removed_row["removed_at"].is_string());

    // Снятый DJ не патчится (его сначала возвращают) — 404.
    let (s, _) = call(
        &env,
        "PATCH",
        &format!("/v1/club/djs/{active_dj}"),
        Some(json!({ "note": "while removed" })),
        Some(&club_tok),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND, "removed DJ is not patchable");

    // Восстановление: снова в составе, доступ активен.
    let (s, body) = call(
        &env,
        "POST",
        &format!("/v1/club/djs/{active_dj}/restore"),
        None,
        Some(&club_tok),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "restore: {body}");
    assert_eq!(body["removed"], false);
    assert_eq!(body["status"], "active");
    let (_, body) = call(&env, "GET", "/v1/club/djs", None, Some(&club_tok)).await;
    assert!(
        body["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["id"] == active_dj.to_string()),
        "restored DJ is back in the roster: {body}"
    );
    // Идемпотентность: повторный restore не падает.
    let (s, _) = call(
        &env,
        "POST",
        &format!("/v1/club/djs/{active_dj}/restore"),
        None,
        Some(&club_tok),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "restore is idempotent");

    // --- открытая смена блокирует удаление ---------------------------
    let session_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO session (id, club_id, dj_id, status, timezone) \
         VALUES ($1, $2, $3, 'active', 'Europe/Madrid')",
    )
    .bind(session_id)
    .bind(club_id)
    .bind(active_dj)
    .execute(&pool)
    .await
    .unwrap();

    let (s, body) = call(
        &env,
        "DELETE",
        &format!("/v1/club/djs/{active_dj}"),
        None,
        Some(&club_tok),
    )
    .await;
    assert_eq!(
        s,
        StatusCode::CONFLICT,
        "on-shift DJ cannot be removed: {body}"
    );
    assert_eq!(body["code"], "dj_on_shift");

    // И список честно показывает смену.
    let (_, body) = call(&env, "GET", "/v1/club/djs", None, Some(&club_tok)).await;
    let row = body["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"] == active_dj.to_string())
        .expect("active dj listed");
    assert_eq!(row["on_shift"], true, "open session is reported: {row}");

    // Закрытая смена — не считается.
    sqlx::query("UPDATE session SET status = 'closed' WHERE id = $1")
        .bind(session_id)
        .execute(&pool)
        .await
        .unwrap();
    let (_, body) = call(&env, "GET", "/v1/club/djs", None, Some(&club_tok)).await;
    let row = body["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"] == active_dj.to_string())
        .expect("active dj listed");
    assert_eq!(row["on_shift"], false, "closed session is off shift: {row}");
}

/// Режим эксплуатации стенда: админская настройка `runtime_mode` и публичный
/// `GET /v1/status`.
///
/// Проверяем ровно контракт нижнего статус-бара:
///   * `PATCH /v1/admin/platform-settings` принимает `runtime_mode` и GET
///     возвращает его же (round-trip + аудит);
///   * значение вне enum → `422`;
///   * публичный `GET /v1/status` без токена отдаёт `{mode, source,
///     runtime_mode, open_signup, payments}`, где ручной режим даёт
///     `source = "admin"`;
///   * после сброса в `auto` режим снова разрешается фактами стенда
///     (в e2e `open_signup = true`, поэтому `test`, если не выставлен
///     `DROPIT_MAINTENANCE`).
#[tokio::test]
async fn e2e_status_runtime_mode() {
    let repo = match test_repo().await {
        Some(r) => r,
        None => {
            eprintln!("DATABASE_URL недоступен — e2e статуса пропущен");
            return;
        }
    };
    let sessions = SessionRepo::new(repo.pool());
    let env = app_env(Some(repo.clone()), Some(sessions), None).await;
    let pool = repo.pool();
    let admin = env
        .keys
        .issue_access(Uuid::now_v7(), Role::Admin, Uuid::now_v7())
        .unwrap();

    // Публичный статус без токена: стартовое значение — auto, факты стенда
    // в e2e всегда тестовые (open_signup = true).
    let (s, body) = call(&env, "GET", "/v1/status", None, None).await;
    assert_eq!(s, StatusCode::OK, "public status: {body}");
    assert_eq!(body["runtime_mode"], "auto", "default runtime_mode");
    assert_eq!(body["open_signup"], true, "e2e env has open signup");
    assert!(
        matches!(body["payments"].as_str(), Some("mock") | Some("live")),
        "payments enum: {body}"
    );

    // Админ ставит ручной режим — он побеждает любые факты стенда.
    let (s, body) = call(
        &env,
        "PATCH",
        "/v1/admin/platform-settings",
        Some(json!({ "runtime_mode": "normal" })),
        Some(&admin),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "runtime_mode patch: {body}");
    assert_eq!(body["runtime_mode"], "normal", "GET returns saved mode");

    let (s, body) = call(&env, "GET", "/v1/status", None, None).await;
    assert_eq!(s, StatusCode::OK, "status after patch: {body}");
    assert_eq!(body["mode"], "normal");
    assert_eq!(body["source"], "admin");
    assert_eq!(body["runtime_mode"], "normal");

    // Значение вне enum отклоняется, настройка не меняется.
    let (s, body) = call(
        &env,
        "PATCH",
        "/v1/admin/platform-settings",
        Some(json!({ "runtime_mode": "production" })),
        Some(&admin),
    )
    .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "bad enum: {body}");
    let (_, body) = call(
        &env,
        "GET",
        "/v1/admin/platform-settings",
        None,
        Some(&admin),
    )
    .await;
    assert_eq!(body["runtime_mode"], "normal", "rejected patch is no-op");

    // Аудит: изменение режима — административное действие.
    let audit: Option<(String,)> = sqlx::query_as(
        "SELECT diff::text FROM audit_log \
         WHERE action = 'platform_settings.update' ORDER BY at DESC LIMIT 1",
    )
    .fetch_optional(&pool)
    .await
    .unwrap_or(None);
    let audit = audit.expect("platform_settings.update audited").0;
    assert!(
        audit.contains("\"runtime_mode\": \"normal\""),
        "audit diff has runtime_mode: {audit}"
    );

    // Назад в auto: режим снова разрешается фактами стенда. Env-флаг
    // проверяется условно — в среде прогона он обычно выключен.
    let (s, _) = call(
        &env,
        "PATCH",
        "/v1/admin/platform-settings",
        Some(json!({ "runtime_mode": "auto" })),
        Some(&admin),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let (s, body) = call(&env, "GET", "/v1/status", None, None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["runtime_mode"], "auto");
    match body["source"].as_str() {
        Some("adaptive") => assert_eq!(
            body["mode"], "test",
            "open signup makes adaptive mode test: {body}"
        ),
        Some("env") => assert_eq!(
            body["mode"], "maintenance",
            "DROPIT_MAINTENANCE forces maintenance: {body}"
        ),
        other => panic!("unexpected source after auto: {other:?} ({body})"),
    }
}
