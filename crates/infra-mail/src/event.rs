//! Разбор вебхук-событий Resend.
//!
//! Разбор отделён от проверки подписи: сначала HTTP-слой проверяет HMAC по
//! сырым байтам ([`crate::WebhookVerifier`]), и только потом эти байты
//! попадают сюда. Иначе недоверенное тело влияло бы на разбор.
//!
//! Главное правило — **прямая совместимость**: незнакомый `type` даёт
//! [`MailEventKind::Other`], а не ошибку. Resend добавляет события
//! (открытия, клики, будущие статусы), и вебхук не должен отвечать 400 на
//! то, чего мы ещё не знаем: событие всё равно пишется в `mail_event`, а
//! переход статуса просто не находится.
//!
//! Ошибка [`MailError::Decode`] — только когда тело не JSON-объект или в
//! нём нет строкового `type`.

use crate::error::MailError;

/// Вид события доставки.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MailEventKind {
    /// Письмо принято провайдером.
    Sent,
    /// Письмо доставлено получателю.
    Delivered,
    /// Доставка задержана (провайдер ещё пробует).
    DeliveryDelayed,
    /// Письмо отскочило.
    Bounced,
    /// Получатель пожаловался на спам.
    Complained,
    /// Письмо открыто (трекинг).
    Opened,
    /// Ссылка в письме открыта (трекинг).
    Clicked,
    /// Отправка не удалась на стороне провайдера.
    Failed,
    /// Адрес в списке подавления, письмо не отправлялось.
    Suppressed,
    /// Незнакомый тип: пишем в журнал, статус не трогаем.
    Other,
}

impl MailEventKind {
    /// Строковое имя события в терминах провайдера (`email.sent`, …).
    ///
    /// Для [`MailEventKind::Other`] возвращается `"other"`: исходный
    /// незнакомый тип остаётся в `raw["type"]` и в `mail_event.payload`.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Sent => "email.sent",
            Self::Delivered => "email.delivered",
            Self::DeliveryDelayed => "email.delivery_delayed",
            Self::Bounced => "email.bounced",
            Self::Complained => "email.complained",
            Self::Opened => "email.opened",
            Self::Clicked => "email.clicked",
            Self::Failed => "email.failed",
            Self::Suppressed => "email.suppressed",
            Self::Other => "other",
        }
    }

    /// Разобрать строку события; незнакомое — [`MailEventKind::Other`].
    fn parse(raw: &str) -> Self {
        match raw {
            "email.sent" => Self::Sent,
            "email.delivered" => Self::Delivered,
            "email.delivery_delayed" => Self::DeliveryDelayed,
            "email.bounced" => Self::Bounced,
            "email.complained" => Self::Complained,
            "email.opened" => Self::Opened,
            "email.clicked" => Self::Clicked,
            "email.failed" => Self::Failed,
            "email.suppressed" => Self::Suppressed,
            _ => Self::Other,
        }
    }
}

/// Разобранное событие вебхука.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MailWebhookEvent {
    /// Вид события.
    pub kind: MailEventKind,
    /// `data.email_id` — идентификатор письма у провайдера.
    pub provider_id: Option<String>,
    /// `data.to[0]` — адрес получателя (для подавления).
    pub email: Option<String>,
    /// `data.bounce.type`: `"hard"` | `"soft"` | `"undetermined"`.
    pub bounce_kind: Option<String>,
    /// Причина: `data.bounce.message` либо `data.failed.reason`.
    pub reason: Option<String>,
    /// Исходный payload целиком — пишется в `mail_event.payload` и
    /// позволяет разобрать то, что появится у провайдера позже.
    pub raw: serde_json::Value,
}

/// Разобрать тело вебхука.
///
/// Тело должно быть JSON-объектом со строковым `type`. Отсутствие
/// остальных полей — норма: не все события несут `email_id` и `to`.
pub fn parse_event(raw: &[u8]) -> Result<MailWebhookEvent, MailError> {
    let value: serde_json::Value = serde_json::from_slice(raw)
        .map_err(|e| MailError::Decode(format!("webhook body is not valid json: {e}")))?;

    let event_type = value
        .get("type")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| MailError::Decode("webhook body has no string `type`".into()))?;

    let data = value.get("data");
    let provider_id = data
        .and_then(|d| d.get("email_id"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let email = data
        .and_then(|d| d.get("to"))
        .and_then(serde_json::Value::as_array)
        .and_then(|to| to.first())
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let bounce_kind = data
        .and_then(|d| d.get("bounce"))
        .and_then(|b| b.get("type"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let reason = data
        .and_then(|d| d.get("bounce"))
        .and_then(|b| b.get("message"))
        .and_then(serde_json::Value::as_str)
        .or_else(|| {
            data.and_then(|d| d.get("failed"))
                .and_then(|f| f.get("reason"))
                .and_then(serde_json::Value::as_str)
        })
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    Ok(MailWebhookEvent {
        kind: MailEventKind::parse(event_type),
        provider_id,
        email,
        bounce_kind,
        reason,
        raw: value,
    })
}
