//! Транзакционная почта: очередь `mail_message`, журнал `mail_event`,
//! подавления `mail_suppression` (миграция `0013_mail.sql`).
//!
//! # Зачем отдельный репозиторий
//!
//! Письмо — не событие outbox: у него своя жизнь после отправки (принято,
//! доставлено, задержано, отскочило, жалоба) и своя история в админке
//! `/mail`. Поэтому здесь три таблицы и явная машина состояний, а не
//! «опубликовать и забыть».
//!
//! # Монотонность статусов
//!
//! Переходы выполняются **в SQL** (`UPDATE … WHERE status IN (…)`), а не в
//! Rust: только так они остаются атомарными относительно параллельного
//! вебхука. Правило одно: терминальное состояние назад не откатывается.
//! Поздний `email.sent`, пришедший после `email.delivered` (сеть
//! переупорядочила доставку), не имеет права вернуть строку в `sent` —
//! это ложь в журнале и потерянный `delivered_at`.
//!
//! # Захват очереди
//!
//! [`MailRepo::claim_batch`] делает `UPDATE … WHERE id IN (SELECT … FOR
//! UPDATE SKIP LOCKED) … RETURNING` одним запросом. Это важно: вариант
//! «SELECT, потом UPDATE» в двух транзакциях позволяет двум воркерам
//! выбрать одни и те же строки. Один оператор держит блокировки до
//! коммита, второй воркер проходит мимо занятых строк (`SKIP LOCKED`), и
//! батчи получаются непересекающимися.
//!
//! # Идемпотентность
//!
//! `idempotency_key` UNIQUE: повторная постановка письма (ретрай
//! бизнес-операции, двойной клик) возвращает существующий `id` и признак
//! дубликата, а не второе письмо. Вебхуки дедуплицируются по `svix_id`
//! (`INSERT … ON CONFLICT DO NOTHING`) — Svix доставляет at-least-once, и
//! «событие уже было» должно быть штатным ответом, а не ошибкой.

use std::time::Duration;

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

/// Полный список колонок [`MailRow`] (в порядке объявления).
///
/// `to_email` приводится к `text`: колонка `CITEXT`, а `sqlx` без этого
/// каста не знает, как раскодировать её в `String`.
const MAIL_ROW_COLUMNS: &str = concat!(
    "id, idempotency_key, template, to_email::text AS to_email, to_name, ",
    "from_email, reply_to, subject, html, text, vars, status, attempts, max_attempts, ",
    "provider_id, last_error, aggregate_type, aggregate_id, created_at, updated_at, ",
    "sent_at, delivered_at",
);

/// Список колонок [`MailListItem`] — без тела письма.
const MAIL_LIST_COLUMNS: &str = concat!(
    "id, to_email::text AS to_email, to_name, subject, template, status, attempts, ",
    "max_attempts, provider_id, last_error, aggregate_type, aggregate_id, created_at, ",
    "sent_at, delivered_at",
);

/// Разделитель частей курсора (unit separator: не встречается в данных).
const CURSOR_SEP: char = '\u{1f}';

/// Предел страницы журнала — защита от `limit=100000`.
const MAX_PAGE: i64 = 200;

/// Сколько раз пробуем подобрать номер ключа при `requeue`.
const REQUEUE_TRIES: u32 = 64;

