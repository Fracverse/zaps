use axum::{extract::State, response::IntoResponse, Json};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sqlx::Row;

#[derive(Serialize)]
pub struct ChallengeResponse {
    pub challenge: String,
}

#[derive(Deserialize)]
pub struct VerifyRequest {
    pub address: String,
    pub signature: String,
    pub challenge: String,
}

#[derive(Serialize)]
pub struct AuthResponse {
    pub token: String,
    pub username: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub exp: usize,
}

#[derive(Deserialize)]
pub struct PrivyAuthRequest {
    pub privy_token: String,
    pub privy_did: String,
    pub stellar_address: String,
}

#[derive(Serialize)]
pub struct PrivyAuthResponse {
    pub token: String,
    pub username: String,
    pub privy_did: String,
}

pub async fn get_challenge() -> impl IntoResponse {
    // Generate cryptographically secure mock challenge using UUID v4
    let challenge = uuid::Uuid::new_v4().to_string();
    Json(ChallengeResponse { challenge })
}

pub async fn verify_signature(
    State(pool): State<sqlx::PgPool>,
    Json(payload): Json<VerifyRequest>,
) -> impl IntoResponse {
    let message_bytes = payload.challenge.as_bytes();

    let signature_bytes = if let Ok(bytes) = hex::decode(&payload.signature) {
        bytes
    } else if let Ok(bytes) =
        base64::Engine::decode(&base64::prelude::BASE64_STANDARD, &payload.signature)
    {
        bytes
    } else {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(
                serde_json::json!({ "error": "Invalid signature format (must be hex or base64)" }),
            ),
        )
            .into_response();
    };

    if !verify_stellar_sig(&payload.address, message_bytes, &signature_bytes) {
        return (
            axum::http::StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "Signature verification failed" })),
        )
            .into_response();
    }

    // Check if user exists in database, if not create them
    let username_prefix = format!("u_{}", &payload.address[1..15]);

    let row = match sqlx::query(
        r#"
        INSERT INTO users (address, username, display_name)
        VALUES ($1, $2, $3)
        ON CONFLICT (address)
        DO UPDATE SET address = users.address
        RETURNING id, username
        "#,
    )
    .bind(&payload.address)
    .bind(&username_prefix)
    .bind(Some(&username_prefix))
    .fetch_one(&pool)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("Database query error in verify_signature: {:?}", e);
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Internal database error" })),
            )
                .into_response();
        }
    };

    let username: String = row.get("username");

    // Generate JWT token
    let secret = std::env::var("JWT_SECRET")
        .unwrap_or_else(|_| "zaps-jwt-secret-placeholder-very-long-key".into());
    let expiration = chrono::Utc::now()
        .checked_add_signed(chrono::Duration::days(1))
        .expect("valid timestamp")
        .timestamp() as usize;

    let claims = Claims {
        sub: payload.address.clone(),
        exp: expiration,
    };

    let token = match jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &claims,
        &jsonwebtoken::EncodingKey::from_secret(secret.as_bytes()),
    ) {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("JWT generation failed: {:?}", e);
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to generate authentication token" })),
            )
                .into_response();
        }
    };

    Json(AuthResponse {
        token,
        username: Some(username),
    })
    .into_response()
}

