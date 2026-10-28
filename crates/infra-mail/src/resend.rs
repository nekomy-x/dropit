//! Отправитель через Resend HTTP API (`POST /emails`) и конфигурация из
//! переменных окружения.
//!
//! Поток одного письма:
//!
//! 1. Тело собирается из [`OutgoingMail`]; `reply_to` не отправляется,
//!    если адреса ответа нет.
//! 2. Запрос уходит с `Authorization: Bearer <ключ>`,
//!    `Idempotency-Key: <mail.idempotency_key>` и
//!    `User-Agent: dropit-infra-mail`.
//! 3. Ответ разбирается по статусу: 200 с `{"id": …}` — успех, остальное —
//!    конкретный вариант [`MailError`], чтобы воркер знал, повторять ли.
//!
//! Ключ API никогда не логируется и не попадает в `Debug`-вывод: ручной
//! `Debug` для [`ResendConfig`] печатает `***` вместо ключа.

use std::time::Duration;

use crate::error::MailError;
use crate::sender::{MailAddress, MailSender, OutgoingMail, SendReceipt};
use crate::transport::{
    sanitize_body, HttpRequest, ReqwestTransport, SharedTransport, DEFAULT_REQUEST_TIMEOUT_SECS,
};

/// База API Resend по умолчанию.
pub const DEFAULT_API_BASE: &str = "https://api.resend.com";

/// Имя провайдера, попадающее в `mail_message.provider` и в квитанцию.
pub const PROVIDER: &str = "resend";

/// Максимальная длина `Idempotency-Key` в байтах (ограничение провайдера).
pub const MAX_IDEMPOTENCY_KEY_LEN: usize = 256;

/// Конфигурация отправителя Resend.
#[derive(Clone)]
pub struct ResendConfig {
    /// API-ключ (`re_…`). Секрет: не логируется.
    pub api_key: String,
    /// База API без завершающего `/` (sandbox/stub подменяет её).
    pub api_base: String,
    /// Отправитель по умолчанию (`DROP IT <no-reply@…>`).
    pub from: MailAddress,
    /// Адрес для ответа, если он отличается от отправителя.
    pub reply_to: Option<MailAddress>,
}

impl std::fmt::Debug for ResendConfig {
    /// Ручной `Debug`: ключ API печатается как `***`, чтобы случайный
    /// `dbg!`/`{:?}` в логе не утёк секретом.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResendConfig")
            .field("api_key", &"***")
            .field("api_base", &self.api_base)
            .field("from", &self.from)
            .field("reply_to", &self.reply_to)
            .finish()
    }
}

impl ResendConfig {
    /// Конфигурация с базой API по умолчанию.
    #[must_use]
    pub fn new(api_key: impl Into<String>, from: MailAddress) -> Self {
        Self {
            api_key: api_key.into(),
            api_base: DEFAULT_API_BASE.to_string(),
            from,
            reply_to: None,
        }
    }
}

/// Отправитель писем через Resend.
pub struct ResendSender {
    config: ResendConfig,
    http: SharedTransport,
}

impl std::fmt::Debug for ResendSender {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResendSender")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl ResendSender {
    /// Собрать отправитель с реальным HTTP-клиентом.
    ///
    /// Ошибка — только конфигурационная (пустой ключ, пустая база API):
    /// сеть здесь не трогается, падать на старте нечему.
    pub fn new(config: ResendConfig) -> Result<Self, MailError> {
        let timeout = Duration::from_secs(DEFAULT_REQUEST_TIMEOUT_SECS);
        let transport = ReqwestTransport::new(timeout)?;
        Self::build(config, std::sync::Arc::new(transport))
    }

    /// Конструктор с внедрённым транспортом (unit-тесты без сети).
    #[cfg(test)]
    pub(crate) fn with_transport(config: ResendConfig, transport: SharedTransport) -> Self {
        Self {
            config,
            http: transport,
        }
    }

    /// Действующая конфигурация.
    #[must_use]
    pub fn config(&self) -> &ResendConfig {
        &self.config
    }

    /// Общая сборка: нормализация и проверка конфигурации.
    fn build(config: ResendConfig, http: SharedTransport) -> Result<Self, MailError> {
        let api_key = config.api_key.trim().to_string();
        if api_key.is_empty() {
            return Err(MailError::Config(
                "RESEND_API_KEY is empty — use NoopMail instead".into(),
            ));
        }
        let api_base = normalize_base(&config.api_base);
        if api_base.is_empty() {
            return Err(MailError::Config("RESEND_API_URL is empty".into()));
        }
        if !api_base.starts_with("http://") && !api_base.starts_with("https://") {
            return Err(MailError::Config(format!(
                "RESEND_API_URL must start with http:// or https://, got {api_base:?}"
            )));
        }
        Ok(Self {
            config: ResendConfig {
                api_key,
                api_base,
                from: config.from,
                reply_to: config.reply_to,
            },
            http,
        })
    }

