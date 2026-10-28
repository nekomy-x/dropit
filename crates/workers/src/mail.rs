//! Диспетчер транзакционных писем: забирает очередь `mail_message`,
//! отправляет через [`MailSender`] и фиксирует результат.
//!
//! # Источник сообщений
//!
//! Очередь — таблица `mail_message` (миграция `0013`). Строка создаётся в
//! обработчике бизнес-операции (`MailRepo::enqueue`) уже с отрисованным телом,
//! поэтому диспетчер не знает про шаблоны: он только доставляет то, что
//! положили в очередь. Это осознанно — рендер в обработчике не может
//! «сломаться» позже из-за смены шаблона, а письмо в журнале всегда совпадает
//! с тем, что реально ушло.
//!
//! # Идемпотентность
//!
//! [`MailStore::claim_batch`] захватывает строки `FOR UPDATE SKIP LOCKED` и
//! переводит их в `sending`, поэтому два параллельных воркера не отправят
//! одно письмо дважды. Повторная доставка возможна только если процесс упал
//! между `send` и `mark_sent` — её схлопывает провайдер по `Idempotency-Key`
//! (ADR-008).
//!
//! # Backoff
//!
//! Неудача → `mark_failed(attempts+1, retry_after)`. Задержка растёт как
//! `min(2^attempts · 5 c, 1 ч)`; `RateLimited` от провайдера уважает его
//! `Retry-After`. После `max_attempts` строка становится `failed` и больше не
//! забирается — «отравленное» письмо не крутится в очереди вечно.
//!
//! # Сетевая безопасность в тестах
//!
//! В тестах подставляются фейковый [`MailStore`] и [`MailSender`]: ни БД, ни
//! сети. Это тот же приём, что в [`crate::push`].

use std::time::Duration;

use async_trait::async_trait;
use dropit_infra_mail::{MailAddress, MailError, MailSender, OutgoingMail};
use uuid::Uuid;

use crate::jobs::JobReport;

/// Максимальная задержка повтора: час. Дольше держать письмо в очереди
/// бессмысленно — за это время адресат либо получит его, либо оно станет
/// операционной проблемой, видимой в админке.
pub const MAX_RETRY_DELAY_SECS: u64 = 3600;

/// Базовая задержка повтора: 5 секунд.
pub const BASE_RETRY_DELAY_SECS: u64 = 5;

/// Письмо, готовое к отправке (проекция строки `mail_message`).
#[derive(Debug, Clone)]
pub struct PendingMail {
    /// Идентификатор строки журнала.
    pub id: Uuid,
    /// Ключ идемпотентности — он же заголовок `Idempotency-Key` у провайдера.
    pub idempotency_key: String,
    /// Адрес отправителя (уже в формате `Name <a@b>` или `a@b`).
    pub from_email: String,
    /// Адрес получателя.
    pub to_email: String,
    /// Отображаемое имя получателя.
    pub to_name: Option<String>,
    /// Адрес для ответа.
    pub reply_to: Option<String>,
    /// Тема письма.
    pub subject: String,
    /// HTML-тело.
    pub html: String,
    /// Текстовое тело.
    pub text: String,
    /// Номер текущей попытки (0 — первая).
    pub attempts: i32,
}

/// Хранилище очереди писем — минимум, который нужен диспетчеру.
///
/// Продакшен-реализация — [`DbMailStore`] поверх [`dropit_infra_db::mail::MailRepo`].
/// В тестах подставляется фейк: диспетчер проверяется без БД.
#[async_trait]
pub trait MailStore: Send + Sync {
    /// Захватить до `limit` писем, готовых к отправке (статус → `sending`).
    async fn claim_batch(&self, limit: i64) -> Result<Vec<PendingMail>, String>;
    /// Пометить отправленным с идентификатором провайдера.
    async fn mark_sent(&self, id: Uuid, provider_id: &str) -> Result<(), String>;
    /// Учесть неудачу: `attempts+1`, backoff или окончательный `failed`.
    async fn mark_failed(
        &self,
        id: Uuid,
        error: &str,
        retry_after: Option<Duration>,
    ) -> Result<(), String>;
    /// Пометить подавленным (адрес в `mail_suppression`).
    async fn mark_suppressed(&self, id: Uuid, reason: &str) -> Result<(), String>;
    /// Адрес в списке подавлений.
    async fn is_suppressed(&self, email: &str) -> Result<bool, String>;
}

