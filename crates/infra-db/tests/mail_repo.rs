//! Интеграционные тесты `MailRepo` на живой Postgres (миграция `0013`).
//!
//! Запуск (локальная БД, сериализованно с другими агентами):
//!
//! ```text
//! scripts/db-test.sh cargo test -p dropit-infra-db mail -- --test-threads=1
//! ```
//!
//! Без `DATABASE_URL` (или при недоступной БД) тест печатает строку и
//! возвращается — тот же приём, что в `gateway-api/src/e2e.rs::test_repo`:
//! CI без сервиса БД не падает, а локально тесты реально проверяют SQL.
//!
//! Каждый тест чистит `mail_message, mail_event, mail_suppression` в начале:
//! `test_repo()` из gateway-api про эти таблицы не знает и порядок его
//! TRUNCATE не трогает (контракт §2).
//!
//! Тесты лежат в модуле `mail`, чтобы фильтр `cargo test … mail` их
//! действительно находил: libtest матчит подстроку полного имени теста.

mod mail {
    use std::time::Duration;

    use chrono::{DateTime, Utc};
    use dropit_infra_db::mail::{MailFilter, MailRepo, NewMail};
    use dropit_infra_db::MailDbError;
    use serde_json::json;
    use sqlx::PgPool;
    use uuid::Uuid;

    /// Подключение к тестовой БД, миграции и очистка таблиц почты.
    ///
    /// `None` — БД недоступна: тест печатает причину и выходит, не падая.
    async fn repo() -> Option<MailRepo> {
        let url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://dropit:dropit_local@localhost:5432/dropit".into());
        let pool = dropit_infra_db::connect(&url).await.ok()?;
        // Миграции идемпотентны (sqlx ведёт _sqlx_migrations). Ошибку не
        // считаем фатальной: схема могла быть применена прошлым прогоном, а
        // сломанной чужой миграцией чужие тесты пусть занимаются.
        if let Err(e) = dropit_infra_db::migrations::run(&pool).await {
            eprintln!("mail repo test: миграции не применились: {e}");
        }
        truncate(&pool).await;
        Some(MailRepo::new(pool))
    }

    /// Очистить таблицы почты (без CASCADE: внешних ключей нет).
    async fn truncate(pool: &PgPool) {
        sqlx::query("TRUNCATE TABLE mail_message, mail_event, mail_suppression")
            .execute(pool)
            .await
            .expect("truncate mail tables");
    }

    /// Шаблон нового письма с заданным ключом идемпотентности.
    fn new_mail(key: &str) -> NewMail {
        NewMail {
            idempotency_key: key.to_string(),
            template: "mail_test".to_string(),
            to_email: "guest@example.com".to_string(),
            to_name: Some("Гость".to_string()),
            from_email: "DROP IT <no-reply@dropit.club>".to_string(),
            reply_to: Some("support@dropit.club".to_string()),
            subject: "Тестовое письмо DROP IT".to_string(),
            html: "<p>Привет</p>".to_string(),
            text: "Привет".to_string(),
            vars: json!({ "sent_by": "test" }),
            aggregate_type: Some("guest_account".to_string()),
            aggregate_id: Some(Uuid::now_v7()),
        }
    }

    /// Поставить письмо в очередь и вернуть его id.
    async fn enqueue(repo: &MailRepo, key: &str) -> Uuid {
        repo.enqueue(new_mail(key)).await.expect("enqueue").0
    }

    /// Строка `(status, attempts, next_attempt_at)` из БД.
    async fn state(pool: &PgPool, id: Uuid) -> (String, i32, DateTime<Utc>) {
        sqlx::query_as("SELECT status, attempts, next_attempt_at FROM mail_message WHERE id = $1")
            .bind(id)
            .fetch_one(pool)
            .await
            .expect("state")
    }

    /// Установить `max_attempts` вручную (моделируем «исчерпал попытки»).
    async fn set_max_attempts(pool: &PgPool, id: Uuid, max: i32) {
        sqlx::query("UPDATE mail_message SET max_attempts = $2 WHERE id = $1")
            .bind(id)
            .bind(max)
            .execute(pool)
            .await
            .expect("set max_attempts");
    }

    // ---------------------------------------------------------
    // Очередь
    // ---------------------------------------------------------

