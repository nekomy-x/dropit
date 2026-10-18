//! Типы контракта Stripe: запросы и ответы API.
//!
//! Деньги — целые минорные единицы (`amount_minor: i64`): 1 € = 100, нулей
//! и плавающей точки здесь нет (инвариант ADR-003). Эти типы — чистый транспорт,
//! бизнес-логика сплита (`compute_split`, доля клуба/DJ, Stripe-fee) живёт в
//! `dropit-domain` и здесь не дублируется.

use std::collections::BTreeMap;

use crate::error::StripeError;

/// Метод захвата платежа Stripe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CaptureMethod {
    /// Захватывается автоматически при подтверждении карты (значение `automatic`).
    #[default]
    Automatic,
    /// Требуется ручной вызов `capture` (`requires_capture` до захвата).
    Manual,
}

impl CaptureMethod {
    /// Строковое значение для Stripe (`automatic` / `manual`).
    pub fn as_stripe_str(&self) -> &'static str {
        match self {
            Self::Automatic => "automatic",
            Self::Manual => "manual",
        }
    }

    /// Разбирает строку статуса/параметра Stripe. Неизвестное значение → `None`.
    pub fn from_stripe_str(value: &str) -> Option<Self> {
        match value {
            "automatic" | "automatic_async" => Some(Self::Automatic),
            "manual" => Some(Self::Manual),
            _ => None,
        }
    }
}

impl std::fmt::Display for CaptureMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_stripe_str())
    }
}

/// Статус PaymentIntent, значимые для DROP IT переходы из техкарты.
///
/// Прочие статусы Stripe (`requires_confirmation`, `requires_action` — 3DS)
/// приравниваются к [`IntentStatus::RequiresPaymentMethod`] — платёж ещё не
/// завершён корректно, деньги не списаны.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntentStatus {
    /// `requires_capture` — средства захолдированы, ждём `capture`.
    RequiresCapture,
    /// `succeeded` — платёж завершён, деньги списаны.
    Succeeded,
    /// `requires_payment_method` (и иные «не завершён») — требуется способ оплаты.
    RequiresPaymentMethod,
    /// `processing` — Stripe обрабатывает платёж.
    Processing,
    /// `canceled` — интент отменён.
    Canceled,
}

impl IntentStatus {
    /// Разбирает строку статуса Stripe. Неизвестное значение → `None`.
    pub fn from_stripe_str(value: &str) -> Option<Self> {
        match value {
            "requires_capture" => Some(Self::RequiresCapture),
            "succeeded" => Some(Self::Succeeded),
            "requires_payment_method" | "requires_confirmation" | "requires_action" => {
                Some(Self::RequiresPaymentMethod)
            }
            "processing" => Some(Self::Processing),
            "canceled" | "cancelled" => Some(Self::Canceled),
            _ => None,
        }
    }

    /// Каноническое имя статуса.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::RequiresCapture => "requires_capture",
            Self::Succeeded => "succeeded",
            Self::RequiresPaymentMethod => "requires_payment_method",
            Self::Processing => "processing",
            Self::Canceled => "canceled",
        }
    }
}

impl std::fmt::Display for IntentStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Параметры создания PaymentIntent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateIntent {
    /// Сумма в минорных единицах валюты (`1000` = 10,00).
    pub amount_minor: i64,
    /// ISO-4217 валюта, 3 буквы (`EUR`, `usd`); нормализуется к нижнему регистру.
    pub currency: String,
    /// `customer` Stripe при наличии (для повторных оплат).
    pub customer_id: Option<String>,
    /// Connect-аккаунт получателя. Маппинг provisional (ADR-004 открыт):
    /// отправляется как `transfer_data[destination]` (destination charges).
    pub connected_account: Option<String>,
    /// Метод захвата.
    pub capture_method: CaptureMethod,
    /// Произвольные метаданные, попадают в `metadata[...]` (ключи сортируются).
    pub metadata: BTreeMap<String, String>,
    /// Idempotency-Key: проксируется в одноимённый заголовок Stripe.
    pub idempotency_key: String,
}

/// Результат создания/захвата PaymentIntent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntentResult {
    /// Stripe-идентификатор интента (`pi_…`).
    pub id: String,
    /// Статус интента.
    pub status: IntentStatus,
    /// Сумма в минорных единицах.
    pub amount_minor: i64,
    /// `client_secret` для Stripe SDK (есть у только что созданных интентов).
    pub client_secret: Option<String>,
}

