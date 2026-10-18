//! `dropit-infra-stripe` — транспорт и криптография Stripe.
//!
//! Крейт реализует настоящий REST-клиент Stripe ([`LiveStripe`]) и
//! детерминированный мок ([`MockStripe`]) за одним трейтом [`StripeApi`].
//! Покрытие: PaymentIntent (create/capture/refund), Connect-онбординг
//! (account + account_link), Transfers, и проверка подписи вебхуков
//! (`t=,v1=`, HMAC-SHA256, допуск по timestamp).
//!
//! # Границы крейта
//!
//! Только транспорт и криптография: **без БД, без axum, без бизнес-логики
//! сплита**. Схема Stripe-сплита (destination charges vs separate transfers)
//! фиксируется ADR-004 (открыт); маппинг `connected_account` здесь
//! provisional — `transfer_data[destination]` + `application_fee_amount`
//! добавляется вызывающим слоем при закрытии ADR-004.
//!
//! Деньги — целые минорные единицы (`i64`), нулей и плавающей точки нет
//! (инвариант ADR-003).
//!
//! # Пример: создание live-клиента из окружения
//!
//! ```no_run
//! use dropit_infra_stripe::{from_env, LiveStripe};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // В dev/CI переменные обычно не заданы → Ok(None) → остаёмся на MockStripe.
//! if let Some((api_key, webhook_secret)) = from_env() {
//!     let stripe = LiveStripe::new(api_key, webhook_secret, None)?;
//!     let _ = stripe;
//! }
//! # Ok(())
//! # }
//! ```
//!
//! # Пример: проверка вебхука
//!
//! ```no_run
//! use dropit_infra_stripe::{verify_webhook_event, WEBHOOK_TOLERANCE_SECS};
//!
//! # fn main() {
//! let _ = verify_webhook_event("whsec_x", b"{}", "t=0,v1=x", WEBHOOK_TOLERANCE_SECS);
//! # }
//! ```

#![deny(missing_docs)]

mod api;
mod env;
mod error;
mod form;
mod json;
mod live;
mod mock;
mod types;
mod webhook;

pub use api::StripeApi;
pub use env::{from_env, ENV_API_KEY, ENV_WEBHOOK_SECRET};
pub use error::StripeError;
pub use form::CONNECT_ACCOUNT_TYPE;
pub use live::{LiveStripe, DEFAULT_BASE_URL, REQUEST_TIMEOUT_SECS, STRIPE_API_VERSION};
pub use mock::MockStripe;
pub use types::{
    CaptureMethod, ConnectAccountReq, ConnectedAccount, CreateIntent, IntentResult, IntentStatus,
    RefundReq, RefundResult, TransferReq, TransferResult, WebhookEvent,
};
pub use webhook::{verify_webhook_event, SIGNATURE_HEADER, WEBHOOK_TOLERANCE_SECS};
