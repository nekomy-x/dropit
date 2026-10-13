//! Стейт-машина сессии (техкарта, Часть III).
//!
//! created → active → paused_acceptance ⇄ active → closed
//!
//! Недопустимый переход — ошибка `InvalidTransition`, а не молчаливое
//! изменение состояния. Это же правило будет в БД-триггере/оптимистичной
//! блокировке на уровне infra.

use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Created,
    Active,
    PausedAcceptance,
    Closed,
}

#[derive(Debug, Error, PartialEq)]
#[error("invalid session transition: {from:?} → {to:?}")]
pub struct InvalidSessionTransition {
    pub from: SessionStatus,
    pub to: SessionStatus,
}

/// Одноразовое событие перехода (не состояние).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionTransition {
    Open,
    Pause,
    Resume,
    Close,
}

impl SessionStatus {
    /// Применяет переход, возвращая новое состояние или ошибку.
    /// Единственный источник истины о допустимости переходов.
    pub fn apply(&self, t: SessionTransition) -> Result<SessionStatus, InvalidSessionTransition> {
        use SessionStatus::*;
        use SessionTransition::*;
        match (*self, t) {
            (Created, Open) => Ok(Active),
            (Active, Pause) => Ok(PausedAcceptance),
            (PausedAcceptance, Resume) => Ok(Active),
            (Active, Close) | (PausedAcceptance, Close) | (Created, Close) => Ok(Closed),
            (from, _) => Err(InvalidSessionTransition {
                from,
                to: target_of(t),
            }),
        }
    }

    /// Принимает ли сессия новые запросы (guard оплаты/запроса).
    pub fn accepts_requests(&self) -> bool {
        matches!(self, SessionStatus::Active)
    }
}

fn target_of(t: SessionTransition) -> SessionStatus {
    match t {
        SessionTransition::Open => SessionStatus::Active,
        SessionTransition::Pause => SessionStatus::PausedAcceptance,
        SessionTransition::Resume => SessionStatus::Active,
        SessionTransition::Close => SessionStatus::Closed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn happy_path() {
        let mut s = SessionStatus::Created;
        s = s.apply(SessionTransition::Open).unwrap();
        assert!(s.accepts_requests());
        s = s.apply(SessionTransition::Pause).unwrap();
        assert!(!s.accepts_requests());
        s = s.apply(SessionTransition::Resume).unwrap();
        s = s.apply(SessionTransition::Close).unwrap();
        assert_eq!(s, SessionStatus::Closed);
        assert!(!s.accepts_requests());
    }

    #[test]
    fn closed_is_terminal() {
        let closed = SessionStatus::Closed;
        assert!(closed.apply(SessionTransition::Open).is_err());
        assert!(closed.apply(SessionTransition::Resume).is_err());
    }

    #[test]
    fn cannot_pause_created() {
        assert!(SessionStatus::Created
            .apply(SessionTransition::Pause)
            .is_err());
    }
}
