//! E2e-тест F6: цикл выплат, transfer-вебхуки, сверка.
//!
//! Критерии приёмки (техкарта Часть VIII, F6):
//!   * тестовая неделя: выплаты сошлись копейка-в-копейку
//!     (reconciliation ok, инварианты по payee держатся);
//!   * порог: нетто < 10 € → перенос (payout не создаётся, split
//!     остаётся pending);
//!   * финальная выплата без порога;
//!   * transfer.failed ×5 → on_hold + инцидент + сплиты возвращены;
//!   * дубль transfer-вебхука — no-op;
//!   * инвариант ПО КАЖДОМУ payee: Σ ledger == Σ сплитов.
//!
//! МОК-провайдер: transfer_id присваивается в репо.

use axum::http::StatusCode;
use serde_json::json;

use crate::e2e::{app_env, call, test_repo};
use dropit_domain::MAX_PAYOUT_ATTEMPTS;
use dropit_infra_db::{BillingRepo, CommissionParams, PayoutRepo, RequestRepo, SessionRepo};

#[tokio::test]
async fn e2e_payout_cycle() {
    let repo = match test_repo().await {
        Some(r) => r,
        None => {
            eprintln!("DATABASE_URL недоступен — e2e F6 пропущен");
            return;
        }
    };
    let sessions = SessionRepo::new(repo.pool());
    let requests = RequestRepo::new(repo.pool());
    let billing = BillingRepo::new(repo.pool());
    let payouts = PayoutRepo::new(repo.pool());
    let env = app_env(
        Some(repo.clone()),
        Some(sessions.clone()),
        Some(requests.clone()),
    )
    .await;

    let pool = repo.pool();
    let admin_id = uuid::Uuid::now_v7();

    // --- Неделя: 2 клуба + 1 DJ, два захваченных платежа -----------
    // Клуб A (богатый: нетто 350 ≥ порога), клуб B (бедный: 30 <
    // порога), DJ обоих клубов.
    let (club_a, dj_id) = fixture(&pool, "a").await;
    let (club_b, dj2_id) = fixture(&pool, "b").await;

    // Захват 1: 30 € в клубе A → сплиты 900(platform)/1050/1050 —
    // доли ≥ порога 10 €.
    let (payment1, _, _) =
        captured_payment(&env, &billing, &pool, club_a, dj_id, 3000, "f6-p1").await;
    // Захват 2: 1 € в клубе B → доли 35 < порога: перенос.
    let (_, club_b_split, dj_split2) =
        captured_payment(&env, &billing, &pool, club_b, dj2_id, 100, "f6-p2").await;
    let _ = payment1;

    // --- Цикл выплат (триггер админа) ------------------------------
    let admin_token = env
        .keys
        .issue_access(admin_id, dropit_common::Role::Admin, uuid::Uuid::now_v7())
        .unwrap();
    let (s, body) = call(
        &env,
        "POST",
        "/v1/admin/payouts/run",
        None,
        Some(&admin_token),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "payout run: {body}");

    // Выплат создано 2: DJ #1 (1050 ≥ порога) и клуб A (1050);
    // DJ #2 (35) и клуб B (35) — перенос.
    assert_eq!(
        body["payouts_created"], 2,
        "two payouts: rich club + rich dj: {body}"
    );

    // --- Transfer-вебхуки: закрываем выплаты ------------------------
    let in_flight = payouts.in_flight_payouts().await.unwrap();
    assert_eq!(in_flight.len(), 2);
    for row in &in_flight {
        let tid = row.stripe_transfer_id.clone().unwrap();
        let (s, body) = call(
            &env,
            "POST",
            "/v1/stripe/transfer-webhook",
            Some(json!({ "event_id": format!("evt_{}", row.id), "type": "transfer.paid", "transfer_id": tid })),
            None,
        )
        .await;
        assert_eq!(s, StatusCode::OK, "transfer paid: {body}");

        // ДУБЛЬ вебхука — no-op (идемпотентно по transfer_id).
        let (s, _) = call(
            &env,
            "POST",
            "/v1/stripe/transfer-webhook",
            Some(json!({ "event_id": format!("evt_dup_{}", row.id), "type": "transfer.paid", "transfer_id": tid })),
            None,
        )
        .await;
        assert_eq!(s, StatusCode::OK, "duplicate transfer webhook: no-op");
    }
    let remaining = payouts.in_flight_payouts().await.unwrap();
    assert_eq!(remaining.len(), 0, "all transfers closed");
    let _ = dj_split2;

    // --- Сверка: «копейка-в-копейку» --------------------------------
    let (s, body) = call(
        &env,
        "POST",
        "/v1/admin/reconciliation/run",
        None,
        Some(&admin_token),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "reconciliation: {body}");
    assert_eq!(body["status"], "ok", "reconciliation must be clean: {body}");

    // Инцидентов нет.
    let (s, body) = call(&env, "GET", "/v1/admin/incidents", None, Some(&admin_token)).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(
        body["items"].as_array().map(Vec::len),
        Some(0),
        "no incidents: {body}"
    );

    // Инвариант ПО КАЖДОМУ payee.
    let violations = payouts.payee_invariant_violations().await.unwrap();
    assert!(
        violations.is_empty(),
        "payee invariant violated: {violations:?}"
    );

    // --- Порог: перенесённые payee остаются pending ------------------
    // клуб B: сплиты не тронуты (нет payout).
    let (cnt,): (i64,) = sqlx::query_as("SELECT count(*) FROM payout WHERE payee_id = $1")
        .bind(club_b)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(cnt, 0, "poor club: payout NOT created (threshold)");
    let (pending,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM split WHERE payee_id = $1 AND status = 'pending'")
            .bind(club_b)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(pending > 0, "poor club splits remain pending (carry over)");
    let _ = club_b_split;

    // --- Финальная выплата без порога (клуб B уходит) ----------------
    let final_rows = payouts
        .run_payout_cycle(&[dropit_infra_db::Payee {
            payee_type: "club",
            payee_id: club_b,
        }])
        .await
        .unwrap();
    // Клуб B нетто < порога, но final → выплата создаётся.
    assert_eq!(final_rows.len(), 1, "final payout without threshold");
    assert!(final_rows[0].is_final);
    // Закрываем: transfer.paid.
    let tid = final_rows[0].stripe_transfer_id.clone().unwrap();
    payouts.handle_transfer_paid(&tid).await.unwrap();

    // --- CHAOS: transfer.failed до исчерпания ретраев ---------------
    // Новая неделя: ещё захват для DJ #1, потом цикл.
    let (payment3, _, _) =
        captured_payment(&env, &billing, &pool, club_a, dj_id, 3000, "f6-p3").await;
    let _ = payment3;
    let rows = payouts.run_payout_cycle(&[]).await.unwrap();
    // Новая неделя: DJ #1 1050 + клуб A 1050 → 2 выплаты.
    assert_eq!(rows.len(), 2, "next week cycle: {rows:?}");

    // Первую ломаем 5 раз → on_hold + инцидент + сплиты pending.
    // (rows не упорядочены GROUP BY — берём детерминированно первую
    // со свежими попытками.)
    let victim = rows
        .iter()
        .find(|r| r.attempts == 1 && r.status == "in_flight")
        .expect("fresh in_flight payout");
    let tid = victim.stripe_transfer_id.clone().unwrap();
    let payee = (victim.payee_type.clone(), victim.payee_id);
    let (before_pending,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM split WHERE payee_id = $1 AND status = 'pending'")
            .bind(victim.payee_id)
            .fetch_one(&pool)
            .await
            .unwrap();

    // 5 неудач (attempts 1 уже есть от create): retry между ними.
    let mut current = payouts
        .handle_transfer_failed(&tid, "insufficient platform balance")
        .await
        .unwrap(); // attempts=2, failed
                   // Ретраи до on_hold: retry → fail (attempts растёт), как только
                   // attempts ≥ MAX_PAYOUT_ATTEMPTS — handle переводит в on_hold.
    let mut guard = 0;
    loop {
        guard += 1;
        assert!(guard < 20, "retry loop runaway");
        current = payouts.retry_payout(current.id).await.unwrap(); // in_flight
        let tid2 = current.stripe_transfer_id.clone().unwrap();
        current = payouts
            .handle_transfer_failed(&tid2, "insufficient platform balance")
            .await
            .unwrap();
        if current.status == "on_hold" {
            break;
        }
    }
    // Итог: попытки исчерпаны (≤ MAX_PAYOUT_ATTEMPTS+1 из-за инкрементов
    // в retry и fail).
    assert_eq!(current.status, "on_hold", "after failures: on_hold");
    assert!(current.attempts >= MAX_PAYOUT_ATTEMPTS);

    // Инцидент создан.
    let incidents = payouts.open_incidents().await.unwrap();
    assert!(
        incidents.iter().any(|(_, kind, _)| kind == "payout_failed"),
        "payout_failed incident created: {incidents:?}"
    );
    // Сплиты payee возвращены в pending (деньги не пропали).
    let (after_pending,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM split WHERE payee_id = $1 AND status = 'pending'")
            .bind(victim.payee_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(
        after_pending > before_pending,
        "splits returned to pending on hold"
    );
    let _ = payee;

    // Админ разбирает инцидент.
    let (inc_id, _, _) = incidents
        .iter()
        .find(|(_, kind, _)| kind == "payout_failed")
        .unwrap();
    let (s, body) = call(
        &env,
        "POST",
        &format!("/v1/admin/incidents/{}/resolve", inc_id),
        None,
        Some(&admin_token),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "resolve incident: {body}");

    // --- Итоговая сверка недели: по-прежнему чисто -------------------
    let (s, body) = call(
        &env,
        "POST",
        "/v1/admin/reconciliation/run",
        None,
        Some(&admin_token),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["status"], "ok", "final reconciliation: {body}");
    let violations = payouts.payee_invariant_violations().await.unwrap();
    assert!(
        violations.is_empty(),
        "final payee invariant: {violations:?}"
    );
}

/// Фикстуры: клуб + DJ.
async fn fixture(pool: &sqlx::PgPool, tag: &str) -> (uuid::Uuid, uuid::Uuid) {
    let club_id = uuid::Uuid::now_v7();
    let dj_id = uuid::Uuid::now_v7();
    sqlx::query(
        "INSERT INTO club (id, name, slug, email, password_hash) VALUES ($1, $2, $3, $4, 'x')",
    )
    .bind(club_id)
    .bind(format!("F6 Club {tag}"))
    .bind(format!("f6-club-{tag}"))
    .bind(format!("f6-{tag}@club.test"))
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO dj (id, club_id, email, password_hash, display_name) VALUES ($1, $2, $3, 'x', 'DJ F6')")
        .bind(dj_id)
        .bind(club_id)
        .bind(format!("f6-{tag}@dj.test"))
        .execute(pool)
        .await
        .unwrap();
    (club_id, dj_id)
}

/// Захваченный платёж (мок-вебхук через репо, как e2e F5).
async fn captured_payment(
    env: &crate::routes::ApiEnv,
    billing: &BillingRepo,
    pool: &sqlx::PgPool,
    club_id: uuid::Uuid,
    dj_id: uuid::Uuid,
    amount: i64,
    key: &str,
) -> (uuid::Uuid, i64, i64) {
    let sessions = SessionRepo::new(pool.clone());
    let requests = RequestRepo::new(pool.clone());
    let session = sessions
        .create_session(club_id, Some(dj_id), None, "Europe/Madrid")
        .await
        .unwrap();
    sessions
        .apply_transition(session.id, dropit_domain::SessionTransition::Open)
        .await
        .unwrap();
    let guest_id = uuid::Uuid::now_v7();
    // Гость+грант напрямую.
    sqlx::query("INSERT INTO guest_device (id, platform) VALUES ($1, 'ios')")
        .bind(guest_id)
        .execute(pool)
        .await
        .unwrap();
    sessions
        .grant_session_guest(session.id, guest_id)
        .await
        .unwrap();
    let (track_id,): (uuid::Uuid,) =
        sqlx::query_as("INSERT INTO track_cache (id, source, source_id, canonical_key, title, artist) VALUES ($1, 'itunes', $2, $3, 'T', 'A') RETURNING id")
            .bind(uuid::Uuid::now_v7())
            .bind(format!("{key}-tr"))
            .bind(format!("itunes:{key}-tr"))
            .fetch_one(pool)
            .await
            .unwrap();
    let request = requests
        .create_request(session.id, guest_id, track_id, None, key)
        .await
        .unwrap()
        .expect("created");
    let payment = billing
        .create_payment(
            request.id,
            amount,
            key,
            &format!("pi_mock_{key}"),
            &CommissionParams {
                commission_bp: 3000,
                dj_share_bp: 5000,
                currency: "EUR".into(),
            },
        )
        .await
        .unwrap();
    billing.confirm_payment(payment.id).await.unwrap();
    billing
        .handle_capture_webhook(
            &format!("evt_{key}"),
            "payment_intent.succeeded",
            payment.id,
            amount,
            club_id,
            Some(dj_id),
        )
        .await
        .unwrap();
    let _ = env;
    // Сплиты payee: club/dj доли.
    let (club_s, dj_s): (i64, i64) = sqlx::query_as(
        "SELECT \
         COALESCE(sum(CASE WHEN payee_type = 'club' THEN amount_minor END), 0)::bigint, \
         COALESCE(sum(CASE WHEN payee_type = 'dj' THEN amount_minor END), 0)::bigint \
         FROM split WHERE payment_id = $1",
    )
    .bind(payment.id)
    .fetch_one(pool)
    .await
    .unwrap();
    (payment.id, club_s, dj_s)
}
