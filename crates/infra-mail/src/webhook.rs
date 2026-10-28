//! Проверка подписи вебхуков Resend (схема Svix).
//!
//! Заголовки: `svix-id`, `svix-timestamp`, `svix-signature` со значениями
//! вида `v1,<base64>` (несколько версий — через пробел). Секрет приходит
//! как `whsec_<base64>`; метка `whsec_` снимается, остаток декодируется в
//! байты ключа HMAC.
//!
//! Подписывается **сырое тело** запроса, а не разобранный JSON:
//! `"{svix_id}.{svix_timestamp}.{raw_body}"`. Поэтому вебхук обязан
//! проверяться до десериализации — иначе любая пересборка JSON меняет
//! подпись.
//!
//! Порядок проверок намеренно такой: формат → допуск по времени → HMAC
//! (постоянное время, `hmac::Mac::verify_slice`). Дрейф часов — отдельная
//! ошибка [`MailError::SignatureExpired`], чтобы в логах отличать replay
//! от подделки.

use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD};
use base64::Engine;
use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::error::MailError;

/// Допуск по timestamp вебхука (секунды). 5 минут — как у Stripe.
pub const WEBHOOK_TOLERANCE_SECS: i64 = 300;

/// Метка секрета Svix.
pub const WHSEC_PREFIX: &str = "whsec_";

/// Имя заголовка идентификатора события.
pub const HEADER_ID: &str = "svix-id";
/// Имя заголовка timestamp.
pub const HEADER_TIMESTAMP: &str = "svix-timestamp";
/// Имя заголовка подписи.
pub const HEADER_SIGNATURE: &str = "svix-signature";

/// Тип HMAC для подписи Svix.
type HmacSha256 = Hmac<Sha256>;

/// Три заголовка подписи вебхука.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebhookHeaders {
    /// `svix-id` — идентификатор события (он же ключ дедупликации).
    pub id: String,
    /// `svix-timestamp` — unix-секунды отправки.
    pub timestamp: String,
    /// `svix-signature` — список `v1,<base64>` через пробел.
    pub signature: String,
}

impl WebhookHeaders {
    /// Собрать заголовки из пар «имя-значение» HTTP-запроса.
    ///
    /// Имена сравниваются без учёта регистра (`svix-id` и `Svix-Id` —
    /// одно и то же). Возвращает `None`, если хотя бы одного заголовка
    /// нет или он пуст: без всех трёх проверить подпись нельзя, и
    /// вызывающий обязан ответить 400, а не «доверять» телу.
    ///
    /// При дублях берётся первое непустое значение.
    pub fn from_pairs<'a>(pairs: impl Iterator<Item = (&'a str, &'a str)>) -> Option<Self> {
        let mut id: Option<String> = None;
        let mut timestamp: Option<String> = None;
        let mut signature: Option<String> = None;
        for (name, value) in pairs {
            let value = value.trim();
            if value.is_empty() {
                continue;
            }
            let slot = if name.eq_ignore_ascii_case(HEADER_ID) {
                &mut id
            } else if name.eq_ignore_ascii_case(HEADER_TIMESTAMP) {
                &mut timestamp
            } else if name.eq_ignore_ascii_case(HEADER_SIGNATURE) {
                &mut signature
            } else {
                continue;
            };
            if slot.is_none() {
                *slot = Some(value.to_string());
            }
        }
        Some(Self {
            id: id?,
            timestamp: timestamp?,
            signature: signature?,
        })
    }
}

/// Проверяющий подпись вебхука.
///
/// Хранит только байты секрета: исходная строка `whsec_…` после
/// конструирования не нужна и не сохраняется.
#[derive(Clone)]
pub struct WebhookVerifier {
    secret: Vec<u8>,
    tolerance_secs: i64,
}

impl std::fmt::Debug for WebhookVerifier {
    /// Секрет не печатается даже в `Debug`.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebhookVerifier")
            .field("secret", &"***")
            .field("tolerance_secs", &self.tolerance_secs)
            .finish()
    }
}

impl WebhookVerifier {
    /// Собрать проверяющего из секрета `whsec_<base64>`.
    ///
    /// Пустой секрет, отсутствие метки `whsec_` и невалидный base64 — это
    /// [`MailError::Config`]: окружение недонастроено, и принимать
    /// вебхуки нельзя. Секрет без метки не «сойдёт как есть» — иначе
    /// один и тот же заголовок подписи проверялся бы разными ключами в
    /// зависимости от конфига.
    pub fn from_whsec(secret: &str) -> Result<Self, MailError> {
        let raw = secret.trim();
        if raw.is_empty() {
            return Err(MailError::Config("RESEND_WEBHOOK_SECRET is empty".into()));
        }
        let encoded = raw.strip_prefix(WHSEC_PREFIX).ok_or_else(|| {
            MailError::Config(format!(
                "webhook secret must start with {WHSEC_PREFIX}, got a value without the label"
            ))
        })?;
        let encoded = encoded.trim();
        if encoded.is_empty() {
            return Err(MailError::Config(
                "webhook secret has an empty body after the whsec_ label".into(),
            ));
        }
        // Svix выдаёт base64 с padding; без padding тоже принимаем, чтобы
        // скопированный руками секрет не ломал intake.
        let bytes = STANDARD
            .decode(encoded)
            .or_else(|_| STANDARD_NO_PAD.decode(encoded.trim_end_matches('=')))
            .map_err(|e| MailError::Config(format!("webhook secret is not valid base64: {e}")))?;
        if bytes.is_empty() {
            return Err(MailError::Config(
                "webhook secret decodes to zero bytes".into(),
            ));
        }
        Ok(Self {
            secret: bytes,
            tolerance_secs: WEBHOOK_TOLERANCE_SECS,
        })
    }