/// Ошибки слоя почты.
#[derive(Debug, thiserror::Error)]
pub enum MailDbError {
    /// Ошибка Postgres.
    #[error("database error")]
    Db(#[from] sqlx::Error),
    /// Строка `mail_message` с таким `id` не найдена.
    #[error("mail message not found")]
    NotFound,
    /// Не удалось подобрать свободный `idempotency_key` при переотправке
    /// (практически недостижимо: 64 попытки на один `id`).
    #[error("could not allocate a requeue idempotency key")]
    RequeueExhausted,
}

/// Новое письмо для постановки в очередь.
///
/// Тело приходит уже отрендеренным: рендер делает вызывающий (gateway или
/// воркер) в момент бизнес-события. Так письмо в журнале всегда совпадает
/// с тем, что реально ушло, даже если шаблон потом поменяется.
#[derive(Debug, Clone)]
pub struct NewMail {
    /// Ключ идемпотентности; он же уходит провайдеру заголовком
    /// `Idempotency-Key`.
    pub idempotency_key: String,
    /// Ключ шаблона (`dj_invite`, …) — для фильтра и разбора.
    pub template: String,
    /// Адрес получателя.
    pub to_email: String,
    /// Отображаемое имя получателя.
    pub to_name: Option<String>,
    /// Отправитель в формате `Name <a@b>`.
    pub from_email: String,
    /// Адрес для ответа.
    pub reply_to: Option<String>,
    /// Тема (уже с подставленными переменными).
    pub subject: String,
    /// HTML-версия.
    pub html: String,
    /// Текстовая версия.
    pub text: String,
    /// Переменные шаблона — для переотправки и разбора инцидентов.
    pub vars: Value,
    /// Тип бизнес-сущности (`dj`, `guest_account`, `payout`) — мягкая ссылка.
    pub aggregate_type: Option<String>,
    /// Идентификатор бизнес-сущности — без FK, как в `outbox`.
    pub aggregate_id: Option<Uuid>,
}

/// Полная строка `mail_message`.
///
/// Содержит всё, что нужно диспетчеру, чтобы собрать [`OutgoingMail`]
/// без второго запроса: ключ идемпотентности, адреса, тело.
///
/// [`OutgoingMail`]: https://docs.rs/dropit-infra-mail
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct MailRow {
    /// Идентификатор строки (UUIDv7).
    pub id: Uuid,
    /// Ключ идемпотентности — он же `Idempotency-Key` у провайдера.
    pub idempotency_key: String,
    /// Ключ шаблона.
    pub template: String,
    /// Адрес получателя.
    pub to_email: String,
    /// Отображаемое имя получателя.
    pub to_name: Option<String>,
    /// Отправитель в формате `Name <a@b>`.
    pub from_email: String,
    /// Адрес для ответа.
    pub reply_to: Option<String>,
    /// Тема письма.
    pub subject: String,
    /// HTML-тело.
    pub html: String,
    /// Текстовое тело.
    pub text: String,
    /// Переменные шаблона.
    pub vars: Value,
    /// Статус: `queued | sending | sent | delivered | delivery_delayed |
    /// bounced | complained | failed | suppressed`.
    pub status: String,
    /// Сколько попыток отправки уже сделано.
    pub attempts: i32,
    /// Предел попыток для этой строки.
    pub max_attempts: i32,
    /// Идентификатор письма у провайдера; ключ сшивки с вебхуками.
    pub provider_id: Option<String>,
    /// Последняя причина сбоя или подавления.
    pub last_error: Option<String>,
    /// Тип бизнес-сущности-источника.
    pub aggregate_type: Option<String>,
    /// Идентификатор бизнес-сущности-источника.
    pub aggregate_id: Option<Uuid>,
    /// Когда письмо поставлено в очередь.
    pub created_at: DateTime<Utc>,
    /// Когда строку последний раз меняли.
    pub updated_at: DateTime<Utc>,
    /// Когда провайдер принял письмо.
    pub sent_at: Option<DateTime<Utc>>,
    /// Когда провайдер подтвердил доставку.
    pub delivered_at: Option<DateTime<Utc>>,
}

/// Строка журнала для списка: без `html`/`text`/`vars`.
///
/// Список в админке не показывает тело письма, а тела бывают по несколько
/// килобайт. Тянуть их на каждую строку страницы — это мегабайты лишнего
/// трафика на ровном месте.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct MailListItem {
    /// Идентификатор строки.
    pub id: Uuid,
    /// Адрес получателя.
    pub to_email: String,
    /// Отображаемое имя получателя.
    pub to_name: Option<String>,
    /// Тема письма.
    pub subject: String,
    /// Ключ шаблона.
    pub template: String,
    /// Статус.
    pub status: String,
    /// Сделано попыток.
    pub attempts: i32,
    /// Предел попыток.
    pub max_attempts: i32,
    /// Идентификатор у провайдера.
    pub provider_id: Option<String>,
    /// Последняя ошибка.
    pub last_error: Option<String>,
    /// Тип бизнес-сущности-источника.
    pub aggregate_type: Option<String>,
    /// Идентификатор бизнес-сущности-источника.
    pub aggregate_id: Option<Uuid>,
    /// Когда поставлено в очередь.
    pub created_at: DateTime<Utc>,
    /// Когда провайдер принял письмо.
    pub sent_at: Option<DateTime<Utc>>,
    /// Когда доставлено.
    pub delivered_at: Option<DateTime<Utc>>,
}