/// Продакшен-хранилище поверх репозитория `MailRepo`.
pub struct DbMailStore {
    repo: dropit_infra_db::mail::MailRepo,
}

impl DbMailStore {
    /// Создать хранилище из репозитория.
    #[must_use]
    pub fn new(repo: dropit_infra_db::mail::MailRepo) -> Self {
        Self { repo }
    }
}

#[async_trait]
impl MailStore for DbMailStore {
    async fn claim_batch(&self, limit: i64) -> Result<Vec<PendingMail>, String> {
        let rows = self
            .repo
            .claim_batch(limit)
            .await
            .map_err(|e| e.to_string())?;
        Ok(rows
            .into_iter()
            .map(|r| PendingMail {
                id: r.id,
                idempotency_key: r.idempotency_key,
                from_email: r.from_email,
                to_email: r.to_email,
                to_name: r.to_name,
                reply_to: r.reply_to,
                subject: r.subject,
                html: r.html,
                text: r.text,
                attempts: r.attempts,
            })
            .collect())
    }

    async fn mark_sent(&self, id: Uuid, provider_id: &str) -> Result<(), String> {
        self.repo
            .mark_sent(id, provider_id)
            .await
            .map_err(|e| e.to_string())
    }

    async fn mark_failed(
        &self,
        id: Uuid,
        error: &str,
        retry_after: Option<Duration>,
    ) -> Result<(), String> {
        self.repo
            .mark_failed(id, error, retry_after)
            .await
            .map_err(|e| e.to_string())
    }

    async fn mark_suppressed(&self, id: Uuid, reason: &str) -> Result<(), String> {
        self.repo
            .mark_suppressed(id, reason)
            .await
            .map_err(|e| e.to_string())
    }

    async fn is_suppressed(&self, email: &str) -> Result<bool, String> {
        self.repo
            .is_suppressed(email)
            .await
            .map_err(|e| e.to_string())
    }
}

/// Задержка повтора для попытки `attempts` (уже увеличенной).
///
/// `5 c · 2^(attempts-1)`, но не больше [`MAX_RETRY_DELAY_SECS`]. Считаем в
/// `u64` с насыщением: `attempts` приходит из БД и теоретически может быть
/// большим, а `1 << 64` — это паника в debug-сборке.
#[must_use]
pub fn retry_delay(attempts: i32) -> Duration {
    let shift = (attempts.max(1) - 1).clamp(0, 32) as u32;
    let secs = BASE_RETRY_DELAY_SECS
        .saturating_mul(1u64 << shift)
        .min(MAX_RETRY_DELAY_SECS);
    Duration::from_secs(secs)
}

