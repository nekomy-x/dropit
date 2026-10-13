//! Стейт-машина платежа (техкарта, Часть III, Payment).
//!
//! intent → processing → captured → partially_refunded → refunded
//! intent → failed      (отказ провайдера; failure_reason + код)
//! intent → cancelling  (TTL-воркер: отмена ИНТЕНТА у провайдера)
//! cancelling → cancelled   (провайдер подтвердил отмену)
//! cancelling → captured    (гонка: захват успел раньше отмены —
//!                           деньги у гостя! → обычный путь capture)
//! intent → cancelled   (гость закрыл экран, TTL 15 мин)
//! captured → captured: NO-OP (дубль вебхука с другим event_id —
//!                       штатно, не 500!)
//!
//! Идемпотентность обработчиков — по (aggregate_id, event_type),
//! НЕ по event_id (у провайдера он разный для
//! payment_intent.succeeded и charge.succeeded).

use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaymentStatus {
    Intent,
    Processing,
    Captured,
    PartiallyRefunded,
    Refunded,
    Failed,
    Cancelling,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaymentAction {
    /// Гость подтвердил оплату (confirm) → processing.
    Confirm,
    /// Вебхук успеха: деньги захвачены → captured.
    Capture,
    /// Отказ провайдера/карты → failed.
    Fail,
    /// Начинаем отмену (TTL/пауза) → cancelling.
    BeginCancel,
    /// Провайдер подтвердил отмену → cancelled.
    CancelConfirmed,
    /// Частичный возврат.
    PartialRefund,
    /// Полный возврат.
    Refund,
}

#[derive(Debug, Error, PartialEq)]
#[error("invalid payment transition: {from:?} + {action:?}")]
pub struct InvalidPaymentTransition {
    pub from: PaymentStatus,
    pub action: PaymentAction,
}

impl PaymentStatus {
    /// Применяет действие. NO-OP-семантика:
    /// captured + Capture = Ok(Captured) — дубль вебхука штатно.
    pub fn apply(&self, action: PaymentAction) -> Result<PaymentStatus, InvalidPaymentTransition> {
        use PaymentAction::*;
        use PaymentStatus::*;
        match (*self, action) {
            // Штатный путь
            (Intent, Confirm) => Ok(Processing),
            (Processing, Capture) => Ok(Captured),
            (Processing, Fail) => Ok(Failed),
            (Intent, Fail) => Ok(Failed),
            (Intent, BeginCancel) => Ok(Cancelling),
            (Processing, BeginCancel) => Ok(Cancelling),
            (Intent, CancelConfirmed) => Ok(Cancelled), // TTL: гость ушёл
            (Cancelling, CancelConfirmed) => Ok(Cancelled),
            // ГОНКА: захват успел раньше отмены — деньги у гостя!
            // → обычный путь capture, закрываем в ledger.
            (Cancelling, Capture) => Ok(Captured),
            // Возвраты (механика с первого дня — политика, не отсутствие)
            (Captured, PartialRefund) => Ok(PartiallyRefunded),
            (Captured, Refund) => Ok(Refunded),
            (PartiallyRefunded, PartialRefund) => Ok(PartiallyRefunded), // повтор части
            (PartiallyRefunded, Refund) => Ok(Refunded),
            // NO-OP: дубль вебхука (разные event_id, одно состояние)
            (Captured, Capture) => Ok(Captured),
            (Failed, Fail) => Ok(Failed),
            (Cancelled, CancelConfirmed) => Ok(Cancelled),
            // Запрещено
            (from, a) => Err(InvalidPaymentTransition { from, action: a }),
        }
    }

    /// Деньги захвачены (ledger должен быть закрыт).
    pub fn is_captured(&self) -> bool {
        matches!(
            self,
            PaymentStatus::Captured | PaymentStatus::PartiallyRefunded | PaymentStatus::Refunded
        )
    }
}

/// Разрез суммы платежа по правилам (ADR-003, техкарта Часть VI).
/// Сплит считается от P − C (P = цена, C = комиссия платформы);
/// остаток P − C делится клуб/диджей по dj_share_bp; остаток цента
/// от бипсис-деления → в комиссию платформы (не теряется).
///
/// Все входы — минорные единицы; float запрещён (ADR-003).
#[derive(Debug, Clone, PartialEq)]
pub struct Split {
    /// Комиссия платформы.
    pub platform_minor: i64,
    /// Доля клуба.
    pub club_minor: i64,
    /// Доля диджея.
    pub dj_minor: i64,
}

/// Считает сплит. `commission_bp` — комиссия платформы в бипсисах
/// (10000 = 100%); `dj_share_bp` — доля DJ в остатке после комиссии.
pub fn compute_split(
    amount_minor: i64,
    commission_bp: i32,
    dj_share_bp: i32,
) -> Result<Split, SplitError> {
    if amount_minor < 0 {
        return Err(SplitError::NegativeAmount);
    }
    if !(0..=10_000).contains(&commission_bp) || !(0..=10_000).contains(&dj_share_bp) {
        return Err(SplitError::BadBasisPoints);
    }
    // Комиссия: P * bp / 10000, округление вниз; остаток цента
    // последнего деления уйдёт платформе.
    let platform_commission = amount_minor * i64::from(commission_bp) / 10_000;
    let remainder = amount_minor - platform_commission;
    // DJ-доля остатка; club получает остаток минус dj — дробный цент
    // остаётся клубу... нет: по §1.1 arch-doc остаток цента → в
    // комиссию платформы. Значит: dj = floor, club = remainder - dj.
    let dj_minor = remainder * i64::from(dj_share_bp) / 10_000;
    let club_minor = remainder - dj_minor;
    Ok(Split {
        platform_minor: platform_commission,
        club_minor,
        dj_minor,
    })
}

#[derive(Debug, Error, PartialEq)]
pub enum SplitError {
    #[error("negative amount")]
    NegativeAmount,
    #[error("basis points out of range 0..10000")]
    BadBasisPoints,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn happy_capture_path() {
        let s = PaymentStatus::Intent;
        let s = s.apply(PaymentAction::Confirm).unwrap();
        assert_eq!(s, PaymentStatus::Processing);
        let s = s.apply(PaymentAction::Capture).unwrap();
        assert_eq!(s, PaymentStatus::Captured);
        assert!(s.is_captured());
    }

    #[test]
    fn duplicate_webhook_capture_is_noop() {
        // Дубль вебхука с ДРУГИМ event_id — штатно, не ошибка.
        let s = PaymentStatus::Captured;
        assert_eq!(
            s.apply(PaymentAction::Capture).unwrap(),
            PaymentStatus::Captured
        );
    }

    #[test]
    fn cancel_race_captured_wins() {
        // ГОНКА: cancel начал, захват успел раньше → captured
        // (деньги у гостя — закрываем в ledger обычным путём).
        let s = PaymentStatus::Cancelling;
        assert_eq!(
            s.apply(PaymentAction::Capture).unwrap(),
            PaymentStatus::Captured
        );
        // А подтверждение отмены после захвата — уже нельзя.
        assert!(PaymentStatus::Captured
            .apply(PaymentAction::CancelConfirmed)
            .is_err());
    }

    #[test]
    fn ttl_paths() {
        // Гость закрыл экран: intent → cancelled напрямую.
        assert_eq!(
            PaymentStatus::Intent
                .apply(PaymentAction::CancelConfirmed)
                .unwrap(),
            PaymentStatus::Cancelled
        );
        // Либо через cancelling (воркер успел начать).
        let s = PaymentStatus::Cancelling;
        assert_eq!(
            s.apply(PaymentAction::CancelConfirmed).unwrap(),
            PaymentStatus::Cancelled
        );
    }

    #[test]
    fn refunds_supported_from_day_one() {
        let s = PaymentStatus::Captured;
        let s = s.apply(PaymentAction::PartialRefund).unwrap();
        assert_eq!(s, PaymentStatus::PartiallyRefunded);
        let s = s.apply(PaymentAction::Refund).unwrap();
        assert_eq!(s, PaymentStatus::Refunded);
    }

    #[test]
    fn split_30_percent_50_50() {
        // Q1-дефолт: 30% комиссия, остаток 50/50.
        // 10.00 € = 1000 центов → комиссия 300, остаток 700 →
        // dj 350, club 350. Σ = 1000 ✅.
        let s = compute_split(1000, 3000, 5000).unwrap();
        assert_eq!(s.platform_minor, 300);
        assert_eq!(s.dj_minor, 350);
        assert_eq!(s.club_minor, 350);
        assert_eq!(s.platform_minor + s.dj_minor + s.club_minor, 1000);
    }

    #[test]
    fn split_cent_remainder_not_lost() {
        // 7 центов, 30%: комиссия floor(7*3000/10000)=2, остаток 5,
        // dj floor(5*5000/10000)=2, club 3. Σ=7 — копейка не теряется.
        let s = compute_split(7, 3000, 5000).unwrap();
        assert_eq!(s.platform_minor + s.dj_minor + s.club_minor, 7);
    }

    #[test]
    fn split_validates_inputs() {
        assert_eq!(
            compute_split(-1, 3000, 5000),
            Err(SplitError::NegativeAmount)
        );
        assert_eq!(
            compute_split(100, 10_001, 5000),
            Err(SplitError::BadBasisPoints)
        );
        assert_eq!(
            compute_split(100, 3000, -1),
            Err(SplitError::BadBasisPoints)
        );
    }
}
