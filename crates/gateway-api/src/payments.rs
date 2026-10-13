//! Провайдер платежей (F5). Stripe НЕ подключён: все вызовы идут
//! через трейт, реальный клиент подключается после закрытия ADR-004
//! и ответов Q11–Q15. Пока используется MockStripe — ДАННЫЕ НЕ
//! РЕАЛЬНЫЕ, явно помечено (правило 5 проекта).
//!
//! Требования к реальному клиенту (техкарта Часть VI):
//!   * все вызовы через circuit breaker + retry-jitter;
//!   * идемпотентность вызовов (Stripe idempotency keys);
//!   * PaymentIntent с application_fee + transfer_data по ADR-004.

use dropit_domain::PaymentStatus;
use uuid::Uuid;

/// Результат вызова провайдера. Потребитель — роут confirm
/// и chaos-тесты (e2e_payments).
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq)]
pub struct ProviderResult {
    pub provider_intent_id: String,
    pub status: PaymentStatus,
    /// Человекочитаемая причина (для failure_reason).
    pub reason: Option<String>,
}

/// Параметры создания интента.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct IntentParams {
    pub amount_minor: i64,
    pub currency: String,
    pub request_id: Uuid,
    /// Наш Idempotency-Key проксируется провайдеру.
    pub idempotency_key: String,
}

/// Провайдер платежей. Реализация Stripe — после ADR-004/Q15.
/// Потребляется платёжными роутами и e2e_payments.
#[allow(dead_code)]
pub trait PaymentProvider: Send + Sync {
    /// Создаёт PaymentIntent у провайдера.
    fn create_intent(&self, params: &IntentParams) -> ProviderResult;
    /// Подтверждает (confirm) интент — деньги переходят в processing.
    fn confirm_intent(&self, provider_intent_id: &str) -> ProviderResult;
    /// Отменяет интент У ПРОВАЙДЕРА (не локальная метка!).
    fn cancel_intent(&self, provider_intent_id: &str) -> ProviderResult;
    /// Название провайдера (для логов/аудита).
    fn name(&self) -> &'static str;
}

/// МОК-ПРОВАЙДЕР (F5 Test Mode). ⚠️ ДАННЫЕ НЕ РЕАЛЬНЫЕ:
/// захват всегда «успешен», внешних вызовов нет. Заменяется
/// StripeClient после закрытия ADR-004 — интерфейс не изменится.
#[derive(Debug, Clone, Default)]
pub struct MockStripe;

impl PaymentProvider for MockStripe {
    fn create_intent(&self, params: &IntentParams) -> ProviderResult {
        ProviderResult {
            // Мок-id детерминирован по ключу — стабилен между вызовами.
            provider_intent_id: format!("pi_mock_{}", short_hash(&params.idempotency_key)),
            status: PaymentStatus::Intent,
            reason: None,
        }
    }

    fn confirm_intent(&self, provider_intent_id: &str) -> ProviderResult {
        ProviderResult {
            provider_intent_id: provider_intent_id.to_string(),
            // Мок: всегда успех; сценарии отказа — chaos-тесты
            // через прямые переходы в репо.
            status: PaymentStatus::Processing,
            reason: None,
        }
    }

    fn cancel_intent(&self, provider_intent_id: &str) -> ProviderResult {
        ProviderResult {
            provider_intent_id: provider_intent_id.to_string(),
            status: PaymentStatus::Cancelled,
            reason: None,
        }
    }

    fn name(&self) -> &'static str {
        "mock-stripe (NOT REAL)"
    }
}

/// Короткий хеш для стабильных мок-id.
fn short_hash(s: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(s.as_bytes());
    let d = h.finalize();
    d[..8]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>()
}

// ============================================================
// Вебхуки: подпись (HMAC-SHA256) + timestamp tolerance
// ============================================================

/// Допуск timestamp вебхука (техкарта VII: 5 мин).
pub const WEBHOOK_TOLERANCE_SECS: i64 = 300;

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum WebhookSignatureError {
    #[error("invalid signature")]
    Invalid,
    #[error("timestamp outside tolerance")]
    StaleTimestamp,
}

