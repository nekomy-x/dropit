//! F2: QR-токены, join-гранты, авто-закрытие по IANA-TZ.
//!
//! Правила (техкарта Часть II/VII):
//!   * QR-токен: 128 бит энтропии; в БД только sha256-хеш;
//!     ротация = деактивация старых + один активный токен;
//!   * expires_at QR — плановое авто-закрытие сессии;
//!   * «активность» сессии = last request/payment/WS-событие;
//!     авто-закрытие: 07:00 в IANA-зоне клуба при неактивности > 2 ч
//!     (Q5). Расчёт «следующего 07:00 в зоне» на каждом тике —
//!     DST-безопасно (не фикс. +24ч);
//!   * join: повторный скан QR той же сессии тем же гостем —
//!     re-grant, не дубль (UNIQUE(session_id, guest_device_id)).

use chrono::{DateTime, Duration, NaiveTime, TimeZone, Utc};

/// Минимальная длина plain-QR-токена (hex): 128 бит = 32 байта.
pub const QR_TOKEN_MIN_HEX_LEN: usize = 64;

/// Порог неактивности для авто-закрытия (Q5: 2 часа).
pub const INACTIVITY_THRESHOLD: Duration = Duration::hours(2);

/// Час закрытия в локальном времени клуба (Q5: 07:00).
pub const CLOSE_HOUR: u32 = 7;

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum QrError {
    #[error("qr token too short")]
    TokenTooShort,
    #[error("qr token not hex")]
    NotHex,
}

/// Валидирует формат plain-QR-токена (hex, ≥128 бит).
pub fn validate_qr_token(token: &str) -> Result<(), QrError> {
    if token.len() < QR_TOKEN_MIN_HEX_LEN {
        return Err(QrError::TokenTooShort);
    }
    if !token.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(QrError::NotHex);
    }
    Ok(())
}

/// Считает «следующий 07:00 в IANA-зоне» от момента `now`.
///
/// DST-безопасно: ищем ближайший момент, где локальное время в `tz`
/// равно 07:00 и он строго позже `now`. На каждом тике джоба
/// пересчитывает заново — смена зоны/законодательства подхватится.
pub fn next_close_at(tz: &chrono_tz::Tz, now: DateTime<Utc>) -> DateTime<Utc> {
    // Локальное «сегодня 07:00» уже прошло? Тогда «завтра 07:00».
    // Перебираем дни (максимум 2): в день перехода DST 07:00
    // существует всегда (переходы весной ~02:00–03:00).
    for day_offset in 0..=2i64 {
        let Some(date) = now
            .with_timezone(tz)
            .date_naive()
            .checked_add_signed(chrono::Duration::days(day_offset))
        else {
            continue;
        };
        let naive = date.and_time(NaiveTime::from_hms_opt(CLOSE_HOUR, 0, 0).expect("07:00 valid"));
        if let Some(candidate) = tz.from_local_datetime(&naive).single() {
            let utc = candidate.with_timezone(&Utc);
            if utc > now {
                return utc;
            }
        }
    }
    unreachable!("next 07:00 always exists within 2 days")
}

/// Решает, пора ли закрыть сессию авто-джобой.
///
/// Условие (Q5): текущее локальное время в зоне клуба — «ночь прошла»
/// (достигнут плановый close_at) и сессия неактивна ≥ 2 часов.
pub fn should_auto_close(
    last_activity: DateTime<Utc>,
    planned_close_at: DateTime<Utc>,
    now: DateTime<Utc>,
) -> bool {
    now >= planned_close_at && (now - last_activity) >= INACTIVITY_THRESHOLD
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Timelike;
    use chrono_tz::Europe::Madrid;

    #[test]
    fn qr_token_format_validated() {
        assert!(validate_qr_token(&"a".repeat(64)).is_ok());
        assert_eq!(
            validate_qr_token(&"a".repeat(63)),
            Err(QrError::TokenTooShort)
        );
        assert_eq!(validate_qr_token(&"z".repeat(64)), Err(QrError::NotHex));
    }

    #[test]
    fn next_close_is_next_7am_local() {
        // 2026-10-13 23:00 UTC = 01:00 в Мадриде (CEST, UTC+2):
        // следующий 07:00 локально = 05:00 UTC следующего дня.
        let now = Utc.with_ymd_and_hms(2026, 10, 13, 23, 0, 0).unwrap();
        let next = next_close_at(&Madrid, now);
        assert_eq!(next, Utc.with_ymd_and_hms(2026, 10, 14, 5, 0, 0).unwrap());
    }

    #[test]
    fn next_close_after_7am_local_is_tomorrow() {
        // 08:00 Мадрида (06:00 UTC) — 07:00 уже прошло → завтра.
        let now = Utc.with_ymd_and_hms(2026, 10, 14, 6, 0, 0).unwrap();
        let next = next_close_at(&Madrid, now);
        assert_eq!(next, Utc.with_ymd_and_hms(2026, 10, 15, 5, 0, 0).unwrap());
    }

    #[test]
    fn dst_fall_back_handled() {
        // 2026-10-25 — перевод часов в ЕС (CEST → CET, 03:00 → 02:00).
        // Сессия в ночь перехода: 23:00 UTC 24.10 = 01:00 CEST Мадрида;
        // следующий 07:00 локальный уже в CET = 06:00 UTC.
        let now = Utc.with_ymd_and_hms(2026, 10, 24, 23, 0, 0).unwrap();
        let next = next_close_at(&Madrid, now);
        assert_eq!(next, Utc.with_ymd_and_hms(2026, 10, 25, 6, 0, 0).unwrap());
        // И это корректное локальное время:
        let local = next.with_timezone(&Madrid);
        assert_eq!(local.hour(), 7);
    }

    #[test]
    fn auto_close_requires_both_conditions() {
        let now = Utc.with_ymd_and_hms(2026, 10, 14, 5, 30, 0).unwrap();
        let planned = Utc.with_ymd_and_hms(2026, 10, 14, 5, 0, 0).unwrap();
        // Достигли планового времени, но активность 1 ч назад → НЕ закрываем.
        let last = now - Duration::hours(1);
        assert!(!should_auto_close(last, planned, now));
        // Достигли и неактивность 2.5 ч → закрываем.
        let last = now - Duration::minutes(150);
        assert!(should_auto_close(last, planned, now));
        // Ещё не достигли планового времени → не закрываем даже при тишине.
        let planned_later = now + Duration::hours(1);
        assert!(!should_auto_close(last, planned_later, now));
    }
}
