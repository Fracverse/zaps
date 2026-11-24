//! # Privy Authentication Provider Service (Issue #940)
//!
//! Production-grade REST & JWKS client for verifying Privy authentication JWT credentials,
//! validating cryptographic signatures against Privy's JSON Web Key Set (JWKS),
//! extracting Privy DID identifiers, and verifying linked wallet accounts.
//!
//! ## Architecture & Security Design:
//! - **JWKS Caching**: In-memory thread-safe cached JWKS with configurable TTL (`JWKS_CACHE_TTL`),
//!   reducing network roundtrips to Privy endpoints while supporting key rotation.
//! - **Algorithm Confusion Protection**: Strictly enforces asymmetric algorithms (ES256, RS256).
//!   Symmetric algorithms like HS256 are unconditionally rejected to prevent public-key forgery attacks.
//! - **Strict Claim Verification**: Validates standard claims:
//!   - `exp`: Token expiration timestamp.
//!   - `aud`: Application ID (Audience claim matching `expected_app_id`).
//!   - `iss`: Issuer claim matching `privy.io`.
//!   - `sub`: Subject claim containing the unique Privy Decentralized Identifier (`did:privy:...`).
//! - **Linked Account Extraction**: Parses and filters linked wallets for the Stellar ecosystem.
//!
//! ## Complexity:
//! - **Time Complexity**:
//!   - JWKS Key Resolution: $O(1)$ amortized (cached read lock).
//!   - JWT Signature Verification: $O(M)$ where $M$ is token size in bytes.
//!   - DID & Linked Account Extraction: $O(L)$ where $L$ is number of linked accounts (typically $\le 10$).
//! - **Space Complexity**:
//!   - Memory footprint: $O(K)$ bounded cache memory where $K$ is number of public keys in JWKS ($\le 5$).

use jsonwebtoken::jwk::JwkSet;
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use serde::{Deserialize, Serialize};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::RwLock;

/// Default duration for caching fetched JWKS public keys (1 hour).
pub const DEFAULT_JWKS_CACHE_TTL: Duration = Duration::from_secs(3600);

/// Supported asymmetric algorithms for Privy JWT tokens.
pub const ALLOWED_ALGORITHMS: &[Algorithm] = &[Algorithm::ES256, Algorithm::RS256];

/// Error types encountered during Privy token verification and DID extraction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrivyAuthError {
    /// Network or HTTP transport error while communicating with Privy JWKS endpoint.
    Http(String),
    /// Malformed JSON payload returned from the JWKS endpoint.
    MalformedJwks(String),
    /// Token header is missing the Key ID (`kid`) parameter.
    MissingKid,
    /// No matching JWK found in the JWKS for the given `kid`.
    UnknownKid(String),
    /// Token algorithm is not in the allowed list (e.g. HS256 rejected).
    UnsupportedAlgorithm,
    /// Failed to construct a decoding key from the JWK.
    InvalidKey(String),
    /// Cryptographic signature verification or token structure validation failed.
    InvalidToken(String),
    /// Token is expired.
    ExpiredToken,
    /// Token audience does not match the expected Privy App ID.
    InvalidAudience(String),
    /// Token issuer does not match `privy.io`.
    InvalidIssuer(String),
    /// Privy DID subject claim is missing or malformed.
    InvalidDid(String),
    /// Linked Stellar wallet address does not match the expected address.
    WalletMismatch,
}

impl std::fmt::Display for PrivyAuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Http(e) => write!(f, "HTTP error fetching Privy JWKS: {e}"),
            Self::MalformedJwks(e) => write!(f, "Malformed Privy JWKS response: {e}"),
            Self::MissingKid => write!(f, "Privy token header is missing 'kid'"),
            Self::UnknownKid(kid) => write!(f, "No JWKS key found for kid '{kid}'"),
            Self::UnsupportedAlgorithm => write!(f, "Privy token algorithm is not supported"),
            Self::InvalidKey(e) => write!(f, "Invalid JWKS decoding key: {e}"),
            Self::InvalidToken(e) => write!(f, "Privy token verification failed: {e}"),
            Self::ExpiredToken => write!(f, "Privy token has expired"),
            Self::InvalidAudience(aud) => write!(f, "Invalid Privy token audience: '{aud}'"),
            Self::InvalidIssuer(iss) => write!(f, "Invalid Privy token issuer: '{iss}'"),
            Self::InvalidDid(did) => write!(f, "Invalid Privy DID format: '{did}'"),
            Self::WalletMismatch => write!(
                f,
                "Stellar wallet address is not authorized in Privy identity"
            ),
        }
    }
}

impl std::error::Error for PrivyAuthError {}

/// Cached JWKS storage with fetch timestamp.
struct CachedJwks {
    keys: JwkSet,
    fetched_at: Instant,
}