/// Параметры возврата средств (`POST /v1/refunds`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefundReq {
    /// PaymentIntent, с которого возвращаем (`pi_…`).
    pub payment_intent: String,
    /// Сумма возврата; `None` — полный возврат.
    pub amount_minor: Option<i64>,
    /// Причина (`duplicate`, `fraudulent`, `requested_by_customer`).
    pub reason: Option<String>,
    /// Idempotency-Key.
    pub idempotency_key: String,
}

/// Результат создания возврата.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefundResult {
    /// Stripe-идентификатор возврата (`re_…`).
    pub id: String,
    /// Сумма возврата в минорных единицах.
    pub amount_minor: i64,
    /// Статус (`succeeded`, `pending`, `failed`, `canceled`).
    pub status: String,
}

/// Параметры создания Connect-аккаунта (`POST /v1/accounts`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectAccountReq {
    /// E-mail владельца аккаунта.
    pub email: String,
    /// Страна (ISO-2, `ES`, `DE`).
    pub country: String,
    /// Бизнес-тип (`individual`, `company`).
    pub business_type: String,
    /// Метаданные (`metadata[...]`).
    pub metadata: BTreeMap<String, String>,
}

/// Данные Connect-аккаунта.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectedAccount {
    /// Stripe-идентификатор (`acct_…`).
    pub id: String,
    /// Разрешены ли платежи на аккаунт.
    pub charges_enabled: bool,
    /// Разрешены ли выплаты с аккаунта.
    pub payouts_enabled: bool,
    /// Пройден ли KYC/онбординг.
    pub details_submitted: bool,
}

/// Параметры выплаты (`POST /v1/transfers`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferReq {
    /// Сумма в минорных единицах.
    pub amount_minor: i64,
    /// Валюта (ISO-4217).
    pub currency: String,
    /// Connect-аккаунт-получатель (`acct_…`).
    pub destination: String,
    /// Метаданные (`metadata[...]`).
    pub metadata: BTreeMap<String, String>,
    /// Idempotency-Key.
    pub idempotency_key: String,
}

/// Результат выплаты.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferResult {
    /// Stripe-идентификатор трансфера (`tr_…`).
    pub id: String,
    /// Сумма в минорных единицах.
    pub amount_minor: i64,
    /// Статус. Stripe не возвращает поле `status` у Transfer —
    /// при отсутствии подставляется `"created"`, финальное состояние
    /// уточняется вебхуком `transfer.paid` / `transfer.failed`.
    pub status: String,
}

/// Разобранное событие вебхука Stripe.
#[derive(Debug, Clone)]
pub struct WebhookEvent {
    /// Идентификатор события (`evt_…`).
    pub id: String,
    /// Тип события (`payment_intent.succeeded`, `transfer.paid`, …).
    pub event_type: String,
    /// Сырой объект `data` события (включает `object` с деталями).
    pub data: serde_json::Value,
}

/// Проверяет, что сумма положительная (минорные единицы).
pub(crate) fn validate_amount(amount_minor: i64) -> Result<(), StripeError> {
    if amount_minor > 0 {
        Ok(())
    } else {
        Err(StripeError::Config(format!(
            "amount_minor должен быть положительным, получено {amount_minor}"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_method_roundtrip() {
        assert_eq!(CaptureMethod::Automatic.as_stripe_str(), "automatic");
        assert_eq!(CaptureMethod::Manual.as_stripe_str(), "manual");
        assert_eq!(
            CaptureMethod::from_stripe_str("manual"),
            Some(CaptureMethod::Manual)
        );
        assert_eq!(
            CaptureMethod::from_stripe_str("automatic_async"),
            Some(CaptureMethod::Automatic)
        );
        assert_eq!(CaptureMethod::from_stripe_str("nope"), None);
    }

    #[test]
    fn intent_status_known_and_unknown() {
        assert_eq!(
            IntentStatus::from_stripe_str("requires_capture"),
            Some(IntentStatus::RequiresCapture)
        );
        assert_eq!(
            IntentStatus::from_stripe_str("requires_action"),
            Some(IntentStatus::RequiresPaymentMethod)
        );
        assert_eq!(
            IntentStatus::from_stripe_str("cancelled"),
            Some(IntentStatus::Canceled)
        );
        assert_eq!(
            IntentStatus::from_stripe_str("requires_payment_method"),
            Some(IntentStatus::RequiresPaymentMethod)
        );
        // Неизвестный статус не мапится — вызывающая сторона получит Decode.
        assert_eq!(IntentStatus::from_stripe_str("completely_unknown"), None);
    }
}
