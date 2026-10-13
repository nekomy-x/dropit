//! Деньги в минорных единицах (ADR-003).
//!
//! `MinorUnits` — новоеtyped-обёртка над i64 (центы). Никаких float в денежных
//! расчётах: сложение/вычитание проверяются на переполнение.
//! `Money` связывает сумму с валютой ISO-4217.

use serde::{Deserialize, Serialize};
use std::str::FromStr;
use thiserror::Error;

/// Ошибки денежных операций.
#[derive(Debug, Error, PartialEq)]
pub enum MoneyError {
    /// Сумма вне допустимого диапазона.
    #[error("amount overflow: {0}")]
    Overflow(i64),
    /// Некорректный код валюты (ожидается 3 заглавные буквы ISO-4217).
    #[error("invalid currency code: {0}")]
    InvalidCurrency(String),
}

/// Сумма в минорных единицах (центах). i64 достаточно для всех сценариев
/// платформы; переполнение ловится на операциях.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MinorUnits(pub i64);

impl MinorUnits {
    pub const ZERO: MinorUnits = MinorUnits(0);

    pub fn checked_add(self, rhs: MinorUnits) -> Result<MinorUnits, MoneyError> {
        self.0
            .checked_add(rhs.0)
            .map(MinorUnits)
            .ok_or(MoneyError::Overflow(rhs.0))
    }

    pub fn checked_sub(self, rhs: MinorUnits) -> Result<MinorUnits, MoneyError> {
        self.0
            .checked_sub(rhs.0)
            .map(MinorUnits)
            .ok_or(MoneyError::Overflow(rhs.0))
    }
}

/// Валюта ISO-4217 (строго 3 заглавные буквы).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Currency(pub String);

impl FromStr for Currency {
    type Err = MoneyError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.len() == 3 && s.chars().all(|c| c.is_ascii_uppercase()) {
            Ok(Currency(s.to_string()))
        } else {
            Err(MoneyError::InvalidCurrency(s.to_string()))
        }
    }
}

/// Деньги: сумма в минорных единицах + валюта.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Money {
    pub amount: MinorUnits,
    pub currency: Currency,
}

impl Money {
    pub fn new(amount: i64, currency: &str) -> Result<Self, MoneyError> {
        Ok(Money {
            amount: MinorUnits(amount),
            currency: currency.parse()?,
        })
    }

    pub fn checked_add(&self, rhs: &Money) -> Result<Money, MoneyError> {
        if self.currency != rhs.currency {
            // Разные валюты нельзя складывать напрямую (ADR-003: счёт = (account, currency)).
            return Err(MoneyError::InvalidCurrency(format!(
                "currency mismatch: {} vs {}",
                self.currency.0, rhs.currency.0
            )));
        }
        Ok(Money {
            amount: self.amount.checked_add(rhs.amount)?,
            currency: self.currency.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minor_units_add() {
        let a = MinorUnits(100);
        let b = MinorUnits(150);
        assert_eq!(a.checked_add(b).unwrap(), MinorUnits(250));
    }

    #[test]
    fn minor_units_overflow_detected() {
        let a = MinorUnits(i64::MAX);
        assert!(a.checked_add(MinorUnits(1)).is_err());
    }

    #[test]
    fn currency_validation() {
        assert!("EUR".parse::<Currency>().is_ok());
        assert!("eur".parse::<Currency>().is_err());
        assert!("EURO".parse::<Currency>().is_err());
    }

    #[test]
    fn money_mismatched_currency_rejected() {
        let a = Money::new(100, "EUR").unwrap();
        let b = Money::new(100, "USD").unwrap();
        assert!(a.checked_add(&b).is_err());
    }
}
