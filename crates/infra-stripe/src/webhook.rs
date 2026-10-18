//! Проверка подписи вебхуков Stripe и разбор события.
//!
//! Формат заголовка `Stripe-Signature`: `t=<unix>,v1=<hex>[,v1=<hex>]…`.
//! Подпись — HMAC-SHA256 по строке `"{timestamp}.{payload}"`. Сравнение
//! через `hmac::Mac::verify_slice` (постоянное время). Timestamp
//! проверяется на допуск `tolerance_secs` (clock-drift — отдельная ошибка).

use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::error::StripeError;
use crate::json;
use crate::types::WebhookEvent;

/// Допуск по timestamp вебхука по умолчанию — 5 минут (техкарта, Часть VII).
pub const WEBHOOK_TOLERANCE_SECS: i64 = 300;

/// Имя HTTP-заголовка подписи Stripe.
pub const SIGNATURE_HEADER: &str = "Stripe-Signature";

/// Тип HMAC-SHA256 для подписи Stripe.
type HmacSha256 = Hmac<Sha256>;

/// Текущее время в unix-секундах (UTC).
pub(crate) fn now_unix() -> i64 {
    chrono::Utc::now().timestamp()
}

/// Разобранный заголовок подписи.
struct ParsedHeader {
    /// `t=` — unix-секунды.
    timestamp: i64,
    /// Декодированные `v1=`-подписи.
    signatures: Vec<Vec<u8>>,
}

/// Разбирает `Stripe-Signature`. Неизвестные схемы (`v0=`, …) игнорируются.
fn parse_header(header: &str) -> Result<ParsedHeader, StripeError> {
    let header = header.trim();
    if header.is_empty() {
        return Err(StripeError::SignatureInvalid(
            "заголовок Stripe-Signature отсутствует или пуст".into(),
        ));
    }
    let mut timestamp: Option<i64> = None;
    let mut signatures: Vec<Vec<u8>> = Vec::new();
    for raw in header.split(',') {
        let part = raw.trim();
        if part.is_empty() {
            continue;
        }
        let (key, value) = part
            .split_once('=')
            .ok_or_else(|| StripeError::SignatureInvalid(format!("сегмент без '=': {part}")))?;
        match key.trim() {
            "t" => {
                let value = value.trim();
                let parsed = value.parse::<i64>().map_err(|_| {
                    StripeError::SignatureInvalid(format!("timestamp не число: {value}"))
                })?;
                timestamp = Some(parsed);
            }
            "v1" => {
                let value = value.trim();
                if value.is_empty() {
                    return Err(StripeError::SignatureInvalid(
                        "пустое значение v1 в заголовке".into(),
                    ));
                }
                let bytes = hex::decode(value).map_err(|_| {
                    StripeError::SignatureInvalid("значение v1 не является hex".into())
                })?;
                signatures.push(bytes);
            }
            _ => {
                // Неизвестная схема — игнорируем (совместимость с будущими версиями).
            }
        }
    }
    let timestamp = timestamp
        .ok_or_else(|| StripeError::SignatureInvalid("в заголовке нет t=<timestamp>".into()))?;
    if signatures.is_empty() {
        return Err(StripeError::SignatureInvalid(
            "в заголовке нет ни одной v1=<hex> подписи".into(),
        ));
    }
    Ok(ParsedHeader {
        timestamp,
        signatures,
    })
}