    #[tokio::test]
    async fn enqueue_dedups_by_idempotency_key() {
        let Some(repo) = repo().await else {
            eprintln!("DATABASE_URL недоступен — mail repo тест пропущен");
            return;
        };
        let (first, duplicate) = repo.enqueue(new_mail("dj_invite:1")).await.expect("first");
        assert!(!duplicate, "первая постановка — не дубликат");

        let (second, duplicate) = repo.enqueue(new_mail("dj_invite:1")).await.expect("second");
        assert!(duplicate, "вторая постановка с тем же ключом — дубликат");
        assert_eq!(first, second, "дубликат возвращает id существующей строки");

        let (count,): (i64,) = sqlx::query_as("SELECT count(*) FROM mail_message")
            .fetch_one(&repo.pool())
            .await
            .unwrap();
        assert_eq!(count, 1, "второе письмо не создано");
    }

    #[tokio::test]
    async fn claim_batch_moves_rows_to_sending_and_returns_content() {
        let Some(repo) = repo().await else {
            eprintln!("DATABASE_URL недоступен — mail repo тест пропущен");
            return;
        };
        enqueue(&repo, "mail_test:claim").await;

        let claimed = repo.claim_batch(10).await.expect("claim");
        assert_eq!(claimed.len(), 1);
        let row = &claimed[0];
        // Диспетчер собирает OutgoingMail только из строки — тело обязано быть.
        assert_eq!(row.idempotency_key, "mail_test:claim");
        assert_eq!(row.status, "sending");
        assert_eq!(row.html, "<p>Привет</p>");
        assert_eq!(row.text, "Привет");
        assert_eq!(row.from_email, "DROP IT <no-reply@dropit.club>");
        assert_eq!(row.to_name.as_deref(), Some("Гость"));
        assert_eq!(row.reply_to.as_deref(), Some("support@dropit.club"));

        // Повторный claim не забирает уже отправляемую строку.
        assert!(repo.claim_batch(10).await.expect("claim2").is_empty());
    }

    #[tokio::test]
    async fn claim_batch_of_two_concurrent_workers_returns_disjoint_rows() {
        let Some(repo) = repo().await else {
            eprintln!("DATABASE_URL недоступен — mail repo тест пропущен");
            return;
        };
        let a = enqueue(&repo, "mail_test:a").await;
        let b = enqueue(&repo, "mail_test:b").await;

        // Два независимых «воркера» на одном пуле, каждый просит по строке.
        let first = repo.clone();
        let second = repo.clone();
        let (one, two) = tokio::join!(async move { first.claim_batch(1).await }, async move {
            second.claim_batch(1).await
        });
        let mut ids: Vec<Uuid> = one.expect("claim1").iter().map(|r| r.id).collect();
        ids.extend(two.expect("claim2").iter().map(|r| r.id));

        assert_eq!(ids.len(), 2, "оба воркера получили по строке");
        assert_ne!(ids[0], ids[1], "батчи не пересекаются (SKIP LOCKED)");
        ids.sort();
        let mut expected = vec![a, b];
        expected.sort();
        assert_eq!(ids, expected);
    }

    #[tokio::test]
    async fn mark_sent_sets_provider_id_and_sent_at() {
        let Some(repo) = repo().await else {
            eprintln!("DATABASE_URL недоступен — mail repo тест пропущен");
            return;
        };
        let id = enqueue(&repo, "mail_test:sent").await;
        repo.claim_batch(1).await.expect("claim");
        repo.mark_sent(id, "pm_123").await.expect("mark_sent");

        let detail = repo.get(id).await.expect("get").expect("row");
        assert_eq!(detail.mail.status, "sent");
        assert_eq!(detail.mail.provider_id.as_deref(), Some("pm_123"));
        assert!(detail.mail.sent_at.is_some(), "sent_at заполнен");
    }