/// POST /api/auth/privy - Create new user account linked to Privy identity
/// Verifies Privy token, links DID to Stellar address, and returns JWT credentials.
pub async fn privy_auth(
    State(pool): State<sqlx::PgPool>,
    Json(payload): Json<PrivyAuthRequest>,
) -> impl IntoResponse {
    // Validate Stellar address format
    if !is_valid_stellar_address(&payload.stellar_address) {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Invalid Stellar address format" })),
        )
            .into_response();
    }

    // Validate Privy DID format (basic check - DIDs typically follow did:* pattern)
    if !payload.privy_did.starts_with("did:") || payload.privy_did.len() < 10 {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Invalid Privy DID format" })),
        )
            .into_response();
    }

    // Verify Privy token (mock verification for now - integrate with actual Privy SDK)
    if !verify_privy_token(&payload.privy_token, &payload.privy_did).await {
        return (
            axum::http::StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "Privy token verification failed" })),
        )
            .into_response();
    }

    // Check if Stellar address is already linked to a different Privy DID
    match sqlx::query("SELECT privy_did FROM users WHERE address = $1")
        .bind(&payload.stellar_address)
        .fetch_optional(&pool)
        .await
    {
        Ok(Some(row)) => {
            let existing_did: Option<String> = row.get("privy_did");
            if let Some(existing_did) = existing_did {
                if existing_did != payload.privy_did {
                    return (
                        axum::http::StatusCode::CONFLICT,
                        Json(serde_json::json!({
                            "error": "This Stellar address is already linked to a different Privy identity"
                        })),
                    )
                        .into_response();
                }
                // DID already linked to this address, proceed to generate token
            }
        }
        Ok(None) => {
            // Address not in DB, will be created
        }
        Err(e) => {
            tracing::error!("Database query error checking address: {:?}", e);
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Internal database error" })),
            )
                .into_response();
        }
    }

    // Check if Privy DID is already linked to a different Stellar address
    match sqlx::query("SELECT address FROM users WHERE privy_did = $1")
        .bind(&payload.privy_did)
        .fetch_optional(&pool)
        .await
    {
        Ok(Some(row)) => {
            let existing_address: String = row.get("address");
            if existing_address != payload.stellar_address {
                return (
                    axum::http::StatusCode::CONFLICT,
                    Json(serde_json::json!({
                        "error": "This Privy identity is already linked to a different Stellar address"
                    })),
                )
                    .into_response();
            }
        }
        Ok(None) => {
            // DID not in DB, will be created
        }
        Err(e) => {
            tracing::error!("Database query error checking DID: {:?}", e);
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Internal database error" })),
            )
                .into_response();
        }
    }

    // Create or update user with Privy DID linkage
    let username_prefix = format!("u_{}", &payload.stellar_address[1..15]);
    let now = chrono::Utc::now();

    let row = match sqlx::query(
        r#"
        INSERT INTO users (address, username, display_name, privy_did, privy_linked_at)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (address)
        DO UPDATE SET
            privy_did = EXCLUDED.privy_did,
            privy_linked_at = EXCLUDED.privy_linked_at
        RETURNING id, username, privy_did
        "#,
    )
    .bind(&payload.stellar_address)
    .bind(&username_prefix)
    .bind(Some(&username_prefix))
    .bind(&payload.privy_did)
    .bind(now)
    .fetch_one(&pool)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            // Handle UNIQUE constraint violation on privy_did
            if e.to_string().contains("privy_did") {
                tracing::warn!("Privy DID constraint violation: {:?}", e);
                return (
                    axum::http::StatusCode::CONFLICT,
                    Json(serde_json::json!({
                        "error": "This Privy identity is already linked to another account"
                    })),
                )
                    .into_response();
            }
            tracing::error!("Database query error in privy_auth: {:?}", e);
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Internal database error" })),
            )
                .into_response();
        }
    };

    let username: String = row.get("username");
    let privy_did: String = row.get("privy_did");

    // Generate JWT token
    let secret = std::env::var("JWT_SECRET")
        .unwrap_or_else(|_| "zaps-jwt-secret-placeholder-very-long-key".into());
    let expiration = chrono::Utc::now()
        .checked_add_signed(chrono::Duration::days(1))
        .expect("valid timestamp")
        .timestamp() as usize;

    let claims = Claims {
        sub: payload.stellar_address.clone(),
        exp: expiration,
    };

    let token = match jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &claims,
        &jsonwebtoken::EncodingKey::from_secret(secret.as_bytes()),
    ) {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("JWT generation failed: {:?}", e);
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to generate authentication token" })),
            )
                .into_response();
        }
    };

    (
        axum::http::StatusCode::CREATED,
        Json(PrivyAuthResponse {
            token,
            username,
            privy_did,
        }),
    )
        .into_response()
}