    /// Допуск по timestamp, с которым работает проверяющий.
    #[must_use]
    pub fn tolerance_secs(&self) -> i64 {
        self.tolerance_secs
    }

    /// Проверить подпись тела вебхука.
    ///
    /// `now_unix` передаётся снаружи, чтобы тест мог зафиксировать время
    /// и проверить допуск детерминированно.
    ///
    /// Возвращает:
    /// * `Ok(())` — хотя бы одна `v1`-подпись сошлась и timestamp в допуске;
    /// * [`MailError::SignatureExpired`] — timestamp вне допуска (replay или
    ///   дрейф часов);
    /// * [`MailError::SignatureInvalid`] — нет заголовка, битый формат,
    ///   чужая подпись;
    /// * [`MailError::Config`] — секрет пуст (валидация защиты от
    ///   случайно сконструированного проверяющего).
    pub fn verify(
        &self,
        raw_body: &[u8],
        headers: &WebhookHeaders,
        now_unix: i64,
    ) -> Result<(), MailError> {
        if self.secret.is_empty() {
            return Err(MailError::Config(
                "webhook secret is empty — signature verification impossible".into(),
            ));
        }

        let id = headers.id.trim();
        if id.is_empty() {
            return Err(MailError::SignatureInvalid("svix-id is empty".into()));
        }
        let timestamp_raw = headers.timestamp.trim();
        let timestamp: i64 = timestamp_raw.parse().map_err(|_| {
            MailError::SignatureInvalid(format!(
                "svix-timestamp is not a unix number: {timestamp_raw:?}"
            ))
        })?;

        let tolerance = self.tolerance_secs.max(0);
        let drift = now_unix.saturating_sub(timestamp).saturating_abs();
        if drift > tolerance {
            return Err(MailError::SignatureExpired);
        }

        // Подписанное содержимое — ровно "{id}.{timestamp}.{body}".
        let mut signed = Vec::with_capacity(id.len() + timestamp_raw.len() + raw_body.len() + 2);
        signed.extend_from_slice(id.as_bytes());
        signed.push(b'.');
        signed.extend_from_slice(timestamp_raw.as_bytes());
        signed.push(b'.');
        signed.extend_from_slice(raw_body);

        let mut saw_v1 = false;
        for entry in headers.signature.split_whitespace() {
            let Some(encoded) = entry.strip_prefix("v1,") else {
                // Чужие схемы (`v1a,`, `v2,`) игнорируем: контракт знает
                // только v1, остальное — совместимость с будущим.
                continue;
            };
            saw_v1 = true;
            let Ok(signature) = STANDARD
                .decode(encoded.trim())
                .or_else(|_| STANDARD_NO_PAD.decode(encoded.trim().trim_end_matches('=')))
            else {
                continue;
            };
            let mut mac = HmacSha256::new_from_slice(&self.secret)
                .map_err(|_| MailError::Config("invalid HMAC key length".into()))?;
            mac.update(&signed);
            // Постоянное время: сравнение не должно зависеть от того,
            // сколько байт подписи совпало.
            if mac.verify_slice(&signature).is_ok() {
                return Ok(());
            }
        }

        if saw_v1 {
            Err(MailError::SignatureInvalid(
                "no v1 signature matched the HMAC".into(),
            ))
        } else {
            Err(MailError::SignatureInvalid(
                "svix-signature has no v1 entries".into(),
            ))
        }
    }
}

/// Собрать проверяющего из `RESEND_WEBHOOK_SECRET`.
///
/// Ошибка [`MailError::Config`], если переменная не задана: HTTP-слой
/// обязан ответить `503 mail_webhook_disabled`, а не принимать
/// неподписанные тела.
pub fn webhook_verifier_from_env() -> Result<WebhookVerifier, MailError> {
    let secret = std::env::var("RESEND_WEBHOOK_SECRET")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .ok_or_else(|| {
            MailError::Config(
                "RESEND_WEBHOOK_SECRET is not set — webhook intake is disabled".into(),
            )
        })?;
    WebhookVerifier::from_whsec(&secret)
}
