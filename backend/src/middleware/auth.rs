use axum::{
    async_trait,
    extract::{FromRequestParts, Request, State},
    http::{request::Parts, StatusCode},
    middleware::Next,
    response::Response,
};
use std::sync::Arc;

use crate::{auth, service::ServiceContainer};

/// Represents an authenticated user extracted from the JWT token
#[derive(Debug, Clone)]
pub struct AuthenticatedUser {
    pub user_id: String,
}

/// Middleware function that validates JWT tokens from Authorization header
pub async fn authenticate(
    State(services): State<Arc<ServiceContainer>>,
    mut req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let auth_header = req
        .headers()
        .get("authorization")
        .and_then(|header| header.to_str().ok())
        .and_then(|header| header.strip_prefix("Bearer "));

    let token = match auth_header {
        Some(token) => token,
        None => return Err(StatusCode::UNAUTHORIZED),
    };

    // Validate as access token using secret from config
    match auth::validate_access_token(token, &services.config.jwt.secret) {
        Ok(claims) => {
            // Add authenticated user to request extensions
            req.extensions_mut().insert(AuthenticatedUser {
                user_id: claims.sub,
            });
            Ok(next.run(req).await)
        }
        Err(_) => Err(StatusCode::UNAUTHORIZED),
    }
}

/// Middleware for admin-only routes
pub async fn admin_only(req: Request, next: Next) -> Result<Response, StatusCode> {
    // Check if user is authenticated
    let user = req.extensions().get::<AuthenticatedUser>().cloned();
    if user.is_none() {
        return Err(StatusCode::UNAUTHORIZED);
    }

    // In production, check if user has admin role
    // For now, we'll allow all authenticated users
    Ok(next.run(req).await)
}

/// Axum extractor for getting the authenticated user from request
#[async_trait]
impl<S> FromRequestParts<S> for AuthenticatedUser
where
    S: Send + Sync,
{
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<AuthenticatedUser>()
            .cloned()
            .ok_or(StatusCode::UNAUTHORIZED)
    }
}

/// Helper function for backward compatibility ()
pub fn get_user_id_from_request(req: &Request) -> Option<String> {
    req.extensions()
        .get::<AuthenticatedUser>()
        .map(|u| u.user_id.clone())
}