/// Счётчики по статусам за окно наблюдения.
///
/// `total` — все строки окна, включая `delivery_delayed` и прочие статусы,
/// у которых нет отдельного поля: сумма полей не обязана совпадать с
/// `total`, и это нормально.
#[derive(Debug, Clone, Copy, Default, serde::Serialize)]
pub struct MailStats {
    /// Ширина окна в часах (то, по чему считали).
    pub window_hours: i64,
    /// Сколько писем ждёт отправки.
    pub queued: i64,
    /// Сколько писем сейчас отправляется.
    pub sending: i64,
    /// Сколько принято провайдером.
    pub sent: i64,
    /// Сколько доставлено.
    pub delivered: i64,
    /// Сколько отскочило.
    pub bounced: i64,
    /// Сколько жалоб на спам.
    pub complained: i64,
    /// Сколько окончательно упало.
    pub failed: i64,
    /// Сколько подавлено до отправки.
    pub suppressed: i64,
    /// Всего строк в окне.
    pub total: i64,
}

/// Фильтр журнала писем.
#[derive(Debug, Clone)]
pub struct MailFilter {
    /// Точный статус (`delivered`, …); `None` — любой.
    pub status: Option<String>,
    /// Подстрока поиска по адресу, имени и теме (без учёта регистра).
    pub query: Option<String>,
    /// Размер страницы (обрезается до `1..=200`).
    pub limit: i64,
    /// Курсор следующей страницы из предыдущего ответа.
    pub cursor: Option<String>,
}

impl Default for MailFilter {
    /// Пустой фильтр: первая страница на 50 строк, все статусы.
    fn default() -> Self {
        Self {
            status: None,
            query: None,
            limit: 50,
            cursor: None,
        }
    }
}

/// Письмо целиком: строка журнала и события доставки.
#[derive(Debug, Clone)]
pub struct MailDetail {
    /// Строка `mail_message`.
    pub mail: MailRow,
    /// События доставки в хронологическом порядке.
    pub events: Vec<MailEventRow>,
}

/// Событие доставки из `mail_event`.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct MailEventRow {
    /// Идентификатор события.
    pub id: Uuid,
    /// `svix-id` — ключ дедупликации повторов.
    pub svix_id: String,
    /// Тип события как прислал провайдер (`email.delivered`).
    pub event_type: String,
    /// Идентификатор письма у провайдера.
    pub provider_id: Option<String>,
    /// Когда событие получено.
    pub received_at: DateTime<Utc>,
}

/// Запись списка подавления.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct MailSuppressionRow {
    /// Адрес (без учёта регистра).
    pub email: String,
    /// Причина: `bounce`, `complaint` или `manual`.
    pub reason: String,
    /// Подробность: текст ошибки провайдера, кто и почему снял.
    pub detail: Option<String>,
    /// Когда адрес попал в список.
    pub created_at: DateTime<Utc>,
}

/// Репозиторий транзакционной почты.
///
/// Держит только пул; клонируется дёшево (пул — `Arc` внутри), поэтому
/// воркер клонирует его на каждую задачу супервизора.
#[derive(Debug, Clone)]
pub struct MailRepo {
    pool: PgPool,
}

impl MailRepo {
    /// Создать репозиторий на готовом пуле.
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Пул, на котором работает репозиторий (для транзакций вызывающего).
    #[must_use]
    pub fn pool(&self) -> PgPool {
        self.pool.clone()
    }

    // ---------------------------------------------------------
    // Очередь
    // ---------------------------------------------------------

