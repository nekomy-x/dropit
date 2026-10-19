//! Тонкая обёртка платёжных путей над [`dropit_infra_stripe::StripeApi`].
//!
//! Локальный мок-провайдер и собственная проверка подписи вебхуков убраны:
//! все вызовы Stripe идут через единственный провайдер
//! `ApiEnv.stripe` (`Arc<dyn StripeApi>`). Здесь остаются только маленькие
//! хелперы для разбора вебхук-события и константа допуска timestamp;
//! второй реализации Stripe-клиента нет.
//!
//! Поведение по умолчанию — рабочая деградация: без `STRIPE_API_KEY` в env
//! `infra::stripe()` подставляет [`dropit_infra_stripe::MockStripe`], и
//! весь платёжный цикл остаётся функциональным (идентификаторы `pi_mock_…`,
//! `tr_mock_…`, предсказуемые статусы). Реальный HMAC-SHA256 вебхуков
//! выполняет `StripeApi::verify_webhook` (LiveStripe — настоящий ключ,
//! MockStripe — принимает любой непустой заголовок, что явно задокументировано
//! в крейте).

use dropit_infra_stripe::WebhookEvent;

/// Допуск timestamp вебхука (техкарта VII: 5 минут). Делегируется в
/// `StripeApi::verify_webhook`; повторно объявлен здесь, чтобы платёжные
/// роуты не зависели от внутренней константы крейта напрямую.
pub const WEBHOOK_TOLERANCE_SECS: i64 = dropit_infra_stripe::WEBHOOK_TOLERANCE_SECS;

/// Извлекает `(payment_intent_id, amount_minor)` из поля `data.object`
/// события `payment_intent.succeeded` / `charge.succeeded`.
///
/// `amount` может отсутствовать у `charge.succeeded` — тогда
/// `amount_minor = None`, и вызывающая сторона берёт сумму из сохранённого
/// в БД платёжа (источник истины — наша запись, не payload вебхука).
pub fn intent_from_event(event: &WebhookEvent) -> Option<(String, Option<i64>)> {
    let object = event.data.get("object")?;
    let id = object.get("id").and_then(|v| v.as_str())?.to_string();
    let amount = object.get("amount").and_then(|v| v.as_i64());
    Some((id, amount))
}

#[cfg(test)]
mod tests {
    use super::*;
    use dropit_infra_stripe::{
        CaptureMethod, CreateIntent, IntentStatus, MockStripe, RefundReq, StripeApi, WebhookEvent,
    };
    use std::collections::BTreeMap;

    /// Полный цикл `create → capture → refund` на [`MockStripe`]:
    /// идентификаторы детерминированы, суммы и статусы корректны.
    #[tokio::test]
    async fn mock_stripe_create_capture_refund_flow() {
        let stripe = MockStripe;
        let req = CreateIntent {
            amount_minor: 2500,
            currency: "EUR".into(),
            customer_id: None,
            connected_account: None,
            capture_method: CaptureMethod::Manual,
            metadata: BTreeMap::new(),
            idempotency_key: "flow-1".into(),
        };
        let intent = stripe.create_payment_intent(req).await.unwrap();
        assert!(intent.id.starts_with("pi_mock_"));
        assert_eq!(intent.status, IntentStatus::RequiresCapture);
        assert_eq!(intent.amount_minor, 2500);
        assert!(intent
            .client_secret
            .as_deref()
            .is_some_and(|s| s.ends_with("_secret_mock")));

        // Capture: тот же id, succeeded, сумма сохранена.
        let captured = stripe
            .capture(&intent.id, Some(2500), "capture:flow-1")
            .await
            .unwrap();
        assert_eq!(captured.id, intent.id);
        assert_eq!(captured.status, IntentStatus::Succeeded);
        assert_eq!(captured.amount_minor, 2500);
        assert!(captured.client_secret.is_none());

        // Refund: re_mock_…, succeeded, сумма возврата.
        let refund = stripe
            .refund(RefundReq {
                payment_intent: intent.id.clone(),
                amount_minor: Some(2500),
                reason: Some("requested_by_customer".into()),
                idempotency_key: "refund:flow-1".into(),
            })
            .await
            .unwrap();
        assert!(refund.id.starts_with("re_mock_"));
        assert_eq!(refund.amount_minor, 2500);
        assert_eq!(refund.status, "succeeded");

        // Детерминизм: повторный create с тем же ключом — тот же id.
        let req2 = CreateIntent {
            amount_minor: 2500,
            currency: "EUR".into(),
            customer_id: None,
            connected_account: None,
            capture_method: CaptureMethod::Manual,
            metadata: BTreeMap::new(),
            idempotency_key: "flow-1".into(),
        };
        let again = stripe.create_payment_intent(req2).await.unwrap();
        assert_eq!(again.id, intent.id);
    }

    /// Автоматический захват — интент сразу succeeded.
    #[tokio::test]
    async fn mock_stripe_automatic_capture_succeeds() {
        let stripe = MockStripe;
        let r = stripe
            .create_payment_intent(CreateIntent {
                amount_minor: 500,
                currency: "usd".into(),
                customer_id: None,
                connected_account: None,
                capture_method: CaptureMethod::Automatic,
                metadata: BTreeMap::new(),
                idempotency_key: "auto-1".into(),
            })
            .await
            .unwrap();
        assert_eq!(r.status, IntentStatus::Succeeded);
        assert_eq!(r.amount_minor, 500);
    }

    #[test]
    fn intent_from_event_extracts_id_and_amount() {
        let ev = WebhookEvent {
            id: "evt_1".into(),
            event_type: "payment_intent.succeeded".into(),
            data: serde_json::json!({ "object": { "id": "pi_123", "amount": 1500 } }),
        };
        let (id, amount) = intent_from_event(&ev).expect("parsed");
        assert_eq!(id, "pi_123");
        assert_eq!(amount, Some(1500));
    }

    #[test]
    fn intent_from_event_amount_optional() {
        // charge.succeeded без amount — summa None.
        let ev = WebhookEvent {
            id: "evt_2".into(),
            event_type: "charge.succeeded".into(),
            data: serde_json::json!({ "object": { "id": "ch_1" } }),
        };
        let (id, amount) = intent_from_event(&ev).expect("parsed");
        assert_eq!(id, "ch_1");
        assert_eq!(amount, None);
    }

    #[test]
    fn intent_from_event_missing_object_returns_none() {
        let ev = WebhookEvent {
            id: "evt_3".into(),
            event_type: "payment_intent.succeeded".into(),
            data: serde_json::Value::Null,
        };
        assert!(intent_from_event(&ev).is_none());
    }
}
