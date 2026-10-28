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
//! Провайдер — `ApiEnv.stripe`: MockStripe (по умолчанию) для основного цикла;
//! `FakeStripe` (со счётчиком вызовов) — для проверок Connect-transfer:
//! успешный transfer → completed, ошибка провайдера → failed, повторный
//! запуск не создаёт второй transfer; отсутствует `stripe_account_id` → 409.

use axum::http::StatusCode;
use serde_json::{json, Value};

use crate::e2e::{app_env, call, test_repo};
use dropit_common::Role;
use dropit_domain::{PayoutStatus, MAX_PAYOUT_ATTEMPTS};
use dropit_infra_db::{BillingRepo, CommissionParams, PayoutRepo, RequestRepo, SessionRepo};
use dropit_infra_stripe as stripe_crate;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

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
        let (s, body) = signed_transfer_webhook(
            &env,
            &format!("evt_{}", row.id),
            "transfer.paid",
            &tid,
            None,
        )
        .await;
        assert_eq!(s, StatusCode::OK, "transfer paid: {body}");

        // ДУБЛЬ вебхука — no-op (идемпотентно по transfer_id).
        let (s, _) = signed_transfer_webhook(
            &env,
            &format!("evt_dup_{}", row.id),
            "transfer.paid",
            &tid,
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
    // create_payout не выдумывает transfer id — он NULL, пока провайдер
    // его не вернёт. Финальная выплата здесь идёт через репо напрямую
    // (без провайдера), поэтому смоделируем ответ провайдера: attach
    // реального id и закрытие через transfer.paid.
    let final_id = final_rows[0].id;
    assert!(
        final_rows[0].stripe_transfer_id.is_none(),
        "create_payout must NOT fabricate a transfer id"
    );
    let tid = format!("tr_final_{final_id}");
    payouts
        .attach_transfer(final_id, &tid, PayoutStatus::InFlight)
        .await
        .unwrap();
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
    // run_payout_cycle создал выплату с stripe_transfer_id = NULL —
    // провайдер не вызывался. Чтобы проверить вебхук-матчинг по
    // реальному id (а не по mock), прикрепим id, как сделал бы роут
    // после Stripe.create_transfer.
    assert!(
        victim.stripe_transfer_id.is_none(),
        "victim payout has no transfer id until attach"
    );
    let victim_id = victim.id;
    let mut tid = format!("tr_chaos_{victim_id}");
    payouts
        .attach_transfer(victim_id, &tid, PayoutStatus::InFlight)
        .await
        .unwrap();
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
        current = payouts.retry_payout(current.id).await.unwrap(); // in_flight, transfer id сброшен в NULL
                                                                   // retry обнулил transfer id — прикрепим новый, как после нового
                                                                   // вызова провайдера, прежде чем ломать вебхуком.
        assert!(
            current.stripe_transfer_id.is_none(),
            "retry_payout must clear transfer id"
        );
        tid = format!("tr_chaos_{}_{}", current.id, current.attempts);
        payouts
            .attach_transfer(current.id, &tid, PayoutStatus::InFlight)
            .await
            .unwrap();
        current = payouts
            .handle_transfer_failed(&tid, "insufficient platform balance")
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

/// Фикстуры: клуб + DJ (с `stripe_account_id` — требуется для Connect-transfer).
async fn fixture(pool: &sqlx::PgPool, tag: &str) -> (uuid::Uuid, uuid::Uuid) {
    let club_id = uuid::Uuid::now_v7();
    let dj_id = uuid::Uuid::now_v7();
    sqlx::query(
        "INSERT INTO club (id, name, slug, email, password_hash, stripe_account_id) \
         VALUES ($1, $2, $3, $4, 'x', $5)",
    )
    .bind(club_id)
    .bind(format!("F6 Club {tag}"))
    .bind(format!("f6-club-{tag}"))
    .bind(format!("f6-{tag}@club.test"))
    .bind(format!("acct_mock_club_{tag}"))
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO dj (id, club_id, email, password_hash, display_name, stripe_account_id) \
         VALUES ($1, $2, $3, 'x', 'DJ F6', $4)",
    )
    .bind(dj_id)
    .bind(club_id)
    .bind(format!("f6-{tag}@dj.test"))
    .bind(format!("acct_mock_dj_{tag}"))
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

// ============================================================
// Connect-transfer: фейковый провайдер со счётчиком вызовов.
// ============================================================

/// Фейковый провайдер Stripe для тестов выплат и онбординга.
///
/// `create_transfer` считает вызовы (`transfer_calls`) и может отказывать
/// (`fail_transfer`); `retrieve_account` возвращает заданное состояние
/// (для онбординг-теста). Остальные методы делегируют `MockStripe`.
struct FakeStripe {
    transfer_calls: AtomicU64,
    fail_transfer: bool,
    account: stripe_crate::ConnectedAccount,
}

impl FakeStripe {
    fn succeeding() -> Arc<Self> {
        Arc::new(Self {
            transfer_calls: AtomicU64::new(0),
            fail_transfer: false,
            account: stripe_crate::ConnectedAccount {
                id: "acct_fake".into(),
                charges_enabled: true,
                payouts_enabled: true,
                details_submitted: true,
            },
        })
    }
    fn failing() -> Arc<Self> {
        Arc::new(Self {
            transfer_calls: AtomicU64::new(0),
            fail_transfer: true,
            account: stripe_crate::ConnectedAccount {
                id: "acct_fake".into(),
                charges_enabled: false,
                payouts_enabled: false,
                details_submitted: false,
            },
        })
    }
    fn transfer_calls(&self) -> u64 {
        self.transfer_calls.load(Ordering::SeqCst)
    }
}

#[async_trait::async_trait]
impl stripe_crate::StripeApi for FakeStripe {
    async fn create_payment_intent(
        &self,
        req: stripe_crate::CreateIntent,
    ) -> Result<stripe_crate::IntentResult, stripe_crate::StripeError> {
        stripe_crate::MockStripe.create_payment_intent(req).await
    }
    async fn capture(
        &self,
        intent_id: &str,
        amount_minor: Option<i64>,
        idempotency_key: &str,
    ) -> Result<stripe_crate::IntentResult, stripe_crate::StripeError> {
        stripe_crate::MockStripe
            .capture(intent_id, amount_minor, idempotency_key)
            .await
    }
    async fn refund(
        &self,
        req: stripe_crate::RefundReq,
    ) -> Result<stripe_crate::RefundResult, stripe_crate::StripeError> {
        stripe_crate::MockStripe.refund(req).await
    }
    async fn create_connected_account(
        &self,
        req: stripe_crate::ConnectAccountReq,
    ) -> Result<stripe_crate::ConnectedAccount, stripe_crate::StripeError> {
        stripe_crate::MockStripe.create_connected_account(req).await
    }
    async fn retrieve_account(
        &self,
        account_id: &str,
    ) -> Result<stripe_crate::ConnectedAccount, stripe_crate::StripeError> {
        let _ = account_id;
        Ok(self.account.clone())
    }
    async fn account_link(
        &self,
        account_id: &str,
        refresh_url: &str,
        return_url: &str,
    ) -> Result<String, stripe_crate::StripeError> {
        stripe_crate::MockStripe
            .account_link(account_id, refresh_url, return_url)
            .await
    }
    async fn create_transfer(
        &self,
        req: stripe_crate::TransferReq,
    ) -> Result<stripe_crate::TransferResult, stripe_crate::StripeError> {
        self.transfer_calls.fetch_add(1, Ordering::SeqCst);
        if self.fail_transfer {
            return Err(stripe_crate::StripeError::Api {
                status: 402,
                code: "insufficient_balance".into(),
                message: "fake: not enough balance".into(),
            });
        }
        Ok(stripe_crate::TransferResult {
            id: format!("tr_fake_{}", req.idempotency_key),
            amount_minor: req.amount_minor,
            status: "paid".into(),
        })
    }
    fn verify_webhook(
        &self,
        payload: &[u8],
        signature_header: &str,
        tolerance_secs: i64,
    ) -> Result<stripe_crate::WebhookEvent, stripe_crate::StripeError> {
        stripe_crate::MockStripe.verify_webhook(payload, signature_header, tolerance_secs)
    }
}

/// Успешный Connect-transfer → payout completed (через transfer.paid),
/// деньги не задвоены; повторный запуск не создаёт второй transfer
/// (проверка по числу вызовов фейкового провайдера).
#[tokio::test]
async fn e2e_payout_transfer_success_and_idempotent_rerun() {
    let repo = match test_repo().await {
        Some(r) => r,
        None => {
            eprintln!("DATABASE_URL недоступен — e2e payout-transfer пропущен");
            return;
        }
    };
    let sessions = SessionRepo::new(repo.pool());
    let requests = RequestRepo::new(repo.pool());
    let billing = BillingRepo::new(repo.pool());
    let payouts = PayoutRepo::new(repo.pool());
    let fake = FakeStripe::succeeding();
    let mut env = app_env(Some(repo.clone()), Some(sessions), Some(requests)).await;
    env.stripe = Some(fake.clone());
    let pool = repo.pool();

    let (club_id, dj_id) = fixture(&pool, "tr").await;
    // Захват 30 € → сплиты 900/1050/1050 (club + dj выше порога).
    captured_payment(&env, &billing, &pool, club_id, dj_id, 3000, "tr-p1").await;

    let admin_token = env
        .keys
        .issue_access(uuid::Uuid::now_v7(), Role::Admin, uuid::Uuid::now_v7())
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
    assert_eq!(body["payouts_created"], 2, "club + dj payouts: {body}");
    // Один transfer на каждую выплату.
    assert_eq!(
        fake.transfer_calls(),
        2,
        "create_transfer called once per payout"
    );

    // Закрываем выплаты через transfer.paid (idempotently по transfer_id).
    let in_flight = payouts.in_flight_payouts().await.unwrap();
    assert_eq!(in_flight.len(), 2);
    for row in &in_flight {
        // РЕАЛЬНЫЙ id провайдера записан в БД (а не выдуманный mock):
        // FakeStripe возвращает "tr_fake_payout:{id}".
        let tid = row
            .stripe_transfer_id
            .clone()
            .expect("real provider transfer id attached");
        assert!(
            tid.starts_with("tr_fake_payout:"),
            "stripe_transfer_id must be the provider's real id, got {tid}"
        );
        let (s, body) = signed_transfer_webhook(
            &env,
            &format!("evt_{}", row.id),
            "transfer.paid",
            &tid,
            None,
        )
        .await;
        assert_eq!(s, StatusCode::OK, "transfer paid: {body}");
        let updated = payouts.find_payout(row.id).await.unwrap().unwrap();
        assert_eq!(
            updated.status, "paid",
            "payout completed after transfer.paid"
        );
        assert_eq!(
            updated.stripe_transfer_id.as_deref(),
            Some(tid.as_str()),
            "transfer id preserved after completion"
        );
    }

    let calls_before_rerun = fake.transfer_calls();
    // Повторный запуск: нетто пусто (сплиты paid_out) → новых выплат нет,
    // create_transfer НЕ вызывается повторно.
    let (s, body) = call(
        &env,
        "POST",
        "/v1/admin/payouts/run",
        None,
        Some(&admin_token),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "rerun: {body}");
    assert_eq!(
        body["payouts_created"], 0,
        "no new payouts on rerun: {body}"
    );
    assert_eq!(
        fake.transfer_calls(),
        calls_before_rerun,
        "rerun must not create a second transfer"
    );

    // Инвариант payee держится, деньги не задвоены.
    let violations = payouts.payee_invariant_violations().await.unwrap();
    assert!(violations.is_empty(), "payee invariant: {violations:?}");
}

/// Ошибка провайдера при transfer → payout failed; деньги не задвоены
/// (сплиты остаются paid_out, повторный цикл их не видит); второй transfer
/// не создаётся.
#[tokio::test]
async fn e2e_payout_transfer_failure_marks_failed() {
    let repo = match test_repo().await {
        Some(r) => r,
        None => {
            eprintln!("DATABASE_URL недоступен — e2e payout-fail пропущен");
            return;
        }
    };
    let sessions = SessionRepo::new(repo.pool());
    let requests = RequestRepo::new(repo.pool());
    let billing = BillingRepo::new(repo.pool());
    let payouts = PayoutRepo::new(repo.pool());
    let fake = FakeStripe::failing();
    let mut env = app_env(Some(repo.clone()), Some(sessions), Some(requests)).await;
    env.stripe = Some(fake.clone());
    let pool = repo.pool();

    let (club_id, dj_id) = fixture(&pool, "fl").await;
    captured_payment(&env, &billing, &pool, club_id, dj_id, 3000, "fl-p1").await;

    let admin_token = env
        .keys
        .issue_access(uuid::Uuid::now_v7(), Role::Admin, uuid::Uuid::now_v7())
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
    assert_eq!(body["payouts_created"], 2, "payouts created: {body}");
    // create_transfer вызван (и отказал) по разу на выплату.
    assert_eq!(
        fake.transfer_calls(),
        2,
        "transfer attempted once per payout"
    );

    // Все созданные выплаты — failed (провайдер отказал).
    let in_flight = payouts.in_flight_payouts().await.unwrap();
    assert!(
        in_flight.is_empty(),
        "no in_flight payouts after provider failure"
    );
    let (failed_cnt,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM payout WHERE status = 'failed'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(failed_cnt, 2, "both payouts marked failed");

    // При ошибке провайдера transfer id НЕ зафиксирован (NULL): матчить
    // вебхук не по чему — выплата провалена по payout_id.
    let (null_tid_cnt,): (i64,) = sqlx::query_as(
        "SELECT count(*) FROM payout WHERE status = 'failed' AND stripe_transfer_id IS NULL",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        null_tid_cnt, 2,
        "failed payouts must have NULL stripe_transfer_id"
    );

    // Деньги не задвоены: сплиты payee остались paid_out (не вернулись в pending).
    let (paid_out_cnt,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM split WHERE payee_id = $1 AND status = 'paid_out'")
            .bind(club_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(
        paid_out_cnt > 0,
        "splits remain paid_out (locked, not doubled)"
    );

    // Повторный запуск: нетто пусто → нет новых выплат, нет второго transfer.
    let calls_before = fake.transfer_calls();
    let (s, body) = call(
        &env,
        "POST",
        "/v1/admin/payouts/run",
        None,
        Some(&admin_token),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "rerun after fail: {body}");
    assert_eq!(body["payouts_created"], 0, "no new payouts: {body}");
    assert_eq!(
        fake.transfer_calls(),
        calls_before,
        "no second transfer on rerun"
    );
}

/// Вебхук `transfer.paid` матчится по РЕАЛЬНОМУ `stripe_transfer_id` из БД:
/// с настоящим id → выплата completed; с чужим id → игнор (200, статус не
/// меняется). Проверяет, что после `attach_transfer` вебхук находит строку
/// именно по провайдерскому идентификатору, а не по выдуманному mock-id.
#[tokio::test]
async fn e2e_payout_transfer_webhook_matches_real_id() {
    let repo = match test_repo().await {
        Some(r) => r,
        None => {
            eprintln!("DATABASE_URL недоступен — e2e payout-webhook-match пропущен");
            return;
        }
    };
    let sessions = SessionRepo::new(repo.pool());
    let requests = RequestRepo::new(repo.pool());
    let billing = BillingRepo::new(repo.pool());
    let payouts = PayoutRepo::new(repo.pool());
    let fake = FakeStripe::succeeding();
    let mut env = app_env(Some(repo.clone()), Some(sessions), Some(requests)).await;
    env.stripe = Some(fake.clone());
    let pool = repo.pool();

    let (club_id, dj_id) = fixture(&pool, "wh").await;
    captured_payment(&env, &billing, &pool, club_id, dj_id, 3000, "wh-p1").await;

    let admin_token = env
        .keys
        .issue_access(uuid::Uuid::now_v7(), Role::Admin, uuid::Uuid::now_v7())
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

    // Берём одну in_flight выплату с прикреплённым реальным id.
    let mut in_flight = payouts.in_flight_payouts().await.unwrap();
    assert!(
        !in_flight.is_empty(),
        "expected in_flight payouts after run"
    );
    let row = in_flight.remove(0);
    let real_tid = row
        .stripe_transfer_id
        .clone()
        .expect("real provider transfer id attached");
    assert!(
        real_tid.starts_with("tr_fake_payout:"),
        "real provider id stored, got {real_tid}"
    );

    // Чужой id — вебхук игнорируется: 200, выплата остаётся in_flight.
    let foreign = "tr_foreign_not_in_db";
    let (s, body) =
        signed_transfer_webhook(&env, "evt_foreign", "transfer.paid", foreign, None).await;
    assert_eq!(s, StatusCode::OK, "foreign-id webhook acknowledged: {body}");
    let untouched = payouts.find_payout(row.id).await.unwrap().unwrap();
    assert_eq!(
        untouched.status, "in_flight",
        "foreign transfer_id must NOT close the payout"
    );

    // Реальный id из БД → выплата completed.
    let (s, body) =
        signed_transfer_webhook(&env, "evt_real", "transfer.paid", &real_tid, None).await;
    assert_eq!(s, StatusCode::OK, "real-id webhook: {body}");
    let closed = payouts.find_payout(row.id).await.unwrap().unwrap();
    assert_eq!(closed.status, "paid", "real transfer_id closes payout");
}

/// Отсутствие `stripe_account_id` у payable payee → 409
/// `stripe_account_missing`, без вызова провайдера.
#[tokio::test]
async fn e2e_payout_missing_stripe_account_409() {
    let repo = match test_repo().await {
        Some(r) => r,
        None => {
            eprintln!("DATABASE_URL недоступен — e2e payout-409 пропущен");
            return;
        }
    };
    let sessions = SessionRepo::new(repo.pool());
    let requests = RequestRepo::new(repo.pool());
    let billing = BillingRepo::new(repo.pool());
    let payouts = PayoutRepo::new(repo.pool());
    let fake = FakeStripe::succeeding();
    let mut env = app_env(Some(repo.clone()), Some(sessions), Some(requests)).await;
    env.stripe = Some(fake.clone());
    let pool = repo.pool();

    // Клуб БЕЗ stripe_account_id + DJ без аккаунта.
    let club_id = uuid::Uuid::now_v7();
    let dj_id = uuid::Uuid::now_v7();
    sqlx::query(
        "INSERT INTO club (id, name, slug, email, password_hash) \
         VALUES ($1, 'NoAcct Club', 'noacct-club', 'noacct@club.test', 'x')",
    )
    .bind(club_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO dj (id, club_id, email, password_hash, display_name) \
         VALUES ($1, $2, 'noacct@dj.test', 'x', 'DJ NA')",
    )
    .bind(dj_id)
    .bind(club_id)
    .execute(&pool)
    .await
    .unwrap();
    captured_payment(&env, &billing, &pool, club_id, dj_id, 3000, "na-p1").await;

    let admin_token = env
        .keys
        .issue_access(uuid::Uuid::now_v7(), Role::Admin, uuid::Uuid::now_v7())
        .unwrap();
    let (s, body) = call(
        &env,
        "POST",
        "/v1/admin/payouts/run",
        None,
        Some(&admin_token),
    )
    .await;
    assert_eq!(s, StatusCode::CONFLICT, "missing account: {s} {body}");
    assert_eq!(body["code"], "stripe_account_missing", "code: {body}");

    // Провайдер НЕ вызывался (pre-check до cycle).
    assert_eq!(fake.transfer_calls(), 0, "no transfer call without account");
    // Выплата не создана.
    let (cnt,): (i64,) = sqlx::query_as("SELECT count(*) FROM payout")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(cnt, 0, "no payout created without stripe_account_id");
    // Сплиты остались pending (не потеряны, не задвоены).
    let (pending,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM split WHERE payee_id = $1 AND status = 'pending'")
            .bind(club_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(pending > 0, "splits remain pending (carry over)");
    let _ = payouts;
}

/// Онбординг-эндпоинты используют `ApiEnv.stripe`: фейковый провайдер
/// отдаёт `restricted` Connect-аккаунт, и `onboarding-status` отражает его
/// (а не захардкоженный MockStripe-ответ).
#[tokio::test]
async fn e2e_stripe_onboarding_uses_provider() {
    let repo = match test_repo().await {
        Some(r) => r,
        None => {
            eprintln!("DATABASE_URL недоступен — e2e onboarding-provider пропущен");
            return;
        }
    };
    let sessions = SessionRepo::new(repo.pool());
    // Провайдер отдаёт restricted-аккаунт: онбординг пройден
    // (details_submitted=true), но charges/payouts ещё выключены →
    // `stripe_status_from_account` = "restricted".
    let fake = Arc::new(FakeStripe {
        transfer_calls: AtomicU64::new(0),
        fail_transfer: false,
        account: stripe_crate::ConnectedAccount {
            id: "acct_restricted".into(),
            charges_enabled: false,
            payouts_enabled: false,
            details_submitted: true,
        },
    });
    let mut env = app_env(Some(repo.clone()), Some(sessions), None).await;
    env.stripe = Some(fake);
    let pool = repo.pool();

    let club_id = uuid::Uuid::now_v7();
    sqlx::query(
        "INSERT INTO club (id, name, slug, email, password_hash, country, stripe_account_id, stripe_onboarding_status) \
         VALUES ($1, 'Onb Club', 'onb-club', 'onb@club.test', 'x', 'ES', 'acct_restricted', 'pending')",
    )
    .bind(club_id)
    .execute(&pool)
    .await
    .unwrap();
    let club_tok = env
        .keys
        .issue_access(club_id, Role::Club, uuid::Uuid::now_v7())
        .unwrap();

    let (s, body) = call(
        &env,
        "GET",
        "/v1/club/stripe/onboarding-status",
        None,
        Some(&club_tok),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "onboarding-status: {body}");
    // Провайдер отдал restricted → эндпоинт отразил его состояние.
    assert_eq!(body["stripe_account_id"], "acct_restricted");
    assert_eq!(body["charges_enabled"], false);
    assert_eq!(body["payouts_enabled"], false);
    assert_eq!(body["details_submitted"], true);
    assert_eq!(body["status"], "restricted");
    // Метка в БД обновлена до 'restricted'.
    let (persisted,): (String,) =
        sqlx::query_as("SELECT stripe_onboarding_status FROM club WHERE id = $1")
            .bind(club_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(persisted, "restricted", "status persisted from provider");
}

// ============================================================
// Подписанный transfer-вебхук (формат события Stripe).
// ============================================================

/// Подписанный transfer-вебхук в формате события Stripe.
///
/// Payload: `{"id":event_id,"type":event_type,"data":{"object":{"id":transfer_id}}}`,
/// где `transfer_id` — `stripe_transfer_id` выплаты. Подпись — любой
/// непустой `Stripe-Signature` (MockStripe не проверяет HMAC, только
/// непустой заголовок; реальная криптография — в LiveStripe).
/// `reason` (для transfer.failed) кладётся в `data.object.failure_message`.
async fn signed_transfer_webhook(
    env: &crate::routes::ApiEnv,
    event_id: &str,
    event_type: &str,
    transfer_id: &str,
    reason: Option<&str>,
) -> (StatusCode, Value) {
    transfer_webhook_raw(env, event_id, event_type, transfer_id, reason, true).await
}

/// Transfer-вебхук без заголовка `Stripe-Signature` (для проверки 400).
async fn unsigned_transfer_webhook(
    env: &crate::routes::ApiEnv,
    event_type: &str,
    transfer_id: &str,
) -> (StatusCode, Value) {
    transfer_webhook_raw(env, "evt_unsigned", event_type, transfer_id, None, false).await
}

/// Низкоуровневая отправка transfer-вебхука: сырые байты + опциональный
/// заголовок `stripe-signature`. `with_signature = false` — заголовок
/// отсутствует (проверка 400 «без подписи»).
async fn transfer_webhook_raw(
    env: &crate::routes::ApiEnv,
    event_id: &str,
    event_type: &str,
    transfer_id: &str,
    reason: Option<&str>,
    with_signature: bool,
) -> (StatusCode, Value) {
    let mut object = json!({ "id": transfer_id });
    if let Some(r) = reason {
        object["failure_message"] = json!(r);
    }
    let body = json!({
        "id": event_id,
        "type": event_type,
        "data": { "object": object },
    })
    .to_string();
    // MockStripe требует лишь непустой заголовок; формат t=,v1= оставлён
    // для совместимости с реальным Stripe.
    let t = chrono::Utc::now().timestamp();
    let sig = format!("t={t},v1=e2e_mock_accepts_any_nonempty_header");

    use axum::body::Body;
    use tower::ServiceExt;
    let router = crate::routes::router(env.clone());
    let mut req = axum::http::Request::builder()
        .method("POST")
        .uri("/v1/stripe/transfer-webhook")
        .header("content-type", "application/json");
    if with_signature {
        req = req.header("stripe-signature", sig);
    }
    let req = req.body(Body::from(body)).unwrap();
    let resp = router.oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap_or_default();
    let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, value)
}

// ============================================================
// ДЕФЕКТ 1: подпись transfer-вебхука (критично).
// ============================================================

/// Фейковый провайдер, отклоняющий ЛЮБУЮ подпись вебхука (имитирует
/// поддельную подпись при LiveStripe). Остальные методы делегируют
/// MockStripe (платёжный цикл не нужен в этом тесте). Подкладывается в
/// `ApiEnv.stripe`, чтобы проверить, что маршрут transfer-webhook НЕ
/// применяет событие при отказе подписи.
struct RejectingStripe;

#[async_trait::async_trait]
impl stripe_crate::StripeApi for RejectingStripe {
    async fn create_payment_intent(
        &self,
        req: stripe_crate::CreateIntent,
    ) -> Result<stripe_crate::IntentResult, stripe_crate::StripeError> {
        stripe_crate::MockStripe.create_payment_intent(req).await
    }
    async fn capture(
        &self,
        intent_id: &str,
        amount_minor: Option<i64>,
        idempotency_key: &str,
    ) -> Result<stripe_crate::IntentResult, stripe_crate::StripeError> {
        stripe_crate::MockStripe
            .capture(intent_id, amount_minor, idempotency_key)
            .await
    }
    async fn refund(
        &self,
        req: stripe_crate::RefundReq,
    ) -> Result<stripe_crate::RefundResult, stripe_crate::StripeError> {
        stripe_crate::MockStripe.refund(req).await
    }
    async fn create_connected_account(
        &self,
        req: stripe_crate::ConnectAccountReq,
    ) -> Result<stripe_crate::ConnectedAccount, stripe_crate::StripeError> {
        stripe_crate::MockStripe.create_connected_account(req).await
    }
    async fn retrieve_account(
        &self,
        account_id: &str,
    ) -> Result<stripe_crate::ConnectedAccount, stripe_crate::StripeError> {
        stripe_crate::MockStripe.retrieve_account(account_id).await
    }
    async fn account_link(
        &self,
        account_id: &str,
        refresh_url: &str,
        return_url: &str,
    ) -> Result<String, stripe_crate::StripeError> {
        stripe_crate::MockStripe
            .account_link(account_id, refresh_url, return_url)
            .await
    }
    async fn create_transfer(
        &self,
        req: stripe_crate::TransferReq,
    ) -> Result<stripe_crate::TransferResult, stripe_crate::StripeError> {
        stripe_crate::MockStripe.create_transfer(req).await
    }
    fn verify_webhook(
        &self,
        _payload: &[u8],
        _signature_header: &str,
        _tolerance_secs: i64,
    ) -> Result<stripe_crate::WebhookEvent, stripe_crate::StripeError> {
        Err(stripe_crate::StripeError::SignatureInvalid(
            "rejecting signature for e2e test".into(),
        ))
    }
}

/// Transfer-вебхук без подписи → 400 `webhook_signature_invalid`,
/// выплата НЕ меняется. С валидной подписью transfer.paid → completed,
/// transfer.failed → failed. Дубль transfer.paid → no-op 200.
#[tokio::test]
async fn e2e_transfer_webhook_signature_security() {
    let repo = match test_repo().await {
        Some(r) => r,
        None => {
            eprintln!("DATABASE_URL недоступен — e2e transfer-webhook-sig пропущен");
            return;
        }
    };
    let sessions = SessionRepo::new(repo.pool());
    let requests = RequestRepo::new(repo.pool());
    let billing = BillingRepo::new(repo.pool());
    let payouts = PayoutRepo::new(repo.pool());
    let fake = FakeStripe::succeeding();
    let mut env = app_env(Some(repo.clone()), Some(sessions), Some(requests)).await;
    env.stripe = Some(fake.clone());
    let pool = repo.pool();

    let (club_id, dj_id) = fixture(&pool, "sig").await;
    captured_payment(&env, &billing, &pool, club_id, dj_id, 3000, "sig-p1").await;

    let admin_token = env
        .keys
        .issue_access(uuid::Uuid::now_v7(), Role::Admin, uuid::Uuid::now_v7())
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

    let mut in_flight = payouts.in_flight_payouts().await.unwrap();
    assert!(
        !in_flight.is_empty(),
        "expected in_flight payouts after run"
    );
    // Берём две выплаты: одну закроем transfer.paid, вторую — transfer.failed.
    let row_paid = in_flight.remove(0);
    let tid_paid = row_paid
        .stripe_transfer_id
        .clone()
        .expect("real provider transfer id attached");

    // --- БЕЗ подписи → 400, выплата не меняется --------------------
    let (s, body) = unsigned_transfer_webhook(&env, "transfer.paid", &tid_paid).await;
    assert_eq!(
        s,
        StatusCode::BAD_REQUEST,
        "unsigned transfer webhook must be 400: {body}"
    );
    assert_eq!(
        body["code"], "webhook_signature_invalid",
        "error code: {body}"
    );
    let unchanged = payouts.find_payout(row_paid.id).await.unwrap().unwrap();
    assert_eq!(
        unchanged.status, "in_flight",
        "unsigned webhook must NOT change payout status"
    );

    // --- С ВАЛИДНОЙ подписью: transfer.paid → completed ------------
    let (s, body) =
        signed_transfer_webhook(&env, "evt_paid_sig", "transfer.paid", &tid_paid, None).await;
    assert_eq!(s, StatusCode::OK, "valid transfer.paid: {body}");
    let closed = payouts.find_payout(row_paid.id).await.unwrap().unwrap();
    assert_eq!(closed.status, "paid", "transfer.paid closes payout");

    // --- ДУБЛЬ transfer.paid → no-op 200, второй проводки нет -------
    let (s, body) =
        signed_transfer_webhook(&env, "evt_paid_dup", "transfer.paid", &tid_paid, None).await;
    assert_eq!(s, StatusCode::OK, "duplicate transfer.paid: no-op {body}");
    let still_closed = payouts.find_payout(row_paid.id).await.unwrap().unwrap();
    assert_eq!(still_closed.status, "paid", "duplicate does not reopen");
    // attempts не вырос (нет «второй проводки»).
    assert_eq!(
        still_closed.attempts, closed.attempts,
        "duplicate must not increment attempts"
    );

    // --- С ВАЛИДНОЙ подписью: transfer.failed → failed -------------
    if !in_flight.is_empty() {
        let row_failed = in_flight.remove(0);
        let tid_failed = row_failed
            .stripe_transfer_id
            .clone()
            .expect("real provider transfer id attached");
        let (s, body) = signed_transfer_webhook(
            &env,
            "evt_failed_sig",
            "transfer.failed",
            &tid_failed,
            Some("insufficient platform balance"),
        )
        .await;
        assert_eq!(s, StatusCode::OK, "valid transfer.failed: {body}");
        let failed = payouts.find_payout(row_failed.id).await.unwrap().unwrap();
        assert_eq!(
            failed.status, "failed",
            "transfer.failed marks payout failed"
        );
    }
}

/// Поддельная подпись (провайдер отклоняет) → 400 `webhook_signature_invalid`,
/// выплата НЕ меняется. Использует `RejectingStripe` (имитирует LiveStripe
/// с неверным HMAC).
#[tokio::test]
async fn e2e_transfer_webhook_rejects_forged_signature() {
    let repo = match test_repo().await {
        Some(r) => r,
        None => {
            eprintln!("DATABASE_URL недоступен — e2e transfer-webhook-forged пропущен");
            return;
        }
    };
    let sessions = SessionRepo::new(repo.pool());
    let requests = RequestRepo::new(repo.pool());
    let billing = BillingRepo::new(repo.pool());
    let payouts = PayoutRepo::new(repo.pool());
    let pool = repo.pool();

    // Создаём выплату через FakeStripe (attach реального transfer id),
    // затем меняем провайдера на RejectingStripe для вебхук-вызова.
    let fake = FakeStripe::succeeding();
    let mut env = app_env(Some(repo.clone()), Some(sessions), Some(requests)).await;
    env.stripe = Some(fake.clone());

    let (club_id, dj_id) = fixture(&pool, "frg").await;
    captured_payment(&env, &billing, &pool, club_id, dj_id, 3000, "frg-p1").await;

    let admin_token = env
        .keys
        .issue_access(uuid::Uuid::now_v7(), Role::Admin, uuid::Uuid::now_v7())
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

    let mut in_flight = payouts.in_flight_payouts().await.unwrap();
    assert!(!in_flight.is_empty(), "expected in_flight payouts");
    let row = in_flight.remove(0);
    let tid = row
        .stripe_transfer_id
        .clone()
        .expect("real provider transfer id attached");

    // Меняем провайдера на отклоняющий — имитация поддельной подписи.
    env.stripe = Some(Arc::new(RejectingStripe));

    let (s, body) = signed_transfer_webhook(&env, "evt_forged", "transfer.paid", &tid, None).await;
    assert_eq!(
        s,
        StatusCode::BAD_REQUEST,
        "forged signature must be 400: {body}"
    );
    assert_eq!(
        body["code"], "webhook_signature_invalid",
        "error code: {body}"
    );
    // Выплата не изменилась — осталась in_flight.
    let untouched = payouts.find_payout(row.id).await.unwrap().unwrap();
    assert_eq!(
        untouched.status, "in_flight",
        "forged signature must NOT close the payout"
    );
}

// ============================================================
// ДЕФЕКТ 2: атомарность create_payout (триггер на UPDATE split).
// ============================================================

/// Атомарность: если UPDATE split → paid_out упадёт (тестовый триггер),
/// INSERT payout откатывается — payout не создан, сплиты остаются pending.
/// Триггер создаётся и удаляется в рамках теста (cleanup перед asserts).
#[tokio::test]
async fn e2e_payout_atomicity_rollback_on_split_update_failure() {
    let repo = match test_repo().await {
        Some(r) => r,
        None => {
            eprintln!("DATABASE_URL недоступен — e2e payout-atomicity пропущен");
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

    let (club_id, dj_id) = fixture(&pool, "atom").await;
    // Захват 30 € → сплиты 900/1050/1050 (club + dj выше порога).
    captured_payment(&env, &billing, &pool, club_id, dj_id, 3000, "atom-p1").await;

    // Триггер, который падает при UPDATE split SET status='paid_out'.
    // Имя уникально (префикс e2e_w5b_) — не конфликтует с другими тестами.
    // Fires ONLY when NEW.status = 'paid_out' — не затрагивает иные UPDATE
    // split (например, возврат в pending при on_hold).
    sqlx::query(
        "CREATE OR REPLACE FUNCTION e2e_w5b_fail_split_paidout() \
         RETURNS trigger LANGUAGE plpgsql AS $$ \
         BEGIN RAISE EXCEPTION 'e2e: forced split update failure'; END; $$",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "CREATE TRIGGER e2e_w5b_fail_split_paidout_trg \
         BEFORE UPDATE OF status ON split FOR EACH ROW \
         WHEN (NEW.status = 'paid_out') \
         EXECUTE FUNCTION e2e_w5b_fail_split_paidout()",
    )
    .execute(&pool)
    .await
    .unwrap();

    // Цикл выплат: create_payout попытается INSERT payout + UPDATE split
    // в одной транзакции. UPDATE упадёт → транзакция откатится → payout
    // не создан, сплиты остаются pending. Цикл логирует и продолжает.
    let result = payouts.run_payout_cycle(&[]).await;

    // Захватываем состояние ДО cleanup (чтобы триггер ещё был активен
    // на случай, если запросы выше ещё выполняются).
    let club_payouts: Result<(i64,), sqlx::Error> =
        sqlx::query_as("SELECT count(*) FROM payout WHERE payee_id = $1")
            .bind(club_id)
            .fetch_one(&pool)
            .await;
    let club_pending: Result<(i64,), sqlx::Error> =
        sqlx::query_as("SELECT count(*) FROM split WHERE payee_id = $1 AND status = 'pending'")
            .bind(club_id)
            .fetch_one(&pool)
            .await;
    let dj_payouts: Result<(i64,), sqlx::Error> =
        sqlx::query_as("SELECT count(*) FROM payout WHERE payee_id = $1")
            .bind(dj_id)
            .fetch_one(&pool)
            .await;

    // CLEANUP: удаляем триггер и функцию ВСЕГДА (до asserts — даже если
    // run_payout_cycle или запросы выше вернули ошибку).
    let _ = sqlx::query("DROP TRIGGER IF EXISTS e2e_w5b_fail_split_paidout_trg ON split")
        .execute(&pool)
        .await;
    let _ = sqlx::query("DROP FUNCTION IF EXISTS e2e_w5b_fail_split_paidout()")
        .execute(&pool)
        .await;

    // Теперь asserts (после cleanup — триггер уже удалён).
    // Цикл не падает (log-and-continue) — возвращает Ok с пустым списком.
    assert!(
        result.is_ok(),
        "run_payout_cycle should not hard-fail on per-payee error: {result:?}"
    );
    let created = result.unwrap();
    assert!(
        created.is_empty(),
        "no payouts should be created when split update fails: {created:?}"
    );

    let club_payouts = club_payouts.unwrap().0;
    let club_pending = club_pending.unwrap().0;
    let dj_payouts = dj_payouts.unwrap().0;
    assert_eq!(
        club_payouts, 0,
        "club payout NOT created (atomic rollback on split update failure)"
    );
    assert_eq!(
        dj_payouts, 0,
        "dj payout NOT created (atomic rollback on split update failure)"
    );
    assert!(
        club_pending > 0,
        "club splits remain pending (not locked, not lost): {club_pending}"
    );

    // Инвариант payee не нарушен (деньги не потеряны, не задвоены).
    let violations = payouts.payee_invariant_violations().await.unwrap();
    assert!(
        violations.is_empty(),
        "payee invariant holds after atomic rollback: {violations:?}"
    );
}

// ============================================================
// ДЕФЕКТ 3: двойная выплата при параллельных циклах (критично).
// ============================================================

/// Гонка: два параллельных запуска цикла выплат (`tokio::join!`) на одних
/// и тех же данных — имитация cron + ручной запуск из панели.
///
/// **До фикса** (`net_by_payee` БЕЗ `FOR UPDATE`, `create_payout` не
/// проверял `rows_affected`): оба цикла видели одни и те же pending-сплиты,
/// оба создавали payout и оба вызывали `stripe.create_transfer` → по два
/// payout на payee и по два реальных transfer (4 всего).
///
/// **После фикса** (`pg_advisory_xact_lock(payee)` + `SELECT … FOR UPDATE`
/// + проверка `rows_affected` в `create_payout`): ровно один payout на
/// payee, `create_transfer` вызван ровно один раз на payee (всего 2).
#[tokio::test]
async fn e2e_payout_concurrent_cycles_no_double_payout() {
    let repo = match test_repo().await {
        Some(r) => r,
        None => {
            eprintln!("DATABASE_URL недоступен — e2e payout-race пропущен");
            return;
        }
    };
    let sessions = SessionRepo::new(repo.pool());
    let requests = RequestRepo::new(repo.pool());
    let billing = BillingRepo::new(repo.pool());
    let payouts = PayoutRepo::new(repo.pool());
    let fake = FakeStripe::succeeding();
    let mut env = app_env(Some(repo.clone()), Some(sessions), Some(requests)).await;
    env.stripe = Some(fake.clone());
    let pool = repo.pool();

    let (club_id, dj_id) = fixture(&pool, "race").await;
    // 30 € → сплиты 900/1050/1050 (club + dj выше порога 10 €).
    captured_payment(&env, &billing, &pool, club_id, dj_id, 3000, "race-p1").await;

    let admin_token = env
        .keys
        .issue_access(uuid::Uuid::now_v7(), Role::Admin, uuid::Uuid::now_v7())
        .unwrap();

    // Два параллельных запуска (tokio::join!) — cron + ручной триггер.
    let env_a = env.clone();
    let env_b = env.clone();
    let tok_a = admin_token.clone();
    let tok_b = admin_token.clone();
    let ((s1, b1), (s2, b2)) = tokio::join!(
        call(&env_a, "POST", "/v1/admin/payouts/run", None, Some(&tok_a)),
        call(&env_b, "POST", "/v1/admin/payouts/run", None, Some(&tok_b)),
    );
    assert_eq!(s1, StatusCode::OK, "race call A: {b1}");
    assert_eq!(s2, StatusCode::OK, "race call B: {b2}");

    // Сумма созданных за оба вызова == 2 (один payout на payee), не 4.
    let created_total =
        b1["payouts_created"].as_i64().unwrap_or(0) + b2["payouts_created"].as_i64().unwrap_or(0);
    assert_eq!(
        created_total, 2,
        "exactly one payout per payee across both parallel cycles: A={b1} B={b2}"
    );

    // В БД ровно по одному payout на payee (никаких дублей).
    let (club_cnt,): (i64,) = sqlx::query_as("SELECT count(*) FROM payout WHERE payee_id = $1")
        .bind(club_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let (dj_cnt,): (i64,) = sqlx::query_as("SELECT count(*) FROM payout WHERE payee_id = $1")
        .bind(dj_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(club_cnt, 1, "club: exactly one payout (no double payout)");
    assert_eq!(dj_cnt, 1, "dj: exactly one payout (no double payout)");

    // Фейковый провайдер вызван ровно один раз на payee (всего 2), не 4 —
    // значит реальный перевод Stripe сработал бы один раз за период.
    assert_eq!(
        fake.transfer_calls(),
        2,
        "create_transfer called once per payee (no double transfer)"
    );

    // Все сплиты club/dj закреплены (paid_out), pending не осталось.
    let (pending,): (i64,) = sqlx::query_as(
        "SELECT count(*) FROM split WHERE payee_id IN ($1, $2) AND status = 'pending'",
    )
    .bind(club_id)
    .bind(dj_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(pending, 0, "no pending splits left after race");

    // Инвариант payee держится — деньги не задвоены, не потеряны.
    let violations = payouts.payee_invariant_violations().await.unwrap();
    assert!(
        violations.is_empty(),
        "payee invariant holds after concurrent cycles: {violations:?}"
    );
}

/// Последовательный повтор цикла после успешной выплаты: сплиты уже
/// `paid_out`, нетто пусто — второй payout не создаётся (двойной выплаты
/// нет даже без гонки). Два `run_payout_cycle` подряд на одних данных.
#[tokio::test]
async fn e2e_payout_sequential_rerun_no_duplicate() {
    let repo = match test_repo().await {
        Some(r) => r,
        None => {
            eprintln!("DATABASE_URL недоступен — e2e payout-rerun пропущен");
            return;
        }
    };
    let sessions = SessionRepo::new(repo.pool());
    let requests = RequestRepo::new(repo.pool());
    let billing = BillingRepo::new(repo.pool());
    let payouts = PayoutRepo::new(repo.pool());
    let env = app_env(Some(repo.clone()), Some(sessions), Some(requests)).await;
    let pool = repo.pool();

    let (club_id, dj_id) = fixture(&pool, "rerun").await;
    // 30 € → сплиты 900/1050/1050 (club + dj выше порога).
    captured_payment(&env, &billing, &pool, club_id, dj_id, 3000, "rerun-p1").await;

    // Первый цикл: 2 payout (club + dj).
    let rows1 = payouts.run_payout_cycle(&[]).await.unwrap();
    assert_eq!(rows1.len(), 2, "first cycle creates 2 payouts: {rows1:?}");
    // Закрываем выплаты (имитация успешного transfer.paid): сплиты
    // остаются paid_out, как после реальной выплаты.
    for r in &rows1 {
        let tid = format!("tr_rerun_{}", r.id);
        payouts
            .attach_transfer(r.id, &tid, PayoutStatus::InFlight)
            .await
            .unwrap();
        payouts.handle_transfer_paid(&tid).await.unwrap();
    }

    // Второй цикл: нетто пусто (сплиты paid_out) — новых payout нет.
    let rows2 = payouts.run_payout_cycle(&[]).await.unwrap();
    assert!(
        rows2.is_empty(),
        "second cycle must NOT create payouts (splits already paid_out): {rows2:?}"
    );

    // По-прежнему ровно 2 payout (по одному на payee).
    let (total,): (i64,) = sqlx::query_as("SELECT count(*) FROM payout WHERE payee_id IN ($1, $2)")
        .bind(club_id)
        .bind(dj_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        total, 2,
        "still exactly 2 payouts after rerun (no duplicate)"
    );

    // Инвариант payee держится.
    let violations = payouts.payee_invariant_violations().await.unwrap();
    assert!(
        violations.is_empty(),
        "payee invariant holds after rerun: {violations:?}"
    );
}
