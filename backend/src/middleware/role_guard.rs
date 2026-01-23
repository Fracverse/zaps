//! Role guard middleware for role-based route protection
//!
//! This module provides middleware guards that restrict route access based on user roles.
//!
//! # Example
//! ```rust,ignore
//! use crate::middleware::role_guard::{require_role, require_any_role};
//! use crate::role::Role;
//!
//! // Require admin role
//! let admin_routes = Router::new()
//!     .route("/admin", get(admin_handler))
//!     .layer(axum::middleware::from_fn(require_role(Role::Admin)));
//!
//! // Require merchant or admin role
//! let merchant_routes = Router::new()
//!     .route("/merchant", get(merchant_handler))
//!     .layer(axum::middleware::from_fn(require_any_role(vec![Role::Merchant, Role::Admin])));
//! ```

use axum::{
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use std::sync::Arc;

use crate::middleware::auth::AuthenticatedUser;
use crate::role::Role;

/// Error response for authorization failures
fn forbidden_response(message: &str) -> Response {
    (
        StatusCode::FORBIDDEN,
        Json(json!({
            "error": "AUTHORIZATION_FAILED",
            "message": message,
            "code": "FORBIDDEN"
        })),
    )
        .into_response()
}

/// Create a middleware that requires a specific role
///
/// Returns 403 Forbidden if the user doesn't have the required role.
/// Admin role always passes any role check.
pub fn require_role(
    required_role: Role,
) -> impl Fn(Request, Next) -> std::pin::Pin<Box<dyn std::future::Future<Output = Response> + Send>>
       + Clone
       + Send
       + 'static {
    move |req: Request, next: Next| {
        let required = required_role;
        Box::pin(async move {
            let auth_user = match req.extensions().get::<AuthenticatedUser>() {
                Some(user) => user.clone(),
                None => {
                    return (StatusCode::UNAUTHORIZED, "Not authenticated").into_response();
                }
            };

            // Check if user has the required role
            if auth_user.role.has_permission(&required) {
                next.run(req).await
            } else {
                forbidden_response(&format!(
                    "Access denied. Required role: {}, your role: {}",
                    required, auth_user.role
                ))
            }
        })
    }
}

/// Create a middleware that requires any of the specified roles
///
/// Returns 403 Forbidden if the user doesn't have at least one of the required roles.
pub fn require_any_role(
    allowed_roles: Vec<Role>,
) -> impl Fn(Request, Next) -> std::pin::Pin<Box<dyn std::future::Future<Output = Response> + Send>>
       + Clone
       + Send
       + 'static {
    let roles = Arc::new(allowed_roles);
    move |req: Request, next: Next| {
        let roles = Arc::clone(&roles);
        Box::pin(async move {
            let auth_user = match req.extensions().get::<AuthenticatedUser>() {
                Some(user) => user.clone(),
                None => {
                    return (StatusCode::UNAUTHORIZED, "Not authenticated").into_response();
                }
            };

            // Check if user has any of the allowed roles
            let has_permission = roles.iter().any(|r| auth_user.role.has_permission(r));

            if has_permission {
                next.run(req).await
            } else {
                let roles_str: Vec<_> = roles.iter().map(|r| r.to_string()).collect();
                forbidden_response(&format!(
                    "Access denied. Required one of: [{}], your role: {}",
                    roles_str.join(", "),
                    auth_user.role
                ))
            }
        })
    }
}

/// Convenience middleware that requires admin role
pub fn admin_only(
) -> impl Fn(Request, Next) -> std::pin::Pin<Box<dyn std::future::Future<Output = Response> + Send>>
       + Clone
       + Send
       + 'static {
    require_role(Role::Admin)
}

/// Convenience middleware that requires merchant or admin role
pub fn merchant_or_admin(
) -> impl Fn(Request, Next) -> std::pin::Pin<Box<dyn std::future::Future<Output = Response> + Send>>
       + Clone
       + Send
       + 'static {
    require_any_role(vec![Role::Merchant, Role::Admin])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_role_permission_check() {
        // Admin should have permission for all roles
        assert!(Role::Admin.has_permission(&Role::Admin));
        assert!(Role::Admin.has_permission(&Role::Merchant));
        assert!(Role::Admin.has_permission(&Role::User));

        // Merchant should have permission for merchant and user
        assert!(!Role::Merchant.has_permission(&Role::Admin));
        assert!(Role::Merchant.has_permission(&Role::Merchant));
        assert!(Role::Merchant.has_permission(&Role::User));

        // User should only have permission for user
        assert!(!Role::User.has_permission(&Role::Admin));
        assert!(!Role::User.has_permission(&Role::Merchant));
        assert!(Role::User.has_permission(&Role::User));
    }
}