    #[tokio::test]
    async fn mark_failed_applies_backoff_and_increments_attempts() {
        let Some(repo) = repo().await else {
            eprintln!("DATABASE_URL недоступен — mail repo тест пропущен");
            return;
        };
        let pool = repo.pool();
        let id = enqueue(&repo, "mail_test:backoff").await;
        repo.claim_batch(1).await.expect("claim");

        let before = Utc::now();
        repo.mark_failed(id, "connection reset", Some(Duration::from_secs(30)))
            .await
            .expect("mark_failed");

        let (status, attempts, next_attempt_at) = state(&pool, id).await;
        assert_eq!(
            status, "queued",
            "попытки не исчерпаны — возвращается в очередь"
        );
        assert_eq!(attempts, 1);
        let delta = (next_attempt_at - before).num_seconds();
        assert!(
            (28..=35).contains(&delta),
            "next_attempt_at ≈ now()+30s, получили {delta}s"
        );

        // Строка снова доступна для захвата (next_attempt_at в будущем —
        // поэтому пока нет; сдвигаем время вручную и проверяем).
        sqlx::query(
            "UPDATE mail_message SET next_attempt_at = now() - interval '1 second' WHERE id = $1",
        )
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
        let claimed = repo.claim_batch(10).await.expect("claim2");
        assert_eq!(claimed.len(), 1);
        assert_eq!(claimed[0].attempts, 1, "attempts сохранён");
    }

    #[tokio::test]
    async fn mark_failed_without_retry_after_fails_immediately() {
        let Some(repo) = repo().await else {
            eprintln!("DATABASE_URL недоступен — mail repo тест пропущен");
            return;
        };
        let pool = repo.pool();
        let id = enqueue(&repo, "mail_test:permanent").await;
        repo.claim_batch(1).await.expect("claim");
        repo.mark_failed(id, "invalid recipient", None)
            .await
            .expect("mark_failed");

        let (status, attempts, _) = state(&pool, id).await;
        assert_eq!(status, "failed", "без retry_after повтор бессмысленен");
        assert_eq!(attempts, 1);
    }

    #[tokio::test]
    async fn exhausted_attempts_become_failed_and_leave_the_queue() {
        let Some(repo) = repo().await else {
            eprintln!("DATABASE_URL недоступен — mail repo тест пропущен");
            return;
        };
        let pool = repo.pool();
        let id = enqueue(&repo, "mail_test:exhausted").await;
        set_max_attempts(&pool, id, 1).await;

        let claimed = repo.claim_batch(10).await.expect("claim");
        assert_eq!(claimed.len(), 1);
        repo.mark_failed(id, "boom", Some(Duration::from_secs(5)))
            .await
            .expect("mark_failed");

        let (status, attempts, _) = state(&pool, id).await;
        assert_eq!(status, "failed");
        assert_eq!(attempts, 1);

        sqlx::query(
            "UPDATE mail_message SET next_attempt_at = now() - interval '1 hour' WHERE id = $1",
        )
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
        assert!(
            repo.claim_batch(10).await.expect("claim2").is_empty(),
            "исчерпавшая попытки строка не забирается"
        );
    }

    #[tokio::test]
    async fn mark_suppressed_moves_queued_row_and_keeps_reason() {
        let Some(repo) = repo().await else {
            eprintln!("DATABASE_URL недоступен — mail repo тест пропущен");
            return;
        };
        let id = enqueue(&repo, "mail_test:suppressed").await;
        repo.claim_batch(1).await.expect("claim");
        repo.mark_suppressed(id, "recipient is in the suppression list")
            .await
            .expect("mark_suppressed");

        let detail = repo.get(id).await.expect("get").expect("row");
        assert_eq!(detail.mail.status, "suppressed");
        assert_eq!(
            detail.mail.last_error.as_deref(),
            Some("recipient is in the suppression list")
        );
    }

    // ---------------------------------------------------------
    // События доставки и монотонность статусов
    // ---------------------------------------------------------

    #[tokio::test]
    async fn happy_path_sent_then_delivered() {
        let Some(repo) = repo().await else {
            eprintln!("DATABASE_URL недоступен — mail repo тест пропущен");
            return;
        };
        let id = enqueue(&repo, "mail_test:happy").await;
        repo.claim_batch(1).await.expect("claim");
        // Отправку фиксирует воркер (mark_sent), вебхук `email.sent` после
        // этого уже ничего не меняет — см. отдельный тест на гонку.
        repo.mark_sent(id, "pm_happy").await.expect("mark_sent");

        let after_send = repo.get(id).await.expect("get").expect("row");
        assert_eq!(after_send.mail.status, "sent");

        assert!(
            repo.apply_event("pm_happy", "email.delivered")
                .await
                .expect("delivered event"),
            "delivered из sent применяется"
        );

        let detail = repo.get(id).await.expect("get").expect("row");
        assert_eq!(detail.mail.status, "delivered");
        assert!(detail.mail.delivered_at.is_some(), "delivered_at заполнен");
        assert!(detail.mail.sent_at.is_some());
    }

