//! Unit tests for authorization and role guards
//!
//! These tests verify role-based access control functionality.

use blinks_backend::role::Role;
use std::str::FromStr;

#[cfg(test)]
mod role_tests {
    use super::*;

    #[test]
    fn test_role_from_str() {
        assert_eq!(Role::from_str("admin").unwrap(), Role::Admin);
        assert_eq!(Role::from_str("Admin").unwrap(), Role::Admin);
        assert_eq!(Role::from_str("ADMIN").unwrap(), Role::Admin);
        assert_eq!(Role::from_str("merchant").unwrap(), Role::Merchant);
        assert_eq!(Role::from_str("Merchant").unwrap(), Role::Merchant);
        assert_eq!(Role::from_str("user").unwrap(), Role::User);
        assert_eq!(Role::from_str("unknown").unwrap(), Role::User); // Default to User
        assert_eq!(Role::from_str("").unwrap(), Role::User);
    }

    #[test]
    fn test_role_as_str() {
        assert_eq!(Role::Admin.as_str(), "admin");
        assert_eq!(Role::Merchant.as_str(), "merchant");
        assert_eq!(Role::User.as_str(), "user");
    }

    #[test]
    fn test_role_display() {
        assert_eq!(format!("{}", Role::Admin), "admin");
        assert_eq!(format!("{}", Role::Merchant), "merchant");
        assert_eq!(format!("{}", Role::User), "user");
    }

    #[test]
    fn test_role_default() {
        assert_eq!(Role::default(), Role::User);
    }

    #[test]
    fn test_admin_has_all_permissions() {
        assert!(Role::Admin.has_permission(&Role::Admin));
        assert!(Role::Admin.has_permission(&Role::Merchant));
        assert!(Role::Admin.has_permission(&Role::User));
    }

    #[test]
    fn test_merchant_permissions() {
        assert!(!Role::Merchant.has_permission(&Role::Admin));
        assert!(Role::Merchant.has_permission(&Role::Merchant));
        assert!(Role::Merchant.has_permission(&Role::User));
    }

    #[test]
    fn test_user_permissions() {
        assert!(!Role::User.has_permission(&Role::Admin));
        assert!(!Role::User.has_permission(&Role::Merchant));
        assert!(Role::User.has_permission(&Role::User));
    }

    #[test]
    fn test_role_serialization() {
        let admin = Role::Admin;
        let json = serde_json::to_string(&admin).unwrap();
        assert_eq!(json, "\"admin\"");

        let merchant = Role::Merchant;
        let json = serde_json::to_string(&merchant).unwrap();
        assert_eq!(json, "\"merchant\"");

        let user = Role::User;
        let json = serde_json::to_string(&user).unwrap();
        assert_eq!(json, "\"user\"");
    }

    #[test]
    fn test_role_deserialization() {
        let admin: Role = serde_json::from_str("\"admin\"").unwrap();
        assert_eq!(admin, Role::Admin);

        let merchant: Role = serde_json::from_str("\"merchant\"").unwrap();
        assert_eq!(merchant, Role::Merchant);

        let user: Role = serde_json::from_str("\"user\"").unwrap();
        assert_eq!(user, Role::User);
    }

    #[test]
    fn test_role_equality() {
        assert_eq!(Role::Admin, Role::Admin);
        assert_eq!(Role::Merchant, Role::Merchant);
        assert_eq!(Role::User, Role::User);
        assert_ne!(Role::Admin, Role::Merchant);
        assert_ne!(Role::Merchant, Role::User);
        assert_ne!(Role::Admin, Role::User);
    }

    #[test]
    fn test_role_clone() {
        let admin = Role::Admin;
        let cloned = admin;
        assert_eq!(admin, cloned);
    }
}

#[cfg(test)]
mod jwt_tests {
    use super::*;
    use blinks_backend::auth::{generate_access_token, validate_jwt};

    #[test]
    fn test_jwt_with_user_role() {
        let token = generate_access_token("user123", Role::User, "test-secret", 1).unwrap();
        let claims = validate_jwt(&token, "test-secret").unwrap();

        assert_eq!(claims.sub, "user123");
        assert_eq!(claims.role, Role::User);
    }

    #[test]
    fn test_jwt_with_admin_role() {
        let token = generate_access_token("admin123", Role::Admin, "test-secret", 1).unwrap();
        let claims = validate_jwt(&token, "test-secret").unwrap();

        assert_eq!(claims.sub, "admin123");
        assert_eq!(claims.role, Role::Admin);
    }

    #[test]
    fn test_jwt_with_merchant_role() {
        let token = generate_access_token("merchant123", Role::Merchant, "test-secret", 1).unwrap();
        let claims = validate_jwt(&token, "test-secret").unwrap();

        assert_eq!(claims.sub, "merchant123");
        assert_eq!(claims.role, Role::Merchant);
    }

    #[test]
    fn test_jwt_invalid_token() {
        let result = validate_jwt("invalid-token", "test-secret");
        assert!(result.is_err());
    }

    #[test]
    fn test_jwt_wrong_secret() {
        let token = generate_access_token("user123", Role::User, "secret1", 1).unwrap();
        let result = validate_jwt(&token, "secret2");
        assert!(result.is_err());
    }

    #[test]
    fn test_jwt_role_preserved_in_claims() {
        for role in [Role::User, Role::Merchant, Role::Admin] {
            let token = generate_access_token("testuser", role, "secret", 1).unwrap();
            let claims = validate_jwt(&token, "secret").unwrap();
            assert_eq!(claims.role, role, "Role should be preserved in JWT claims");
        }
    }
}

#[cfg(test)]
mod authenticated_user_tests {
    use super::*;
    use blinks_backend::middleware::auth::AuthenticatedUser;

    #[test]
    fn test_authenticated_user_creation() {
        let user = AuthenticatedUser {
            user_id: "user123".to_string(),
            role: Role::Admin,
        };

        assert_eq!(user.user_id, "user123");
        assert_eq!(user.role, Role::Admin);
    }

    #[test]
    fn test_authenticated_user_clone() {
        let user = AuthenticatedUser {
            user_id: "user123".to_string(),
            role: Role::Merchant,
        };

        let cloned = user.clone();
        assert_eq!(user.user_id, cloned.user_id);
        assert_eq!(user.role, cloned.role);
    }

    #[test]
    fn test_authenticated_user_serialization() {
        let user = AuthenticatedUser {
            user_id: "user123".to_string(),
            role: Role::User,
        };

        let json = serde_json::to_string(&user).unwrap();
        assert!(json.contains("\"user_id\":\"user123\""));
        assert!(json.contains("\"role\":\"user\""));

        let deserialized: AuthenticatedUser = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.user_id, user.user_id);
        assert_eq!(deserialized.role, user.role);
    }
}