/// Проверяет подпись вебхука (без разбора тела события).
///
/// Возвращает `Ok(())` при совпадении любой из `v1`-подписей и попадании
/// `t` в допуск `tolerance_secs`. Порядок проверок: разбор заголовка →
/// допуск timestamp → HMAC (постоянное время). `tolerance_secs < 0`
/// трактуется как «нулевой допуск» (отрицательное значение игнорируется).
pub(crate) fn verify_signature(
    secret: &[u8],
    payload: &[u8],
    signature_header: &str,
    tolerance_secs: i64,
    now_unix: i64,
) -> Result<(), StripeError> {
    if secret.is_empty() {
        return Err(StripeError::Config(
            "webhook_secret пуст — проверка подписи невозможна".into(),
        ));
    }
    let parsed = parse_header(signature_header)?;

    let tolerance = tolerance_secs.max(0);
    let drift = now_unix.saturating_sub(parsed.timestamp).saturating_abs();
    if drift > tolerance {
        return Err(StripeError::SignatureExpired);
    }

    // signed payload = "{timestamp}.{payload}" (как байты)
    let mut signed = Vec::with_capacity(payload.len() + 16);
    signed.extend_from_slice(parsed.timestamp.to_string().as_bytes());
    signed.push(b'.');
    signed.extend_from_slice(payload);

    // Каждую v1-подпись сверяем с HMAC в постоянное время.
    for sig in &parsed.signatures {
        let mut mac = HmacSha256::new_from_slice(secret)
            .map_err(|_| StripeError::Config("недопустимая длина ключа HMAC".into()))?;
        mac.update(&signed);
        if mac.verify_slice(sig).is_ok() {
            return Ok(());
        }
    }
    Err(StripeError::SignatureInvalid(
        "ни одна v1-подпись не совпала с HMAC".into(),
    ))
}

