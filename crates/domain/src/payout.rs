//! Стейт-машина выплаты (техкарта, Часть III, Payout).
//!
//! scheduled → in_flight → paid
//! in_flight → failed → (retry ≤5, backoff) → on_hold (ручной разбор)
//!
//! Завершение Transfer отслеживается вебхуками
//! transfer.created/failed/paid (идемпотентно по
//! stripe_transfer_id) + часовой poller незакрытых in_flight.
//!
//! Порог (Q4): нетто < 10 € → перенос на след. неделю
//! (scheduled). Финальная выплата БЕЗ порога — при блокировке/уходе
//! клуба (иначе подпороговые деньги зависают навсегда).

use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PayoutStatus {
    Scheduled,
    InFlight,
    Paid,
    Failed,
    OnHold,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PayoutAction {
    /// Воркер запускает Transfer → in_flight.
    Send,
    /// Вебхук transfer.paid → paid.
    TransferPaid,
    /// Вебхук transfer.failed → failed.
    TransferFailed,
    /// Backoff-ретрей исчерпаны → on_hold (админ разбирает).
    Hold,
}

/// Максимум попыток Transfer до on_hold (техкарта: retry ≤5).
pub const MAX_PAYOUT_ATTEMPTS: i32 = 5;

/// Порог выплаты по умолчанию (Q4: 10 € в минорных единицах).
pub const DEFAULT_PAYOUT_THRESHOLD_MINOR: i64 = 1000;

#[derive(Debug, Error, PartialEq)]
#[error("invalid payout transition: {from:?} + {action:?}")]
pub struct InvalidPayoutTransition {
    pub from: PayoutStatus,
    pub action: PayoutAction,
}

impl PayoutStatus {
    pub fn apply(&self, action: PayoutAction) -> Result<PayoutStatus, InvalidPayoutTransition> {
        use PayoutAction::*;
        use PayoutStatus::*;
        match (*self, action) {
            (Scheduled, Send) => Ok(InFlight),
            (InFlight, TransferPaid) => Ok(Paid),
            (InFlight, TransferFailed) => Ok(Failed),
            (Failed, Send) => Ok(InFlight), // retry
            (Failed, Hold) => Ok(OnHold),   // попытки исчерпаны
            // NO-OP: повторные вебхуки
            (Paid, TransferPaid) => Ok(Paid),
            (OnHold, Hold) => Ok(OnHold),
            // Запрещено
            (from, a) => Err(InvalidPayoutTransition { from, action: a }),
        }
    }

    /// Выплата закрыта (успешно или нет — новых попыток не будет).
    pub fn is_terminal(&self) -> bool {
        matches!(self, PayoutStatus::Paid | PayoutStatus::OnHold)
    }
}

/// Решение payout-воркера по payee за период.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PayoutDecision {
    /// Создать payout и отправить Transfer.
    Send(i64),
    /// Ниже порога — перенос (Q4).
    BelowThreshold { net: i64, threshold: i64 },
    /// Нечего платить.
    Nothing,
}

/// Решает: платить ли нетто `net_minor` при пороге `threshold_minor`.
/// Финальная выплата (final=true) идёт без порога — при уходе/блокировке.
pub fn decide_payout(net_minor: i64, threshold_minor: i64, final_payout: bool) -> PayoutDecision {
    if net_minor <= 0 {
        return PayoutDecision::Nothing;
    }
    if final_payout || net_minor >= threshold_minor {
        PayoutDecision::Send(net_minor)
    } else {
        PayoutDecision::BelowThreshold {
            net: net_minor,
            threshold: threshold_minor,
        }
    }
}

/// Backoff между ретреями (экспоненциальный, сек; кап 1 ч).
pub fn retry_backoff_secs(attempt: i32) -> i64 {
    let base = 2i64.saturating_pow(attempt.clamp(1, 10) as u32);
    base.min(3600)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn happy_payout_path() {
        let s = PayoutStatus::Scheduled;
        let s = s.apply(PayoutAction::Send).unwrap();
        assert_eq!(s, PayoutStatus::InFlight);
        let s = s.apply(PayoutAction::TransferPaid).unwrap();
        assert_eq!(s, PayoutStatus::Paid);
        assert!(s.is_terminal());
    }

    #[test]
    fn failure_retry_then_hold() {
        let s = PayoutStatus::InFlight
            .apply(PayoutAction::TransferFailed)
            .unwrap();
        assert_eq!(s, PayoutStatus::Failed);
        // Retry после неудачи.
        let s = s.apply(PayoutAction::Send).unwrap();
        assert_eq!(s, PayoutStatus::InFlight);
        // Исчерпаны → on_hold.
        let s = PayoutStatus::Failed.apply(PayoutAction::Hold).unwrap();
        assert_eq!(s, PayoutStatus::OnHold);
        assert!(s.is_terminal());
    }

    #[test]
    fn duplicate_transfer_webhook_is_noop() {
        // transfer.paid пришёл дважды — штатно.
        assert_eq!(
            PayoutStatus::Paid
                .apply(PayoutAction::TransferPaid)
                .unwrap(),
            PayoutStatus::Paid
        );
    }

    #[test]
    fn threshold_carry_over() {
        // 8 € < 10 € → перенос.
        assert_eq!(
            decide_payout(800, 1000, false),
            PayoutDecision::BelowThreshold {
                net: 800,
                threshold: 1000
            }
        );
        // 10 € ровно → платим.
        assert_eq!(decide_payout(1000, 1000, false), PayoutDecision::Send(1000));
        // 8 € но ФИНАЛЬНАЯ (клуб заблокирован) → платим без порога.
        assert_eq!(decide_payout(800, 1000, true), PayoutDecision::Send(800));
        // Нетто ≤ 0 → ничего.
        assert_eq!(decide_payout(0, 1000, true), PayoutDecision::Nothing);
    }

    #[test]
    fn backoff_grows_but_capped() {
        assert_eq!(retry_backoff_secs(1), 2);
        assert_eq!(retry_backoff_secs(3), 8);
        assert!(retry_backoff_secs(20) <= 3600);
    }
}
