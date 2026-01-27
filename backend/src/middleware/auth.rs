use crate::role::Role;
use crate::{auth, service::ServiceContainer};
use axum::{
    async_trait,
    extract::{FromRequestParts, Request, State},
    http::{request::Parts, StatusCode},
    middleware::Next,
    response::Response,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Authenticated user information extracted from JWT
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthenticatedUser {
    pub user_id: String,
    pub role: Role,
}

/// Authentication middleware - validates JWT and extracts user info
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
            let auth_user = AuthenticatedUser {
                user_id: claims.sub,
                role: claims.role,
            };
            req.extensions_mut().insert(auth_user);
            Ok(next.run(req).await)
        }
        Err(_) => Err(StatusCode::UNAUTHORIZED),
    }
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

/// Get authenticated user from request extensions
pub fn get_authenticated_user(req: &Request) -> Option<AuthenticatedUser> {
    req.extensions().get::<AuthenticatedUser>().cloned()
}

/// Legacy helper for backwards compatibility
pub fn get_user_id_from_request(req: &Request) -> Option<String> {
    get_authenticated_user(req).map(|u| u.user_id)
}
