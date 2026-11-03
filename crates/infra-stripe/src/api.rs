//! Трейт абстракции над Stripe: транспорт + криптография, без бизнес-логики.
//!
//! Реализации:
//!  * [`crate::MockStripe`] — детерминированный мок для dev/test (без сети);
//!  * [`crate::LiveStripe`] — настоящий REST-клиент + HMAC-проверка вебхуков.
//!
//! Бизнес-логика сплита (`compute_split`, доли клуба/DJ, Stripe-fee), БД и axum
//! сюда не попадают — это ответственность `dropit-domain` / `gateway-api`.

use crate::error::StripeError;
use crate::types::{
    ConnectAccountReq, ConnectedAccount, CreateIntent, IntentResult, RefundReq, RefundResult,
    TransferReq, TransferResult, WebhookEvent,
};

/// Абстракция над Stripe API: платежи, Connect-онбординг, выплаты, вебхуки.
///
/// Все сетевые методы — асинхронные (`#[async_trait]`); `verify_webhook` —
/// синхронный (чистая криптография + разбор JSON, без I/O).
#[async_trait::async_trait]
pub trait StripeApi: Send + Sync {
    /// Only a non-network simulator may enable the test payment method.
    fn is_mock(&self) -> bool { false }

    /// `POST /v1/payment_intents` — создание PaymentIntent.
    async fn create_payment_intent(&self, req: CreateIntent) -> Result<IntentResult, StripeError>;

    /// Recover an existing checkout after a lost HTTP response, without
    /// creating or authorizing another payment.
    async fn retrieve_payment_intent(&self, _intent_id: &str) -> Result<IntentResult, StripeError> {
        Err(StripeError::Config("payment intent retrieval unsupported".into()))
    }

    /// `POST /v1/payment_intents/{id}/capture` — захват холда.
    /// `amount_minor = None` → полный захват.
    async fn capture(
        &self,
        intent_id: &str,
        amount_minor: Option<i64>,
        idempotency_key: &str,
    ) -> Result<IntentResult, StripeError>;

    /// `POST /v1/refunds` — возврат средств.
    async fn refund(&self, req: RefundReq) -> Result<RefundResult, StripeError>;

    /// `POST /v1/accounts` — создание Connect-аккаунта.
    async fn create_connected_account(
        &self,
        req: ConnectAccountReq,
    ) -> Result<ConnectedAccount, StripeError>;

    /// `GET /v1/accounts/{id}` — состояние Connect-аккаунта.
    async fn retrieve_account(&self, account_id: &str) -> Result<ConnectedAccount, StripeError>;

    /// `POST /v1/account_links` — URL онбординга Stripe-hosted.
    /// Возвращает `url` (строка).
    async fn account_link(
        &self,
        account_id: &str,
        refresh_url: &str,
        return_url: &str,
    ) -> Result<String, StripeError>;

    /// `POST /v1/transfers` — выплата на Connect-аккаунт.
    async fn create_transfer(&self, req: TransferReq) -> Result<TransferResult, StripeError>;

    /// Проверка подписи вебхука (`t=,v1=`, HMAC-SHA256, допуск `tolerance_secs`)
    /// и разбор события. Без I/O.
    fn verify_webhook(
        &self,
        payload: &[u8],
        signature_header: &str,
        tolerance_secs: i64,
    ) -> Result<WebhookEvent, StripeError>;
}
