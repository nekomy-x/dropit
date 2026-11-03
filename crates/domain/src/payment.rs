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

/// Ошибки распределения возврата. Отдельный тип, чтобы вызывающий слой
/// отвечал `422`, а не `500`, на некорректную сумму/несходящийся сплит.
#[derive(Debug, Error, PartialEq)]
pub enum RefundError {
    #[error("payment amount must be positive")]
    NonPositiveTotal,
    #[error("refund amount must be within 1..=total")]
    RefundOutOfRange,
    #[error("share amount is negative")]
    NegativeShare,
    #[error("shares do not sum to the payment amount")]
    ShareSumMismatch,
    #[error("refund allocation does not fit the caller's integers")]
    Overflow,
}

/// Распределяет сумму возврата по долям пропорционально их размеру.
///
/// Входы — минорные единицы (`i64`), никаких float: возврат — это деньги,
/// а `0.1 + 0.2` не должен решать, кому уйдёт цент.
///
/// Правила (инварианты возврата):
///   * `0 < refund <= total`, иначе [`RefundError::RefundOutOfRange`];
///   * `Σ shares == total` — разрез платежа, а не произвольные числа;
///   * каждая доля возврата `floor(share * refund / total)` — пропорционально
///     ровно, без перекоса в чью-то пользу;
///   * остаток от округления (`refund − Σ floor`) раздаётся по одному центу
///     долям в переданном порядке, пока у доли есть непокрытый остаток.
///     Вызывающий передаёт доли в каноническом порядке `platform, club, dj`,
///     поэтому платформа первой забирает дробный цент — так же, как при
///     [`compute_split`], где остаток бипсис-деления уходит платформе;
///   * ни одна доля не получает больше, чем по ней осталось вернуть, и
///     `Σ результата == refund` — сумма возврата не теряется и не
///     задваивается.
///
/// `shares` — именно **остатки** долей (`amount − reversed`), а не исходный
/// разрез: при втором частичном возврате распределять нужно по тому, что
/// ещё не возвращено, иначе доля может уйти в минус.
pub fn allocate_refund(
    total_minor: i64,
    refund_minor: i64,
    shares: &[i64],
) -> Result<Vec<i64>, RefundError> {
    if total_minor <= 0 {
        return Err(RefundError::NonPositiveTotal);
    }
    if refund_minor <= 0 || refund_minor > total_minor {
        return Err(RefundError::RefundOutOfRange);
    }
    if shares.iter().any(|share| *share < 0) {
        return Err(RefundError::NegativeShare);
    }
    // i128: произведение долей не должно переполнить i64 даже на граничных
    // суммах; дальше всё возвращается в i64.
    let share_sum: i128 = shares.iter().map(|share| i128::from(*share)).sum();
    if share_sum != i128::from(total_minor) {
        return Err(RefundError::ShareSumMismatch);
    }

    let mut allocated = Vec::with_capacity(shares.len());
    let mut distributed: i64 = 0;
    for share in shares {
        let part = (i128::from(*share) * i128::from(refund_minor) / i128::from(total_minor))
            .try_into()
            .map_err(|_| RefundError::Overflow)?;
        // Σ floor ≤ refund, поэтому переполнения здесь нет.
        distributed += part;
        allocated.push(part);
    }

    let mut remainder = refund_minor - distributed;
    while remainder > 0 {
        let mut moved = false;
        for (index, share) in shares.iter().enumerate() {
            if remainder == 0 {
                break;
            }
            // Доля не может вернуть больше, чем по ней осталось.
            if allocated[index] < *share {
                allocated[index] += 1;
                remainder -= 1;
                moved = true;
            }
        }
        if !moved {
            // Недостижимо: Σ shares == total ≥ refund, значит свободного
            // остатка хватает. Если всё же случилось — не выдаём «почти
            // распределённый» возврат молча.
            return Err(RefundError::ShareSumMismatch);
        }
    }
    Ok(allocated)
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
    fn refund_full_returns_every_share_whole() {
        // 10 €: 300/350/350. Полный возврат — ровно исходные доли.
        let alloc = allocate_refund(1000, 1000, &[300, 350, 350]).unwrap();
        assert_eq!(alloc, vec![300, 350, 350]);
        assert_eq!(alloc.iter().sum::<i64>(), 1000);
    }

    #[test]
    fn refund_partial_is_proportional() {
        // 300 из 10 € → 90/105/105, Σ == 300.
        let alloc = allocate_refund(1000, 300, &[300, 350, 350]).unwrap();
        assert_eq!(alloc, vec![90, 105, 105]);
        assert_eq!(alloc.iter().sum::<i64>(), 300);
    }

    #[test]
    fn refund_remainder_goes_to_canonical_order() {
        // 1 цент из 10 €: floor везде 0, остаток отдаётся платформе первой.
        let alloc = allocate_refund(1000, 1, &[300, 350, 350]).unwrap();
        assert_eq!(alloc, vec![1, 0, 0]);
        // Неровный разрез: 7 центов → 2/2/3, возврат 3 → [1,1,1].
        let alloc = allocate_refund(7, 3, &[2, 2, 3]).unwrap();
        assert_eq!(alloc, vec![1, 1, 1]);
        assert_eq!(alloc.iter().sum::<i64>(), 3);
    }

    #[test]
    fn refund_never_exceeds_left_share() {
        // Остатки долей после первого возврата: 2/2/3 → возврат 4.
        // Платформа не может получить больше своих 2.
        let alloc = allocate_refund(7, 4, &[2, 2, 3]).unwrap();
        assert_eq!(alloc, vec![2, 1, 1]);
        assert_eq!(alloc.iter().sum::<i64>(), 4);
        for (part, left) in alloc.iter().zip([2, 2, 3]) {
            assert!(*part <= left, "refund share exceeds its own remainder");
        }
    }

    #[test]
    fn refund_zero_share_stays_zero() {
        // DJ не участвует (dj_minor = 0, строки сплита нет): весь возврат
        // делят платформа и клуб, Σ == 500.
        let alloc = allocate_refund(1000, 500, &[300, 700, 0]).unwrap();
        assert_eq!(alloc, vec![150, 350, 0]);
    }

    #[test]
    fn refund_validates_inputs() {
        assert_eq!(
            allocate_refund(0, 0, &[]),
            Err(RefundError::NonPositiveTotal)
        );
        assert_eq!(
            allocate_refund(1000, 0, &[300, 350, 350]),
            Err(RefundError::RefundOutOfRange)
        );
        assert_eq!(
            allocate_refund(1000, 1001, &[300, 350, 350]),
            Err(RefundError::RefundOutOfRange)
        );
        assert_eq!(
            allocate_refund(1000, 100, &[300, 350]),
            Err(RefundError::ShareSumMismatch)
        );
        assert_eq!(
            allocate_refund(1000, 100, &[300, 800, -100]),
            Err(RefundError::NegativeShare)
        );
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