/// Linked account item inside the verified Privy JWT payload.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PrivyLinkedAccount {
    /// Account type: "wallet", "email", "phone", "google_oauth", "apple_oauth", etc.
    #[serde(rename = "type")]
    pub account_type: String,
    /// Public wallet address if account_type == "wallet".
    #[serde(default)]
    pub address: Option<String>,
    /// Blockchain chain type if account_type == "wallet" (e.g. "stellar", "ethereum", "solana").
    #[serde(default)]
    pub chain_type: Option<String>,
    /// RFC3339 timestamp when the account was verified.
    #[serde(default)]
    pub verified_at: Option<String>,
}

/// Verified Privy JWT claims.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PrivyClaims {
    /// Subject identifier: Privy DID (e.g. "did:privy:cm1234567890abcdef").
    #[serde(rename = "sub")]
    pub subject: String,
    /// Audience: Privy App ID.
    #[serde(default)]
    pub aud: Option<serde_json::Value>,
    /// Issuer: e.g. "privy.io".
    #[serde(default)]
    pub iss: Option<String>,
    /// Expiration timestamp in seconds since Unix epoch.
    pub exp: usize,
    /// Issued-at timestamp in seconds since Unix epoch.
    #[serde(default)]
    pub iat: Option<usize>,
    /// Array of linked identity accounts associated with the user.
    #[serde(default)]
    pub linked_accounts: Vec<PrivyLinkedAccount>,
}

/// Production-grade Privy Authentication Service Client.
#[derive(Clone)]
pub struct PrivyAuthService {
    http: reqwest::Client,
    jwks_url: String,
    cache_ttl: Duration,
    cache: Arc<RwLock<Option<CachedJwks>>>,
}

impl PrivyAuthService {
    /// Construct a new `PrivyAuthService` with the standard JWKS endpoint and default cache TTL.
    pub fn new(jwks_url: String) -> Self {
        Self::with_ttl(jwks_url, DEFAULT_JWKS_CACHE_TTL)
    }

