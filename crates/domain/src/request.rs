//! Стейт-машина запроса (техкарта, Часть III).
//!
//! pending → accepted → played
//! pending → rejected (reason)
//! pending → expired (сессия закрыта / DJ на паузе > N мин)
//! accepted → played
//! accepted → rejected (передумал до игры)
//!
//! Идемпотентная семантика DJ-действий (правило техкарты):
//! повторный переход в то же состояние — Ok-NO-OP, а не ошибка;
//! `played` из `pending` разрешён (DJ сыграл, не нажав accept).

use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestStatus {
    Pending,
    Accepted,
    Rejected,
    Played,
    Expired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestAction {
    Accept,
    Reject,
    Play,
    Expire,
}

#[derive(Debug, Error, PartialEq)]
#[error("invalid request transition: {from:?} + {action:?}")]
pub struct InvalidRequestTransition {
    pub from: RequestStatus,
    pub action: RequestAction,
}

impl RequestStatus {
    /// Применяет действие DJ/системы с no-op-семантикой дублей.
    pub fn apply(&self, action: RequestAction) -> Result<RequestStatus, InvalidRequestTransition> {
        use RequestAction::*;
        use RequestStatus::*;
        match (*self, action) {
            // Штатные пути
            (Pending, Accept) => Ok(Accepted),
            (Pending, Reject) => Ok(Rejected),
            (Pending, Play) => Ok(Played), // DJ сыграл без accept — разрешено
            (Pending, Expire) => Ok(Expired),
            (Accepted, Play) => Ok(Played),
            (Accepted, Reject) => Ok(Rejected), // передумал до игры
            // NO-OP: повтор того же действия в то же состояние
            (Accepted, Accept) => Ok(Accepted),
            (Rejected, Reject) => Ok(Rejected),
            (Played, Play) => Ok(Played),
            (Expired, Expire) => Ok(Expired),
            // Запрещено
            (from, a) => Err(InvalidRequestTransition { from, action: a }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn happy_paths() {
        let s = RequestStatus::Pending;
        assert_eq!(
            s.apply(RequestAction::Accept).unwrap(),
            RequestStatus::Accepted
        );
        assert_eq!(s.apply(RequestAction::Play).unwrap(), RequestStatus::Played);
        assert_eq!(
            s.apply(RequestAction::Reject).unwrap(),
            RequestStatus::Rejected
        );
        assert_eq!(
            s.apply(RequestAction::Expire).unwrap(),
            RequestStatus::Expired
        );
    }

    #[test]
    fn played_from_pending_allowed() {
        // DJ сыграл трек, не приняв его явно
        assert_eq!(
            RequestStatus::Pending.apply(RequestAction::Play).unwrap(),
            RequestStatus::Played
        );
    }

    #[test]
    fn duplicate_actions_are_noop() {
        // Гонка accept/played на двух устройствах DJ: no-op, не ошибка
        assert_eq!(
            RequestStatus::Accepted
                .apply(RequestAction::Accept)
                .unwrap(),
            RequestStatus::Accepted
        );
        assert_eq!(
            RequestStatus::Played.apply(RequestAction::Play).unwrap(),
            RequestStatus::Played
        );
    }

    #[test]
    fn terminal_states_reject_new_actions() {
        assert!(RequestStatus::Played.apply(RequestAction::Accept).is_err());
        assert!(RequestStatus::Rejected.apply(RequestAction::Play).is_err());
        assert!(RequestStatus::Expired.apply(RequestAction::Accept).is_err());
    }
}
