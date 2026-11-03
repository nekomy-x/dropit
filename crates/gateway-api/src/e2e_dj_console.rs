//! E2e: вход клуба в консоль диджея одной кнопкой (handoff).
//!
//! Проверяется контракт, на котором стоит кнопка «Консоль диджея»:
//!   * выдача (`POST …/session`) резолвит диджея из смены клуба и её
//!     `dj_id` — не из тела запроса;
//!   * выданный токен действительно DJ-токен: работает на `/v1/dj/shift`
//!     и `/v1/dj/queue` и указывает на диджея смены;
//!   * вход пишется в `audit_log` как `club_dj_console_open`;
//!   * каждая причина отказа — свой `409`-код (`no_open_shift`,
//!     `shift_without_dj`, `dj_unavailable`), потому что у панели для
//!     каждой своё действие;
//!   * чужой клуб не может открыть консоль диджея этого клуба, а роль DJ
//!     не может выписать себе пару через клубный роут.
//!
//! Требует живой Postgres (`DATABASE_URL`); без БД тест пропускается, как
//! и остальные e2e-модули.

#![cfg(test)]

use axum::http::StatusCode;
use dropit_common::Role;
use serde_json::json;
use uuid::Uuid;

use crate::e2e::{app_env, call, test_repo};

macro_rules! maybe_skip {
    ($repo:expr) => {
        match $repo {
            Some(r) => r,
            None => {
                eprintln!("DATABASE_URL недоступен — e2e консоли диджея пропущен");
                return;
            }
        }
    };
}

/// Строка-помощник: клуб + диджей + смена с назначенным DJ.
async fn seed_shift(pool: &sqlx::PgPool, club_id: Uuid, dj_id: Uuid, with_dj: bool) -> Uuid {
    sqlx::query(
        "INSERT INTO club (id, name, slug, email, password_hash) VALUES ($1, $2, $3, $4, 'x')",
    )
    .bind(club_id)
    .bind(format!("Club {club_id}"))
    .bind(format!("club-{club_id}"))
    .bind(format!("{club_id}@club.test"))
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO dj (id, club_id, email, password_hash, display_name, status) \
         VALUES ($1, $2, $3, 'x', 'DJ Handoff', 'active')",
    )
    .bind(dj_id)
    .bind(club_id)
    .bind(format!("{dj_id}@dj.test"))
    .execute(pool)
    .await
    .unwrap();
    let session_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO session (id, club_id, dj_id, status, timezone) \
         VALUES ($1, $2, $3, 'active', 'Europe/Madrid')",
    )
    .bind(session_id)
    .bind(club_id)
    .bind(if with_dj { Some(dj_id) } else { None })
    .execute(pool)
    .await
    .unwrap();
    session_id
}

