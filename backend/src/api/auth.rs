use axum::{
    extract::{ConnectInfo, State},
    http::{Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

// ── Per-IP rate limiting for public auth endpoints ─────────────────────────
//
// Registration/login (`/api/auth/challenge`, `/api/auth/verify`,
// `/api/auth/privy`) are unauthenticated by definition, which makes them the
// natural target for credential-stuffing / brute-force attempts. This is a
// dedicated, stricter limiter for just those routes (10 requests/minute/IP)
// independent of the coarser global token-bucket layered in main.rs.

const AUTH_RATE_LIMIT_MAX_REQUESTS: u32 = 10;
const AUTH_RATE_LIMIT_WINDOW: Duration = Duration::from_secs(60);

struct AuthRateWindow {
    count: u32,
    window_started_at: Instant,
}

/// Fixed-window counter keyed by client IP, shared across the auth routes.
#[derive(Clone)]
pub struct AuthRateLimiter {
    windows: Arc<Mutex<HashMap<String, AuthRateWindow>>>,
}

impl AuthRateLimiter {
    pub fn new() -> Self {
        Self {
            windows: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Returns `true` if the request should be allowed, `false` if the caller
    /// has exceeded `AUTH_RATE_LIMIT_MAX_REQUESTS` within the current window.
    async fn check(&self, key: &str) -> bool {
        let mut windows = self.windows.lock().await;
        let now = Instant::now();

        match windows.get_mut(key) {
            Some(w) if now.duration_since(w.window_started_at) >= AUTH_RATE_LIMIT_WINDOW => {
                w.count = 1;
                w.window_started_at = now;
                true
            }
            Some(w) if w.count < AUTH_RATE_LIMIT_MAX_REQUESTS => {
                w.count += 1;
                true
            }
            Some(_) => false,
            None => {
                windows.insert(
                    key.to_string(),
                    AuthRateWindow {
                        count: 1,
                        window_started_at: now,
                    },
                );
                true
            }
        }
    }
}

impl Default for AuthRateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

/// Best-effort client IP extraction: prefers the first hop of `X-Forwarded-For`
/// (set by a reverse proxy/load balancer), falling back to the socket's peer
/// address when the app is reached directly.
fn client_ip<B>(request: &Request<B>) -> String {
    request
        .headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .map(|ip| ip.trim().to_string())
        .filter(|ip| !ip.is_empty())
        .or_else(|| {
            request
                .extensions()
                .get::<ConnectInfo<SocketAddr>>()
                .map(|info| info.0.ip().to_string())
        })
        .unwrap_or_else(|| "unknown".to_string())
}

/// Axum middleware enforcing `AUTH_RATE_LIMIT_MAX_REQUESTS` per
/// `AUTH_RATE_LIMIT_WINDOW` per client IP. Responds `429 Too Many Requests`
/// on overflow.
pub async fn auth_rate_limit(
    State(limiter): State<AuthRateLimiter>,
    request: Request<axum::body::Body>,
    next: Next,
) -> Response {
    let ip = client_ip(&request);

    if limiter.check(&ip).await {
        next.run(request).await
    } else {
        tracing::warn!("Rate limit exceeded for IP {ip} on auth endpoint");
        (
            StatusCode::TOO_MANY_REQUESTS,
            Json(serde_json::json!({
                "error": "Too many requests. Please try again in a minute."
            })),
        )
            .into_response()
    }
}

/// Shared state for the auth router: the DB pool plus the Privy JWKS client
/// used to verify Privy-issued session tokens in `privy_auth`.
#[derive(Clone)]
pub struct AuthState {
    pub pool: sqlx::PgPool,
    pub privy: Arc<super::privy_jwks::PrivyJwksClient>,
    pub privy_app_id: String,
}

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
    State(state): State<AuthState>,
    Json(payload): Json<VerifyRequest>,
) -> impl IntoResponse {
    let pool = state.pool;
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
    State(state): State<AuthState>,
    Json(payload): Json<PrivyAuthRequest>,
) -> impl IntoResponse {
    let pool = state.pool;

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

    // Verify the Privy token's signature (against Privy's JWKS), expiry,
    // issuer and audience, and extract its claims.
    let claims: PrivyTokenPayload = match state
        .privy
        .verify_token(&payload.privy_token, &state.privy_app_id)
        .await
    {
        Ok(claims) => claims,
        Err(e) => {
            tracing::warn!("Privy token verification failed: {e}");
            return (
                axum::http::StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": "Privy token verification failed" })),
            )
                .into_response();
        }
    };

    // The verified token's subject must match the DID the client claims to be.
    if claims.subject != payload.privy_did {
        return (
            axum::http::StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({
                "error": "Privy token does not belong to the supplied privy_did"
            })),
        )
            .into_response();
    }

    // Issue #562: Verify that the submitted Stellar address is authorized in the Privy token
    if !stellar_address_in_linked_accounts(&claims, &payload.stellar_address) {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "The submitted Stellar address does not match any wallet linked to your Privy identity"
            })),
        )
            .into_response();
    }
    tracing::debug!(
        "Stellar address {} verified in Privy token payload",
        payload.stellar_address
    );

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