    /// Собрать тело запроса `POST /emails`.
    fn build_body(&self, mail: &OutgoingMail) -> Result<Vec<u8>, MailError> {
        let mut body = serde_json::Map::new();
        body.insert(
            "from".to_string(),
            serde_json::Value::String(mail.from.to_string()),
        );
        body.insert(
            "to".to_string(),
            serde_json::Value::Array(vec![serde_json::Value::String(mail.to.to_string())]),
        );
        body.insert(
            "subject".to_string(),
            serde_json::Value::String(mail.subject.clone()),
        );
        body.insert(
            "html".to_string(),
            serde_json::Value::String(mail.html.clone()),
        );
        body.insert(
            "text".to_string(),
            serde_json::Value::String(mail.text.clone()),
        );
        if let Some(reply_to) = &mail.reply_to {
            body.insert(
                "reply_to".to_string(),
                serde_json::Value::Array(vec![serde_json::Value::String(reply_to.to_string())]),
            );
        }
        serde_json::to_vec(&serde_json::Value::Object(body))
            .map_err(|e| MailError::Decode(format!("encode resend request body: {e}")))
    }

    /// Разобрать ответ провайдера в квитанцию или ошибку.
    fn interpret(
        &self,
        status: u16,
        headers: &[(String, String)],
        body: &[u8],
    ) -> Result<SendReceipt, MailError> {
        match status {
            200 => {
                let parsed: serde_json::Value = serde_json::from_slice(body).map_err(|e| {
                    MailError::Decode(format!("resend response is not valid json: {e}"))
                })?;
                let id = parsed
                    .get("id")
                    .and_then(serde_json::Value::as_str)
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .ok_or_else(|| {
                        MailError::Decode("resend response has no non-empty `id`".into())
                    })?;
                Ok(SendReceipt {
                    provider_id: id.to_string(),
                    provider: PROVIDER,
                })
            }
            // Контракт, а не догадка: провайдер обязан ответить 200 с `id`.
            // Любой другой 2xx (201/202/204) означает, что API изменился, и
            // делать вид, что письмо отправлено, нельзя.
            201..=299 => Err(MailError::Decode(format!(
                "unexpected resend status {status}: expected 200 with {{\"id\"}}"
            ))),
            401 | 403 => Err(MailError::Unauthorized(sanitize_body(
                "resend /emails",
                body,
            ))),
            429 => Err(MailError::RateLimited {
                retry_after_secs: retry_after_secs(headers),
            }),
            400 | 404 | 409 | 422 => {
                Err(MailError::Rejected(sanitize_body("resend /emails", body)))
            }
            500..=599 => Err(MailError::Unavailable(sanitize_body(
                "resend /emails",
                body,
            ))),
            // Прочие 4xx (402, 405, 413, …) — тоже «не повторять»: ответ
            // детерминирован, повтор упрётся в ту же стену. Отдельного
            // варианта контракт не заводит.
            other => Err(MailError::Rejected(sanitize_body(
                &format!("resend /emails status {other}"),
                body,
            ))),
        }
    }
}

#[async_trait::async_trait]
impl MailSender for ResendSender {
    async fn send(&self, mail: &OutgoingMail) -> Result<SendReceipt, MailError> {
        let key = mail.idempotency_key.trim();
        if key.is_empty() || key.len() > MAX_IDEMPOTENCY_KEY_LEN {
            return Err(MailError::Config(format!(
                "idempotency key must be 1..={MAX_IDEMPOTENCY_KEY_LEN} bytes, got {}",
                key.len()
            )));
        }
        if mail.to.email.trim().is_empty() {
            return Err(MailError::Config("recipient e-mail is empty".into()));
        }

        let body = self.build_body(mail)?;
        let request = HttpRequest {
            url: format!("{}/emails", self.config.api_base),
            headers: vec![
                (
                    "Authorization".to_string(),
                    format!("Bearer {}", self.config.api_key),
                ),
                ("Content-Type".to_string(), "application/json".to_string()),
                ("User-Agent".to_string(), "dropit-infra-mail".to_string()),
                ("Idempotency-Key".to_string(), key.to_string()),
            ],
            body,
        };

        // Транспорт уже свёл сетевые сбои к Timeout/Unavailable.
        let response = self.http.execute(request).await?;
        self.interpret(response.status, &response.headers, &response.body)
    }
}