#[tokio::test]
async fn e2e_club_dj_console_handoff() {
    let repo = maybe_skip!(test_repo().await);
    let env = app_env(
        Some(repo.clone()),
        Some(dropit_infra_db::SessionRepo::new(repo.pool())),
        None,
    )
    .await;
    let pool = repo.pool();

    let club_id = Uuid::now_v7();
    let dj_id = Uuid::now_v7();
    let session_id = seed_shift(&pool, club_id, dj_id, true).await;
    let club_tok = env
        .keys
        .issue_access(club_id, Role::Club, Uuid::now_v7())
        .unwrap();

    // --- Выдача: пара DJ-токенов ---
    let (s, body) = call(
        &env,
        "POST",
        "/v1/club/dj-console/session",
        Some(json!({})),
        Some(&club_tok),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "handoff: {body}");
    let dj_token = body["access_token"].as_str().unwrap().to_string();
    assert!(!dj_token.is_empty(), "access token issued");
    assert!(
        body["refresh_token"]
            .as_str()
            .is_some_and(|t| !t.is_empty()),
        "refresh token issued"
    );
    assert_eq!(body["token_type"], "Bearer");
    assert_eq!(body["dj"]["id"], dj_id.to_string());
    assert_eq!(body["session"]["id"], session_id.to_string());

    // Токен — именно DJ-роль: shift и очередь отвечают, смена та же.
    let (s, body) = call(&env, "GET", "/v1/dj/shift", None, Some(&dj_token)).await;
    assert_eq!(s, StatusCode::OK, "dj shift with handed token: {body}");
    assert_eq!(body["session"]["id"], session_id.to_string());
    let (s, body) = call(&env, "GET", "/v1/dj/queue?sort=new", None, Some(&dj_token)).await;
    assert_eq!(s, StatusCode::OK, "dj queue with handed token: {body}");
    assert!(body["items"].is_array());

    // Вход записан в audit_log (без токена в diff).
    let (n,): (i64,) = sqlx::query_as(
        "SELECT count(*) FROM audit_log \
         WHERE action = 'club_dj_console_open' AND actor_type = 'club' AND actor_id = $1",
    )
    .bind(club_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(n, 1, "club_dj_console_open audit entry written");

    // Чужой клуб не видит этот состав и не может открыть его консоль.
    let other_club = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO club (id, name, slug, email, password_hash) VALUES ($1, 'Other', $2, $3, 'x')",
    )
    .bind(other_club)
    .bind(format!("other-{other_club}"))
    .bind(format!("{other_club}@club.test"))
    .execute(&pool)
    .await
    .unwrap();
    let other_tok = env
        .keys
        .issue_access(other_club, Role::Club, Uuid::now_v7())
        .unwrap();
    let (s, body) = call(
        &env,
        "POST",
        "/v1/club/dj-console/session",
        Some(json!({})),
        Some(&other_tok),
    )
    .await;
    assert_eq!(s, StatusCode::CONFLICT, "other club handoff: {body}");
    assert_eq!(body["code"], "no_open_shift");

    // --- Причины отказа: свой код на каждую ---
    // 1. Смена без диджея и один активный диджей: сервер назначает его сам,
    //    иначе «одна кнопка» упиралась бы в форму с единственным кандидатом.
    let solo_club = Uuid::now_v7();
    let solo_dj = Uuid::now_v7();
    let solo_session = seed_shift(&pool, solo_club, solo_dj, false).await;
    let solo_tok = env
        .keys
        .issue_access(solo_club, Role::Club, Uuid::now_v7())
        .unwrap();
    let (s, body) = call(
        &env,
        "POST",
        "/v1/club/dj-console/session",
        Some(json!({})),
        Some(&solo_tok),
    )
    .await;
    assert_eq!(
        s,
        StatusCode::OK,
        "single DJ is assigned automatically: {body}"
    );
    assert_eq!(body["dj"]["id"], solo_dj.to_string());
    let (assigned,): (Option<Uuid>,) = sqlx::query_as("SELECT dj_id FROM session WHERE id = $1")
        .bind(solo_session)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(assigned, Some(solo_dj), "shift now carries the DJ");

    // 1b. Два активных диджея — сервер не выбирает за клуб: 409 с составом,
    //     затем явный выбор открывает консоль и закрепляет смену.
    let pick_club = Uuid::now_v7();
    let pick_a = Uuid::now_v7();
    let pick_b = Uuid::now_v7();
    let pick_session = seed_shift(&pool, pick_club, pick_a, false).await;
    sqlx::query(
        "INSERT INTO dj (id, club_id, email, password_hash, display_name, status) \
         VALUES ($1, $2, $3, 'x', 'DJ Second', 'active')",
    )
    .bind(pick_b)
    .bind(pick_club)
    .bind(format!("{pick_b}@dj.test"))
    .execute(&pool)
    .await
    .unwrap();
    let pick_tok = env
        .keys
        .issue_access(pick_club, Role::Club, Uuid::now_v7())
        .unwrap();
    let (s, body) = call(
        &env,
        "POST",
        "/v1/club/dj-console/session",
        Some(json!({})),
        Some(&pick_tok),
    )
    .await;
    assert_eq!(s, StatusCode::CONFLICT);
    assert_eq!(body["code"], "dj_required");
    let candidates = body["details"]["candidates"]
        .as_array()
        .expect("candidates list");
    assert_eq!(candidates.len(), 2, "both active DJs are offered");
    let (s, body) = call(
        &env,
        "POST",
        "/v1/club/dj-console/session",
        Some(json!({ "dj_id": pick_b })),
        Some(&pick_tok),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "chosen DJ opens the console: {body}");
    assert_eq!(body["dj"]["id"], pick_b.to_string());
    let (assigned,): (Option<Uuid>,) = sqlx::query_as("SELECT dj_id FROM session WHERE id = $1")
        .bind(pick_session)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(assigned, Some(pick_b), "the chosen DJ is on the shift");

    // 1c. В составе нет активных диджеев — открывать консоль некому.
    let empty_club = Uuid::now_v7();
    let empty_dj = Uuid::now_v7();
    let _empty_session = seed_shift(&pool, empty_club, empty_dj, false).await;
    sqlx::query("UPDATE dj SET status = 'suspended' WHERE id = $1")
        .bind(empty_dj)
        .execute(&pool)
        .await
        .unwrap();
    let empty_tok = env
        .keys
        .issue_access(empty_club, Role::Club, Uuid::now_v7())
        .unwrap();
    let (s, body) = call(
        &env,
        "POST",
        "/v1/club/dj-console/session",
        Some(json!({})),
        Some(&empty_tok),
    )
    .await;
    assert_eq!(s, StatusCode::CONFLICT);
    assert_eq!(body["code"], "no_djs");

    // 2. Диджей приостановлен — консоль не открывается отозванным доступом.
    sqlx::query("UPDATE dj SET status = 'suspended' WHERE id = $1")
        .bind(dj_id)
        .execute(&pool)
        .await
        .unwrap();
    let (s, body) = call(
        &env,
        "POST",
        "/v1/club/dj-console/session",
        Some(json!({})),
        Some(&club_tok),
    )
    .await;
    assert_eq!(s, StatusCode::CONFLICT);
    assert_eq!(body["code"], "dj_unavailable");
    sqlx::query("UPDATE dj SET status = 'active' WHERE id = $1")
        .bind(dj_id)
        .execute(&pool)
        .await
        .unwrap();

    // 3. Открытой смены нет.
    sqlx::query("UPDATE session SET status = 'closed', closed_at = now() WHERE id = $1")
        .bind(session_id)
        .execute(&pool)
        .await
        .unwrap();
    let (s, body) = call(
        &env,
        "POST",
        "/v1/club/dj-console/session",
        Some(json!({})),
        Some(&club_tok),
    )
    .await;
    assert_eq!(s, StatusCode::CONFLICT);
    assert_eq!(body["code"], "no_open_shift");

    // --- RBAC: без токена 401, роль DJ 403 (себе пару не выписать) ---
    let dj_owned = dj_token.clone();
    for (label, token) in [("no token", None), ("dj token", Some(dj_owned.as_str()))] {
        let (s, _) = call(
            &env,
            "POST",
            "/v1/club/dj-console/session",
            Some(json!({})),
            token,
        )
        .await;
        let expected = if label == "no token" {
            StatusCode::UNAUTHORIZED
        } else {
            StatusCode::FORBIDDEN
        };
        assert_eq!(s, expected, "{label} on handoff");
    }
}