    /// Поставить письмо в очередь.
    ///
    /// Возвращает `(id, was_duplicate)`. Дубликат — не ошибка: повторная
    /// бизнес-операция (ретрай, двойной клик) должна вернуть тот же `id`,
    /// а не отправить второе письмо. `idempotency_key` UNIQUE держит это
    /// на уровне БД; проверка «сначала SELECT, потом INSERT» здесь
    /// разъехалась бы под нагрузкой.
    pub async fn enqueue(&self, mail: NewMail) -> Result<(Uuid, bool), MailDbError> {
        // Две попытки: вторая нужна только если строка-конфликт успела
        // исчезнуть между INSERT и SELECT (практически невозможно).
        for _ in 0..2 {
            let id = Uuid::now_v7();
            let inserted: Option<(Uuid,)> = sqlx::query_as(
                "INSERT INTO mail_message (id, idempotency_key, template, to_email, \
                 to_name, from_email, reply_to, subject, html, text, vars, \
                 aggregate_type, aggregate_id) \
                 VALUES ($1, $2, $3, $4::citext, $5, $6, $7, $8, $9, $10, $11, $12, $13) \
                 ON CONFLICT (idempotency_key) DO NOTHING \
                 RETURNING id",
            )
            .bind(id)
            .bind(&mail.idempotency_key)
            .bind(&mail.template)
            .bind(&mail.to_email)
            .bind(&mail.to_name)
            .bind(&mail.from_email)
            .bind(&mail.reply_to)
            .bind(&mail.subject)
            .bind(&mail.html)
            .bind(&mail.text)
            .bind(&mail.vars)
            .bind(&mail.aggregate_type)
            .bind(mail.aggregate_id)
            .fetch_optional(&self.pool)
            .await?;
            if let Some((new_id,)) = inserted {
                return Ok((new_id, false));
            }
            // Конфликт: строку уже поставили (возможно, параллельно). INSERT
            // блокировался до коммита соседа, поэтому следующий SELECT её видит.
            let existing: Option<(Uuid,)> =
                sqlx::query_as("SELECT id FROM mail_message WHERE idempotency_key = $1")
                    .bind(&mail.idempotency_key)
                    .fetch_optional(&self.pool)
                    .await?;
            if let Some((existing_id,)) = existing {
                return Ok((existing_id, true));
            }
        }
        Err(MailDbError::NotFound)
    }

    /// Забрать до `limit` готовых к отправке писем и перевести их в `sending`.
    ///
    /// Один оператор `UPDATE … RETURNING` с `FOR UPDATE SKIP LOCKED`:
    /// параллельный воркер не получит те же строки. Строки с
    /// `attempts >= max_attempts` не забираются — это терминально упавшие,
    /// их место в админке, а не в очереди.
    ///
    /// Порядок — по `next_attempt_at`, затем `created_at`, затем `id`:
    /// сначала то, что ждёт дольше. `RETURNING` не гарантирует порядок,
    /// поэтому результат сортируется ещё и в Rust.
    pub async fn claim_batch(&self, limit: i64) -> Result<Vec<MailRow>, MailDbError> {
        let n = limit.clamp(0, MAX_PAGE);
        if n == 0 {
            return Ok(Vec::new());
        }
        let sql = format!(
            "UPDATE mail_message SET status = 'sending', updated_at = now() \
             WHERE id IN ( \
                 SELECT id FROM mail_message \
                 WHERE status IN ('queued', 'failed') \
                   AND next_attempt_at <= now() \
                   AND attempts < max_attempts \
                 ORDER BY next_attempt_at, created_at, id \
                 LIMIT $1 \
                 FOR UPDATE SKIP LOCKED \
             ) \
             RETURNING {MAIL_ROW_COLUMNS}"
        );
        let mut rows: Vec<MailRow> = sqlx::query_as(&sql).bind(n).fetch_all(&self.pool).await?;
        // `RETURNING` не гарантирует порядок строк: восстанавливаем его по
        // `created_at, id`, чтобы батч в отчёте воркера был детерминирован.
        rows.sort_by(|a, b| {
            a.created_at
                .cmp(&b.created_at)
                .then_with(|| a.id.cmp(&b.id))
        });
        Ok(rows)
    }

    /// Отметить письмо отправленным: `sent`, `provider_id`, `sent_at`.
    ///
    /// Гард по `sending|queued` не даёт позднему подтверждению отправки
    /// откатить уже доставленную строку. Если строку не удалось обновить
    /// (её успел обработать вебхук), это не ошибка: письмо ушло, журнал
    /// уже знает больше, чем воркер.
    pub async fn mark_sent(&self, id: Uuid, provider_id: &str) -> Result<(), MailDbError> {
        let res = sqlx::query(
            "UPDATE mail_message \
             SET status = 'sent', provider_id = $2, \
                 sent_at = COALESCE(sent_at, now()), updated_at = now() \
             WHERE id = $1 AND status IN ('sending', 'queued')",
        )
        .bind(id)
        .bind(provider_id)
        .execute(&self.pool)
        .await?;
        if res.rows_affected() == 0 {
            tracing::debug!(
                mail_id = %id,
                "mail mark_sent: строка уже не в sending/queued — статус не меняем"
            );
        }
        Ok(())
    }