/// Один прогон диспетчера писем.
///
/// Возвращает [`JobReport`] со счётчиками: `claimed` (захвачено),
/// `sent` (успешно), `failed` (ошибка отправки, будет повтор),
/// `suppressed` (получатель в списке подавлений, отправки не было),
/// `noop` (транспорт-заглушка: письма «ушли» без сети).
///
/// Не падает на ошибке одного письма: логирует и оставляет его в очереди для
/// следующей попытки.
pub async fn run_mail(store: &dyn MailStore, sender: &dyn MailSender, batch: i64) -> JobReport {
    let pending = match store.claim_batch(batch).await {
        Ok(rows) => rows,
        Err(e) => return JobReport::error("mail", format!("claim: {e}")),
    };
    let claimed = pending.len();
    let live = sender.is_live();

    let mut sent = 0usize;
    let mut failed = 0usize;
    let mut suppressed = 0usize;
    let mut noop = 0usize;

    for row in pending {
        // Подавлённый адрес не отправляем вовсе: bounce/complaint уже
        // сказали своё, повтор только портит репутацию домена.
        match store.is_suppressed(&row.to_email).await {
            Ok(true) => {
                if let Err(e) = store
                    .mark_suppressed(row.id, "recipient is in the suppression list")
                    .await
                {
                    tracing::warn!(error = %e, mail_id = %row.id, "mail: не пометить подавленным");
                }
                suppressed += 1;
                continue;
            }
            Ok(false) => {}
            Err(e) => {
                // Не знаем статус подавления — лучше отправить, чем потерять
                // письмо: ложная отправка исправима, потерянное письмо — нет.
                tracing::warn!(error = %e, mail_id = %row.id, "mail: не прочитать подавления, продолжаем");
            }
        }

        let mail = match build_outgoing(&row) {
            Ok(m) => m,
            Err(e) => {
                // Битый адрес — это баг в источнике, повтор не поможет.
                tracing::warn!(error = %e, mail_id = %row.id, "mail: не собрать письмо, помечаем failed");
                if let Err(e) = store.mark_failed(row.id, &e.to_string(), None).await {
                    tracing::warn!(error = %e, mail_id = %row.id, "mail: не пометить failed");
                }
                failed += 1;
                continue;
            }
        };

        match sender.send(&mail).await {
            Ok(receipt) => {
                if let Err(e) = store.mark_sent(row.id, &receipt.provider_id).await {
                    // Письмо ушло, но отметка не встала: строка вернётся в
                    // очередь и уйдёт повторно — провайдер схлопнет по
                    // Idempotency-Key.
                    tracing::warn!(error = %e, mail_id = %row.id, "mail: не отметить sent");
                }
                if live {
                    sent += 1;
                } else {
                    noop += 1;
                }
            }
            Err(e) => {
                let retry_after = match &e {
                    MailError::RateLimited { retry_after_secs } => {
                        retry_after_secs.map(|s| Duration::from_secs(s.max(0) as u64))
                    }
                    _ => None,
                };
                let delay = retry_after.unwrap_or_else(|| retry_delay(row.attempts + 1));
                tracing::warn!(
                    error = %e,
                    mail_id = %row.id,
                    attempts = row.attempts,
                    retry_in_secs = delay.as_secs(),
                    "mail: отправка не удалась"
                );
                if let Err(e) = store.mark_failed(row.id, &e.to_string(), Some(delay)).await {
                    tracing::warn!(error = %e, mail_id = %row.id, "mail: не пометить failed");
                }
                failed += 1;
            }
        }
    }

    JobReport::ok(
        "mail",
        serde_json::json!({
            "claimed": claimed,
            "sent": sent,
            "failed": failed,
            "suppressed": suppressed,
            "noop": noop,
            "live": live,
        }),
    )
}