    #[tokio::test]
    async fn sent_event_applies_only_while_status_is_sending_or_queued() {
        let Some(repo) = repo().await else {
            eprintln!("DATABASE_URL недоступен — mail repo тест пропущен");
            return;
        };
        let pool = repo.pool();
        let id = enqueue(&repo, "mail_test:sent-event").await;
        repo.claim_batch(1).await.expect("claim");

        // Гонка «вебхук пришёл раньше mark_sent»: provider_id известен из
        // события, а строка ещё в sending. Только в этом случае у перехода
        // `sent` есть работа.
        sqlx::query("UPDATE mail_message SET provider_id = 'pm_race' WHERE id = $1")
            .bind(id)
            .execute(&pool)
            .await
            .expect("seed provider_id");

        assert!(
            repo.apply_event("pm_race", "email.sent")
                .await
                .expect("sent event"),
            "sent из sending применяется"
        );
        let detail = repo.get(id).await.expect("get").expect("row");
        assert_eq!(detail.mail.status, "sent");
        assert!(detail.mail.sent_at.is_some(), "sent_at заполнен вебхуком");

        // Повторный `email.sent` уже не применяется: строка не в sending/queued.
        assert!(
            !repo
                .apply_event("pm_race", "email.sent")
                .await
                .expect("sent event 2"),
            "из sent переход sent не применяется"
        );
    }

    #[tokio::test]
    async fn late_sent_event_after_delivered_is_noop() {
        let Some(repo) = repo().await else {
            eprintln!("DATABASE_URL недоступен — mail repo тест пропущен");
            return;
        };
        let id = enqueue(&repo, "mail_test:late").await;
        repo.claim_batch(1).await.expect("claim");
        repo.mark_sent(id, "pm_late").await.expect("mark_sent");
        repo.apply_event("pm_late", "delivered")
            .await
            .expect("delivered");

        let applied = repo
            .apply_event("pm_late", "sent")
            .await
            .expect("late sent event");
        assert!(!applied, "поздний sent не меняет ни одной строки");

        let detail = repo.get(id).await.expect("get").expect("row");
        assert_eq!(detail.mail.status, "delivered", "статус не откатился");
        assert!(detail.mail.delivered_at.is_some(), "delivered_at сохранён");
    }

    #[tokio::test]
    async fn delivery_delayed_then_delivered() {
        let Some(repo) = repo().await else {
            eprintln!("DATABASE_URL недоступен — mail repo тест пропущен");
            return;
        };
        let id = enqueue(&repo, "mail_test:delayed").await;
        repo.claim_batch(1).await.expect("claim");
        repo.mark_sent(id, "pm_delayed").await.expect("mark_sent");

        assert!(repo
            .apply_event("pm_delayed", "email.delivery_delayed")
            .await
            .expect("delayed"));
        let detail = repo.get(id).await.expect("get").expect("row");
        assert_eq!(detail.mail.status, "delivery_delayed");

        assert!(repo
            .apply_event("pm_delayed", "email.delivered")
            .await
            .expect("delivered"));
        let detail = repo.get(id).await.expect("get").expect("row");
        assert_eq!(detail.mail.status, "delivered");
    }

    #[tokio::test]
    async fn bounce_adds_suppression_for_recipient() {
        let Some(repo) = repo().await else {
            eprintln!("DATABASE_URL недоступен — mail repo тест пропущен");
            return;
        };
        let id = enqueue(&repo, "mail_test:bounce").await;
        repo.claim_batch(1).await.expect("claim");
        repo.mark_sent(id, "pm_bounce").await.expect("mark_sent");

        assert!(repo
            .apply_event("pm_bounce", "email.bounced")
            .await
            .expect("bounced"));

        let detail = repo.get(id).await.expect("get").expect("row");
        assert_eq!(detail.mail.status, "bounced");
        assert!(
            repo.is_suppressed("guest@example.com")
                .await
                .expect("suppressed"),
            "жёсткий bounce подавляет адрес"
        );
        let suppressed = repo.list_suppressions(10).await.expect("list");
        assert_eq!(suppressed.len(), 1);
        assert_eq!(suppressed[0].reason, "bounce");
        // Адрес сравнивается без учёта регистра (CITEXT).
        assert!(repo.is_suppressed("GUEST@Example.COM").await.expect("ci"));
    }