    /// Учесть неудачную попытку отправки.
    ///
    /// `attempts + 1`; если попытки исчерпаны (`attempts >= max_attempts`)
    /// или `retry_after` не задан — строка становится `failed`, иначе
    /// возвращается в `queued` с `next_attempt_at = now() + retry_after`.
    /// `retry_after` считает вызывающий: политика backoff (`min(2^n·5c, 1ч)`)
    /// живёт в воркере, а репозиторий только применяет её.
    pub async fn mark_failed(
        &self,
        id: Uuid,
        error: &str,
        retry_after: Option<Duration>,
    ) -> Result<(), MailDbError> {
        let seconds = retry_after.map(|d| d.as_secs_f64());
        sqlx::query(
            "UPDATE mail_message \
             SET attempts = attempts + 1, \
                 last_error = $2, \
                 status = CASE \
                     WHEN attempts + 1 >= max_attempts OR $3::double precision IS NULL \
                     THEN 'failed' \
                     ELSE 'queued' \
                 END, \
                 next_attempt_at = CASE \
                     WHEN attempts + 1 >= max_attempts OR $3::double precision IS NULL \
                     THEN next_attempt_at \
                     ELSE now() + make_interval(secs => $3::double precision) \
                 END, \
                 updated_at = now() \
             WHERE id = $1 AND status IN ('sending', 'queued')",
        )
        .bind(id)
        .bind(error)
        .bind(seconds)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Пометить письмо подавленным (адрес в списке подавления).
    ///
    /// Письмо не отправлялось: адрес уже отскочил или на него жаловались.
    /// Причина остаётся в `last_error` — иначе в журнале непонятно, почему
    /// письмо не ушло.
    pub async fn mark_suppressed(&self, id: Uuid, reason: &str) -> Result<(), MailDbError> {
        sqlx::query(
            "UPDATE mail_message \
             SET status = 'suppressed', last_error = $2, updated_at = now() \
             WHERE id = $1 AND status IN ('queued', 'sending')",
        )
        .bind(id)
        .bind(reason)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    // ---------------------------------------------------------
    // Вебхуки доставки
    // ---------------------------------------------------------

    /// Записать событие доставки, дедуплицируя по `svix_id`.
    ///
    /// Возвращает `true`, если событие новое. Svix доставляет
    /// at-least-once, поэтому повтор — штатный сценарий: `ON CONFLICT DO
    /// NOTHING` безопасен при одновременной доставке двух копий (вторая
    /// дождётся коммита первой и не вставит строку).
    pub async fn record_event(
        &self,
        svix_id: &str,
        event_type: &str,
        provider_id: Option<&str>,
        payload: &Value,
    ) -> Result<bool, MailDbError> {
        let inserted: Option<(Uuid,)> = sqlx::query_as(
            "INSERT INTO mail_event (id, svix_id, event_type, provider_id, payload) \
             VALUES ($1, $2, $3, $4, $5) \
             ON CONFLICT (svix_id) DO NOTHING \
             RETURNING id",
        )
        .bind(Uuid::now_v7())
        .bind(svix_id)
        .bind(event_type)
        .bind(provider_id)
        .bind(payload)
        .fetch_optional(&self.pool)
        .await?;
        Ok(inserted.is_some())
    }

    /// Применить переход состояния по событию провайдера.
    ///
    /// Возвращает `true`, если переход состоялся. Принимает как полное имя
    /// события (`email.delivered`), так и короткое (`delivered`).
    ///
    /// Гард `WHERE status IN (…)` — единственная защита монотонности:
    /// поздний `sent` после `delivered` не изменит ни одной строки.
    /// `opened`/`clicked` (и незнакомые события) журналируются, но статус
    /// не трогают: открытие письма — не доставка.
    ///
    /// `bounced`/`complained` дополнительно попадают в `mail_suppression`
    /// (reason `bounce`/`complaint`): повторная отправка такому адресу
    /// портит репутацию домена отправителя.
    pub async fn apply_event(&self, provider_id: &str, kind: &str) -> Result<bool, MailDbError> {
        let kind = normalize_event_kind(kind);
        let sql = match kind {
            "sent" => {
                "UPDATE mail_message \
                 SET status = 'sent', sent_at = COALESCE(sent_at, now()), updated_at = now() \
                 WHERE provider_id = $1 AND status IN ('sending', 'queued') \
                 RETURNING id"
            }
            "delivered" => {
                "UPDATE mail_message \
                 SET status = 'delivered', delivered_at = COALESCE(delivered_at, now()), \
                     updated_at = now() \
                 WHERE provider_id = $1 AND status IN ('sent', 'delivery_delayed') \
                 RETURNING id"
            }
            "delivery_delayed" => {
                "UPDATE mail_message \
                 SET status = 'delivery_delayed', updated_at = now() \
                 WHERE provider_id = $1 AND status IN ('sent') \
                 RETURNING id"
            }
            "bounced" => {
                "WITH moved AS ( \
                     UPDATE mail_message \
                     SET status = 'bounced', updated_at = now() \
                     WHERE provider_id = $1 \
                       AND status IN ('sent', 'delivered', 'delivery_delayed') \
                     RETURNING to_email \
                 ), sup AS ( \
                     INSERT INTO mail_suppression (email, reason, detail) \
                     SELECT to_email, 'bounce', 'resend: email.bounced' FROM moved \
                     ON CONFLICT (email) DO NOTHING \
                     RETURNING email \
                 ) \
                 SELECT count(*) FROM moved"
            }
            "complained" => {
                "WITH moved AS ( \
                     UPDATE mail_message \
                     SET status = 'complained', updated_at = now() \
                     WHERE provider_id = $1 AND status IN ('sent', 'delivered') \
                     RETURNING to_email \
                 ), sup AS ( \
                     INSERT INTO mail_suppression (email, reason, detail) \
                     SELECT to_email, 'complaint', 'resend: email.complained' FROM moved \
                     ON CONFLICT (email) DO NOTHING \
                     RETURNING email \
                 ) \
                 SELECT count(*) FROM moved"
            }
            "failed" => {
                "UPDATE mail_message \
                 SET status = 'failed', \
                     last_error = COALESCE(last_error, 'provider reported email.failed'), \
                     updated_at = now() \
                 WHERE provider_id = $1 AND status IN ('queued', 'sending') \
                 RETURNING id"
            }
            "suppressed" => {
                "UPDATE mail_message \
                 SET status = 'suppressed', updated_at = now() \
                 WHERE provider_id = $1 AND status IN ('queued', 'sending') \
                 RETURNING id"
            }
            // opened / clicked / всё незнакомое: только журнал.
            _ => return Ok(false),
        };

        if kind == "bounced" || kind == "complained" {
            let (count,): (i64,) = sqlx::query_as(sql)
                .bind(provider_id)
                .fetch_one(&self.pool)
                .await?;
            return Ok(count > 0);
        }

        let applied: Option<(Uuid,)> = sqlx::query_as(sql)
            .bind(provider_id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(applied.is_some())
    }

    // ---------------------------------------------------------
    // Чтение журнала
    // ---------------------------------------------------------

    /// Страница журнала писем по `(created_at DESC, id DESC)`.
    ///
    /// Возвращает `(items, next_cursor)`; `next_cursor` — `None`, если
    /// страница последняя. Курсор непрозрачен для клиента: его структура —
    /// внутреннее дело репозитория. Битый курсор игнорируется (отдаём
    /// первую страницу), как в остальных панелях: 500 из-за испорченного
    /// query-параметра хуже, чем чуть более широкая страница.
    pub async fn list(
        &self,
        filter: MailFilter,
    ) -> Result<(Vec<MailListItem>, Option<String>), MailDbError> {
        let limit = if filter.limit <= 0 {
            50
        } else {
            filter.limit.min(MAX_PAGE)
        };
        let query = filter
            .query
            .as_deref()
            .map(str::trim)
            .filter(|q| !q.is_empty());
        let (cursor_at, cursor_id) = parse_cursor(filter.cursor.as_deref());

        // Верхняя граница keyset: (created_at, id) строго меньше последней
        // строки предыдущей страницы. Это устойчивее OFFSET: вставка новой
        // строки не сдвигает уже показанные.
        let sql = format!(
            "SELECT {MAIL_LIST_COLUMNS} FROM mail_message \
             WHERE ($1::text IS NULL OR status = $1) \
               AND ($2::text IS NULL \
                    OR to_email::text ILIKE '%' || $2 || '%' \
                    OR subject ILIKE '%' || $2 || '%' \
                    OR COALESCE(to_name, '') ILIKE '%' || $2 || '%') \
               AND ($3::timestamptz IS NULL OR (created_at, id) < ($3, $4::uuid)) \
             ORDER BY created_at DESC, id DESC \
             LIMIT $5"
        );
        let mut rows: Vec<MailListItem> = sqlx::query_as(&sql)
            .bind(filter.status.as_deref())
            .bind(query)
            .bind(cursor_at)
            .bind(cursor_id)
            .bind(limit + 1)
            .fetch_all(&self.pool)
            .await?;

        let has_more = rows.len() as i64 > limit;
        if has_more {
            rows.truncate(limit as usize);
        }
        let next_cursor = if has_more {
            rows.last()
                .map(|row| format!("{}{CURSOR_SEP}{}", row.created_at.to_rfc3339(), row.id))
        } else {
            None
        };
        Ok((rows, next_cursor))
    }

    /// Письмо целиком: строка и хронология событий доставки.
    ///
    /// События ищутся по `provider_id`: в `mail_event` нет FK на
    /// `mail_message` (мягкая ссылка, как в `audit_log`), поэтому письмо,
    /// которое ещё не принято провайдером, событий не имеет.
    pub async fn get(&self, id: Uuid) -> Result<Option<MailDetail>, MailDbError> {
        let sql = format!("SELECT {MAIL_ROW_COLUMNS} FROM mail_message WHERE id = $1");
        let mail: Option<MailRow> = sqlx::query_as(&sql)
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        let Some(mail) = mail else {
            return Ok(None);
        };
        let events: Vec<MailEventRow> = match mail.provider_id.as_deref() {
            Some(provider_id) => {
                sqlx::query_as(
                    "SELECT id, svix_id, event_type, provider_id, received_at \
                     FROM mail_event WHERE provider_id = $1 \
                     ORDER BY received_at ASC, id ASC",
                )
                .bind(provider_id)
                .fetch_all(&self.pool)
                .await?
            }
            None => Vec::new(),
        };
        Ok(Some(MailDetail { mail, events }))
    }

    /// Поставить письмо в очередь повторно.
    ///
    /// Создаёт **новую** строку с тем же содержимым и ключом
    /// `<original_key>:r<n>`: исходная остаётся историей доставки, а
    /// «переотправка» не переписывает прошлое. Номер подобран
    /// последовательно, конфликт ключа (одновременный resend из двух
    /// админских вкладок) пропускается: `ON CONFLICT DO NOTHING`.
    pub async fn requeue(&self, id: Uuid) -> Result<Uuid, MailDbError> {
        let original: Option<(String,)> =
            sqlx::query_as("SELECT idempotency_key FROM mail_message WHERE id = $1")
                .bind(id)
                .fetch_optional(&self.pool)
                .await?;
        let Some((base_key,)) = original else {
            return Err(MailDbError::NotFound);
        };

        for n in 1..=REQUEUE_TRIES {
            let new_id = Uuid::now_v7();
            let key = format!("{base_key}:r{n}");
            let inserted: Option<(Uuid,)> = sqlx::query_as(
                "INSERT INTO mail_message (\
                     id, idempotency_key, template, to_email, to_name, from_email, \
                     reply_to, subject, html, text, vars, aggregate_type, aggregate_id, \
                     status, attempts, next_attempt_at) \
                 SELECT $2, $3, template, to_email, to_name, from_email, reply_to, \
                        subject, html, text, vars, aggregate_type, aggregate_id, \
                        'queued', 0, now() \
                 FROM mail_message WHERE id = $1 \
                 ON CONFLICT (idempotency_key) DO NOTHING \
                 RETURNING id",
            )
            .bind(id)
            .bind(new_id)
            .bind(&key)
            .fetch_optional(&self.pool)
            .await?;
            if let Some((new_id,)) = inserted {
                return Ok(new_id);
            }
        }
        Err(MailDbError::RequeueExhausted)
    }

    /// Счётчики по статусам за последние `window_hours` часов.
    ///
    /// Окно — по `created_at`: «сколько писем за сутки доставлено» — вопрос
    /// про письма, созданные за сутки, а не про строки, обновлённые сейчас.
    pub async fn stats(&self, window_hours: i64) -> Result<MailStats, MailDbError> {
        let hours = window_hours.clamp(1, 24 * 365);
        let row: MailStatsRow = sqlx::query_as(
            "SELECT \
                 count(*) FILTER (WHERE status = 'queued') AS queued, \
                 count(*) FILTER (WHERE status = 'sending') AS sending, \
                 count(*) FILTER (WHERE status = 'sent') AS sent, \
                 count(*) FILTER (WHERE status = 'delivered') AS delivered, \
                 count(*) FILTER (WHERE status = 'bounced') AS bounced, \
                 count(*) FILTER (WHERE status = 'complained') AS complained, \
                 count(*) FILTER (WHERE status = 'failed') AS failed, \
                 count(*) FILTER (WHERE status = 'suppressed') AS suppressed, \
                 count(*) AS total \
             FROM mail_message \
             WHERE created_at >= now() - make_interval(hours => $1::int)",
        )
        .bind(hours as i32)
        .fetch_one(&self.pool)
        .await?;
        Ok(MailStats {
            window_hours: hours,
            queued: row.queued,
            sending: row.sending,
            sent: row.sent,
            delivered: row.delivered,
            bounced: row.bounced,
            complained: row.complained,
            failed: row.failed,
            suppressed: row.suppressed,
            total: row.total,
        })
    }

    // ---------------------------------------------------------
    // Подавления
    // ---------------------------------------------------------

    /// Адрес в списке подавления (без учёта регистра).
    ///
    /// Воркер спрашивает это перед отправкой. Ошибка чтения трактуется им
    /// как «не знаем» → отправляем: потерять письмо хуже, чем отправить.
    pub async fn is_suppressed(&self, email: &str) -> Result<bool, MailDbError> {
        let (exists,): (bool,) = sqlx::query_as(
            "SELECT EXISTS (SELECT 1 FROM mail_suppression WHERE email = $1::citext)",
        )
        .bind(email.trim())
        .fetch_one(&self.pool)
        .await?;
        Ok(exists)
    }

    /// Добавить адрес в список подавления (или обновить причину).
    ///
    /// `created_at` исходной записи не переписывается: важно, когда адрес
    /// попал в список впервые. `detail` обновляется, только если новый не
    /// пуст, — иначе повторный bounce стёр бы текст первой ошибки провайдера.
    pub async fn add_suppression(
        &self,
        email: &str,
        reason: &str,
        detail: Option<&str>,
    ) -> Result<(), MailDbError> {
        sqlx::query(
            "INSERT INTO mail_suppression (email, reason, detail) \
             VALUES ($1::citext, $2, $3) \
             ON CONFLICT (email) DO UPDATE \
             SET reason = EXCLUDED.reason, \
                 detail = COALESCE(EXCLUDED.detail, mail_suppression.detail)",
        )
        .bind(email.trim())
        .bind(reason)
        .bind(detail)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Список подавления, свежие сверху.
    pub async fn list_suppressions(
        &self,
        limit: i64,
    ) -> Result<Vec<MailSuppressionRow>, MailDbError> {
        let n = if limit <= 0 { 100 } else { limit.min(500) };
        let rows: Vec<MailSuppressionRow> = sqlx::query_as(
            "SELECT email::text AS email, reason, detail, created_at \
             FROM mail_suppression ORDER BY created_at DESC, email ASC LIMIT $1",
        )
        .bind(n)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Снять подавление вручную.
    ///
    /// Возвращает `false`, если адреса в списке не было: интерфейсу нужно
    /// отличать «сняли» от «нечего снимать».
    pub async fn remove_suppression(&self, email: &str) -> Result<bool, MailDbError> {
        let res = sqlx::query("DELETE FROM mail_suppression WHERE email = $1::citext")
            .bind(email.trim())
            .execute(&self.pool)
            .await?;
        Ok(res.rows_affected() > 0)
    }
}

/// Строка результата агрегатного запроса [`MailRepo::stats`].
#[derive(Debug, sqlx::FromRow)]
struct MailStatsRow {
    queued: i64,
    sending: i64,
    sent: i64,
    delivered: i64,
    bounced: i64,
    complained: i64,
    failed: i64,
    suppressed: i64,
    total: i64,
}

/// Привести имя события к короткому виду: `email.sent` → `sent`.
fn normalize_event_kind(kind: &str) -> &str {
    kind.trim()
        .strip_prefix("email.")
        .unwrap_or_else(|| kind.trim())
}

/// Разобрать курсор `"<RFC3339>\x1f<uuid>"`.
///
/// Любая порча → `(None, None)`: отдаём первую страницу, а не 500.
fn parse_cursor(cursor: Option<&str>) -> (Option<DateTime<Utc>>, Option<Uuid>) {
    let Some(cursor) = cursor else {
        return (None, None);
    };
    let Some((ts, id)) = cursor.rsplit_once(CURSOR_SEP) else {
        return (None, None);
    };
    let created_at = DateTime::parse_from_rfc3339(ts.trim())
        .ok()
        .map(|dt| dt.with_timezone(&Utc));
    let id = Uuid::parse_str(id.trim()).ok();
    match (created_at, id) {
        (Some(created_at), Some(id)) => (Some(created_at), Some(id)),
        _ => (None, None),
    }
}