/// Собрать [`OutgoingMail`] из строки очереди.
///
/// Адреса разбираются тем же парсером, что и на входе в очередь, поэтому
/// формат `Name <a@b>` проходит круг без потерь.
pub fn build_outgoing(row: &PendingMail) -> Result<OutgoingMail, MailError> {
    let from = MailAddress::parse(&row.from_email)?;
    let mut to = MailAddress::parse(&row.to_email)?;
    if let Some(name) = row
        .to_name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        to.name = Some(name.to_string());
    }
    let reply_to = match row
        .reply_to
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(raw) => Some(MailAddress::parse(raw)?),
        None => None,
    };
    Ok(OutgoingMail {
        id: row.id,
        idempotency_key: row.idempotency_key.clone(),
        from,
        to,
        reply_to,
        subject: row.subject.clone(),
        html: row.html.clone(),
        text: row.text.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use dropit_infra_mail::{NoopMail, SendReceipt};
    use std::collections::HashMap;
    use std::sync::Mutex;

    fn pending(n: u128, to: &str) -> PendingMail {
        PendingMail {
            id: Uuid::from_u128(n),
            idempotency_key: format!("key-{n}"),
            from_email: "DROP IT <no-reply@dropit.club>".into(),
            to_email: to.into(),
            to_name: Some("Guest".into()),
            reply_to: None,
            subject: "Тема".into(),
            html: "<p>hi</p>".into(),
            text: "hi".into(),
            attempts: 0,
        }
    }

    /// Фейковое хранилище: очередь в памяти + журнал вызовов.
    #[derive(Default)]
    struct FakeStore {
        queued: Mutex<Vec<PendingMail>>,
        suppressed: Mutex<Vec<String>>,
        sent: Mutex<Vec<(Uuid, String)>>,
        failed: Mutex<Vec<(Uuid, String, Option<Duration>)>>,
        marked_suppressed: Mutex<Vec<(Uuid, String)>>,
    }

    impl FakeStore {
        fn with(mails: Vec<PendingMail>) -> Self {
            Self {
                queued: Mutex::new(mails),
                ..Default::default()
            }
        }

        fn suppress(mut self, email: &str) -> Self {
            self.suppressed.get_mut().unwrap().push(email.to_string());
            self
        }
    }

    #[async_trait]
    impl MailStore for FakeStore {
        async fn claim_batch(&self, limit: i64) -> Result<Vec<PendingMail>, String> {
            let mut queued = self.queued.lock().unwrap();
            let take = (limit.max(0) as usize).min(queued.len());
            Ok(queued.drain(..take).collect())
        }

        async fn mark_sent(&self, id: Uuid, provider_id: &str) -> Result<(), String> {
            self.sent
                .lock()
                .unwrap()
                .push((id, provider_id.to_string()));
            Ok(())
        }

        async fn mark_failed(
            &self,
            id: Uuid,
            error: &str,
            retry_after: Option<Duration>,
        ) -> Result<(), String> {
            self.failed
                .lock()
                .unwrap()
                .push((id, error.to_string(), retry_after));
            Ok(())
        }

        async fn mark_suppressed(&self, id: Uuid, reason: &str) -> Result<(), String> {
            self.marked_suppressed
                .lock()
                .unwrap()
                .push((id, reason.to_string()));
            Ok(())
        }

        async fn is_suppressed(&self, email: &str) -> Result<bool, String> {
            Ok(self
                .suppressed
                .lock()
                .unwrap()
                .iter()
                .any(|e| e.eq_ignore_ascii_case(email)))
        }
    }

    /// Отправитель, который падает заданной ошибкой.
    struct FailingSender {
        error: fn() -> MailError,
    }

    #[async_trait]
    impl MailSender for FailingSender {
        async fn send(&self, _mail: &OutgoingMail) -> Result<SendReceipt, MailError> {
            Err((self.error)())
        }
    }

    #[tokio::test]
    async fn sends_and_marks_sent() {
        let store = FakeStore::with(vec![pending(1, "a@b.c"), pending(2, "d@e.f")]);
        let report = run_mail(&store, &NoopMail, 10).await;
        assert_eq!(report.status, "ok");
        assert_eq!(report.detail["claimed"], 2);
        assert_eq!(report.detail["noop"], 2, "NoopMail не живой транспорт");
        assert_eq!(report.detail["sent"], 0);
        assert_eq!(store.sent.lock().unwrap().len(), 2);
        assert_eq!(store.sent.lock().unwrap()[0].0, Uuid::from_u128(1));
    }

    #[tokio::test]
    async fn live_sender_counts_as_sent() {
        struct Live;
        #[async_trait]
        impl MailSender for Live {
            async fn send(&self, mail: &OutgoingMail) -> Result<SendReceipt, MailError> {
                Ok(SendReceipt {
                    provider_id: format!("resend-{}", mail.id),
                    provider: "resend",
                })
            }
        }
        let store = FakeStore::with(vec![pending(1, "a@b.c")]);
        let report = run_mail(&store, &Live, 10).await;
        assert_eq!(report.detail["sent"], 1);
        assert_eq!(report.detail["noop"], 0);
        assert_eq!(report.detail["live"], true);
    }

    #[tokio::test]
    async fn suppressed_recipient_is_not_sent() {
        let store = FakeStore::with(vec![pending(1, "bounce@x.y"), pending(2, "ok@x.y")])
            .suppress("bounce@x.y");
        let report = run_mail(&store, &NoopMail, 10).await;
        assert_eq!(report.detail["suppressed"], 1);
        assert_eq!(report.detail["noop"], 1);
        assert_eq!(store.marked_suppressed.lock().unwrap().len(), 1);
        // Подавлённому не отправляли: mark_sent только для второго.
        let sent = store.sent.lock().unwrap();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].0, Uuid::from_u128(2));
    }

    #[tokio::test]
    async fn transport_failure_schedules_retry() {
        let store = FakeStore::with(vec![pending(1, "a@b.c")]);
        let sender = FailingSender {
            error: || MailError::Unavailable("boom".into()),
        };
        let report = run_mail(&store, &sender, 10).await;
        assert_eq!(report.detail["failed"], 1);
        assert_eq!(report.detail["sent"], 0);
        let failed = store.failed.lock().unwrap();
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].2, Some(retry_delay(1)));
    }

    #[tokio::test]
    async fn rate_limit_honours_retry_after() {
        let store = FakeStore::with(vec![pending(1, "a@b.c")]);
        let sender = FailingSender {
            error: || MailError::RateLimited {
                retry_after_secs: Some(42),
            },
        };
        run_mail(&store, &sender, 10).await;
        let failed = store.failed.lock().unwrap();
        assert_eq!(failed[0].2, Some(Duration::from_secs(42)));
    }

    #[tokio::test]
    async fn one_bad_row_does_not_abort_batch() {
        let mut bad = pending(1, "a@b.c");
        bad.to_email = "not-an-email".into();
        let store = FakeStore::with(vec![bad, pending(2, "ok@x.y")]);
        let report = run_mail(&store, &NoopMail, 10).await;
        assert_eq!(report.detail["failed"], 1, "битый адрес → failed");
        assert_eq!(report.detail["noop"], 1, "остаток батча обработан");
    }

    #[tokio::test]
    async fn empty_batch_is_noop() {
        let store = FakeStore::with(vec![]);
        let report = run_mail(&store, &NoopMail, 10).await;
        assert_eq!(report.detail["claimed"], 0);
        assert_eq!(report.detail["sent"], 0);
        assert!(store.sent.lock().unwrap().is_empty());
    }

    #[test]
    fn retry_delay_grows_and_caps() {
        assert_eq!(retry_delay(1), Duration::from_secs(5));
        assert_eq!(retry_delay(2), Duration::from_secs(10));
        assert_eq!(retry_delay(3), Duration::from_secs(20));
        assert_eq!(retry_delay(0), Duration::from_secs(5), "0 трактуется как 1");
        assert_eq!(retry_delay(100), Duration::from_secs(MAX_RETRY_DELAY_SECS));
    }

    #[test]
    fn build_outgoing_round_trips_addresses() {
        let mut row = pending(1, "guest@x.y");
        row.reply_to = Some("support@dropit.club".into());
        let mail = build_outgoing(&row).expect("валидная строка");
        assert_eq!(mail.from.email, "no-reply@dropit.club");
        assert_eq!(mail.from.name.as_deref(), Some("DROP IT"));
        assert_eq!(mail.to.email, "guest@x.y");
        assert_eq!(mail.to.name.as_deref(), Some("Guest"));
        assert_eq!(
            mail.reply_to.as_ref().map(|r| r.email.as_str()),
            Some("support@dropit.club")
        );
    }

    #[test]
    fn build_outgoing_ignores_blank_name_and_reply_to() {
        let mut row = pending(1, "guest@x.y");
        row.to_name = Some("   ".into());
        row.reply_to = Some("".into());
        let mail = build_outgoing(&row).expect("валидная строка");
        assert!(mail.to.name.is_none());
        assert!(mail.reply_to.is_none());
    }

    #[test]
    fn pending_mail_is_debug() {
        // Компиляционный тест: тип должен оставаться Debug для логов.
        let row = pending(1, "a@b.c");
        let _ = format!("{row:?}");
        let mut map: HashMap<String, usize> = HashMap::new();
        map.insert(row.idempotency_key, 1);
    }
}
