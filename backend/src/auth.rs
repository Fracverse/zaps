use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

use crate::role::Role;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub sub: String, // user_id
    pub role: Role,  // user role
    pub exp: usize,  // expiration timestamp
    pub iat: usize,  // issued at timestamp
}

pub fn generate_jwt(
    user_id: &str,
    role: Role,
    secret: &str,
    expiration_hours: i64,
) -> Result<String, jsonwebtoken::errors::Error> {
    let now = Utc::now();
    let expire = now + Duration::hours(expiration_hours);

    let claims = Claims {
        sub: user_id.to_string(),
        role,
        exp: expire.timestamp() as usize,
        iat: now.timestamp() as usize,
    };

    let header = Header::default();
    let encoding_key = EncodingKey::from_secret(secret.as_bytes());

    encode(&header, &claims, &encoding_key)
}

pub fn validate_jwt(token: &str, secret: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
    let decoding_key = DecodingKey::from_secret(secret.as_bytes());
    let validation = Validation::default();

    let token_data = decode::<Claims>(token, &decoding_key, &validation)?;
    Ok(token_data.claims)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_and_validate_jwt() {
        let secret = "test-secret-key";
        let user_id = "user123";
        let role = Role::Admin;

        let token = generate_jwt(user_id, role, secret, 1).unwrap();
        let claims = validate_jwt(&token, secret).unwrap();

        assert_eq!(claims.sub, user_id);
        assert_eq!(claims.role, Role::Admin);
    }

    #[test]
    fn test_jwt_with_different_roles() {
        let secret = "test-secret-key";

        for role in [Role::User, Role::Merchant, Role::Admin] {
            let token = generate_jwt("testuser", role, secret, 1).unwrap();
            let claims = validate_jwt(&token, secret).unwrap();
            assert_eq!(claims.role, role);
        }
    }

    #[test]
    fn test_invalid_token() {
        let result = validate_jwt("invalid-token", "secret");
        assert!(result.is_err());
    }

    #[test]
    fn test_wrong_secret() {
        let token = generate_jwt("user123", Role::User, "secret1", 1).unwrap();
        let result = validate_jwt(&token, "secret2");
        assert!(result.is_err());
    }
}
