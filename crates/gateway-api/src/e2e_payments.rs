//! E2e-тест F5: платёжный цикл, chaos-минимум (Test Mode, МОК).
//!
//! Критерии (техкарта Часть V, «Отказоустойчивость цикла»):
//!   * двойной вебхук с РАЗНЫМИ event_id → ОДНА проводка
//!     (идемпотентность по (aggregate_id, event_type));
//!   * cancel-гонка: cancelling → captured — деньги закрыты в ledger;
//!   * инвариант Σ ledger_entry(txn) == 0;
//!   * инвариант Σ split(payment) == amount_minor;
//!   * GUARD confirm: пауза сессии → 409 + интент отменён.
//!
//! МОК-ПРОВАЙДЕР: захват имитируется прямым вебхуком (подпись —
//! мок-sha256, правило 5).

use axum::http::StatusCode;
use serde_json::{json, Value};

use crate::e2e::{app_env, call, test_repo};
use dropit_infra_db::{BillingRepo, CommissionParams, RequestRepo, SessionRepo};

#[tokio::test]
async fn e2e_payment_chaos() {
    let repo = match test_repo().await {
        Some(r) => r,
        None => {
            eprintln!("DATABASE_URL недоступен — e2e F5 пропущен");
            return;
        }
    };
    let sessions = SessionRepo::new(repo.pool());
    let requests = RequestRepo::new(repo.pool());
    let billing = BillingRepo::new(repo.pool());
    let env = app_env(
        Some(repo.clone()),
        Some(sessions.clone()),
        Some(requests.clone()),
    )
    .await;

    let pool = repo.pool();

    // --- Фикстуры: клуб/DJ/сессия active/гость/трек/запрос --------
    let club_id = uuid::Uuid::now_v7();
    let dj_id = uuid::Uuid::now_v7();
    sqlx::query("INSERT INTO club (id, name, slug, email, password_hash) VALUES ($1, 'F5 Club', 'f5-club', 'f5@club.test', 'x')")
        .bind(club_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO dj (id, club_id, email, password_hash, display_name) VALUES ($1, $2, 'f5@dj.test', 'x', 'DJ F5')")
        .bind(dj_id)
        .bind(club_id)
        .execute(&pool)
        .await
        .unwrap();
    let session = sessions
        .create_session(club_id, Some(dj_id), None, "Europe/Madrid")
        .await
        .unwrap();
    sessions
        .apply_transition(session.id, dropit_domain::SessionTransition::Open)
        .await
        .unwrap();
    let guest_id = uuid::Uuid::now_v7();
    repo.create_guest_device(guest_id, "ios", None)
        .await
        .unwrap();
    sessions
        .grant_session_guest(session.id, guest_id)
        .await
        .unwrap();
    let (track_id,): (uuid::Uuid,) =
        sqlx::query_as("INSERT INTO track_cache (id, source, source_id, canonical_key, title, artist) VALUES ($1, 'itunes', 'f5-tr', 'itunes:f5-tr', 'T', 'A') RETURNING id")
            .bind(uuid::Uuid::now_v7())
            .fetch_one(&pool)
            .await
            .unwrap();
    let request = requests
        .create_request(session.id, guest_id, track_id, None, "f5-idem")
        .await
        .unwrap()
        .expect("created");

    // --- Платёж: intent + confirm --------------------------------
    let payment = billing
        .create_payment(
            request.id,
            1000, // 10 €
            "f5-pay-key",
            "pi_mock_f5",
            &CommissionParams {
                commission_bp: 3000, // 30% (Q1-дефолт)
                dj_share_bp: 5000,   // 50/50 остаток
                currency: "EUR".into(),
            },
        )
        .await
        .unwrap();
    assert_eq!(payment.status, "intent");

    let (s, body) = call(
        &env,
        "POST",
        &format!("/v1/payments/{}/confirm", payment.id),
        Some(json!({ "payment_method": "pm_mock" })),
        Some(&guest_id.to_string()),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "confirm: {body}");
    assert_eq!(body["status"], "processing");

    // --- Вебхук 1: захват → captured + ledger ---------------------
    let (s, body) = signed_webhook(
        &env,
        "evt_100",
        "payment_intent.succeeded",
        payment.id,
        1000,
    )
    .await;
    assert_eq!(s, StatusCode::OK, "webhook 1: {body}");

    let after = billing.find_payment(payment.id).await.unwrap().unwrap();
    assert_eq!(after.status, "captured");
    assert!(after.captured_at.is_some());

    // --- CHAOS 1: двойной вебхук с РАЗНЫМИ event_id ---------------
    // payment_intent.succeeded уже обработан по (aggregate, type):
    // charge.succeeded — другой event_id, но тот же переход → no-op.
    let (s, body) = signed_webhook(&env, "evt_101", "charge.succeeded", payment.id, 1000).await;
    assert_eq!(s, StatusCode::OK, "duplicate semantic webhook: {body}");

    // И delivery-дубль того же event_id — тоже no-op.
    let (s, _) = signed_webhook(
        &env,
        "evt_100",
        "payment_intent.succeeded",
        payment.id,
        1000,
    )
    .await;
    assert_eq!(s, StatusCode::OK, "delivery duplicate: no-op");

    // В ledger РОВНО ОДНА проводка захвата (не три!).
    let (txn_cnt,): (i64,) = sqlx::query_as(
        "SELECT count(DISTINCT txn_id) FROM ledger_entry WHERE entry_type = 'capture'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        txn_cnt, 1,
        "double webhook must NOT create second ledger txn"
    );

    // --- Инварианты ------------------------------------------------
    let broken = billing.ledger_txn_invariant().await.unwrap();
    assert!(broken.is_empty(), "Σ ledger(txn) == 0 violated: {broken:?}");
    let broken = billing.split_invariant().await.unwrap();
    assert!(broken.is_empty(), "Σ split == amount violated: {broken:?}");

    // Сплит по Q1-дефолту: 300/350/350.
    let (platform, club, dj): (i64, i64, i64) = sqlx::query_as(
        "SELECT \
         sum(CASE WHEN payee_type = 'platform' THEN amount_minor ELSE 0 END), \
         sum(CASE WHEN payee_type = 'club' THEN amount_minor ELSE 0 END), \
         sum(CASE WHEN payee_type = 'dj' THEN amount_minor ELSE 0 END) \
         FROM split WHERE payment_id = $1",
    )
    .bind(payment.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!((platform, club, dj), (300, 350, 350), "30% + 50/50 split");
    assert_eq!(platform + club + dj, 1000);

    // --- CHAOS 2: cancel-гонка cancelling → captured ---------------
    // Второй платёж: воркер начал отмену, но захват успел раньше.
    let request2 = requests
        .create_request(session.id, guest_id, track_id, None, "f5-idem-2")
        .await
        .unwrap()
        .expect("created");
    let payment2 = billing
        .create_payment(
            request2.id,
            500,
            "f5-pay-key-2",
            "pi_mock_f5_2",
            &CommissionParams {
                commission_bp: 3000,
                dj_share_bp: 5000,
                currency: "EUR".into(),
            },
        )
        .await
        .unwrap();
    billing.confirm_payment(payment2.id).await.unwrap();
    // Воркер успел пометить cancelling ДО вебхука захвата.
    billing.begin_cancel(payment2.id).await.unwrap();
    let p2 = billing.find_payment(payment2.id).await.unwrap().unwrap();
    assert_eq!(p2.status, "cancelling");

    // Вебхук захвата при cancelling → captured (деньги у гостя —
    // закрываем в ledger обычным путём, НЕ теряем).
    let (s, body) = signed_webhook(
        &env,
        "evt_200",
        "payment_intent.succeeded",
        payment2.id,
        500,
    )
    .await;
    assert_eq!(s, StatusCode::OK, "cancel race: {body}");
    let p2 = billing.find_payment(payment2.id).await.unwrap().unwrap();
    assert_eq!(p2.status, "captured", "race: capture wins over cancel");

    // Инварианты после гонки.
    assert!(billing.ledger_txn_invariant().await.unwrap().is_empty());
    assert!(billing.split_invariant().await.unwrap().is_empty());

    // --- GUARD: пауза → confirm 409 + интент отменён ---------------
    let request3 = requests
        .create_request(session.id, guest_id, track_id, None, "f5-idem-3")
        .await
        .unwrap()
        .expect("created");
    let payment3 = billing
        .create_payment(
            request3.id,
            500,
            "f5-pay-key-3",
            "pi_mock_f5_3",
            &CommissionParams {
                commission_bp: 3000,
                dj_share_bp: 5000,
                currency: "EUR".into(),
            },
        )
        .await
        .unwrap();
    // DJ ставит паузу ПОСЛЕ создания интента.
    sessions
        .apply_transition(session.id, dropit_domain::SessionTransition::Pause)
        .await
        .unwrap();
    let (s, body) = call(
        &env,
        "POST",
        &format!("/v1/payments/{}/confirm", payment3.id),
        Some(json!({ "payment_method": "pm_mock" })),
        Some(&guest_id.to_string()),
    )
    .await;
    assert_eq!(s, StatusCode::CONFLICT, "guard: paused session: {s} {body}");
    assert_eq!(body["code"], "session_paused");
    // Интент перешёл в cancelling (авто-отмена).
    let p3 = billing.find_payment(payment3.id).await.unwrap().unwrap();
    assert_eq!(p3.status, "cancelling", "intent auto-cancelled by guard");

    // --- Вебхук-безопасность: мусорная подпись → 400 ---------------
    let (s, _) = call(&env, "POST", "/v1/stripe/webhook", Some(json!({})), None).await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "unsigned webhook rejected");
}

/// Подписанный мок-вебхук.
async fn signed_webhook(
    env: &crate::routes::ApiEnv,
    event_id: &str,
    event_type: &str,
    payment_id: uuid::Uuid,
    amount_minor: i64,
) -> (StatusCode, Value) {
    let body = serde_json::json!({
        "event_id": event_id,
        "type": event_type,
        "payment_id": payment_id,
        "amount_minor": amount_minor,
    })
    .to_string();
    // Мок-подпись: sha256("<t>.<body>") — формат t=...,v1=...
    use sha2::Digest;
    let t = chrono::Utc::now().timestamp();
    let digest = sha2::Sha256::new_with_prefix(format!("{t}.{body}"));
    let hex: String = digest
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let sig = format!("t={t},v1={hex}");

    use axum::body::Body;
    use tower::ServiceExt;
    let router = crate::routes::router(env.clone());
    let req = axum::http::Request::builder()
        .method("POST")
        .uri("/v1/stripe/webhook")
        .header("content-type", "application/json")
        .header("stripe-signature", sig)
        .body(Body::from(body))
        .unwrap();
    let resp = router.oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap_or_default();
    let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, value)
}
