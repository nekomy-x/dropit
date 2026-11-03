//! Детерминированный мок Stripe для dev/test (без сети).
//!
//! Идентификаторы детерминированы по seed (Idempotency-Key / e-mail) через
//! SHA-256 → UUID v4-формата: `pi_mock_<uuid>`, `tr_mock_<uuid>`,
//! `acct_mock_<uuid>`, `re_mock_<uuid>`. Один и тот же вход → тот же выход,
//! что стабильно между вызовами и в e2e-тестах.
//!
//! ⚠️ Мок НЕ проверяет криптографию подписи вебхука: он разбирает событие
//! из payload и принимает любой непустой заголовок `Stripe-Signature`.
//! Реальная проверка — в [`crate::LiveStripe`] / [`crate::verify_webhook_event`].
//! Помечено по правилу 5 проекта (данные не реальные).

use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::api::StripeApi;
use crate::error::StripeError;
use crate::json;
use crate::types::{
    CaptureMethod, ConnectAccountReq, ConnectedAccount, CreateIntent, IntentResult, IntentStatus,
    RefundReq, RefundResult, TransferReq, TransferResult, WebhookEvent,
};

/// МОК-реализация Stripe: детерминированные id, без сети.
///
/// Все статусы — happy-path: интенты сразу доходят до терминального состояния
/// по `capture_method`, аккаунты считаются верифицированными. Сценарии отказа
/// (отклонённая карта, онбординг не пройден) моделируются в e2e/chaos-тестах
/// напрямую через домен — мок их не эмулирует.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MockStripe;

/// Префикс seed для изоляции пространств имён мок-id.
const MOCK_SEED_PREFIX: &str = "dropit-infra-stripe/mock:";

/// Детерминированный UUID v4-формата из SHA-256(seed).
fn mock_uuid(seed: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(MOCK_SEED_PREFIX.as_bytes());
    hasher.update(seed.as_bytes());
    let digest = hasher.finalize();
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    // Версия 4 и вариант RFC 4122 — чтобы UUID выглядел «настоящим».
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes).to_string()
}

#[async_trait::async_trait]
impl StripeApi for MockStripe {
    fn is_mock(&self) -> bool { true }

    async fn retrieve_payment_intent(&self, intent_id: &str) -> Result<IntentResult, StripeError> {
        Ok(IntentResult {
            id: intent_id.to_string(),
            status: IntentStatus::RequiresCapture,
            amount_minor: 0,
            client_secret: Some(format!("{intent_id}_secret_mock")),
        })
    }

    async fn create_payment_intent(&self, req: CreateIntent) -> Result<IntentResult, StripeError> {
        let id = format!(
            "pi_mock_{}",
            mock_uuid(&format!("intent:{}", req.idempotency_key))
        );
        let status = match req.capture_method {
            CaptureMethod::Manual => IntentStatus::RequiresCapture,
            CaptureMethod::Automatic => IntentStatus::Succeeded,
        };
        let client_secret = format!("{id}_secret_mock");
        Ok(IntentResult {
            id,
            status,
            amount_minor: req.amount_minor,
            client_secret: Some(client_secret),
        })
    }

    async fn capture(
        &self,
        intent_id: &str,
        amount_minor: Option<i64>,
        idempotency_key: &str,
    ) -> Result<IntentResult, StripeError> {
        let id = if intent_id.is_empty() {
            format!(
                "pi_mock_{}",
                mock_uuid(&format!("capture:{idempotency_key}"))
            )
        } else {
            intent_id.to_string()
        };
        Ok(IntentResult {
            id,
            status: IntentStatus::Succeeded,
            amount_minor: amount_minor.unwrap_or(0),
            client_secret: None,
        })
    }

    async fn refund(&self, req: RefundReq) -> Result<RefundResult, StripeError> {
        let id = format!(
            "re_mock_{}",
            mock_uuid(&format!(
                "refund:{}:{}",
                req.payment_intent, req.idempotency_key
            ))
        );
        // Мок не хранит интенты — сумму возврата не знаем при полном возврате.
        let amount_minor = req.amount_minor.unwrap_or(0);
        Ok(RefundResult {
            id,
            amount_minor,
            status: "succeeded".to_string(),
        })
    }

