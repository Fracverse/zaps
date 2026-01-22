use axum::{extract::Request, http::StatusCode, middleware::Next, response::Response};
use jsonwebtoken::{decode, DecodingKey, Validation};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String, // user_id
    pub exp: usize,
    pub iat: usize,
}

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

    // In production, get the secret from config
    let decoding_key = DecodingKey::from_secret(b"your-secret-key");

    match decode::<Claims>(token, &decoding_key, &Validation::default()) {
        Ok(token_data) => {
            // Add user_id to request extensions
            req.extensions_mut().insert(token_data.claims.sub);
            Ok(next.run(req).await)
        }
        Err(_) => Err(StatusCode::UNAUTHORIZED),
    }
}

pub async fn admin_only(req: Request, next: Next) -> Result<Response, StatusCode> {
    // First check if user is authenticated
    let user_id = req.extensions().get::<String>().cloned();
    if user_id.is_none() {
        return Err(StatusCode::UNAUTHORIZED);
    }

    // In production, check if user has admin role
    // For now, we'll allow all authenticated users
    Ok(next.run(req).await)
}

pub fn get_user_id_from_request(req: &Request) -> Option<String> {
    req.extensions().get::<String>().cloned()
}
