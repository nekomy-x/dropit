//! E2e-тест почтовой инфраструктуры (ADR-008).
//!
//! Проверяет HTTP-границу, а не внутренности: подписанный/неподписанный/
//! повторный вебхук доставки, журнал и повторную отправку в админке, RBAC
//! (клуб в админ-роуты не проходит), автопостановку письма при инвайте DJ и
//! полный флоу подтверждения e-mail гостя.
//!
//! Требует живой Postgres (`DATABASE_URL`): при недоступной БД тест
//! печатает причину и выходит, как остальные e2e (`test_repo`).
//!
//! # Подпись вебхука
//!
//! Тест собирает настоящий Svix-заголовок: `HMAC-SHA256(secret,
//! "{svix_id}.{timestamp}.{body}")`. Это проверяет реальную совместимость
//! алгоритма, а не «подпись, которую мы же и придумали»: секрет задаётся
//! через `whsec_`-строку, как у провайдера.
//!
//! # Изоляция
//!
//! `test_repo()` не знает про почтовые таблицы (они появились миграцией
//! 0013), поэтому тест чистит их сам — как `e2e_guest_upgrade` поступает с
//! `guest_account`. Переменные окружения, которые читает HTTP-слой
//! (`RESEND_WEBHOOK_SECRET`, `MAIL_FROM`), выставляются один раз на тест;
//! файл держит общий mutex, чтобы параллельные `#[tokio::test]` не
//! перетирали их друг другу.

#![cfg(test)]

use axum::http::StatusCode;
use base64::Engine as _;
use hmac::{Hmac, Mac};
use serde_json::{json, Value};
use sha2::Sha256;
use uuid::Uuid;

use crate::e2e::{app_env, call, call_with_headers, test_repo};
use dropit_infra_db::mail::NewMail;
use dropit_infra_db::SessionRepo;

/// Секрет подписи в формате провайдера (`whsec_` + base64).
const TEST_WEBHOOK_SECRET: &str = "whsec_MfKQ9r8GKYqrTwjUPD8ILPZIo2LaLaSw";

/// Мьютекс на тест: env-переменные — глобальное состояние процесса, а
/// cargo запускает тесты одного бинаря параллельно.
///
/// Асинхронный, а не `std::sync::Mutex`: guard живёт через `.await` внутри
/// теста (там HTTP-вызовы), и синхронный мьютекс держал бы поток-лок на
/// время ожидания — clippy справедливо это запрещает.
static ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Выставить переменные почтового окружения и вернуть guard.
async fn with_mail_env() -> tokio::sync::MutexGuard<'static, ()> {
    let guard = ENV_LOCK.lock().await;
    std::env::set_var("RESEND_WEBHOOK_SECRET", TEST_WEBHOOK_SECRET);
    std::env::set_var("MAIL_FROM", "DROP IT <no-reply@dropit.test>");
    std::env::set_var("MAIL_APP_BASE_URL", "https://app.dropit.test");
    guard
}

/// Подписать тело как Svix: `v1,<base64(HMAC-SHA256)>`.
fn svix_signature(secret: &str, svix_id: &str, timestamp: i64, body: &str) -> String {
    let raw = secret
        .strip_prefix("whsec_")
        .expect("секрет в тесте — whsec_");
    let key = base64::engine::general_purpose::STANDARD
        .decode(raw)
        .expect("ключ декодируется из base64");
    let mut mac = Hmac::<Sha256>::new_from_slice(&key).expect("hmac key");
    mac.update(format!("{svix_id}.{timestamp}.{body}").as_bytes());
    let sig = mac.finalize().into_bytes();
    format!(
        "v1,{}",
        base64::engine::general_purpose::STANDARD.encode(sig)
    )
}

/// Тело события доставки Resend.
fn event_body(event_type: &str, provider_id: &str, to: &str) -> String {
    json!({
        "type": event_type,
        "created_at": "2026-10-27T18:00:00Z",
        "data": {
            "email_id": provider_id,
            "to": [to],
            "subject": "Тема",
            "from": "DROP IT <no-reply@dropit.test>",
        }
    })
    .to_string()
}

