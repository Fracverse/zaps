use axum::{extract::Request, http::StatusCode, middleware::Next, response::Response};
use jsonwebtoken::{decode, DecodingKey, Validation};
use serde::{Deserialize, Serialize};

use crate::role::Role;

/// Authenticated user information extracted from JWT
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthenticatedUser {
    pub user_id: String,
    pub role: Role,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String, // user_id
    pub role: Role,
    pub exp: usize,
    pub iat: usize,
}

/// Authentication middleware - validates JWT and extracts user info
pub async fn authenticate(mut req: Request, next: Next) -> Result<Response, StatusCode> {
    let auth_header = req
        .headers()
        .get("authorization")
        .and_then(|header| header.to_str().ok())
        .and_then(|header| header.strip_prefix("Bearer "));

    let token = match auth_header {
        Some(token) => token,
        None => return Err(StatusCode::UNAUTHORIZED),
    };

    // In production, get the secret from config via state
    let decoding_key = DecodingKey::from_secret(b"your-secret-key");

    match decode::<Claims>(token, &decoding_key, &Validation::default()) {
        Ok(token_data) => {
            let auth_user = AuthenticatedUser {
                user_id: token_data.claims.sub,
                role: token_data.claims.role,
            };
            req.extensions_mut().insert(auth_user);
            Ok(next.run(req).await)
        }
        Err(_) => Err(StatusCode::UNAUTHORIZED),
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