    async fn create_connected_account(
        &self,
        req: ConnectAccountReq,
    ) -> Result<ConnectedAccount, StripeError> {
        let id = format!("acct_mock_{}", mock_uuid(&format!("account:{}", req.email)));
        // Happy-path: аккаунт сразу «верифицирован» (для dev-флоу).
        Ok(ConnectedAccount {
            id,
            charges_enabled: true,
            payouts_enabled: true,
            details_submitted: true,
        })
    }

    async fn retrieve_account(&self, account_id: &str) -> Result<ConnectedAccount, StripeError> {
        // Эхо переданного id (мок не хранит состояние).
        Ok(ConnectedAccount {
            id: account_id.to_string(),
            charges_enabled: true,
            payouts_enabled: true,
            details_submitted: true,
        })
    }

    async fn account_link(
        &self,
        account_id: &str,
        refresh_url: &str,
        return_url: &str,
    ) -> Result<String, StripeError> {
        let token = mock_uuid(&format!("link:{account_id}:{refresh_url}:{return_url}"));
        Ok(format!("https://connect.stripe.com/setup/mock/{token}"))
    }

    async fn create_transfer(&self, req: TransferReq) -> Result<TransferResult, StripeError> {
        let id = format!(
            "tr_mock_{}",
            mock_uuid(&format!("transfer:{}", req.idempotency_key))
        );
        Ok(TransferResult {
            id,
            amount_minor: req.amount_minor,
            status: "paid".to_string(),
        })
    }