/// Собрать заголовки подписанного вебхука.
fn signed_headers(
    secret: &str,
    svix_id: &str,
    body: &str,
    drift_secs: i64,
) -> Vec<(String, String)> {
    let ts = chrono::Utc::now().timestamp() + drift_secs;
    vec![
        ("svix-id".to_string(), svix_id.to_string()),
        ("svix-timestamp".to_string(), ts.to_string()),
        (
            "svix-signature".to_string(),
            svix_signature(secret, svix_id, ts, body),
        ),
        ("content-type".to_string(), "application/json".to_string()),
    ]
}

/// Поставить письмо в очередь напрямую через репозиторий.
async fn seed_mail(repo: &dropit_infra_db::mail::MailRepo, to: &str, key: &str) -> Uuid {
    let (id, _) = repo
        .enqueue(NewMail {
            idempotency_key: key.to_string(),
            template: "mail_test".to_string(),
            to_email: to.to_string(),
            to_name: None,
            from_email: "DROP IT <no-reply@dropit.test>".to_string(),
            reply_to: None,
            subject: "Тестовое письмо DROP IT".to_string(),
            html: "<p>hi</p>".to_string(),
            text: "hi".to_string(),
            vars: json!({}),
            aggregate_type: None,
            aggregate_id: None,
        })
        .await
        .expect("enqueue");
    id
}

/// Чистит почтовые таблицы, которых не знает `test_repo()`.
async fn clean_mail(pool: &sqlx::PgPool) {
    let _ = sqlx::query("TRUNCATE TABLE mail_message, mail_event, mail_suppression")
        .execute(pool)
        .await;
    let _ = sqlx::query("TRUNCATE TABLE guest_email_verification")
        .execute(pool)
        .await;
}

// ============================================================
// Вебхук доставки
// ============================================================