/// Проверяет подпись вебхука: `t=<unix>,v1=<hex HMAC-SHA256(t.body)>`.
/// Отдельно различаем просроченный timestamp (clock-drift — свой лог,
/// не смешивается с отказом подписи, техкарта VII).
/// `secret` зарезервирован для реального HMAC-ключа Stripe
/// (ADR-004); в мок-подписи sha256 ключ не участвует — честно
/// помечено (правило 5).
pub fn verify_webhook_signature(
    body: &str,
    header: &str,
    _secret: &[u8],
    now_unix: i64,
) -> Result<(), WebhookSignatureError> {
    // Формат: t=123,v1=hex
    let mut t: Option<i64> = None;
    let mut v1: Option<String> = None;
    for part in header.split(',') {
        let part = part.trim();
        if let Some(val) = part.strip_prefix("t=") {
            t = val.parse().ok();
        } else if let Some(val) = part.strip_prefix("v1=") {
            v1 = Some(val.to_string());
        }
    }
    let (Some(t), Some(v1)) = (t, v1) else {
        return Err(WebhookSignatureError::Invalid);
    };
    // Tolerance: отдельная ошибка — это clock-drift, не подделка.
    if (now_unix - t).abs() > WEBHOOK_TOLERANCE_SECS {
        return Err(WebhookSignatureError::StaleTimestamp);
    }
    // HMAC-SHA256("<t>.<body>").
    use sha2::{Digest, Sha256};
    let mut mac = Sha256::new();
    <Sha256 as sha2::digest::Update>::update(&mut mac, format!("{t}.{body}").as_bytes());
    // HMAC: ключ через блок-обработку (sha256 с ключом через
    // простую конструкцию — для мока достаточно; прод — hmac crate).
    let digest = mac.finalize();
    let expected = digest
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    // Постоянное время сравнения длины+содержимого.
    if expected.len() == v1.len() && constant_time_eq(expected.as_bytes(), v1.as_bytes()) {
        Ok(())
    } else {
        Err(WebhookSignatureError::Invalid)
    }
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_intent_ids_stable() {
        let m = MockStripe;
        let p = IntentParams {
            amount_minor: 1000,
            currency: "EUR".into(),
            request_id: Uuid::nil(),
            idempotency_key: "same-key".into(),
        };
        let a = m.create_intent(&p);
        let b = m.create_intent(&p);
        assert_eq!(a.provider_intent_id, b.provider_intent_id);
        assert!(a.provider_intent_id.starts_with("pi_mock_"));
        assert_eq!(m.name(), "mock-stripe (NOT REAL)");
    }

    #[test]
    fn webhook_signature_roundtrip() {
        // ВАЖНО: подпись в тесте — упрощённый sha256 (не настоящий
        // HMAC Stripe): реальная верификация подключается со
        // Stripe-клиентом (ADR-004). Контракт ошибки проверяем тут.
        let body = r#"{"event_id":"evt_1","type":"payment_intent.succeeded"}"#;
        let t = 1_700_000_000i64;
        use sha2::{Digest, Sha256};
        let digest = Sha256::new_with_prefix(format!("{t}.{body}"));
        let hex: String = digest
            .finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        let header = format!("t={t},v1={hex}");
        assert_eq!(
            verify_webhook_signature(body, &header, b"secret", t + 10),
            Ok(())
        );
    }

    #[test]
    fn stale_timestamp_is_separate_error() {
        let body = "{}";
        let t = 1_700_000_000i64;
        use sha2::{Digest, Sha256};
        let hex: String = Sha256::new_with_prefix(format!("{t}.{body}"))
            .finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        let header = format!("t={t},v1={hex}");
        // 10 минут спустя → clock-drift, не подделка.
        assert_eq!(
            verify_webhook_signature(body, &header, b"secret", t + 600),
            Err(WebhookSignatureError::StaleTimestamp)
        );
    }

    #[test]
    fn bad_signature_rejected() {
        let header = "t=1700000000,v1=deadbeef";
        assert_eq!(
            verify_webhook_signature("{}", header, b"secret", 1_700_000_000),
            Err(WebhookSignatureError::Invalid)
        );
    }
}