    /// Construct a new `PrivyAuthService` with custom cache TTL (useful for testing).
    pub fn with_ttl(jwks_url: String, cache_ttl: Duration) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        Self {
            http,
            jwks_url,
            cache_ttl,
            cache: Arc::new(RwLock::new(None)),
        }
    }

    /// Fetch JWKS from remote endpoint or local mock server.
    async fn fetch_jwks(&self) -> Result<JwkSet, PrivyAuthError> {
        let resp = self
            .http
            .get(&self.jwks_url)
            .send()
            .await
            .map_err(|e| PrivyAuthError::Http(e.to_string()))?
            .error_for_status()
            .map_err(|e| PrivyAuthError::Http(e.to_string()))?;

        let jwks: JwkSet = resp
            .json()
            .await
            .map_err(|e| PrivyAuthError::MalformedJwks(e.to_string()))?;

        let mut cache = self.cache.write().await;
        *cache = Some(CachedJwks {
            keys: jwks.clone(),
            fetched_at: Instant::now(),
        });

        Ok(jwks)
    }

    /// Resolve a specific JWK by `kid` from the cache or fetch a fresh set on cache miss.
    pub async fn get_key(&self, kid: &str) -> Result<jsonwebtoken::jwk::Jwk, PrivyAuthError> {
        {
            let cache = self.cache.read().await;
            if let Some(cached) = cache.as_ref() {
                if cached.fetched_at.elapsed() < self.cache_ttl {
                    if let Some(key) = cached.keys.find(kid) {
                        return Ok(key.clone());
                    }
                }
            }
        }

        // Cache miss or expired: fetch fresh JWKS
        let jwks = self.fetch_jwks().await?;
        jwks.find(kid)
            .cloned()
            .ok_or_else(|| PrivyAuthError::UnknownKid(kid.to_string()))
    }

    /// Verify an incoming Privy JWT token against Privy JWKS public keys.
    ///
    /// Validates:
    /// 1. Token header format and algorithm allowlist (ES256, RS256).
    /// 2. Cryptographic signature using the corresponding public JWK.
    /// 3. Token expiration (`exp`).
    /// 4. Expected Privy App ID (`aud`).
    /// 5. Expected Issuer (`iss` == `privy.io`).
    ///
    /// Returns the verified [`PrivyClaims`] on success.
    pub async fn verify_token(
        &self,
        token: &str,
        expected_app_id: &str,
    ) -> Result<PrivyClaims, PrivyAuthError> {
        let header =
            decode_header(token).map_err(|e| PrivyAuthError::InvalidToken(e.to_string()))?;

        if !ALLOWED_ALGORITHMS.contains(&header.alg) {
            return Err(PrivyAuthError::UnsupportedAlgorithm);
        }

        let kid = header.kid.ok_or(PrivyAuthError::MissingKid)?;
        let jwk = self.get_key(&kid).await?;

        let decoding_key =
            DecodingKey::from_jwk(&jwk).map_err(|e| PrivyAuthError::InvalidKey(e.to_string()))?;

        let mut validation = Validation::new(header.alg);
        validation.algorithms = vec![header.alg];
        validation.set_audience(&[expected_app_id]);
        validation.set_issuer(&["privy.io"]);
        validation.set_required_spec_claims(&["exp", "aud", "iss"]);

        let data = decode::<PrivyClaims>(token, &decoding_key, &validation).map_err(|e| match e
            .kind()
        {
            jsonwebtoken::errors::ErrorKind::ExpiredSignature => PrivyAuthError::ExpiredToken,
            jsonwebtoken::errors::ErrorKind::InvalidAudience => {
                PrivyAuthError::InvalidAudience(expected_app_id.to_string())
            }
            jsonwebtoken::errors::ErrorKind::InvalidIssuer => {
                PrivyAuthError::InvalidIssuer("privy.io".to_string())
            }
            _ => PrivyAuthError::InvalidToken(e.to_string()),
        })?;

        // Ensure subject starts with 'did:'
        if !Self::is_valid_did(&data.claims.subject) {
            return Err(PrivyAuthError::InvalidDid(data.claims.subject));
        }

        Ok(data.claims)
    }

    /// Convenience function: Decodes, verifies, and extracts the verified Privy DID identifier.
    pub async fn extract_privy_did(
        &self,
        token: &str,
        expected_app_id: &str,
    ) -> Result<String, PrivyAuthError> {
        let claims = self.verify_token(token, expected_app_id).await?;
        Ok(claims.subject)
    }

    /// Extract all linked Stellar wallet addresses from verified claims.
    pub fn get_linked_stellar_addresses(claims: &PrivyClaims) -> Vec<&str> {
        claims
            .linked_accounts
            .iter()
            .filter(|acc| {
                acc.account_type == "wallet"
                    && acc.chain_type.as_deref() == Some("stellar")
                    && acc.address.is_some()
            })
            .filter_map(|acc| acc.address.as_deref())
            .collect()
    }

    /// Check if a specific Stellar address is authorized in the user's linked accounts.
    pub fn verify_stellar_address_match(
        claims: &PrivyClaims,
        expected_stellar_address: &str,
    ) -> bool {
        let stellar_wallets = Self::get_linked_stellar_addresses(claims);
        stellar_wallets
            .iter()
            .any(|&addr| addr.eq_ignore_ascii_case(expected_stellar_address))
    }

    /// Validate Privy Decentralized Identifier (DID) format.
    pub fn is_valid_did(did: &str) -> bool {
        did.starts_with("did:") && did.len() >= 10
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_did_validation() {
        assert!(PrivyAuthService::is_valid_did("did:privy:user_12345678"));
        assert!(PrivyAuthService::is_valid_did(
            "did:key:z6MkhaXgBZDvotDkL5257faiz4574GMaPb28jWtJ32sE8pfC"
        ));
        assert!(!PrivyAuthService::is_valid_did("user_12345678"));
        assert!(!PrivyAuthService::is_valid_did("did:123"));
    }

    #[test]
    fn test_stellar_address_matching() {
        let claims = PrivyClaims {
            subject: "did:privy:test_user".to_string(),
            aud: Some(serde_json::json!("test-app")),
            iss: Some("privy.io".to_string()),
            exp: 9999999999,
            iat: Some(1000000000),
            linked_accounts: vec![
                PrivyLinkedAccount {
                    account_type: "wallet".to_string(),
                    address: Some(
                        "GBPK7THXDEPNBQB5K3EMQL5FZAQLHJ4XPBWJFNV3EPJN7CVPQGJZ6PBN".to_string(),
                    ),
                    chain_type: Some("stellar".to_string()),
                    verified_at: None,
                },
                PrivyLinkedAccount {
                    account_type: "email".to_string(),
                    address: Some("user@example.com".to_string()),
                    chain_type: None,
                    verified_at: None,
                },
            ],
        };

        assert!(PrivyAuthService::verify_stellar_address_match(
            &claims,
            "GBPK7THXDEPNBQB5K3EMQL5FZAQLHJ4XPBWJFNV3EPJN7CVPQGJZ6PBN"
        ));
        assert!(!PrivyAuthService::verify_stellar_address_match(
            &claims,
            "GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN"
        ));
    }

    #[tokio::test]
    async fn test_unsupported_algorithm_rejected() {
        let claims = serde_json::json!({
            "sub": "did:privy:test_user",
            "aud": "test-app",
            "iss": "privy.io",
            "exp": 9999999999u64
        });
        let token = jsonwebtoken::encode(
            &jsonwebtoken::Header::new(Algorithm::HS256),
            &claims,
            &jsonwebtoken::EncodingKey::from_secret(b"secret"),
        )
        .unwrap();

        let client = PrivyAuthService::new("http://127.0.0.1:0/jwks.json".to_string());
        let result = client.verify_token(&token, "test-app").await;
        assert_eq!(result, Err(PrivyAuthError::UnsupportedAlgorithm));
    }
}
