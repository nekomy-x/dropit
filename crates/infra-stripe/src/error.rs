//! Ошибки транспорта Stripe.
//!
//! Один enum на все слои: подпись вебхука, ответ API, сеть, разбор JSON,
//! конфигурация. Публичный API крейта не использует `anyhow` — вызывающая
//! сторона (gateway-api, workers) сама решает, как мапить ошибку в конверт
//! `ApiError { code, message, details }`.

/// Ошибка клиента Stripe.
///
/// Варианты намеренно разделены: подпись (`SignatureInvalid`),
/// дрейф часов (`SignatureExpired`) и отказ API (`Api`) обрабатываются
/// по-разному — у каждого свой лог и своя метрика (техкарта, Часть VII).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StripeError {
    /// Подпись вебхука недействительна: битый заголовок, нет `v1=`,
    /// ни одна подпись не сошлась с HMAC, отсутствует сам заголовок.
    #[error("недействительная подпись вебхука: {0}")]
    SignatureInvalid(String),

    /// Timestamp вебхука вне допуска — это clock-drift, а не подделка.
    #[error("timestamp вебхука вне допуска (clock drift)")]
    SignatureExpired,

    /// Stripe ответил не-2xx. `code` — машинный код Stripe
    /// (`card_declined`, `rate_limit`, …), `message` — человекочитаемый текст.
    #[error("Stripe API {status}: {code}: {message}")]
    Api {
        /// HTTP-статус ответа (402, 409, 429, 500, …).
        status: u16,
        /// Машинный код ошибки Stripe (поле `error.code` / `error.type`).
        code: String,
        /// Текст ошибки Stripe (поле `error.message`).
        message: String,
    },

    /// Сетевой сбой: DNS, TLS, таймаут, обрыв соединения.
    #[error("транспортная ошибка Stripe: {0}")]
    Transport(String),

    /// Ответ (или payload вебхука) не разобрался как ожидаемый JSON.
    #[error("не удалось разобрать ответ Stripe: {0}")]
    Decode(String),

    /// Некорректная конфигурация клиента или аргументы вызова
    /// (пустой ключ, отрицательная сумма, битый `base_url`).
    #[error("некорректная конфигурация Stripe: {0}")]
    Config(String),
}

impl StripeError {
    /// Имеет ли смысл повторять вызов (retry с backoff).
    ///
    /// Повторяемы: транспортные сбои, 429 (rate limit) и 5xx.
    /// Не повторяемы: ошибки подписи, декодирования, конфигурации
    /// и бизнес-отказы API (402/409 — карта отклонена, конфликт идемпотентности).
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::Transport(_) => true,
            Self::Api { status, .. } => *status == 429 || *status >= 500,
            _ => false,
        }
    }

    /// HTTP-статус ответа, если ошибка пришла от API.
    pub fn api_status(&self) -> Option<u16> {
        match self {
            Self::Api { status, .. } => Some(*status),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retryable_classification() {
        assert!(StripeError::Transport("timeout".into()).is_retryable());
        assert!(StripeError::Api {
            status: 429,
            code: "rate_limit".into(),
            message: "slow down".into()
        }
        .is_retryable());
        assert!(StripeError::Api {
            status: 503,
            code: "api_error".into(),
            message: "unavailable".into()
        }
        .is_retryable());
        assert!(!StripeError::Api {
            status: 402,
            code: "card_declined".into(),
            message: "declined".into()
        }
        .is_retryable());
        assert!(!StripeError::SignatureExpired.is_retryable());
        assert!(!StripeError::Config("empty key".into()).is_retryable());
    }

    #[test]
    fn api_status_is_exposed() {
        let err = StripeError::Api {
            status: 409,
            code: "idempotency_error".into(),
            message: "keys must be unique".into(),
        };
        assert_eq!(err.api_status(), Some(409));
        assert_eq!(StripeError::SignatureExpired.api_status(), None);
    }
}
