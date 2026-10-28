//! E2e-тест F5: платёжный цикл, chaos-минимум (Test Mode, MockStripe).
//!
//! Критерии (техкарта Часть V, «Отказоустойчивость цикла»):
//!   * двойной вебхук с РАЗНЫМИ event_id → ОДНА проводка
//!     (идемпотентность по (aggregate_id, event_type));
//!   * cancel-гонка: cancelling → captured — деньги закрыты в ledger;
//!   * инвариант Σ ledger_entry(txn) == 0;
//!   * инвариант Σ split(payment) == amount_minor;
//!   * GUARD confirm: пауза сессии → 409 + интент отменён;
//!   * вебхук с невалидной подписью → 400 `webhook_signature_invalid`
//!     и запись в `webhook_event` НЕ появляется.
//!
//! Вебхук присылается в формате события Stripe (`{id,type,data:{object}}`);
//! подпись проверяется `StripeApi::verify_webhook` (MockStripe принимает
//! любой непустой `Stripe-Signature`; RejectingStripe — отклоняет).

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

    let (s, body) = http_call(
        &env,
        "POST",
        &format!("/v1/payments/{}/confirm", payment.id),
        Some(json!({ "payment_method": "pm_mock" })),
        Some(&guest_id.to_string()),
        Some("f5-confirm-1"),
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
         sum(CASE WHEN payee_type = 'platform' THEN amount_minor ELSE 0 END)::bigint, \
         sum(CASE WHEN payee_type = 'club' THEN amount_minor ELSE 0 END)::bigint, \
         sum(CASE WHEN payee_type = 'dj' THEN amount_minor ELSE 0 END)::bigint \
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
    let (s, body) = http_call(
        &env,
        "POST",
        &format!("/v1/payments/{}/confirm", payment3.id),
        Some(json!({ "payment_method": "pm_mock" })),
        Some(&guest_id.to_string()),
        Some("f5-confirm-3"),
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

/// Подписанный мок-вебхук в формате события Stripe.
///
/// Payload: `{"id":event_id,"type":event_type,"data":{"object":{"id":intent_id,"amount":...}}}`,
/// где `intent_id` — `stripe_payment_intent_id` платёжа. Подпись — любой
/// непустой `Stripe-Signature` (MockStripe не проверяет HMAC, только
/// непустой заголовок; реальная криптография — в LiveStripe).
async fn signed_webhook(
    env: &crate::routes::ApiEnv,
    event_id: &str,
    event_type: &str,
    payment_id: uuid::Uuid,
    amount_minor: i64,
) -> (StatusCode, Value) {
    let pool = env.repo.as_ref().expect("repo configured in e2e").pool();
    let (intent_id,): (String,) =
        sqlx::query_as("SELECT stripe_payment_intent_id FROM payment WHERE id = $1")
            .bind(payment_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let body = serde_json::json!({
        "id": event_id,
        "type": event_type,
        "data": { "object": { "id": intent_id, "amount": amount_minor } },
    })
    .to_string();
    // MockStripe требует лишь непустой заголовок; формат t=,v1= оставлён
    // для совместимости с реальным Stripe.
    let t = chrono::Utc::now().timestamp();
    let sig = format!("t={t},v1=e2e_mock_accepts_any_nonempty_header");

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

// ============================================================
// Вебхук с отклоняемой подписью → 400, без записи webhook_event.
// ============================================================

/// Фейковый провайдер, отклоняющий ЛЮБУЮ подпись вебхука. Остальные методы
/// делегируют MockStripe (платёжный цикл не нужен в этом тесте). Подкладывается
/// в `ApiEnv.stripe`, чтобы проверить, что маршрут НЕ применяет событие и НЕ
/// пишет `webhook_event` при отказе подписи.
struct RejectingStripe;

#[async_trait::async_trait]
impl dropit_infra_stripe::StripeApi for RejectingStripe {
    async fn create_payment_intent(
        &self,
        req: dropit_infra_stripe::CreateIntent,
    ) -> Result<dropit_infra_stripe::IntentResult, dropit_infra_stripe::StripeError> {
        dropit_infra_stripe::MockStripe
            .create_payment_intent(req)
            .await
    }
    async fn capture(
        &self,
        intent_id: &str,
        amount_minor: Option<i64>,
        idempotency_key: &str,
    ) -> Result<dropit_infra_stripe::IntentResult, dropit_infra_stripe::StripeError> {
        dropit_infra_stripe::MockStripe
            .capture(intent_id, amount_minor, idempotency_key)
            .await
    }
    async fn refund(
        &self,
        req: dropit_infra_stripe::RefundReq,
    ) -> Result<dropit_infra_stripe::RefundResult, dropit_infra_stripe::StripeError> {
        dropit_infra_stripe::MockStripe.refund(req).await
    }
    async fn create_connected_account(
        &self,
        req: dropit_infra_stripe::ConnectAccountReq,
    ) -> Result<dropit_infra_stripe::ConnectedAccount, dropit_infra_stripe::StripeError> {
        dropit_infra_stripe::MockStripe
            .create_connected_account(req)
            .await
    }
    async fn retrieve_account(
        &self,
        account_id: &str,
    ) -> Result<dropit_infra_stripe::ConnectedAccount, dropit_infra_stripe::StripeError> {
        dropit_infra_stripe::MockStripe
            .retrieve_account(account_id)
            .await
    }
    async fn account_link(
        &self,
        account_id: &str,
        refresh_url: &str,
        return_url: &str,
    ) -> Result<String, dropit_infra_stripe::StripeError> {
        dropit_infra_stripe::MockStripe
            .account_link(account_id, refresh_url, return_url)
            .await
    }
    async fn create_transfer(
        &self,
        req: dropit_infra_stripe::TransferReq,
    ) -> Result<dropit_infra_stripe::TransferResult, dropit_infra_stripe::StripeError> {
        dropit_infra_stripe::MockStripe.create_transfer(req).await
    }
    fn verify_webhook(
        &self,
        _payload: &[u8],
        _signature_header: &str,
        _tolerance_secs: i64,
    ) -> Result<dropit_infra_stripe::WebhookEvent, dropit_infra_stripe::StripeError> {
        Err(dropit_infra_stripe::StripeError::SignatureInvalid(
            "rejecting signature for e2e test".into(),
        ))
    }
}

/// Вебхук с отклонённой подписью → 400 `webhook_signature_invalid`,
/// запись в `webhook_event` НЕ появляется (событие не применяется).
#[tokio::test]
async fn e2e_webhook_rejects_invalid_signature() {
    let repo = match test_repo().await {
        Some(r) => r,
        None => {
            eprintln!("DATABASE_URL недоступен — e2e webhook-signature пропущен");
            return;
        }
    };
    let sessions = SessionRepo::new(repo.pool());
    let requests = RequestRepo::new(repo.pool());
    let mut env = app_env(Some(repo.clone()), Some(sessions), Some(requests)).await;
    env.stripe = Some(std::sync::Arc::new(RejectingStripe));
    let pool = repo.pool();

    let body = serde_json::json!({
        "id": "evt_rejected",
        "type": "payment_intent.succeeded",
        "data": { "object": { "id": "pi_nonexistent", "amount": 100 } },
    })
    .to_string();
    use axum::body::Body;
    use tower::ServiceExt;
    let router = crate::routes::router(env.clone());
    let req = axum::http::Request::builder()
        .method("POST")
        .uri("/v1/stripe/webhook")
        .header("content-type", "application/json")
        .header("stripe-signature", "t=1,v1=fake_signature")
        .body(Body::from(body))
        .unwrap();
    let resp = router.oneshot(req).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::BAD_REQUEST,
        "invalid signature must be rejected with 400"
    );

    // Событие НЕ применено — записи в webhook_event нет.
    let (cnt,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM webhook_event WHERE event_id = 'evt_rejected'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(cnt, 0, "invalid signature must not persist webhook_event");
}

// ============================================================
// W5-A: IDOR / двойное списание / идемпотентность платежей.
// ============================================================

/// HTTP-вызов с произвольными заголовками Authorization и Idempotency-Key
/// (встроенный `call` их для платежных путей не подставляет).
async fn http_call(
    env: &crate::routes::ApiEnv,
    method: &str,
    uri: &str,
    body: Option<Value>,
    token: Option<&str>,
    idem_key: Option<&str>,
) -> (StatusCode, Value) {
    use axum::body::Body;
    use tower::ServiceExt;
    let router = crate::routes::router(env.clone());
    let mut req = axum::http::Request::builder().method(method).uri(uri);
    if let Some(t) = token {
        req = req.header("authorization", format!("Bearer {t}"));
    }
    if let Some(k) = idem_key {
        req = req.header("idempotency-key", k);
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

/// Мок-провайдер со счётчиком вызовов `create_payment_intent` — чтобы
/// проверить «провайдер вызван ровно один раз» на replay-дубле. Остальные
/// методы делегируют `MockStripe` (платёжный цикл не нужен в этих тестах).
struct CountingStripe {
    create_calls: std::sync::Arc<std::sync::atomic::AtomicU64>,
}

impl CountingStripe {
    /// Возвращает (провайдер, счётчик) — счётчик читает тест после вызовов.
    fn new() -> (Self, std::sync::Arc<std::sync::atomic::AtomicU64>) {
        let counter = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
        (
            Self {
                create_calls: counter.clone(),
            },
            counter,
        )
    }
}

#[async_trait::async_trait]
impl dropit_infra_stripe::StripeApi for CountingStripe {
    async fn create_payment_intent(
        &self,
        req: dropit_infra_stripe::CreateIntent,
    ) -> Result<dropit_infra_stripe::IntentResult, dropit_infra_stripe::StripeError> {
        self.create_calls
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        dropit_infra_stripe::MockStripe
            .create_payment_intent(req)
            .await
    }
    async fn capture(
        &self,
        intent_id: &str,
        amount_minor: Option<i64>,
        idempotency_key: &str,
    ) -> Result<dropit_infra_stripe::IntentResult, dropit_infra_stripe::StripeError> {
        dropit_infra_stripe::MockStripe
            .capture(intent_id, amount_minor, idempotency_key)
            .await
    }
    async fn refund(
        &self,
        req: dropit_infra_stripe::RefundReq,
    ) -> Result<dropit_infra_stripe::RefundResult, dropit_infra_stripe::StripeError> {
        dropit_infra_stripe::MockStripe.refund(req).await
    }
    async fn create_connected_account(
        &self,
        req: dropit_infra_stripe::ConnectAccountReq,
    ) -> Result<dropit_infra_stripe::ConnectedAccount, dropit_infra_stripe::StripeError> {
        dropit_infra_stripe::MockStripe
            .create_connected_account(req)
            .await
    }
    async fn retrieve_account(
        &self,
        account_id: &str,
    ) -> Result<dropit_infra_stripe::ConnectedAccount, dropit_infra_stripe::StripeError> {
        dropit_infra_stripe::MockStripe
            .retrieve_account(account_id)
            .await
    }
    async fn account_link(
        &self,
        account_id: &str,
        refresh_url: &str,
        return_url: &str,
    ) -> Result<String, dropit_infra_stripe::StripeError> {
        dropit_infra_stripe::MockStripe
            .account_link(account_id, refresh_url, return_url)
            .await
    }
    async fn create_transfer(
        &self,
        req: dropit_infra_stripe::TransferReq,
    ) -> Result<dropit_infra_stripe::TransferResult, dropit_infra_stripe::StripeError> {
        dropit_infra_stripe::MockStripe.create_transfer(req).await
    }
    fn verify_webhook(
        &self,
        payload: &[u8],
        signature_header: &str,
        tolerance_secs: i64,
    ) -> Result<dropit_infra_stripe::WebhookEvent, dropit_infra_stripe::StripeError> {
        dropit_infra_stripe::MockStripe.verify_webhook(payload, signature_header, tolerance_secs)
    }
}

/// Фикстуры для платежных e2e: клуб/DJ/active-сессия/гость/трек/pending
/// запрос. `stripe` — опциональная подмена провайдера (для счётчика).
struct PaymentFixtures {
    env: crate::routes::ApiEnv,
    pool: sqlx::PgPool,
    billing: BillingRepo,
    guest_id: uuid::Uuid,
    request_id: uuid::Uuid,
    /// id сессии — для паузы/закрытия в GUARD-тестах (W7-A).
    session_id: uuid::Uuid,
    /// id трека — для создания второго request (тест чужого ключа).
    track_id: uuid::Uuid,
}

async fn payment_fixtures(
    stripe: Option<std::sync::Arc<dyn dropit_infra_stripe::StripeApi>>,
) -> Option<PaymentFixtures> {
    let repo = test_repo().await?;
    let sessions = SessionRepo::new(repo.pool());
    let requests = RequestRepo::new(repo.pool());
    let billing = BillingRepo::new(repo.pool());
    let mut env = app_env(
        Some(repo.clone()),
        Some(sessions.clone()),
        Some(requests.clone()),
    )
    .await;
    if let Some(s) = stripe {
        env.stripe = Some(s);
    }
    let pool = repo.pool();

    // Уникальные значения per-вызов — нулевая вероятность collide с другим
    // прогоном даже без truncate (truncate всё равно делает test_repo).
    let tag = uuid::Uuid::now_v7().to_string();
    let club_id = uuid::Uuid::now_v7();
    let dj_id = uuid::Uuid::now_v7();
    sqlx::query(
        "INSERT INTO club (id, name, slug, email, password_hash) \
         VALUES ($1, $2, $2, $3, 'x')",
    )
    .bind(club_id)
    .bind(format!("club-{tag}"))
    .bind(format!("{tag}@club.test"))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO dj (id, club_id, email, password_hash, display_name) \
         VALUES ($1, $2, $3, 'x', 'DJ')",
    )
    .bind(dj_id)
    .bind(club_id)
    .bind(format!("{tag}@dj.test"))
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
    let (track_id,): (uuid::Uuid,) = sqlx::query_as(
        "INSERT INTO track_cache (id, source, source_id, canonical_key, title, artist) \
         VALUES ($1, 'itunes', $2, $3, 'T', 'A') RETURNING id",
    )
    .bind(uuid::Uuid::now_v7())
    .bind(format!("tr-{tag}"))
    .bind(format!("itunes:tr-{tag}"))
    .fetch_one(&pool)
    .await
    .unwrap();
    let request = requests
        .create_request(session.id, guest_id, track_id, None, &format!("fix-{tag}"))
        .await
        .unwrap()
        .expect("created");

    Some(PaymentFixtures {
        env,
        pool,
        billing,
        guest_id,
        request_id: request.id,
        session_id: session.id,
        track_id,
    })
}

/// ДЕФЕКТ 2: один request → один платёж. Второй POST на тот же request с
/// ДРУГИМ Idempotency-Key возвращает тот же payment_id, провайдер вызван
/// ровно один раз, в БД одна строка payment.
#[tokio::test]
async fn e2e_payment_one_per_request_provider_called_once() {
    let (stripe, counter) = CountingStripe::new();
    let fx = match payment_fixtures(Some(std::sync::Arc::new(stripe))).await {
        Some(f) => f,
        None => {
            eprintln!("DATABASE_URL недоступен — e2e one-per-request пропущен");
            return;
        }
    };
    let uri = format!("/v1/requests/{}/payment", fx.request_id);
    let token = fx.guest_id.to_string();

    // POST 1 (key A) → 201 created.
    let (s1, b1) = http_call(&fx.env, "POST", &uri, None, Some(&token), Some("pay-key-A")).await;
    assert_eq!(s1, StatusCode::CREATED, "create intent: {b1}");
    let id1 = b1["payment_id"].as_str().expect("payment_id").to_string();

    // POST 2 (key B, тот же request) → 200 replay, тот же payment_id.
    let (s2, b2) = http_call(&fx.env, "POST", &uri, None, Some(&token), Some("pay-key-B")).await;
    assert_eq!(s2, StatusCode::OK, "second POST must replay: {b2}");
    let id2 = b2["payment_id"].as_str().expect("payment_id").to_string();
    assert_eq!(id1, id2, "second POST must return SAME payment_id");

    // Провайдер вызван ровно один раз (replay не дёргает Stripe).
    assert_eq!(
        counter.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "provider must be called exactly once"
    );

    // В БД ровно одна строка payment для request.
    let (cnt,): (i64,) = sqlx::query_as("SELECT count(*) FROM payment WHERE request_id = $1")
        .bind(fx.request_id)
        .fetch_one(&fx.pool)
        .await
        .unwrap();
    assert_eq!(cnt, 1, "exactly one payment per request");
}

/// ДЕФЕКТ 3: ретрай тем же Idempotency-Key → 200/тот же id, без 500.
#[tokio::test]
async fn e2e_payment_idempotency_key_replay_no_500() {
    let (stripe, counter) = CountingStripe::new();
    let fx = match payment_fixtures(Some(std::sync::Arc::new(stripe))).await {
        Some(f) => f,
        None => {
            eprintln!("DATABASE_URL недоступен — e2e idem-replay пропущен");
            return;
        }
    };
    let uri = format!("/v1/requests/{}/payment", fx.request_id);
    let token = fx.guest_id.to_string();

    let (s1, b1) = http_call(&fx.env, "POST", &uri, None, Some(&token), Some("same-key")).await;
    assert_eq!(s1, StatusCode::CREATED, "create: {b1}");
    let id1 = b1["payment_id"].as_str().unwrap().to_string();

    // Сетевой ретрай тем же ключом — НЕ 500, а replay 200 + тот же id.
    let (s2, b2) = http_call(&fx.env, "POST", &uri, None, Some(&token), Some("same-key")).await;
    assert_ne!(
        s2,
        StatusCode::INTERNAL_SERVER_ERROR,
        "replay must NOT return 500: {b2}"
    );
    assert_eq!(s2, StatusCode::OK, "replay: {b2}");
    let id2 = b2["payment_id"].as_str().unwrap().to_string();
    assert_eq!(id1, id2, "same payment_id on replay");

    // Провайдер вызван один раз — повтор не дошёл до Stripe.
    assert_eq!(
        counter.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "provider called once on same-key replay"
    );

    let (cnt,): (i64,) = sqlx::query_as("SELECT count(*) FROM payment WHERE request_id = $1")
        .bind(fx.request_id)
        .fetch_one(&fx.pool)
        .await
        .unwrap();
    assert_eq!(cnt, 1, "no duplicate payment row");
}

/// ДЕФЕКТ 2 (гонка): два параллельных POST на один request (разные ключи)
/// создают ровно один платёж (проверка SQL-запросом).
#[tokio::test]
async fn e2e_payment_parallel_race_creates_one_payment() {
    let fx = match payment_fixtures(None).await {
        Some(f) => f,
        None => {
            eprintln!("DATABASE_URL недоступен — e2e parallel-race пропущен");
            return;
        }
    };
    let uri = format!("/v1/requests/{}/payment", fx.request_id);
    let token = fx.guest_id.to_string();

    // Два параллельных запроса (join) с разными Idempotency-Key.
    let env1 = fx.env.clone();
    let env2 = fx.env.clone();
    let uri1 = uri.clone();
    let uri2 = uri.clone();
    let t1 = token.clone();
    let t2 = token.clone();
    let (r1, r2) = tokio::join!(
        async { http_call(&env1, "POST", &uri1, None, Some(&t1), Some("race-A")).await },
        async { http_call(&env2, "POST", &uri2, None, Some(&t2), Some("race-B")).await },
    );
    // Оба успешны (один 201, другой 200 replay-выигравшего).
    assert!(r1.0.is_success(), "race r1: {r1:?}");
    assert!(r2.0.is_success(), "race r2: {r2:?}");

    // В БД ровно ОДНА строка payment — FOR UPDATE по request сериализовал.
    let (cnt,): (i64,) = sqlx::query_as("SELECT count(*) FROM payment WHERE request_id = $1")
        .bind(fx.request_id)
        .fetch_one(&fx.pool)
        .await
        .unwrap();
    assert_eq!(cnt, 1, "parallel race must create exactly one payment");

    // И оба ответа указывают на один и тот же payment_id.
    let id1 = r1.1["payment_id"].as_str().unwrap().to_string();
    let id2 = r2.1["payment_id"].as_str().unwrap().to_string();
    assert_eq!(id1, id2, "both parallel responses return same payment_id");
}

/// ДЕФЕКТ 1: confirm требует аутентификацию гостя и владение платежом.
/// Без токена → 401; чужой device-token → 404; владелец → 200.
#[tokio::test]
async fn e2e_confirm_auth_and_ownership() {
    let fx = match payment_fixtures(None).await {
        Some(f) => f,
        None => {
            eprintln!("DATABASE_URL недоступен — e2e confirm-auth пропущен");
            return;
        }
    };
    // Платёж, принадлежащий fx.guest_id (через request).
    let payment = fx
        .billing
        .create_payment(
            fx.request_id,
            1000,
            "auth-pay-key",
            "pi_mock_auth",
            &CommissionParams {
                commission_bp: 3000,
                dj_share_bp: 5000,
                currency: "EUR".into(),
            },
        )
        .await
        .unwrap();
    let uri = format!("/v1/payments/{}/confirm", payment.id);
    let owner = fx.guest_id.to_string();
    // Чужой device-token (UUID, в БД может не быть — auth по формату,
    // владение всё равно не пройдёт).
    let foreign = uuid::Uuid::now_v7().to_string();
    let body = json!({ "payment_method": "pm_mock" });

    // Без токена → 401.
    let (s, _) = http_call(
        &fx.env,
        "POST",
        &uri,
        Some(body.clone()),
        None,
        Some("c-noauth"),
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED, "no token → 401");

    // Чужой device-token → 404 (владение, существование не раскрывается).
    let (s, b) = http_call(
        &fx.env,
        "POST",
        &uri,
        Some(body.clone()),
        Some(&foreign),
        Some("c-foreign"),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND, "foreign token → 404: {b}");

    // Владелец → 200 processing.
    let (s, b) = http_call(
        &fx.env,
        "POST",
        &uri,
        Some(body),
        Some(&owner),
        Some("c-owner"),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "owner → 200: {b}");
    assert_eq!(b["status"], "processing");
}

/// ДЕФЕКТ 1 (идемпотентность confirm): повтор тем же Idempotency-Key →
/// 200 и состояние не меняется второй раз.
#[tokio::test]
async fn e2e_confirm_idempotent_replay() {
    let fx = match payment_fixtures(None).await {
        Some(f) => f,
        None => {
            eprintln!("DATABASE_URL недоступен — e2e confirm-replay пропущен");
            return;
        }
    };
    let payment = fx
        .billing
        .create_payment(
            fx.request_id,
            1000,
            "replay-pay-key",
            "pi_mock_replay",
            &CommissionParams {
                commission_bp: 3000,
                dj_share_bp: 5000,
                currency: "EUR".into(),
            },
        )
        .await
        .unwrap();
    let uri = format!("/v1/payments/{}/confirm", payment.id);
    let owner = fx.guest_id.to_string();
    let body = json!({ "payment_method": "pm_mock" });

    // Первый confirm → 200 processing.
    let (s1, b1) = http_call(
        &fx.env,
        "POST",
        &uri,
        Some(body.clone()),
        Some(&owner),
        Some("replay-key"),
    )
    .await;
    assert_eq!(s1, StatusCode::OK, "confirm 1: {b1}");
    assert_eq!(b1["status"], "processing");

    // Повтор тем же ключом → 200 processing, состояние НЕ меняется.
    let (s2, b2) = http_call(
        &fx.env,
        "POST",
        &uri,
        Some(body),
        Some(&owner),
        Some("replay-key"),
    )
    .await;
    assert_eq!(s2, StatusCode::OK, "confirm replay: {b2}");
    assert_eq!(b2["status"], "processing");

    // Состояние в БД не изменилось повторным вызовом.
    let p = fx.billing.find_payment(payment.id).await.unwrap().unwrap();
    assert_eq!(p.status, "processing", "state unchanged after replay");
}

// ============================================================
// W7-A: гонки в платежах (confirm + идемпотентная вставка).
// ============================================================

/// Комиссионные параметры Q1-дефолта (30% / 50/50) — общий хелпер для
/// новых W7-A-тестов, чтобы не дублировать литерал.
fn q1_commission() -> CommissionParams {
    CommissionParams {
        commission_bp: 3000,
        dj_share_bp: 5000,
        currency: "EUR".into(),
    }
}

/// W7-A ДЕФЕКТ 1 (гонка confirm): два параллельных `POST /confirm` на один
/// платёж. До фикса второй вызов получал `InvalidTransition` (платёж уже
/// `processing` после параллельного подтверждения) и хендлер вызывал
/// `begin_cancel`, который «слепым» `UPDATE` переводил подтверждённый
/// платёж в `cancelling` → гость получал 409 `session_paused`.
///
/// После фикса: `InvalidTransition` — это replay, а не GUARD-провал;
/// `begin_cancel` не вызывается, хендлер перечитывает статус и отвечает
/// `200 processing`. В БД статус `processing`, НЕ `cancelling`.
#[tokio::test]
async fn e2e_confirm_parallel_race_does_not_cancel() {
    let fx = match payment_fixtures(None).await {
        Some(f) => f,
        None => {
            eprintln!("DATABASE_URL недоступен — e2e confirm-race пропущен");
            return;
        }
    };
    let payment = fx
        .billing
        .create_payment(
            fx.request_id,
            1000,
            "race-cfm-key",
            "pi_mock_race_cfm",
            &q1_commission(),
        )
        .await
        .unwrap();
    assert_eq!(payment.status, "intent");

    let uri = format!("/v1/payments/{}/confirm", payment.id);
    let token = fx.guest_id.to_string();
    let body = json!({ "payment_method": "pm_mock" });

    // Два параллельных confirm через tokio::join! на один платёж.
    let env1 = fx.env.clone();
    let env2 = fx.env.clone();
    let uri1 = uri.clone();
    let uri2 = uri.clone();
    let b1 = body.clone();
    let b2 = body.clone();
    let t1 = token.clone();
    let t2 = token.clone();
    let (r1, r2) = tokio::join!(
        async {
            http_call(
                &env1,
                "POST",
                &uri1,
                Some(b1),
                Some(&t1),
                Some("cfm-race-A"),
            )
            .await
        },
        async {
            http_call(
                &env2,
                "POST",
                &uri2,
                Some(b2),
                Some(&t2),
                Some("cfm-race-B"),
            )
            .await
        },
    );

    // Оба успешны (200): один применил переход intent→processing, второй —
    // replay уже подтверждённого.
    assert_eq!(r1.0, StatusCode::OK, "race r1 must be 200: {:?}", r1);
    assert_eq!(r2.0, StatusCode::OK, "race r2 must be 200: {:?}", r2);
    assert_eq!(r1.1["status"], "processing", "race r1 status: {:?}", r1);
    assert_eq!(r2.1["status"], "processing", "race r2 status: {:?}", r2);

    // ГЛАВНАЯ проверка дефекта: в БД статус processing, НЕ cancelling.
    // До фикса второй вызов отменял подтверждённый платёж → cancelling.
    let p = fx.billing.find_payment(payment.id).await.unwrap().unwrap();
    assert_eq!(
        p.status, "processing",
        "parallel confirm must NOT cancel confirmed payment"
    );

    // Деньги не захвачены (capture — отдельный вебхук), ledger/split пусты.
    let (lc,): (i64,) = sqlx::query_as("SELECT count(*) FROM ledger_entry WHERE ref = $1")
        .bind(payment.id.to_string())
        .fetch_one(&fx.pool)
        .await
        .unwrap();
    let (sc,): (i64,) = sqlx::query_as("SELECT count(*) FROM split WHERE payment_id = $1")
        .bind(payment.id)
        .fetch_one(&fx.pool)
        .await
        .unwrap();
    assert_eq!(lc, 0, "no ledger on confirm-only");
    assert_eq!(sc, 0, "no split on confirm-only");
}

/// W7-A: GUARD-провал (сессия закрыта) → платёж `cancelling`, деньги НЕ
/// захвачены. Это отдельный тест от гонки confirm: здесь отмена ОЖИДАЕМА
/// и корректна (сессия реально не active → «заплачено за expired»
/// предотвращается авто-cancel интента).
#[tokio::test]
async fn e2e_confirm_guard_failure_cancels_no_capture() {
    let fx = match payment_fixtures(None).await {
        Some(f) => f,
        None => {
            eprintln!("DATABASE_URL недоступен — e2e guard-cancel пропущен");
            return;
        }
    };
    let payment = fx
        .billing
        .create_payment(
            fx.request_id,
            1000,
            "guard-cfm-key",
            "pi_mock_guard_cfm",
            &q1_commission(),
        )
        .await
        .unwrap();
    assert_eq!(payment.status, "intent");

    // Закрыть сессию (имитация паузы DJ-ем) — GUARD confirm провалится.
    sqlx::query("UPDATE session SET status = 'paused' WHERE id = $1")
        .bind(fx.session_id)
        .execute(&fx.pool)
        .await
        .unwrap();

    let uri = format!("/v1/payments/{}/confirm", payment.id);
    let token = fx.guest_id.to_string();
    let body = json!({ "payment_method": "pm_mock" });
    let (s, b) = http_call(
        &fx.env,
        "POST",
        &uri,
        Some(body),
        Some(&token),
        Some("guard-cfm"),
    )
    .await;
    assert_eq!(s, StatusCode::CONFLICT, "guard: closed session: {s} {b}");
    assert_eq!(b["code"], "session_paused", "guard code: {b}");

    // Интент перешёл в cancelling (авто-отмена).
    let p = fx.billing.find_payment(payment.id).await.unwrap().unwrap();
    assert_eq!(p.status, "cancelling", "intent auto-cancelled by guard");

    // Деньги НЕ захвачены: ledger/split пусты.
    let (lc,): (i64,) = sqlx::query_as("SELECT count(*) FROM ledger_entry WHERE ref = $1")
        .bind(payment.id.to_string())
        .fetch_one(&fx.pool)
        .await
        .unwrap();
    let (sc,): (i64,) = sqlx::query_as("SELECT count(*) FROM split WHERE payment_id = $1")
        .bind(payment.id)
        .fetch_one(&fx.pool)
        .await
        .unwrap();
    assert_eq!(lc, 0, "no ledger on cancelled intent");
    assert_eq!(sc, 0, "no split on cancelled intent");
}

/// W7-A ДЕФЕКТ 2 (чужой Idempotency-Key): request A создаёт платёж с
/// ключом K; request B (другой `request_id`) с тем же K → 409
/// `idempotency_conflict`, платёж B НЕ создан, гость B НЕ получил чужой
/// `payment_id`. До фикса `ON CONFLICT … DO UPDATE RETURNING` отдавал
/// строку request A → гость B получал чужой payment_id.
///
/// Также проверяется replay: повтор того же request A с тем же ключом →
/// тот же `payment_id` (идемпотентность сохранена).
#[tokio::test]
async fn e2e_idempotency_key_foreign_request_conflict() {
    let fx = match payment_fixtures(None).await {
        Some(f) => f,
        None => {
            eprintln!("DATABASE_URL недоступен — e2e idem-conflict пропущен");
            return;
        }
    };
    let requests = RequestRepo::new(fx.pool.clone());
    // Второй request того же гостя/сессии (другой request_id).
    let req_b = requests
        .create_request(
            fx.session_id,
            fx.guest_id,
            fx.track_id,
            None,
            &format!("idem-conflict-req-{}", uuid::Uuid::now_v7()),
        )
        .await
        .unwrap()
        .expect("created");

    let token = fx.guest_id.to_string();
    let uri_a = format!("/v1/requests/{}/payment", fx.request_id);
    let uri_b = format!("/v1/requests/{}/payment", req_b.id);

    // A с ключом K → 201 created.
    let (s, b) = http_call(
        &fx.env,
        "POST",
        &uri_a,
        None,
        Some(&token),
        Some("shared-key-K"),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "create A: {b}");
    let id_a = b["payment_id"]
        .as_str()
        .expect("payment_id for A")
        .to_string();

    // B (другой request) с тем же ключом K → 409 idempotency_conflict.
    let (s, b) = http_call(
        &fx.env,
        "POST",
        &uri_b,
        None,
        Some(&token),
        Some("shared-key-K"),
    )
    .await;
    assert_eq!(s, StatusCode::CONFLICT, "foreign key must conflict: {b}");
    assert_eq!(b["code"], "idempotency_conflict", "conflict code: {b}");
    // Чужой payment_id НЕ выдан.
    assert!(
        b["payment_id"].is_null(),
        "foreign request must NOT receive payment_id: {b}"
    );

    // Платёж B НЕ создан.
    let (cnt_b,): (i64,) = sqlx::query_as("SELECT count(*) FROM payment WHERE request_id = $1")
        .bind(req_b.id)
        .fetch_one(&fx.pool)
        .await
        .unwrap();
    assert_eq!(cnt_b, 0, "no payment created for foreign request B");

    // Replay того же request A с тем же ключом → тот же payment_id (200).
    let (s, b) = http_call(
        &fx.env,
        "POST",
        &uri_a,
        None,
        Some(&token),
        Some("shared-key-K"),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "replay A: {b}");
    assert_eq!(
        b["payment_id"].as_str().unwrap(),
        id_a.as_str(),
        "same request + same key → same payment_id"
    );
}
