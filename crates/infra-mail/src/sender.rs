//! Контракт отправки: адрес, письмо, квитанция, трейт [`MailSender`] и
//! заглушка [`NoopMail`].

use crate::MailError;

/// Почтовый адрес с необязательным отображаемым именем.
///
/// Хранится в разобранном виде, а не строкой, чтобы форматирование для
/// провайдера (`Display`) и разбор входящей конфигурации (`parse`) были
/// одним правилом. `Display` — обратная операция к [`MailAddress::parse`]:
/// `parse(to_string())` возвращает тот же адрес.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MailAddress {
    /// Собственно адрес (`a@b.c`), без имени и без угловых скобок.
    pub email: String,
    /// Отображаемое имя (`DROP IT`, `Иван`), если задано.
    pub name: Option<String>,
}

impl MailAddress {
    /// Адрес без отображаемого имени.
    #[must_use]
    pub fn new(email: impl Into<String>) -> Self {
        Self {
            email: email.into(),
            name: None,
        }
    }

    /// Разобрать `"Имя <a@b.c>"` или `"a@b.c"`.
    ///
    /// Угловые скобки ищутся с конца: имя может само содержать `<`, и
    /// тогда `Display` → `parse` остаётся круговым. Кавычки вокруг имени
    /// снимаются — так адрес переживает ручной ввод в конфиге.
    ///
    /// Проверка намеренно мягкая (не пусто, ровно один `@`, непустые
    /// части, без пробелов и скобок): каноническая валидация домена —
    /// дело провайдера, а излишне строгий парсер ломал бы отправку
    /// тестовых адресов вроде `a@b`.
    pub fn parse(raw: &str) -> Result<Self, MailError> {
        let raw = raw.trim();
        if raw.is_empty() {
            return Err(MailError::Config("mail address is empty".into()));
        }
        let (name, email) = match raw.rfind('<') {
            Some(open) if raw.ends_with('>') => {
                let name = raw[..open].trim();
                let email = raw[open + 1..raw.len() - 1].trim();
                let name = strip_quotes(name);
                (name.map(str::to_string), email.to_string())
            }
            _ => (None, raw.to_string()),
        };
        validate_email(&email)?;
        Ok(Self {
            email,
            name: name.filter(|n| !n.is_empty()),
        })
    }

    /// Отформатировать для провайдера: `"Имя <a@b.c>"` либо `"a@b.c"`.
    fn fmt_for_header(&self) -> String {
        match &self.name {
            Some(name) if !name.trim().is_empty() => format!("{name} <{}>", self.email),
            _ => self.email.clone(),
        }
    }
}

impl std::fmt::Display for MailAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.fmt_for_header())
    }
}

/// Снять обрамляющие двойные кавычки у отображаемого имени.
fn strip_quotes(name: &str) -> Option<&str> {
    let trimmed = name.trim();
    if trimmed.len() >= 2 && trimmed.starts_with('"') && trimmed.ends_with('"') {
        let inner = &trimmed[1..trimmed.len() - 1];
        return Some(inner);
    }
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

/// Мягкая проверка адреса: одна `@`, непустые части, без пробелов/скобок.
fn validate_email(email: &str) -> Result<(), MailError> {
    if email.is_empty() {
        return Err(MailError::Config("mail address is empty".into()));
    }
    if email.chars().any(|c| c.is_whitespace()) {
        return Err(MailError::Config(format!(
            "mail address contains whitespace: {email:?}"
        )));
    }
    if email.contains('<') || email.contains('>') {
        return Err(MailError::Config(format!(
            "mail address contains angle brackets: {email:?}"
        )));
    }
    let mut parts = email.split('@');
    let (Some(local), Some(domain), None) = (parts.next(), parts.next(), parts.next()) else {
        return Err(MailError::Config(format!(
            "mail address must contain exactly one '@': {email:?}"
        )));
    };
    if local.is_empty() || domain.is_empty() {
        return Err(MailError::Config(format!(
            "mail address has empty local or domain part: {email:?}"
        )));
    }
    Ok(())
}

/// Готовое к отправке письмо.
///
/// Собирается вызывающим (воркером из строки `mail_message`) и передаётся
/// в [`MailSender`]. Провайдер-специфичных полей здесь нет: `id` и
/// `idempotency_key` — общие для всех отправителей, а `provider_id`
/// появляется только в [`SendReceipt`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutgoingMail {
    /// Идентификатор строки `mail_message` (UUIDv7).
    pub id: uuid::Uuid,
    /// Ключ идемпотентности; он же — заголовок `Idempotency-Key`.
    pub idempotency_key: String,
    /// Отправитель.
    pub from: MailAddress,
    /// Получатель.
    pub to: MailAddress,
    /// Адрес для ответа, если он отличается от отправителя.
    pub reply_to: Option<MailAddress>,
    /// Тема письма.
    pub subject: String,
    /// HTML-версия.
    pub html: String,
    /// Текстовая версия (обязательна: она спасает от спам-фильтров).
    pub text: String,
}

/// Квитанция провайдера об успешной отправке.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendReceipt {
    /// Идентификатор письма у провайдера (`data.id` из ответа Resend).
    ///
    /// По нему вебхуки доставки сшиваются со строкой `mail_message`.
    pub provider_id: String,
    /// Имя провайдера — попадает в колонку `mail_message.provider`.
    pub provider: &'static str,
}

/// Асинхронный отправитель писем.
///
/// Реализации: [`NoopMail`] (деградация и тесты) и
/// [`ResendSender`](crate::ResendSender).
///
/// Контракт: ошибка отправки одного письма возвращается как [`MailError`]
/// и никогда не «глотается» — воркер решает по варианту ошибки, повторять
/// ли отправку.
#[async_trait::async_trait]
pub trait MailSender: Send + Sync {
    /// Отправить письмо и вернуть квитанцию провайдера.
    async fn send(&self, mail: &OutgoingMail) -> Result<SendReceipt, MailError>;

    /// `true`, если транспорт действительно выходит из процесса.
    ///
    /// [`NoopMail`] возвращает `false`: воркер по этому признаку
    /// понимает, что «успешная отправка» — синтетическая, и не должен
    /// считать это доказательством работоспособности Resend.
    fn is_live(&self) -> bool {
        true
    }
}

/// Заглушка-отправитель: в сеть не ходит, возвращает синтетическую
/// квитанцию `noop-<uuid>`.
///
/// Применяется как безопасная деградация, когда `RESEND_API_KEY` не задан:
/// письма «уходят» в лог, строки в БД помечаются отправленными, продукт
/// продолжает работать. Второе назначение — тесты диспетчера.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopMail;

#[async_trait::async_trait]
impl MailSender for NoopMail {
    async fn send(&self, mail: &OutgoingMail) -> Result<SendReceipt, MailError> {
        let provider_id = format!("noop-{}", uuid::Uuid::now_v7());
        tracing::info!(
            target: "dropit.mail.noop",
            mail_id = %mail.id,
            to = %mail.to.email,
            subject = %mail.subject,
            provider_id = %provider_id,
            "RESEND_API_KEY не задан: письмо не отправлено (noop), возвращена синтетическая квитанция"
        );
        Ok(SendReceipt {
            provider_id,
            provider: "noop",
        })
    }

    fn is_live(&self) -> bool {
        false
    }
}
