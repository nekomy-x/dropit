//! Единый формат ошибок API (ADR: контракт ошибок).
//!
//! Envelope: `{ code, message, details }` — `code` стабилен и используется
//! клиентами для i18n-ключа, `message` — человекочитаемый fallback.

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorEnvelope {
    /// Стабильный машиночитаемый код (i18n-ключ клиента).
    pub code: String,
    /// Человекочитаемое сообщение (fallback, не для отображения напрямую).
    pub message: String,
    /// Опциональные детали (идентификаторы, ограничения).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

/// Коды ошибок платформы. Расширение — только добавлением новых вариантов.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiError {
    // 4xx
    BadRequest,
    Unauthorized,
    Forbidden,
    NotFound,
    Conflict,
    ValidationError,
    RateLimited,
    SessionClosed,
    SessionPaused,
    // 5xx
    Internal,
    ExternalServiceUnavailable,
}

impl ApiError {
    pub fn http_status(&self) -> u16 {
        match self {
            ApiError::BadRequest => 400,
            ApiError::Unauthorized => 401,
            ApiError::Forbidden => 403,
            ApiError::NotFound => 404,
            ApiError::Conflict => 409,
            ApiError::ValidationError => 422,
            ApiError::RateLimited => 429,
            ApiError::SessionClosed | ApiError::SessionPaused => 409,
            ApiError::Internal => 500,
            ApiError::ExternalServiceUnavailable => 503,
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            ApiError::BadRequest => "bad_request",
            ApiError::Unauthorized => "unauthorized",
            ApiError::Forbidden => "forbidden",
            ApiError::NotFound => "not_found",
            ApiError::Conflict => "conflict",
            ApiError::ValidationError => "validation_error",
            ApiError::RateLimited => "rate_limited",
            ApiError::SessionClosed => "session_closed",
            ApiError::SessionPaused => "session_paused",
            ApiError::Internal => "internal",
            ApiError::ExternalServiceUnavailable => "external_unavailable",
        }
    }

    pub fn message(&self) -> &'static str {
        match self {
            ApiError::BadRequest => "The request is malformed.",
            ApiError::Unauthorized => "Authentication is required.",
            ApiError::Forbidden => "You do not have access to this resource.",
            ApiError::NotFound => "Resource not found.",
            ApiError::Conflict => "The request conflicts with the current state.",
            ApiError::ValidationError => "The request failed validation.",
            ApiError::RateLimited => "Too many requests. Try again later.",
            ApiError::SessionClosed => "This session is closed.",
            ApiError::SessionPaused => "This session is not accepting requests right now.",
            ApiError::Internal => "Internal server error.",
            ApiError::ExternalServiceUnavailable => "An external service is unavailable.",
        }
    }

    pub fn envelope(&self) -> ErrorEnvelope {
        ErrorEnvelope {
            code: self.code().to_string(),
            message: self.message().to_string(),
            details: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statuses_match_codes() {
        assert_eq!(ApiError::SessionClosed.http_status(), 409);
        assert_eq!(ApiError::SessionClosed.code(), "session_closed");
        assert_eq!(ApiError::RateLimited.http_status(), 429);
        assert_eq!(ApiError::Internal.http_status(), 500);
    }
}