    #[tokio::test]
    async fn complaint_adds_suppression() {
        let Some(repo) = repo().await else {
            eprintln!("DATABASE_URL недоступен — mail repo тест пропущен");
            return;
        };
        let id = enqueue(&repo, "mail_test:complaint").await;
        repo.claim_batch(1).await.expect("claim");
        repo.mark_sent(id, "pm_complaint").await.expect("mark_sent");

        assert!(repo
            .apply_event("pm_complaint", "email.complained")
            .await
            .expect("complained"));
        let detail = repo.get(id).await.expect("get").expect("row");
        assert_eq!(detail.mail.status, "complained");
        let suppressed = repo.list_suppressions(10).await.expect("list");
        assert_eq!(suppressed.len(), 1);
        assert_eq!(suppressed[0].reason, "complaint");
    }

    #[tokio::test]
    async fn opened_and_clicked_do_not_change_status() {
        let Some(repo) = repo().await else {
            eprintln!("DATABASE_URL недоступен — mail repo тест пропущен");
            return;
        };
        let id = enqueue(&repo, "mail_test:opened").await;
        repo.claim_batch(1).await.expect("claim");
        repo.mark_sent(id, "pm_opened").await.expect("mark_sent");

        assert!(!repo
            .apply_event("pm_opened", "email.opened")
            .await
            .expect("opened"));
        assert!(!repo
            .apply_event("pm_opened", "email.clicked")
            .await
            .expect("clicked"));
        let detail = repo.get(id).await.expect("get").expect("row");
        assert_eq!(detail.mail.status, "sent", "открытие — не доставка");
    }