/// Issue #562/#563: Privy JWT claims, including linked accounts. Deserialized
/// only after `PrivyJwksClient::verify_token` has confirmed the token's
/// signature, expiry, issuer and audience.
#[derive(Debug, Deserialize)]
struct PrivyTokenPayload {
    #[serde(rename = "sub")]
    pub subject: String, // The Privy DID (e.g., "did:privy:user_abc123")
    pub exp: usize,
    pub iat: Option<usize>,
    #[serde(default)]
    pub linked_accounts: Vec<PrivyLinkedAccount>,
}

#[derive(Debug, Deserialize)]
struct PrivyLinkedAccount {
    #[serde(rename = "type")]
    pub account_type: String, // "wallet", "email", "phone", etc.
    pub address: Option<String>, // Wallet address (present when type="wallet")
    pub chain_type: Option<String>, // "stellar", "ethereum", "solana", etc.
    pub verified_at: Option<String>,
}

/// Issue #562: Checks whether `expected_stellar_address` appears among the
/// verified token's linked Stellar wallets, preventing a caller from
/// submitting an address that doesn't belong to their Privy identity.
fn stellar_address_in_linked_accounts(
    claims: &PrivyTokenPayload,
    expected_stellar_address: &str,
) -> bool {
    let stellar_wallets: Vec<&str> = claims
        .linked_accounts
        .iter()
        .filter(|acc| {
            acc.account_type == "wallet"
                && acc.chain_type.as_deref() == Some("stellar")
                && acc.address.is_some()
        })
        .filter_map(|acc| acc.address.as_deref())
        .collect();

    tracing::debug!(
        "Privy token contains {} Stellar wallet(s): {:?}",
        stellar_wallets.len(),
        stellar_wallets
    );

    let is_authorized = stellar_wallets
        .iter()
        .any(|&addr| addr.eq_ignore_ascii_case(expected_stellar_address));

    if !is_authorized {
        tracing::warn!(
            "Stellar address {} not found in Privy token's linked_accounts. Available: {:?}",
            expected_stellar_address,
            stellar_wallets
        );
    }

    is_authorized
}

#[cfg(test)]
mod rate_limit_tests {
    use super::*;

    #[tokio::test]
    async fn allows_up_to_the_limit_then_blocks() {
        let limiter = AuthRateLimiter::new();
        for i in 0..AUTH_RATE_LIMIT_MAX_REQUESTS {
            assert!(
                limiter.check("1.2.3.4").await,
                "request {i} should be allowed within the limit"
            );
        }
        assert!(
            !limiter.check("1.2.3.4").await,
            "request past the limit should be blocked"
        );
    }

    #[tokio::test]
    async fn tracks_each_ip_independently() {
        let limiter = AuthRateLimiter::new();
        for _ in 0..AUTH_RATE_LIMIT_MAX_REQUESTS {
            assert!(limiter.check("1.1.1.1").await);
        }
        assert!(
            !limiter.check("1.1.1.1").await,
            "1.1.1.1 should now be blocked"
        );
        assert!(
            limiter.check("2.2.2.2").await,
            "a different IP must have its own budget"
        );
    }

    #[test]
    fn client_ip_prefers_first_hop_of_x_forwarded_for() {
        let request = Request::builder()
            .header("x-forwarded-for", "203.0.113.9, 10.0.0.1")
            .body(axum::body::Body::empty())
            .unwrap();
        assert_eq!(client_ip(&request), "203.0.113.9");
    }

    #[test]
    fn client_ip_falls_back_to_unknown_without_any_source() {
        let request = Request::builder().body(axum::body::Body::empty()).unwrap();
        assert_eq!(client_ip(&request), "unknown");
    }
}

