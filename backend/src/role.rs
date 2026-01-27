use serde::{Deserialize, Serialize};
use std::fmt;

/// User roles for authorization
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// Standard user with basic access
    #[default]
    User,
    /// Merchant with payment-related permissions
    Merchant,
    /// Administrator with full system access
    Admin,
}

impl Role {
    /// Parse role from string (case-insensitive)
    pub fn from_string(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "admin" => Role::Admin,
            "merchant" => Role::Merchant,
            _ => Role::User,
        }
    }

    /// Convert role to string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            Role::User => "user",
            Role::Merchant => "merchant",
            Role::Admin => "admin",
        }
    }

    /// Check if this role has at least the permissions of another role
    pub fn has_permission(&self, required: &Role) -> bool {
        match (self, required) {
            // Admin has all permissions
            (Role::Admin, _) => true,
            // Merchant has merchant and user permissions
            (Role::Merchant, Role::Merchant | Role::User) => true,
            // User only has user permissions
            (Role::User, Role::User) => true,
            _ => false,
        }
    }
}

// impl Default for Role {
//     fn default() -> Self {
//         Role::User
//     }
// }

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_role_from_str() {
        assert_eq!(Role::from_string("admin"), Role::Admin);
        assert_eq!(Role::from_string("ADMIN"), Role::Admin);
        assert_eq!(Role::from_string("merchant"), Role::Merchant);
        assert_eq!(Role::from_string("user"), Role::User);
        assert_eq!(Role::from_string("unknown"), Role::User);
    }

    #[test]
    fn test_role_as_str() {
        assert_eq!(Role::Admin.as_str(), "admin");
        assert_eq!(Role::Merchant.as_str(), "merchant");
        assert_eq!(Role::User.as_str(), "user");
    }

    #[test]
    fn test_role_permissions() {
        // Admin can do everything
        assert!(Role::Admin.has_permission(&Role::Admin));
        assert!(Role::Admin.has_permission(&Role::Merchant));
        assert!(Role::Admin.has_permission(&Role::User));

        // Merchant can do merchant and user things
        assert!(!Role::Merchant.has_permission(&Role::Admin));
        assert!(Role::Merchant.has_permission(&Role::Merchant));
        assert!(Role::Merchant.has_permission(&Role::User));

        // User can only do user things
        assert!(!Role::User.has_permission(&Role::Admin));
        assert!(!Role::User.has_permission(&Role::Merchant));
        assert!(Role::User.has_permission(&Role::User));
    }

    #[test]
    fn test_role_serialization() {
        let admin = Role::Admin;
        let json = serde_json::to_string(&admin).unwrap();
        assert_eq!(json, "\"admin\"");

        let parsed: Role = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, Role::Admin);
    }

    #[test]
    fn test_role_default() {
        assert_eq!(Role::default(), Role::User);
    }
}
