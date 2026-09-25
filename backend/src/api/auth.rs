use axum::{
    extract::{ConnectInfo, State},
    http::{header, HeaderValue, Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use redis::aio::{ConnectionManager, ConnectionManagerConfig};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::{
    collections::{HashMap, VecDeque},
    net::SocketAddr,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::sync::Mutex;

// ── Per-IP rate limiting for public auth endpoints ─────────────────────────
//
// Registration/login (`/api/auth/challenge`, `/api/auth/verify`,
// `/api/auth/privy`) are unauthenticated by definition, which makes them the
// natural target for credential-stuffing / brute-force attempts. This is a
// dedicated, stricter limiter for just those routes (10 requests/minute/IP)
// independent of the coarser global token-bucket layered in main.rs.
//
// #949: The window is a true sliding window (a log of request timestamps per
// IP), so a client cannot burst 2x the limit across a fixed-window boundary.
// With Redis configured the log is a sorted set shared by every API instance;
// without Redis — or while Redis is unreachable — an in-process log is used
// so the auth routes are never left unlimited.

const AUTH_RATE_LIMIT_MAX_REQUESTS: u32 = 10;
const AUTH_RATE_LIMIT_WINDOW: Duration = Duration::from_secs(60);

const REDIS_KEY_PREFIX: &str = "zaps:ratelimit:auth:";
const REDIS_CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
const REDIS_RESPONSE_TIMEOUT: Duration = Duration::from_millis(500);

/// Prune idle IPs from the in-process log once it holds this many keys.
const LOCAL_PRUNE_THRESHOLD: usize = 10_000;

/// Atomic sliding-window-log check. Trims entries older than the window,
/// then records this request only if the IP is under the limit, so rejected
/// requests don't extend a lockout.
///
/// KEYS[1] = per-IP key; ARGV = now_ms, window_ms, limit, unique member.
/// Returns `{1, 0}` when allowed, `{0, retry_after_ms}` when limited.
const SLIDING_WINDOW_LUA: &str = r#"
local key = KEYS[1]
local now = tonumber(ARGV[1])
local window = tonumber(ARGV[2])
local limit = tonumber(ARGV[3])
redis.call('ZREMRANGEBYSCORE', key, '-inf', now - window)
if redis.call('ZCARD', key) < limit then
  redis.call('ZADD', key, now, ARGV[4])
  redis.call('PEXPIRE', key, window)
  return {1, 0}
end
local oldest = redis.call('ZRANGE', key, 0, 0, 'WITHSCORES')
local retry_after = window
if oldest[2] then
  retry_after = math.max(tonumber(oldest[2]) + window - now, 1)
end
return {0, retry_after}
"#;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateLimitDecision {
    Allowed,
    Limited { retry_after: Duration },
}

/// Sliding-window limiter keyed by client IP, shared across the auth routes.
#[derive(Clone)]
pub struct AuthRateLimiter {
    redis: Option<ConnectionManager>,
    local: Arc<Mutex<HashMap<String, VecDeque<Instant>>>>,
    max_requests: u32,
    window: Duration,
}

impl AuthRateLimiter {
    /// In-process limiter only (single instance, tests).
    pub fn new() -> Self {
        Self {
            redis: None,
            local: Arc::new(Mutex::new(HashMap::new())),
            max_requests: AUTH_RATE_LIMIT_MAX_REQUESTS,
            window: AUTH_RATE_LIMIT_WINDOW,
        }
    }

    /// #949: Redis-backed limiter shared across instances. Connects lazily,
    /// so an unreachable Redis at boot doesn't stop the API from starting.
    pub fn with_redis(redis_url: &str) -> Result<Self, redis::RedisError> {
        let client = redis::Client::open(redis_url)?;
        let config = ConnectionManagerConfig::new()
            .set_connection_timeout(Some(REDIS_CONNECT_TIMEOUT))
            .set_response_timeout(Some(REDIS_RESPONSE_TIMEOUT));
        Ok(Self {
            redis: Some(ConnectionManager::new_lazy_with_config(client, config)?),
            ..Self::new()
        })
    }

    /// Redis-backed when `redis_url` is set and valid, in-process otherwise.
    pub fn from_redis_url(redis_url: Option<&str>) -> Self {
        match redis_url.map(Self::with_redis) {
            Some(Ok(limiter)) => {
                tracing::info!("Auth rate limiter using Redis sliding window");
                limiter
            }
            Some(Err(e)) => {
                tracing::error!(
                    "Failed to initialize Redis auth rate limiter, using in-process limiter: {e}"
                );
                Self::new()
            }
            None => {
                tracing::warn!("REDIS_URL not set; auth rate limiter is per-instance only");
                Self::new()
            }
        }
    }

    /// Record a request from `key` and decide whether it may proceed.
    pub async fn check(&self, key: &str) -> RateLimitDecision {
        if let Some(redis) = &self.redis {
            match self.check_redis(redis, key).await {
                Ok(decision) => return decision,
                Err(e) => {
                    tracing::warn!("Redis auth rate limit check failed, using in-process: {e}")
                }
            }
        }
        self.check_local(key, Instant::now()).await
    }

    async fn check_redis(
        &self,
        redis: &ConnectionManager,
        key: &str,
    ) -> Result<RateLimitDecision, redis::RedisError> {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        // Unique member so concurrent requests in the same millisecond each
        // count as a separate entry in the sorted set.
        let member = format!("{now_ms}-{}", uuid::Uuid::new_v4());

        let (allowed, retry_after_ms): (i64, i64) = redis::cmd("EVAL")
            .arg(SLIDING_WINDOW_LUA)
            .arg(1)
            .arg(format!("{REDIS_KEY_PREFIX}{key}"))
            .arg(now_ms)
            .arg(self.window.as_millis() as u64)
            .arg(self.max_requests)
            .arg(member)
            .query_async(&mut redis.clone())
            .await?;

        Ok(if allowed == 1 {
            RateLimitDecision::Allowed
        } else {
            RateLimitDecision::Limited {
                retry_after: Duration::from_millis(retry_after_ms.max(1) as u64),
            }
        })
    }

    async fn check_local(&self, key: &str, now: Instant) -> RateLimitDecision {
        let mut logs = self.local.lock().await;

        if logs.len() >= LOCAL_PRUNE_THRESHOLD {
            let window = self.window;
            logs.retain(|_, log| {
                log.back()
                    .is_some_and(|last| now.duration_since(*last) < window)
            });
        }

        let log = logs.entry(key.to_string()).or_default();
        while log
            .front()
            .is_some_and(|oldest| now.duration_since(*oldest) >= self.window)
        {
            log.pop_front();
        }

        if log.len() < self.max_requests as usize {
            log.push_back(now);
            RateLimitDecision::Allowed
        } else {
            let oldest = *log.front().expect("log is at the limit, so non-empty");
            RateLimitDecision::Limited {
                retry_after: (oldest + self.window).saturating_duration_since(now),
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

/// Axum middleware enforcing `AUTH_RATE_LIMIT_MAX_REQUESTS` per sliding
/// `AUTH_RATE_LIMIT_WINDOW` per client IP. Responds `429 Too Many Requests`
/// with a `Retry-After` header on overflow.
pub async fn auth_rate_limit(
    State(limiter): State<AuthRateLimiter>,
    request: Request<axum::body::Body>,
    next: Next,
) -> Response {
    let ip = client_ip(&request);

    match limiter.check(&ip).await {
        RateLimitDecision::Allowed => next.run(request).await,
        RateLimitDecision::Limited { retry_after } => {
            tracing::warn!("Rate limit exceeded for IP {ip} on auth endpoint");
            rate_limited_response(retry_after)
        }
    }
}

fn rate_limited_response(retry_after: Duration) -> Response {
    // Round up so clients never retry before the window has actually moved.
    let retry_after_secs = (retry_after.as_millis().div_ceil(1000) as u64).max(1);
    let mut response = (
        StatusCode::TOO_MANY_REQUESTS,
        Json(serde_json::json!({
            "error": "Too many requests. Please try again later.",
            "retry_after_secs": retry_after_secs,
        })),
    )
        .into_response();
    response
        .headers_mut()
        .insert(header::RETRY_AFTER, HeaderValue::from(retry_after_secs));
    response
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

/// Validates Stellar address format (56 chars, G-prefix, valid Base32)
fn is_valid_stellar_address(address: &str) -> bool {
    address.len() == 56
        && address.starts_with('G')
        && address
            .chars()
            .all(|c| c.is_ascii_uppercase() || (b'2'..=b'7').contains(&(c as u8)))
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

    fn is_allowed(decision: RateLimitDecision) -> bool {
        decision == RateLimitDecision::Allowed
    }

    #[tokio::test]
    async fn allows_up_to_the_limit_then_blocks() {
        let limiter = AuthRateLimiter::new();
        for i in 0..AUTH_RATE_LIMIT_MAX_REQUESTS {
            assert!(
                is_allowed(limiter.check("1.2.3.4").await),
                "request {i} should be allowed within the limit"
            );
        }
        assert!(
            !is_allowed(limiter.check("1.2.3.4").await),
            "request past the limit should be blocked"
        );
    }

    #[tokio::test]
    async fn tracks_each_ip_independently() {
        let limiter = AuthRateLimiter::new();
        for _ in 0..AUTH_RATE_LIMIT_MAX_REQUESTS {
            assert!(is_allowed(limiter.check("1.1.1.1").await));
        }
        assert!(
            !is_allowed(limiter.check("1.1.1.1").await),
            "1.1.1.1 should now be blocked"
        );
        assert!(
            is_allowed(limiter.check("2.2.2.2").await),
            "a different IP must have its own budget"
        );
    }

    #[tokio::test]
    async fn window_slides_instead_of_resetting() {
        let limiter = AuthRateLimiter::new();
        let start = Instant::now();
        let half = AUTH_RATE_LIMIT_WINDOW / 2;

        // 5 requests at t=0, 5 more at t=30s: the budget is now spent.
        for _ in 0..5 {
            assert!(is_allowed(limiter.check_local("ip", start).await));
        }
        for _ in 0..5 {
            assert!(is_allowed(limiter.check_local("ip", start + half).await));
        }
        assert!(!is_allowed(limiter.check_local("ip", start + half).await));

        // At t=60s only the first 5 have aged out, so exactly 5 more fit.
        // A fixed window would have reset and allowed 10.
        let later = start + AUTH_RATE_LIMIT_WINDOW;
        for _ in 0..5 {
            assert!(is_allowed(limiter.check_local("ip", later).await));
        }
        assert!(!is_allowed(limiter.check_local("ip", later).await));
    }

    #[tokio::test]
    async fn limited_decision_reports_time_until_oldest_expires() {
        let limiter = AuthRateLimiter::new();
        let start = Instant::now();
        for _ in 0..AUTH_RATE_LIMIT_MAX_REQUESTS {
            limiter.check_local("ip", start).await;
        }
        let decision = limiter
            .check_local("ip", start + Duration::from_secs(20))
            .await;
        assert_eq!(
            decision,
            RateLimitDecision::Limited {
                retry_after: Duration::from_secs(40)
            }
        );
    }

    #[tokio::test]
    async fn rejected_requests_do_not_extend_the_lockout() {
        let limiter = AuthRateLimiter::new();
        let start = Instant::now();
        for _ in 0..AUTH_RATE_LIMIT_MAX_REQUESTS {
            limiter.check_local("ip", start).await;
        }
        for secs in 1..30 {
            limiter
                .check_local("ip", start + Duration::from_secs(secs))
                .await;
        }
        assert!(is_allowed(
            limiter
                .check_local("ip", start + AUTH_RATE_LIMIT_WINDOW)
                .await
        ));
    }

    #[test]
    fn limited_response_is_429_with_retry_after() {
        let response = rate_limited_response(Duration::from_millis(1500));
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(response.headers()[header::RETRY_AFTER], "2");
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
