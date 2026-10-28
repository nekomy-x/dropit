//! Транзакционная почта DROP IT: Resend как провайдер, Svix-подписанные
//! вебхуки, шаблоны писем.
//!
//! Крейт отвечает ровно за три вещи: отправить письмо, проверить, что
//! вебхук доставки пришёл от провайдера, и собрать письмо из шаблона.
//! Очередь, статусы и подавления живут в БД (`dropit_infra_db::mail`),
//! диспетчер — в воркере; здесь нет ни пула, ни политики повторов.
//!
//! # Деградация вместо падения
//!
//! Почта — вспомогательный канал: без неё продукт работает, но молча
//! теряет письма. Поэтому:
//!
//! * нет `RESEND_API_KEY` → [`from_env`] возвращает `None`, вызывающий
//!   берёт [`NoopMail`]: письма не уходят в сеть, в лог пишется строка,
//!   наружу возвращается синтетическая квитанция `noop-<uuid>`;
//! * нет `RESEND_WEBHOOK_SECRET` → [`webhook_verifier_from_env`] отдаёт
//!   [`MailError::Config`], и HTTP-слой обязан ответить `503`, а не
//!   принимать неподписанные тела: «доверять, потому что не настроено» —
//!   это дыра, а не деградация;
//! * ключ задан, а `MAIL_FROM` нет → `warn!` и `None`: отправитель без
//!   адреса всё равно получил бы 422 от провайдера.
//!
//! # Секреты
//!
//! Ключ API и signing secret не попадают ни в `Debug` (ручные реализации
//! печатают `***`), ни в сообщения ошибок: тела ответов провайдера
//! проходят через `sanitize_body` — короткий префикс, обрезанный по
//! границе символа.
//!
//! # Типы
//!
//! * Отправка: [`MailAddress`], [`OutgoingMail`], [`SendReceipt`],
//!   [`MailSender`], [`NoopMail`], [`ResendSender`].
//! * Конфигурация: [`ResendConfig`], [`MailConfig`], [`from_env`].
//! * Вебхуки: [`WebhookHeaders`], [`WebhookVerifier`],
//!   [`webhook_verifier_from_env`], [`parse_event`], [`MailEventKind`],
//!   [`MailWebhookEvent`].
//! * Шаблоны: [`TemplateKey`], [`render`], [`escape_html`],
//!   [`RenderedMail`].
//!
//! # Идемпотентность
//!
//! `mail_message.idempotency_key` уходит провайдеру заголовком
//! `Idempotency-Key`, поэтому повторная отправка той же строки не создаёт
//! второе письмо. Пустой ключ или ключ длиннее
//! [`MAX_IDEMPOTENCY_KEY_LEN`] — [`MailError::Config`] до сетевого вызова.

#![deny(missing_docs)]

mod error;
mod event;
mod resend;
mod sender;
mod template;
mod transport;
mod webhook;

#[cfg(test)]
mod tests;

pub use error::MailError;
pub use event::{parse_event, MailEventKind, MailWebhookEvent};
pub use resend::{
    from_env, MailConfig, ResendConfig, ResendSender, DEFAULT_API_BASE, MAX_IDEMPOTENCY_KEY_LEN,
    PROVIDER,
};
pub use sender::{MailAddress, MailSender, NoopMail, OutgoingMail, SendReceipt};
pub use template::{escape_html, render, RenderedMail, TemplateKey};
pub use webhook::{
    webhook_verifier_from_env, WebhookHeaders, WebhookVerifier, WEBHOOK_TOLERANCE_SECS,
    WHSEC_PREFIX,
};