    fn verify_webhook(
        &self,
        payload: &[u8],
        signature_header: &str,
        _tolerance_secs: i64,
    ) -> Result<WebhookEvent, StripeError> {
        // Мок не проверяет HMAC: требует лишь непустой заголовок и разбирает
        // событие. Это явно задокументировано выше — для dev/test.
        if signature_header.trim().is_empty() {
            return Err(StripeError::SignatureInvalid(
                "отсутствует заголовок Stripe-Signature (mock не проверяет подпись, но заголовок обязателен)".into(),
            ));
        }
        json::parse_event(payload)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::webhook::WEBHOOK_TOLERANCE_SECS;
    use std::collections::BTreeMap;

    fn idemp(k: &str) -> String {
        k.to_string()
    }

    #[tokio::test]
    async fn create_intent_manual_is_deterministic() {
        let m = MockStripe;
        let req = CreateIntent {
            amount_minor: 1500,
            currency: "EUR".into(),
            customer_id: None,
            connected_account: None,
            capture_method: CaptureMethod::Manual,
            metadata: BTreeMap::new(),
            idempotency_key: idemp("same-key"),
        };
        let a = m.create_payment_intent(req.clone()).await.unwrap();
        let b = m.create_payment_intent(req).await.unwrap();
        assert_eq!(a.id, b.id);
        assert!(a.id.starts_with("pi_mock_"));
        // UUID-хвост парсится как настоящий UUID.
        let tail = a.id.strip_prefix("pi_mock_").unwrap();
        assert!(Uuid::parse_str(tail).is_ok());
        assert_eq!(a.status, IntentStatus::RequiresCapture);
        assert_eq!(a.amount_minor, 1500);
        assert!(a
            .client_secret
            .as_deref()
            .is_some_and(|s| s.ends_with("_secret_mock")));
    }

    #[tokio::test]
    async fn create_intent_automatic_succeeds() {
        let m = MockStripe;
        let req = CreateIntent {
            amount_minor: 500,
            currency: "usd".into(),
            customer_id: None,
            connected_account: None,
            capture_method: CaptureMethod::Automatic,
            metadata: BTreeMap::new(),
            idempotency_key: idemp("auto"),
        };
        let r = m.create_payment_intent(req).await.unwrap();
        assert_eq!(r.status, IntentStatus::Succeeded);
    }

    #[tokio::test]
    async fn different_keys_yield_different_ids() {
        let m = MockStripe;
        let mk = |k: &str| {
            m.create_payment_intent(CreateIntent {
                amount_minor: 100,
                currency: "eur".into(),
                customer_id: None,
                connected_account: None,
                capture_method: CaptureMethod::Manual,
                metadata: BTreeMap::new(),
                idempotency_key: idemp(k),
            })
        };
        let a = mk("k1").await.unwrap();
        let b = mk("k2").await.unwrap();
        assert_ne!(a.id, b.id);
    }

    #[tokio::test]
    async fn capture_succeeds() {
        let m = MockStripe;
        let r = m.capture("pi_mock_x", Some(123), "cap-1").await.unwrap();
        assert_eq!(r.id, "pi_mock_x");
        assert_eq!(r.status, IntentStatus::Succeeded);
        assert_eq!(r.amount_minor, 123);
    }

    #[tokio::test]
    async fn refund_deterministic() {
        let m = MockStripe;
        let req = RefundReq {
            payment_intent: "pi_mock_x".into(),
            amount_minor: Some(700),
            reason: None,
            idempotency_key: idemp("r1"),
        };
        let r = m.refund(req.clone()).await.unwrap();
        assert!(r.id.starts_with("re_mock_"));
        assert_eq!(r.amount_minor, 700);
        assert_eq!(r.status, "succeeded");
        // Детерминизм.
        assert_eq!(m.refund(req).await.unwrap().id, r.id);
    }

    #[tokio::test]
    async fn connect_account_onboarding_link() {
        let m = MockStripe;
        let req = ConnectAccountReq {
            email: "dj@club.io".into(),
            country: "ES".into(),
            business_type: "individual".into(),
            metadata: BTreeMap::new(),
        };
        let acc = m.create_connected_account(req.clone()).await.unwrap();
        assert!(acc.id.starts_with("acct_mock_"));
        assert!(acc.charges_enabled);
        assert!(acc.payouts_enabled);
        assert!(acc.details_submitted);
        // Тот же e-mail → тот же id.
        assert_eq!(m.create_connected_account(req).await.unwrap().id, acc.id);
        // retrieve эхо.
        let got = m.retrieve_account(&acc.id).await.unwrap();
        assert_eq!(got.id, acc.id);
        // link.
        let url = m
            .account_link(&acc.id, "https://r", "https://ret")
            .await
            .unwrap();
        assert!(url.starts_with("https://connect.stripe.com/setup/mock/"));
    }

    #[tokio::test]
    async fn transfer_deterministic() {
        let m = MockStripe;
        let req = TransferReq {
            amount_minor: 1200,
            currency: "EUR".into(),
            destination: "acct_dj1".into(),
            metadata: BTreeMap::new(),
            idempotency_key: idemp("t1"),
        };
        let t = m.create_transfer(req.clone()).await.unwrap();
        assert!(t.id.starts_with("tr_mock_"));
        assert_eq!(t.amount_minor, 1200);
        assert_eq!(t.status, "paid");
        assert_eq!(m.create_transfer(req).await.unwrap().id, t.id);
    }

    #[test]
    fn mock_verify_webhook_parses_event() {
        let m = MockStripe;
        let payload =
            br#"{"id":"evt_1","type":"payment_intent.succeeded","data":{"object":{"id":"pi_1"}}}"#;
        let ev = m
            .verify_webhook(payload, "t=1,v1=anything", WEBHOOK_TOLERANCE_SECS)
            .unwrap();
        assert_eq!(ev.id, "evt_1");
        assert_eq!(ev.event_type, "payment_intent.succeeded");
    }

    #[test]
    fn mock_verify_webhook_requires_header() {
        let m = MockStripe;
        let err = m
            .verify_webhook(b"{}", "", WEBHOOK_TOLERANCE_SECS)
            .unwrap_err();
        assert!(matches!(err, StripeError::SignatureInvalid(_)));
    }

    #[test]
    fn mock_verify_webhook_rejects_bad_json() {
        let m = MockStripe;
        let err = m
            .verify_webhook(b"not json", "t=1,v1=x", WEBHOOK_TOLERANCE_SECS)
            .unwrap_err();
        assert!(matches!(err, StripeError::Decode(_)));
    }
}
