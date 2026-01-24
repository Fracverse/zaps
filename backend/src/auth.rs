use crate::api_error::ApiError;
use bcrypt::{hash, verify, DEFAULT_COST};
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

/// Token type for distinguishing access vs refresh tokens
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TokenType {
    Access,
    Refresh,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String, // user_id
    pub exp: usize,
    pub iat: usize,
    pub token_type: TokenType,
}

/// Generate an access token (short-lived)
pub fn generate_access_token(
    user_id: &str,
    secret: &str,
    expiration_hours: i64,
) -> Result<String, jsonwebtoken::errors::Error> {
    generate_token(user_id, secret, expiration_hours, TokenType::Access)
}

/// Generate a refresh token (long-lived)
pub fn generate_refresh_token(
    user_id: &str,
    secret: &str,
    expiration_hours: i64,
) -> Result<String, jsonwebtoken::errors::Error> {
    generate_token(user_id, secret, expiration_hours, TokenType::Refresh)
}

fn generate_token(
    user_id: &str,
    secret: &str,
    expiration_hours: i64,
    token_type: TokenType,
) -> Result<String, jsonwebtoken::errors::Error> {
    let now = Utc::now();
    let expire = now + Duration::hours(expiration_hours);

    let claims = Claims {
        sub: user_id.to_string(),
        exp: expire.timestamp() as usize,
        iat: now.timestamp() as usize,
        token_type,
    };

    let header = Header::default();
    let encoding_key = EncodingKey::from_secret(secret.as_bytes());

    encode(&header, &claims, &encoding_key)
}

/// Validate a JWT token and return claims
pub fn validate_jwt(token: &str, secret: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
    let decoding_key = DecodingKey::from_secret(secret.as_bytes());
    let validation = Validation::default();

    let token_data = decode::<Claims>(token, &decoding_key, &validation)?;
    Ok(token_data.claims)
}

/// Validate that a token is specifically an access token
pub fn validate_access_token(
    token: &str,
    secret: &str,
) -> Result<Claims, jsonwebtoken::errors::Error> {
    let claims = validate_jwt(token, secret)?;
    if claims.token_type != TokenType::Access {
        return Err(jsonwebtoken::errors::Error::from(
            jsonwebtoken::errors::ErrorKind::InvalidToken,
        ));
    }
    Ok(claims)
}

/// Validate that a token is specifically a refresh token
pub fn validate_refresh_token(
    token: &str,
    secret: &str,
) -> Result<Claims, jsonwebtoken::errors::Error> {
    let claims = validate_jwt(token, secret)?;
    if claims.token_type != TokenType::Refresh {
        return Err(jsonwebtoken::errors::Error::from(
            jsonwebtoken::errors::ErrorKind::InvalidToken,
        ));
    }
    Ok(claims)
}

// Keep backward compatibility alias
pub fn generate_jwt(
    user_id: &str,
    secret: &str,
    expiration_hours: i64,
) -> Result<String, jsonwebtoken::errors::Error> {
    generate_access_token(user_id, secret, expiration_hours)
}

/// Hash a PIN using bcrypt with default cost (12)
pub fn hash_pin(pin: &str) -> Result<String, ApiError> {
    hash(pin, DEFAULT_COST).map_err(|_| ApiError::InternalServerError)
}

/// Verify a PIN against a bcrypt hash
pub fn verify_pin(pin: &str, hash: &str) -> Result<bool, ApiError> {
    verify(pin, hash).map_err(|_| ApiError::InternalServerError)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_SECRET: &str = "test-secret-key";

    #[test]
    fn test_access_token_generation_and_validation() {
        let user_id = "user123";
        let token = generate_access_token(user_id, TEST_SECRET, 24).expect("Failed to generate");

        let claims = validate_access_token(&token, TEST_SECRET).expect("Failed to validate");
        assert_eq!(claims.sub, user_id);
        assert_eq!(claims.token_type, TokenType::Access);
    }

    #[test]
    fn test_refresh_token_generation_and_validation() {
        let user_id = "user123";
        let token = generate_refresh_token(user_id, TEST_SECRET, 168).expect("Failed to generate");

        let claims = validate_refresh_token(&token, TEST_SECRET).expect("Failed to validate");
        assert_eq!(claims.sub, user_id);
        assert_eq!(claims.token_type, TokenType::Refresh);
    }

    #[test]
    fn test_access_token_rejected_as_refresh() {
        let token = generate_access_token("user123", TEST_SECRET, 24).expect("Failed to generate");

        let result = validate_refresh_token(&token, TEST_SECRET);
        assert!(result.is_err());
    }

    #[test]
    fn test_refresh_token_rejected_as_access() {
        let token =
            generate_refresh_token("user123", TEST_SECRET, 168).expect("Failed to generate");

        let result = validate_access_token(&token, TEST_SECRET);
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_secret_rejected() {
        let token = generate_access_token("user123", TEST_SECRET, 24).expect("Failed to generate");

        let result = validate_jwt(&token, "wrong-secret");
        assert!(result.is_err());
    }
}