/// Проверяет подпись вебхука и разбирает событие.
///
/// Сначала проверяется подпись (тело не разбирается до проверки криптографии),
/// затем из проверенного payload читаются поля `id`, `type`, `data`.
/// `now` берётся из системных часов UTC.
///
/// Независимая от трейта функция: удобна для webhook-intake (C10), который
/// держит только секрет, а не объект [`crate::StripeApi`].
pub fn verify_webhook_event(
    webhook_secret: &str,
    payload: &[u8],
    signature_header: &str,
    tolerance_secs: i64,
) -> Result<WebhookEvent, StripeError> {
    verify_signature(
        webhook_secret.as_bytes(),
        payload,
        signature_header,
        tolerance_secs,
        now_unix(),
    )?;
    json::parse_event(payload)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &[u8] = b"whsec_test_secret";

    /// Вычисляет hex-HMAC-SHA256 для `"{t}.{payload}"`.
    fn sign(t: i64, payload: &[u8], secret: &[u8]) -> String {
        let mut mac = HmacSha256::new_from_slice(secret).unwrap();
        let mut signed = Vec::with_capacity(payload.len() + 16);
        signed.extend_from_slice(t.to_string().as_bytes());
        signed.push(b'.');
        signed.extend_from_slice(payload);
        mac.update(&signed);
        hex::encode(mac.finalize().into_bytes())
    }

    const NOW: i64 = 1_700_000_000;
    const PAYLOAD: &[u8] =
        br#"{"id":"evt_1","type":"payment_intent.succeeded","data":{"object":{"id":"pi_1"}}}"#;

    #[test]
    fn valid_signature_accepted() {
        let v1 = sign(NOW, PAYLOAD, SECRET);
        let header = format!("t={NOW},v1={v1}");
        verify_signature(SECRET, PAYLOAD, &header, 300, NOW + 10).unwrap();
    }

    #[test]
    fn multiple_v1_one_valid_accepted() {
        let good = sign(NOW, PAYLOAD, SECRET);
        // Первая подпись — мусор (валидный hex, но не HMAC), вторая — настоящая.
        let header = format!("t={NOW},v1={:064x},v1={good}", 0u128);
        verify_signature(SECRET, PAYLOAD, &header, 300, NOW).unwrap();
    }

    #[test]
    fn expired_timestamp_rejected() {
        let v1 = sign(NOW, PAYLOAD, SECRET);
        let header = format!("t={NOW},v1={v1}");
        let err = verify_signature(SECRET, PAYLOAD, &header, 300, NOW + 600).unwrap_err();
        assert_eq!(err, StripeError::SignatureExpired);
    }

    #[test]
    fn tolerance_boundary_accepted() {
        let v1 = sign(NOW, PAYLOAD, SECRET);
        let header = format!("t={NOW},v1={v1}");
        // Ровно на границе допуска — принимаем.
        verify_signature(SECRET, PAYLOAD, &header, 300, NOW + 300).unwrap();
    }

    #[test]
    fn forged_payload_rejected() {
        let v1 = sign(NOW, PAYLOAD, SECRET);
        let header = format!("t={NOW},v1={v1}");
        let forged = b"{\"id\":\"evt_x\",\"type\":\"payment_intent.succeeded\",\"data\":{}}";
        let err = verify_signature(SECRET, forged, &header, 300, NOW).unwrap_err();
        assert!(matches!(err, StripeError::SignatureInvalid(_)));
    }

    #[test]
    fn missing_header_rejected() {
        let err = verify_signature(SECRET, PAYLOAD, "", 300, NOW).unwrap_err();
        assert!(matches!(err, StripeError::SignatureInvalid(_)));
    }

    #[test]
    fn whitespace_only_header_rejected() {
        let err = verify_signature(SECRET, PAYLOAD, "   ", 300, NOW).unwrap_err();
        assert!(matches!(err, StripeError::SignatureInvalid(_)));
    }

    #[test]
    fn garbage_format_rejected() {
        let cases = ["hello", "t=abc,v1=zz", "garbage=no_equals_segment"];
        for header in cases {
            let err = verify_signature(SECRET, PAYLOAD, header, 300, NOW).unwrap_err();
            assert!(
                matches!(err, StripeError::SignatureInvalid(_)),
                "header {header:?} should be SignatureInvalid, got {err:?}"
            );
        }
    }

    #[test]
    fn missing_t_rejected() {
        let v1 = sign(NOW, PAYLOAD, SECRET);
        let header = format!("v1={v1}");
        let err = verify_signature(SECRET, PAYLOAD, &header, 300, NOW).unwrap_err();
        assert!(matches!(err, StripeError::SignatureInvalid(_)));
    }

    #[test]
    fn missing_v1_rejected() {
        let header = format!("t={NOW}");
        let err = verify_signature(SECRET, PAYLOAD, &header, 300, NOW).unwrap_err();
        assert!(matches!(err, StripeError::SignatureInvalid(_)));
    }

    #[test]
    fn non_hex_v1_rejected() {
        let header = format!("t={NOW},v1=not-hex");
        let err = verify_signature(SECRET, PAYLOAD, &header, 300, NOW).unwrap_err();
        assert!(matches!(err, StripeError::SignatureInvalid(_)));
    }

    #[test]
    fn empty_secret_rejected_before_header_parse() {
        let err = verify_signature(&[], PAYLOAD, "t=1,v1=abc", 300, NOW).unwrap_err();
        assert!(matches!(err, StripeError::Config(_)));
    }

    #[test]
    fn verify_webhook_event_returns_parsed_event() {
        // verify_webhook_event использует системные часы — подписываем «сейчас».
        let now = now_unix();
        let v1 = sign(now, PAYLOAD, SECRET);
        let header = format!("t={now},v1={v1}");
        let event =
            verify_webhook_event(std::str::from_utf8(SECRET).unwrap(), PAYLOAD, &header, 300)
                .unwrap();
        assert_eq!(event.id, "evt_1");
        assert_eq!(event.event_type, "payment_intent.succeeded");
        assert_eq!(event.data["object"]["id"], "pi_1");
    }

    #[test]
    fn verify_webhook_event_rejects_before_parse_when_bad_sig() {
        let now = now_unix();
        let header = format!("t={now},v1=deadbeef");
        let err = verify_webhook_event("whsec_test_secret", PAYLOAD, &header, 300).unwrap_err();
        assert!(matches!(err, StripeError::SignatureInvalid(_)));
    }

    #[test]
    fn non_utf8_payload_signature_still_works() {
        // Подпись считается по байтам, UTF-8 не требуется.
        let payload = &[0x00, 0xFF, 0x42, 0x7F];
        let v1 = sign(NOW, payload, SECRET);
        let header = format!("t={NOW},v1={v1}");
        verify_signature(SECRET, payload, &header, 300, NOW).unwrap();
    }
}
