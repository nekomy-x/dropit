//! Роли и субъекты доступа (F1, техкарта Часть I §3).
//!
//! RBAC: роль фиксируется в JWT-клеймах, мидлварь gateway проверяет
//! соответствие роута и роли. Гость — отдельный тип субъекта: device-token,
//! не JWT-парольная сессия.

use serde::{Deserialize, Serialize};

/// Роль субъекта. Порядок вариантов — не иерархия: роли несравнимы,
/// мидлварь проверяет точное соответствие или принадлежность списку.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// Гость: device-token, без регистрации.
    Guest,
    /// Диджей клуба: email + пароль + TOTP.
    Dj,
    /// Клуб: email + пароль + TOTP.
    Club,
    /// Админ DROP IT: 2FA обязательна.
    Admin,
}

impl Role {
    /// Строковое имя (используется в JWT `role`-клейме и логах).
    pub fn as_str(&self) -> &'static str {
        match self {
            Role::Guest => "guest",
            Role::Dj => "dj",
            Role::Club => "club",
            Role::Admin => "admin",
        }
    }

    /// Парольные роли (для логина/refresh). Гость сюда не входит.
    pub fn is_password_role(&self) -> bool {
        matches!(self, Role::Dj | Role::Club | Role::Admin)
    }
}

/// Клеймы access-JWT (15 мин, stateless).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessClaims {
    /// Идентификатор субъекта (dj/club/admin id или guest_device id).
    pub sub: uuid::Uuid,
    /// Роль.
    pub role: Role,
    /// Семья refresh-токена (для ревока всей семьи при компрометации).
    pub fam: uuid::Uuid,
    /// Время выпуска (unix).
    pub iat: i64,
    /// Время истечения (unix).
    pub exp: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_serialization_roundtrip() {
        for role in [Role::Guest, Role::Dj, Role::Club, Role::Admin] {
            let json = serde_json::to_string(&role).unwrap();
            let back: Role = serde_json::from_str(&json).unwrap();
            assert_eq!(back, role);
        }
    }

    #[test]
    fn role_names_stable() {
        // Стабильные строки — на них завязан JWT и открытый API.
        assert_eq!(Role::Guest.as_str(), "guest");
        assert_eq!(Role::Dj.as_str(), "dj");
        assert_eq!(Role::Club.as_str(), "club");
        assert_eq!(Role::Admin.as_str(), "admin");
    }

    #[test]
    fn guest_is_not_password_role() {
        assert!(!Role::Guest.is_password_role());
        assert!(Role::Admin.is_password_role());
    }
}