    #[tokio::test]
    async fn record_event_dedups_by_svix_id() {
        let Some(repo) = repo().await else {
            eprintln!("DATABASE_URL недоступен — mail repo тест пропущен");
            return;
        };
        let payload = json!({"type": "email.delivered", "data": {"email_id": "pm_1"}});

        let first = repo
            .record_event("msg_1", "email.delivered", Some("pm_1"), &payload)
            .await
            .expect("first");
        assert!(first, "первое событие новое");

        let second = repo
            .record_event("msg_1", "email.delivered", Some("pm_1"), &payload)
            .await
            .expect("second");
        assert!(!second, "повтор svix_id — не новое");

        let (count,): (i64,) = sqlx::query_as("SELECT count(*) FROM mail_event")
            .fetch_one(&repo.pool())
            .await
            .unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn get_returns_events_in_chronological_order() {
        let Some(repo) = repo().await else {
            eprintln!("DATABASE_URL недоступен — mail repo тест пропущен");
            return;
        };
        let id = enqueue(&repo, "mail_test:detail").await;
        repo.claim_batch(1).await.expect("claim");
        repo.mark_sent(id, "pm_detail").await.expect("mark_sent");
        repo.record_event(
            "msg_sent",
            "email.sent",
            Some("pm_detail"),
            &json!({"type": "email.sent"}),
        )
        .await
        .expect("event 1");
        repo.record_event(
            "msg_delivered",
            "email.delivered",
            Some("pm_detail"),
            &json!({"type": "email.delivered"}),
        )
        .await
        .expect("event 2");

        let detail = repo.get(id).await.expect("get").expect("row");
        assert_eq!(detail.events.len(), 2);
        assert_eq!(detail.events[0].event_type, "email.sent");
        assert_eq!(detail.events[1].event_type, "email.delivered");
        assert_eq!(
            detail.events[0].provider_id.as_deref(),
            Some("pm_detail"),
            "события сшиты с письмом по provider_id"
        );

        // Письмо без provider_id событий не имеет (и это не ошибка).
        let fresh = enqueue(&repo, "mail_test:noprovider").await;
        let detail = repo.get(fresh).await.expect("get").expect("row");
        assert!(detail.events.is_empty());
    }

    // ---------------------------------------------------------
    // Журнал, пагинация, статистика, переотправка
    // ---------------------------------------------------------

    #[tokio::test]
    async fn keyset_pagination_walks_two_pages() {
        let Some(repo) = repo().await else {
            eprintln!("DATABASE_URL недоступен — mail repo тест пропущен");
            return;
        };
        let pool = repo.pool();
        // Три письма с явными created_at: порядок не зависит от часов машины.
        let ids = [
            enqueue(&repo, "mail_test:p1").await,
            enqueue(&repo, "mail_test:p2").await,
            enqueue(&repo, "mail_test:p3").await,
        ];
        for (idx, id) in ids.iter().enumerate() {
            sqlx::query(
                "UPDATE mail_message SET created_at = now() - make_interval(secs => $2) \
                 WHERE id = $1",
            )
            .bind(id)
            .bind((30 - idx as i32 * 10) as f64)
            .execute(&pool)
            .await
            .unwrap();
        }

        let (page1, cursor) = repo
            .list(MailFilter {
                limit: 2,
                ..Default::default()
            })
            .await
            .expect("page1");
        assert_eq!(page1.len(), 2);
        let cursor = cursor.expect("есть вторая страница");
        // DESC по created_at: сначала самое свежее (p3), затем p2.
        assert_eq!(page1[0].id, ids[2]);
        assert_eq!(page1[1].id, ids[1]);
        // Строка списка — слим-проекция, но поля журнала на месте.
        assert_eq!(page1[0].subject, "Тестовое письмо DROP IT");
        assert_eq!(page1[0].status, "queued");

        let (page2, cursor2) = repo
            .list(MailFilter {
                limit: 2,
                cursor: Some(cursor),
                ..Default::default()
            })
            .await
            .expect("page2");
        assert_eq!(page2.len(), 1);
        assert_eq!(page2[0].id, ids[0]);
        assert!(cursor2.is_none(), "страница последняя");

        // Битый курсор не роняет запрос — отдаёт первую страницу.
        let (fallback, _) = repo
            .list(MailFilter {
                limit: 2,
                cursor: Some("не-курсор".to_string()),
                ..Default::default()
            })
            .await
            .expect("bad cursor");
        assert_eq!(fallback.len(), 2);
    }

    #[tokio::test]
    async fn list_filters_by_status_and_query() {
        let Some(repo) = repo().await else {
            eprintln!("DATABASE_URL недоступен — mail repo тест пропущен");
            return;
        };
        let failed = enqueue(&repo, "mail_test:f1").await;
        enqueue(&repo, "mail_test:f2").await;
        repo.claim_batch(1).await.expect("claim");
        repo.mark_failed(failed, "boom", None).await.expect("fail");

        let (by_status, _) = repo
            .list(MailFilter {
                status: Some("failed".to_string()),
                ..Default::default()
            })
            .await
            .expect("status filter");
        assert_eq!(by_status.len(), 1);
        assert_eq!(by_status[0].id, failed);

        let (by_query, _) = repo
            .list(MailFilter {
                query: Some("guest@example".to_string()),
                ..Default::default()
            })
            .await
            .expect("query filter");
        assert_eq!(by_query.len(), 2, "поиск по адресу находит оба письма");

        let (none, _) = repo
            .list(MailFilter {
                query: Some("нет-такого-адреса".to_string()),
                ..Default::default()
            })
            .await
            .expect("no match");
        assert!(none.is_empty());
    }

    #[tokio::test]
    async fn stats_counts_statuses_in_window() {
        let Some(repo) = repo().await else {
            eprintln!("DATABASE_URL недоступен — mail repo тест пропущен");
            return;
        };
        let queued = enqueue(&repo, "mail_test:st1").await;
        let delivered = enqueue(&repo, "mail_test:st2").await;
        let failed = enqueue(&repo, "mail_test:st3").await;
        repo.claim_batch(10).await.expect("claim");
        repo.mark_sent(delivered, "pm_st2").await.expect("sent");
        repo.apply_event("pm_st2", "delivered")
            .await
            .expect("delivered");
        repo.mark_failed(failed, "boom", None)
            .await
            .expect("failed");

        let stats = repo.stats(24).await.expect("stats");
        assert_eq!(stats.window_hours, 24);
        assert_eq!(stats.total, 3);
        assert_eq!(stats.delivered, 1);
        assert_eq!(stats.failed, 1);
        assert_eq!(stats.queued, 0, "все три строки ушли из queued");

        // Окно в прошлом ничего не находит.
        sqlx::query("UPDATE mail_message SET created_at = now() - interval '3 days'")
            .execute(&repo.pool())
            .await
            .unwrap();
        let old = repo.stats(24).await.expect("stats old");
        assert_eq!(old.total, 0);

        let _ = queued;
    }

    #[tokio::test]
    async fn requeue_creates_new_row_with_suffixed_key() {
        let Some(repo) = repo().await else {
            eprintln!("DATABASE_URL недоступен — mail repo тест пропущен");
            return;
        };
        let original = enqueue(&repo, "dj_invite:42").await;
        repo.claim_batch(1).await.expect("claim");
        repo.mark_sent(original, "pm_req").await.expect("sent");

        let first = repo.requeue(original).await.expect("requeue 1");
        assert_ne!(first, original, "переотправка — новая строка");
        let detail = repo.get(first).await.expect("get").expect("row");
        assert_eq!(detail.mail.idempotency_key, "dj_invite:42:r1");
        assert_eq!(detail.mail.status, "queued");
        assert_eq!(detail.mail.attempts, 0, "счётчик попыток сброшен");
        assert_eq!(detail.mail.subject, "Тестовое письмо DROP IT");
        assert_eq!(detail.mail.html, "<p>Привет</p>");

        let second = repo.requeue(original).await.expect("requeue 2");
        let detail = repo.get(second).await.expect("get").expect("row");
        assert_eq!(detail.mail.idempotency_key, "dj_invite:42:r2");

        // Исходная строка осталась историей, а не переписалась.
        let original_detail = repo.get(original).await.expect("get").expect("row");
        assert_eq!(original_detail.mail.status, "sent");
    }

    #[tokio::test]
    async fn requeue_unknown_id_is_not_found() {
        let Some(repo) = repo().await else {
            eprintln!("DATABASE_URL недоступен — mail repo тест пропущен");
            return;
        };
        let err = repo.requeue(Uuid::now_v7()).await.unwrap_err();
        assert!(matches!(err, MailDbError::NotFound), "{err:?}");
        assert!(repo.get(Uuid::now_v7()).await.expect("get").is_none());
    }

    // ---------------------------------------------------------
    // Подавления
    // ---------------------------------------------------------

    #[tokio::test]
    async fn suppression_is_queryable_but_blocks_nothing_at_db_level() {
        let Some(repo) = repo().await else {
            eprintln!("DATABASE_URL недоступен — mail repo тест пропущен");
            return;
        };
        repo.add_suppression("bounce@example.com", "manual", Some("проверено вручную"))
            .await
            .expect("add");
        assert!(repo.is_suppressed("bounce@example.com").await.expect("q"));
        assert!(!repo.is_suppressed("clean@example.com").await.expect("q2"));

        // Письмо на подавленный адрес всё равно ставится в очередь: решение
        // «не отправлять» принимает воркер, а не БД (иначе ретрай бизнес-
        // операции молча терял бы письмо без записи в журнале).
        let mut mail = new_mail("mail_test:suppressed-enqueue");
        mail.to_email = "bounce@example.com".to_string();
        let id = repo.enqueue(mail).await.expect("enqueue").0;
        let detail = repo.get(id).await.expect("get").expect("row");
        assert_eq!(detail.mail.status, "queued");

        let listed = repo.list_suppressions(10).await.expect("list");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].email, "bounce@example.com");
        assert_eq!(listed[0].reason, "manual");
        assert_eq!(listed[0].detail.as_deref(), Some("проверено вручную"));

        // Повторное добавление обновляет причину, но не created_at.
        let created_before = listed[0].created_at;
        repo.add_suppression("BOUNCE@example.com", "bounce", None)
            .await
            .expect("upsert");
        let listed = repo.list_suppressions(10).await.expect("list2");
        assert_eq!(listed.len(), 1, "CITEXT: регистр не создаёт вторую запись");
        assert_eq!(listed[0].reason, "bounce");
        assert_eq!(
            listed[0].detail.as_deref(),
            Some("проверено вручную"),
            "пустой detail не затирает прежний"
        );
        assert_eq!(listed[0].created_at, created_before, "created_at сохранён");
    }

    #[tokio::test]
    async fn remove_suppression_reports_whether_it_existed() {
        let Some(repo) = repo().await else {
            eprintln!("DATABASE_URL недоступен — mail repo тест пропущен");
            return;
        };
        repo.add_suppression("gone@example.com", "bounce", None)
            .await
            .expect("add");
        assert!(repo
            .remove_suppression("gone@example.com")
            .await
            .expect("remove"));
        assert!(!repo
            .remove_suppression("gone@example.com")
            .await
            .expect("remove again"));
        assert!(!repo.is_suppressed("gone@example.com").await.expect("q"));
    }
}