/// Подписанный вебхук двигает состояние письма; повтор идемпотентен;
/// подделка и просрочка отвергаются; без секрета — 503.
#[tokio::test]
async fn e2e_mail_webhook_delivery() {
    let _env = with_mail_env().await;
    let repo = match test_repo().await {
        Some(r) => r,
        None => {
            eprintln!("DATABASE_URL недоступен — e2e почты пропущен");
            return;
        }
    };
    let pool = repo.pool();
    clean_mail(&pool).await;
    let env = app_env(
        Some(repo.clone()),
        Some(SessionRepo::new(pool.clone())),
        None,
    )
    .await;
    let mail = env
        .mail
        .as_ref()
        .expect("mail repo в окружении теста")
        .clone();

    let id = seed_mail(&mail, "guest@example.test", "webhook-1").await;
    // Провайдер принял письмо: без provider_id вебхук не с чем сшить.
    mail.mark_sent(id, "prov-mail-1").await.expect("mark_sent");

    // --- 1. Подписанный email.delivered → delivered --------------
    let body = event_body("email.delivered", "prov-mail-1", "guest@example.test");
    let (status, resp) = call_with_raw(
        &env,
        "/webhooks/resend",
        &body,
        &signed_headers(TEST_WEBHOOK_SECRET, "msg_1", &body, 0),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "signed webhook: {resp}");
    assert_eq!(resp["received"], true);
    assert_eq!(resp["duplicate"], false);

    let detail = mail.get(id).await.unwrap().expect("письмо есть");
    assert_eq!(detail.mail.status, "delivered", "статус доставлен");
    assert!(detail.mail.delivered_at.is_some());
    assert_eq!(detail.events.len(), 1, "событие записано");
    assert_eq!(detail.events[0].event_type, "email.delivered");

    // --- 2. Повтор того же svix-id → duplicate, без изменений ----
    let (status, resp) = call_with_raw(
        &env,
        "/webhooks/resend",
        &body,
        &signed_headers(TEST_WEBHOOK_SECRET, "msg_1", &body, 0),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "duplicate: {resp}");
    assert_eq!(resp["duplicate"], true);
    let detail = mail.get(id).await.unwrap().expect("письмо есть");
    assert_eq!(
        detail.events.len(),
        1,
        "дубликат не записан вторым событием"
    );

    // --- 3. Поздний email.sent не откатывает delivered -----------
    let late = event_body("email.sent", "prov-mail-1", "guest@example.test");
    let (status, _) = call_with_raw(
        &env,
        "/webhooks/resend",
        &late,
        &signed_headers(TEST_WEBHOOK_SECRET, "msg_2", &late, 0),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let detail = mail.get(id).await.unwrap().expect("письмо есть");
    assert_eq!(
        detail.mail.status, "delivered",
        "монотонность: поздний sent не откатывает delivered"
    );

    // --- 4. Подделанная подпись → 400 ---------------------------
    let forged = signed_headers(TEST_WEBHOOK_SECRET, "msg_3", &body, 0);
    let (status, resp) = call_with_raw(
        &env,
        "/webhooks/resend",
        // тело изменено, подпись — от другого тела
        &event_body("email.bounced", "prov-mail-1", "other@example.test"),
        &forged,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "tampered: {resp}");
    assert_eq!(resp["code"], "mail_webhook_invalid");

    // --- 5. Просроченный timestamp → 400 ------------------------
    let stale = signed_headers(TEST_WEBHOOK_SECRET, "msg_4", &body, -3600);
    let (status, resp) = call_with_raw(&env, "/webhooks/resend", &body, &stale).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "expired: {resp}");

    // --- 6. Нет заголовков подписи → 400 ------------------------
    let (status, resp) = call_with_raw(
        &env,
        "/webhooks/resend",
        &body,
        &[("content-type".to_string(), "application/json".to_string())],
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "unsigned: {resp}");

    // --- 7. Жалоба → complained + подавление адреса --------------
    let complaint = event_body("email.complained", "prov-mail-1", "guest@example.test");
    let (status, _) = call_with_raw(
        &env,
        "/webhooks/resend",
        &complaint,
        &signed_headers(TEST_WEBHOOK_SECRET, "msg_5", &complaint, 0),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let detail = mail.get(id).await.unwrap().expect("письмо есть");
    assert_eq!(detail.mail.status, "complained");
    assert!(
        mail.is_suppressed("guest@example.test").await.unwrap(),
        "жалоба добавляет адрес в подавления"
    );

    // --- 8. Без секрета → 503 -----------------------------------
    std::env::remove_var("RESEND_WEBHOOK_SECRET");
    let (status, resp) = call_with_raw(
        &env,
        "/webhooks/resend",
        &body,
        &signed_headers(TEST_WEBHOOK_SECRET, "msg_6", &body, 0),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::SERVICE_UNAVAILABLE,
        "без секрета вебхук выключен: {resp}"
    );
    assert_eq!(resp["code"], "mail_webhook_disabled");
    // Возвращаем секрет: соседние тесты в этом файле на него рассчитывают.
    std::env::set_var("RESEND_WEBHOOK_SECRET", TEST_WEBHOOK_SECRET);
}

// ============================================================
// Админ-журнал
// ============================================================

/// Журнал, статистика, повторная отправка и снятие подавления.
#[tokio::test]
async fn e2e_admin_mail_panel() {
    let _env = with_mail_env().await;
    let repo = match test_repo().await {
        Some(r) => r,
        None => {
            eprintln!("DATABASE_URL недоступен — e2e почты пропущен");
            return;
        }
    };
    let pool = repo.pool();
    clean_mail(&pool).await;
    let env = app_env(
        Some(repo.clone()),
        Some(SessionRepo::new(pool.clone())),
        None,
    )
    .await;
    let mail = env.mail.as_ref().expect("mail repo").clone();

    let admin = env
        .keys
        .issue_access(Uuid::now_v7(), dropit_common::Role::Admin, Uuid::now_v7())
        .unwrap();
    let club = env
        .keys
        .issue_access(Uuid::now_v7(), dropit_common::Role::Club, Uuid::now_v7())
        .unwrap();

    let first = seed_mail(&mail, "one@example.test", "panel-1").await;
    let second = seed_mail(&mail, "two@example.test", "panel-2").await;

    // --- 1. Список: две строки, курсора дальше нет --------------
    let (status, body) = call(&env, "GET", "/v1/admin/mail", None, Some(&admin)).await;
    assert_eq!(status, StatusCode::OK, "list: {body}");
    assert_eq!(body["items"].as_array().map(Vec::len), Some(2));
    assert!(body["next_cursor"].is_null(), "вторая страница не нужна");

    // Порядок — от новых к старым.
    assert_eq!(body["items"][0]["id"], json!(second));
    assert_eq!(body["items"][0]["to"], "two@example.test");
    assert_eq!(body["items"][0]["status"], "queued");
    // В списке нет тела письма: это отдельный запрос.
    assert!(
        body["items"][0].get("html").is_none(),
        "html не тянется в список"
    );

    // --- 2. Фильтр по статусу -----------------------------------
    let (status, body) = call(
        &env,
        "GET",
        "/v1/admin/mail?status=failed",
        None,
        Some(&admin),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body["items"].as_array().map(Vec::len),
        Some(0),
        "failed пусто"
    );

    // Мусорный статус — 400, а не «просто пустой список».
    let (status, body) = call(
        &env,
        "GET",
        "/v1/admin/mail?status=garbage",
        None,
        Some(&admin),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["code"], "invalid_status");

    // --- 3. Поиск ------------------------------------------------
    let (status, body) = call(
        &env,
        "GET",
        "/v1/admin/mail?q=one@example",
        None,
        Some(&admin),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["items"].as_array().map(Vec::len), Some(1));
    assert_eq!(body["items"][0]["to"], "one@example.test");

    // --- 4. Пагинация: limit=1 и курсор -------------------------
    let (status, page1) = call(&env, "GET", "/v1/admin/mail?limit=1", None, Some(&admin)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(page1["items"].as_array().map(Vec::len), Some(1));
    let cursor = page1["next_cursor"]
        .as_str()
        .expect("курсор есть")
        .to_string();
    let uri = format!("/v1/admin/mail?limit=1&cursor={}", urlenc(&cursor));
    let (status, page2) = call(&env, "GET", &uri, None, Some(&admin)).await;
    assert_eq!(status, StatusCode::OK, "page2: {page2}");
    assert_eq!(page2["items"].as_array().map(Vec::len), Some(1));
    assert_ne!(
        page1["items"][0]["id"], page2["items"][0]["id"],
        "вторая страница не повторяет первую"
    );

    // --- 5. Деталь: тело и события -------------------------------
    let uri = format!("/v1/admin/mail/{first}");
    let (status, body) = call(&env, "GET", &uri, None, Some(&admin)).await;
    assert_eq!(status, StatusCode::OK, "detail: {body}");
    assert_eq!(body["html"], "<p>hi</p>");
    assert_eq!(body["text"], "hi");
    assert!(body["events"].is_array());

    let missing = Uuid::now_v7();
    let (status, body) = call(
        &env,
        "GET",
        &format!("/v1/admin/mail/{missing}"),
        None,
        Some(&admin),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["code"], "mail_not_found");

    // Не-UUID в пути — 404, а не 500.
    let (status, _) = call(&env, "GET", "/v1/admin/mail/not-a-uuid", None, Some(&admin)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // --- 6. Статистика -------------------------------------------
    let (status, body) = call(&env, "GET", "/v1/admin/mail/stats", None, Some(&admin)).await;
    assert_eq!(status, StatusCode::OK, "stats: {body}");
    assert_eq!(body["window_hours"], 24);
    assert_eq!(body["queued"], 2, "оба письма в очереди");
    assert_eq!(body["total"], 2);

    // --- 7. Повторная отправка -----------------------------------
    let uri = format!("/v1/admin/mail/{first}/resend");
    let (status, body) = call(&env, "POST", &uri, None, Some(&admin)).await;
    assert_eq!(status, StatusCode::OK, "resend: {body}");
    assert_eq!(body["status"], "queued");
    let requeued = Uuid::parse_str(body["id"].as_str().expect("id")).expect("uuid");
    assert_ne!(requeued, first, "переотправка создаёт новое письмо");

    let detail = mail
        .get(requeued)
        .await
        .unwrap()
        .expect("новое письмо есть");
    assert!(
        detail.mail.idempotency_key.starts_with("panel-1:r"),
        "ключ переотправки: {}",
        detail.mail.idempotency_key
    );
    assert_eq!(detail.mail.to_email, "one@example.test");
    // Исходное письмо не переписано.
    let original = mail.get(first).await.unwrap().expect("исходное есть");
    assert_eq!(original.mail.status, "queued");

    // --- 8. Тестовое письмо --------------------------------------
    let (status, body) = call(
        &env,
        "POST",
        "/v1/admin/mail/test",
        Some(json!({ "to": "ops@dropit.test" })),
        Some(&admin),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "test mail: {body}");
    assert_eq!(body["status"], "queued");

    let (status, body) = call(
        &env,
        "POST",
        "/v1/admin/mail/test",
        Some(json!({ "to": "not-an-email" })),
        Some(&admin),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["code"], "mail_validation_failed");

    // --- 9. Подавления -------------------------------------------
    mail.add_suppression("bounce@example.test", "bounce", Some("hard bounce"))
        .await
        .expect("suppression");
    let (status, body) = call(
        &env,
        "GET",
        "/v1/admin/mail/suppressions",
        None,
        Some(&admin),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "suppressions: {body}");
    assert_eq!(body["items"].as_array().map(Vec::len), Some(1));
    assert_eq!(body["items"][0]["email"], "bounce@example.test");
    assert_eq!(body["items"][0]["reason"], "bounce");

    let (status, body) = call(
        &env,
        "DELETE",
        "/v1/admin/mail/suppressions/bounce@example.test",
        None,
        Some(&admin),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["removed"], true);
    assert!(!mail.is_suppressed("bounce@example.test").await.unwrap());

    // Повторное удаление идемпотентно.
    let (status, body) = call(
        &env,
        "DELETE",
        "/v1/admin/mail/suppressions/bounce@example.test",
        None,
        Some(&admin),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["removed"], false);

    // --- 10. RBAC: клуб в админ-роуты не проходит ----------------
    for (method, uri) in [
        ("GET", "/v1/admin/mail"),
        ("GET", "/v1/admin/mail/stats"),
        ("GET", &format!("/v1/admin/mail/{first}") as &str),
        ("POST", &format!("/v1/admin/mail/{first}/resend") as &str),
        ("GET", "/v1/admin/mail/suppressions"),
    ] {
        let (status, body) = call(&env, method, uri, None, Some(&club)).await;
        assert_eq!(
            status,
            StatusCode::FORBIDDEN,
            "клуб не должен иметь доступ к {method} {uri}: {body}"
        );
    }
    // Без токена — 401.
    let (status, _) = call(&env, "GET", "/v1/admin/mail", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

// ============================================================
// Триггеры бизнес-событий
// ============================================================

/// Инвайт DJ ставит в очередь письмо с приглашением.
#[tokio::test]
async fn e2e_dj_invite_queues_mail() {
    let _env = with_mail_env().await;
    let repo = match test_repo().await {
        Some(r) => r,
        None => {
            eprintln!("DATABASE_URL недоступен — e2e почты пропущен");
            return;
        }
    };
    let pool = repo.pool();
    clean_mail(&pool).await;
    let env = app_env(
        Some(repo.clone()),
        Some(SessionRepo::new(pool.clone())),
        None,
    )
    .await;

    let club_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO club (id, name, slug, email, password_hash) \
         VALUES ($1, 'Mail Club', 'mail-club', 'mail@club.test', 'x')",
    )
    .bind(club_id)
    .execute(&pool)
    .await
    .unwrap();
    let club = env
        .keys
        .issue_access(club_id, dropit_common::Role::Club, Uuid::now_v7())
        .unwrap();

    let (status, body) = call(
        &env,
        "POST",
        "/v1/club/djs/invite",
        Some(json!({ "email": "invited@dj.test", "display_name": "Invited DJ" })),
        Some(&club),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "invite: {body}");
    let invite_token = body["invite_token"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    assert!(!invite_token.is_empty());

    // Письмо в очереди: ровно одно, с шаблоном приглашения.
    let mail = env.mail.as_ref().expect("mail repo").clone();
    let rows = mail
        .list(dropit_infra_db::mail::MailFilter {
            limit: 10,
            ..Default::default()
        })
        .await
        .expect("list")
        .0;
    assert_eq!(rows.len(), 1, "одно письмо на инвайт");
    assert_eq!(rows[0].template, "dj_invite");
    assert_eq!(rows[0].to_email, "invited@dj.test");
    assert_eq!(rows[0].status, "queued");
    assert_eq!(rows[0].aggregate_type.as_deref(), Some("dj"));

    // В теле письма — ссылка с токеном и название клуба.
    let detail = mail.get(rows[0].id).await.unwrap().expect("письмо есть");
    assert!(
        detail.mail.html.contains(&invite_token),
        "ссылка приглашения содержит токен"
    );
    assert!(
        detail.mail.html.contains("Mail Club"),
        "в письме название клуба"
    );
    assert!(
        detail.mail.idempotency_key.starts_with("dj_invite:"),
        "ключ идемпотентности: {}",
        detail.mail.idempotency_key
    );

    // Повторный инвайт того же адреса → 409, второго письма нет.
    let (status, _) = call(
        &env,
        "POST",
        "/v1/club/djs/invite",
        Some(json!({ "email": "invited@dj.test", "display_name": "Invited DJ" })),
        Some(&club),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let rows = mail
        .list(dropit_infra_db::mail::MailFilter {
            limit: 10,
            ..Default::default()
        })
        .await
        .expect("list")
        .0;
    assert_eq!(rows.len(), 1, "отказ не порождает письмо");
}

/// Апгрейд гостя ставит письмо с кодом, а `/v1/guest/verify` подтверждает адрес.
#[tokio::test]
async fn e2e_guest_email_verification() {
    let _env = with_mail_env().await;
    let repo = match test_repo().await {
        Some(r) => r,
        None => {
            eprintln!("DATABASE_URL недоступен — e2e почты пропущен");
            return;
        }
    };
    let pool = repo.pool();
    clean_mail(&pool).await;
    let _ = sqlx::query("TRUNCATE TABLE guest_account")
        .execute(&pool)
        .await;
    let env = app_env(
        Some(repo.clone()),
        Some(SessionRepo::new(pool.clone())),
        None,
    )
    .await;

    let guest = Uuid::now_v7();
    repo.create_guest_device(guest, "ios", None).await.unwrap();

    // --- 1. Апгрейд ставит письмо с кодом -----------------------
    let (status, body) = call(
        &env,
        "POST",
        "/v1/guest/upgrade",
        Some(json!({ "email": "guest@example.test" })),
        Some(&guest.to_string()),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "upgrade: {body}");
    assert_eq!(body["verified"], false);

    let mail = env.mail.as_ref().expect("mail repo").clone();
    let rows = mail
        .list(dropit_infra_db::mail::MailFilter {
            limit: 10,
            ..Default::default()
        })
        .await
        .expect("list")
        .0;
    assert_eq!(rows.len(), 1, "одно письмо на апгрейд");
    assert_eq!(rows[0].template, "guest_verify");
    assert_eq!(rows[0].aggregate_type.as_deref(), Some("guest_account"));

    // Код лежит в переменных письма (в БД он есть только как хеш).
    let detail = mail.get(rows[0].id).await.unwrap().expect("письмо есть");
    let code = detail.mail.vars["code"]
        .as_str()
        .expect("код в vars")
        .to_string();
    assert_eq!(code.len(), 6);

    // В БД — хеш, не сам код.
    let stored: Option<(String,)> =
        sqlx::query_as("SELECT code_hash FROM guest_email_verification LIMIT 1")
            .fetch_optional(&pool)
            .await
            .unwrap();
    let (hash,) = stored.expect("код сохранён");
    assert_ne!(hash, code, "код не хранится открытым текстом");
    assert_eq!(hash.len(), 64, "sha256 в hex");

    // --- 2. Неверный код → 422 с остатком попыток ---------------
    let (status, body) = call(
        &env,
        "POST",
        "/v1/guest/verify",
        Some(json!({ "code": "000000" })),
        Some(&guest.to_string()),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "wrong: {body}");
    assert_eq!(body["code"], "invalid_code");
    assert_eq!(body["details"]["attempts_left"], 4);

    // --- 3. Верный код → verified ------------------------------
    let (status, body) = call(
        &env,
        "POST",
        "/v1/guest/verify",
        Some(json!({ "code": code })),
        Some(&guest.to_string()),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "verify: {body}");
    assert_eq!(body["verified"], true);

    let (verified,): (bool,) = sqlx::query_as("SELECT verified FROM guest_account LIMIT 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(verified, "адрес подтверждён в БД");

    // Код одноразовый: повторный ввод → 410.
    let (status, body) = call(
        &env,
        "POST",
        "/v1/guest/verify",
        Some(json!({ "code": code })),
        Some(&guest.to_string()),
    )
    .await;
    assert_eq!(status, StatusCode::GONE, "replay: {body}");
    assert_eq!(body["code"], "code_expired");
}

/// Пять неверных попыток гасят код: дальше нужен новый.
#[tokio::test]
async fn e2e_guest_verify_attempt_limit() {
    let _env = with_mail_env().await;
    let repo = match test_repo().await {
        Some(r) => r,
        None => {
            eprintln!("DATABASE_URL недоступен — e2e почты пропущен");
            return;
        }
    };
    let pool = repo.pool();
    clean_mail(&pool).await;
    let _ = sqlx::query("TRUNCATE TABLE guest_account")
        .execute(&pool)
        .await;
    let env = app_env(Some(repo.clone()), None, None).await;

    let guest = Uuid::now_v7();
    repo.create_guest_device(guest, "android", None)
        .await
        .unwrap();
    call(
        &env,
        "POST",
        "/v1/guest/upgrade",
        Some(json!({ "email": "limits@example.test" })),
        Some(&guest.to_string()),
    )
    .await;

    for left in (0..5).rev() {
        let (status, body) = call(
            &env,
            "POST",
            "/v1/guest/verify",
            Some(json!({ "code": "111111" })),
            Some(&guest.to_string()),
        )
        .await;
        if left > 0 {
            assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
            assert_eq!(body["details"]["attempts_left"], left);
        } else {
            // Пятая попытка гасит код целиком.
            assert_eq!(status, StatusCode::GONE, "{body}");
        }
    }

    let (count,): (i64,) = sqlx::query_as("SELECT count(*) FROM guest_email_verification")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0, "код погашен после исчерпания попыток");
}

// ============================================================
// Хелперы
// ============================================================

/// Процент-кодирование значения для query-string (курсор содержит `:`).
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

/// `oneshot`-запрос с произвольными заголовками и сырым телом.
///
/// Обычный [`call`] умеет только JSON-тело и Bearer-токен; вебхуку нужны
/// `svix-*` и ровно те байты, которые подписаны.
async fn call_with_raw(
    env: &crate::routes::ApiEnv,
    uri: &str,
    body: &str,
    headers: &[(String, String)],
) -> (StatusCode, Value) {
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    let router = crate::routes::router(env.clone());
    let mut builder = Request::builder().method("POST").uri(uri);
    for (name, value) in headers {
        builder = builder.header(name.as_str(), value.as_str());
    }
    let request = builder
        .body(Body::from(body.to_string()))
        .expect("request builds");
    let response = router.oneshot(request).await.expect("router responds");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 2 * 1024 * 1024)
        .await
        .expect("body readable");
    let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, value)
}

// `call_with_headers` остаётся в crate::e2e и используется другими тестами;
// здесь он импортирован для единообразия импортов и не должен «повиснуть»
// неиспользованным предупреждением.
#[allow(dead_code)]
fn _keep_imports() {
    let _ = call_with_headers;
}