/// Нормализовать базу API: trim и без завершающих `/`.
fn normalize_base(raw: &str) -> String {
    raw.trim().trim_end_matches('/').to_string()
}

/// Достать подсказку `Retry-After` из заголовков ответа.
///
/// Формат — секунды (обычный случай Resend) либо HTTP-date (best effort,
/// RFC 2822/1123). Нечитаемое значение трактуется как «провайдер не
/// сказал»: `None` означает, что воркер применит свой backoff.
fn retry_after_secs(headers: &[(String, String)]) -> Option<i64> {
    let raw = headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("retry-after"))
        .map(|(_, value)| value.trim())?;
    if raw.is_empty() {
        return None;
    }
    if let Ok(secs) = raw.parse::<i64>() {
        return Some(secs.max(0));
    }
    let at = chrono::DateTime::parse_from_rfc2822(raw).ok()?;
    let now = chrono::Utc::now().timestamp();
    Some(at.timestamp().saturating_sub(now).max(0))
}

/// Полная конфигурация почты: транспорт + секрет вебхука.
#[derive(Clone)]
pub struct MailConfig {
    /// Конфигурация отправителя Resend.
    pub resend: ResendConfig,
    /// Секрет подписи вебхуков (`whsec_…`). `None` — вебхук выключен.
    pub webhook_secret: Option<String>,
}

impl std::fmt::Debug for MailConfig {
    /// Ручной `Debug`: секрет вебхука не печатается.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MailConfig")
            .field("resend", &self.resend)
            .field(
                "webhook_secret",
                &self.webhook_secret.as_ref().map(|_| "***"),
            )
            .finish()
    }
}

/// Собрать конфигурацию почты из переменных окружения.
///
/// Возвращает `None`, если почта не настроена: вызывающий обязан
/// деградировать до [`NoopMail`](crate::NoopMail), а не падать. Ни одна
/// ветка не паникует и не логирует секреты.
///
/// Переменные: `RESEND_API_KEY`, `RESEND_API_URL`, `MAIL_FROM`,
/// `MAIL_REPLY_TO`, `RESEND_WEBHOOK_SECRET`. `MAIL_APP_BASE_URL` читает
/// вызывающий — рендер шаблонов окружение не смотрит (контракт §3).
pub fn from_env() -> Option<MailConfig> {
    from_env_via(|key| {
        std::env::var(key)
            .ok()
            .filter(|value| !value.trim().is_empty())
    })
}

/// Внутренняя, тестируемая версия [`from_env`] с абстрактным источником
/// значений.
///
/// Правила деградации (контракт §6):
/// * нет `RESEND_API_KEY` → `None` (тихо: это штатный local-режим);
/// * ключ есть, а `MAIL_FROM` нет или он не разбирается → `warn!` и
///   `None`, потому что отправитель без адреса — гарантированный 422;
/// * `MAIL_REPLY_TO` не разбирается → `warn!` и поле игнорируется:
///   адрес ответа не критичен для доставки.
pub(crate) fn from_env_via<F>(get: F) -> Option<MailConfig>
where
    F: Fn(&str) -> Option<String>,
{
    // Нет ключа — штатный local-режим: вызывающий возьмёт NoopMail.
    let api_key = get("RESEND_API_KEY")?.trim().to_string();
    if api_key.is_empty() {
        return None;
    }

    let api_base = get("RESEND_API_URL")
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| DEFAULT_API_BASE.to_string());

    let from = match get("MAIL_FROM").map(|v| MailAddress::parse(&v)) {
        Some(Ok(from)) => from,
        Some(Err(e)) => {
            tracing::warn!(
                target: "dropit.mail.config",
                error = %e,
                "MAIL_FROM не разобран: почта деградирует до NoopMail"
            );
            return None;
        }
        None => {
            tracing::warn!(
                target: "dropit.mail.config",
                "RESEND_API_KEY задан, а MAIL_FROM — нет: почта деградирует до NoopMail"
            );
            return None;
        }
    };

    let reply_to = match get("MAIL_REPLY_TO") {
        Some(raw) => match MailAddress::parse(&raw) {
            Ok(addr) => Some(addr),
            Err(e) => {
                tracing::warn!(
                    target: "dropit.mail.config",
                    error = %e,
                    "MAIL_REPLY_TO не разобран: адрес ответа игнорируется"
                );
                None
            }
        },
        None => None,
    };

    let webhook_secret = get("RESEND_WEBHOOK_SECRET")
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());

    Some(MailConfig {
        resend: ResendConfig {
            api_key,
            api_base,
            from,
            reply_to,
        },
        webhook_secret,
    })
}