fn verify_stellar_sig(address: &str, message: &[u8], signature_bytes: &[u8]) -> bool {
    let decoded = match decode_base32(address) {
        Some(d) => d,
        None => return false,
    };
    if decoded.len() != 35 {
        return false;
    }
    if decoded[0] != 0x30 {
        // G prefix (48 in base32 version byte)
        return false;
    }
    let pubkey_bytes = &decoded[1..33];
    let checksum_bytes = &decoded[33..35];

    let calculated_crc = crc16(&decoded[0..33]);
    let expected_crc = ((checksum_bytes[1] as u16) << 8) | (checksum_bytes[0] as u16);
    if calculated_crc != expected_crc {
        return false;
    }

    let verifying_key = match VerifyingKey::from_bytes(pubkey_bytes.try_into().unwrap()) {
        Ok(k) => k,
        Err(_) => return false,
    };

    let sig = match Signature::from_slice(signature_bytes) {
        Ok(s) => s,
        Err(_) => return false,
    };

    verifying_key.verify(message, &sig).is_ok()
}

fn decode_base32(s: &str) -> Option<Vec<u8>> {
    let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let mut bits = 0u32;
    let mut bit_count = 0;
    let mut result = Vec::new();

    for &byte in s.as_bytes() {
        let val = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'2'..=b'7' => byte - b'2' + 26,
            _ => return None,
        };
        bits = (bits << 5) | (val as u32);
        bit_count += 5;
        if bit_count >= 8 {
            bit_count -= 8;
            result.push((bits >> bit_count) as u8);
        }
    }
    Some(result)
}

fn crc16(data: &[u8]) -> u16 {
    let mut crc: u16 = 0;
    for &byte in data {
        let mut code = (crc >> 8) ^ (byte as u16);
        code ^= code >> 4;
        crc = (crc << 8) ^ (code << 12) ^ (code << 5) ^ code;
    }
    crc
}

/// Validates Stellar address format (56 chars, G-prefix, valid checksum)
fn is_valid_stellar_address(address: &str) -> bool {
    if address.len() != 56 {
        return false;
    }
    if !address.starts_with('G') {
        return false;
    }
    // Decode and validate checksum
    match decode_base32(address) {
        Some(decoded) => {
            if decoded.len() != 35 {
                return false;
            }
            if decoded[0] != 0x30 {
                return false;
            }
            let checksum_bytes = &decoded[33..35];
            let calculated_crc = crc16(&decoded[0..33]);
            let expected_crc = ((checksum_bytes[1] as u16) << 8) | (checksum_bytes[0] as u16);
            calculated_crc == expected_crc
        }
        None => false,
    }
}

/// Verifies Privy token and DID linkage
/// Note: This is a placeholder implementation. In production, verify against Privy's API.
/// Reference: https://docs.privy.com/reference
async fn verify_privy_token(token: &str, did: &str) -> bool {
    // TODO: In production, call Privy's verification endpoint:
    // POST https://auth.privy.io/api/v1/verify_token
    // with the token and validate the returned DID matches the one provided
    
    // For now, perform basic validation:
    // - Token should be a non-empty string
    // - DID should match expected format
    if token.trim().is_empty() || did.trim().is_empty() {
        return false;
    }
    
    // In a real implementation, you would:
    // 1. Decode the JWT token
    // 2. Verify the signature using Privy's public key
    // 3. Extract and validate the DID claim
    // 4. Check token expiration
    
    // For this implementation, we assume the client provides a valid Privy token
    // and we perform server-side verification in production
    tracing::debug!("Privy token verification (placeholder - implement with actual Privy SDK)");
    true
}

