//! privy_jwks.rs
//!
//! Fetches and caches Privy's JWKS (JSON Web Key Set) and verifies Privy-issued
//! session JWTs against it, replacing the placeholder signature-less checks
//! previously in `auth.rs`.
//!
//! Privy signs user session tokens with an app-specific ES256 key, published
//! at `https://auth.privy.io/api/v1/apps/{app_id}/jwks.json`
//! (see https://docs.privy.io/guide/server/authorization/verification).
//!
//! Only ES256/RS256 are ever accepted. HS256 is intentionally excluded: since
//! JWKS keys are public by definition, treating one as an HMAC secret would
//! let anyone who can read the JWKS endpoint forge tokens (the classic
//! RS256/HS256 "algorithm confusion" attack).

use jsonwebtoken::jwk::JwkSet;
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use serde::de::DeserializeOwned;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::RwLock;

/// How long a fetched JWKS is trusted before being refetched on next use.
const JWKS_CACHE_TTL: Duration = Duration::from_secs(3600);

const ALLOWED_ALGORITHMS: &[Algorithm] = &[Algorithm::ES256, Algorithm::RS256];

#[derive(Debug)]
pub enum PrivyAuthError {
    Http(String),
    MalformedJwks(String),
    MissingKid,
    UnknownKid(String),
    UnsupportedAlgorithm,
    InvalidKey(String),
    InvalidToken(jsonwebtoken::errors::Error),
}

impl std::fmt::Display for PrivyAuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Http(e) => write!(f, "failed to fetch Privy JWKS: {e}"),
            Self::MalformedJwks(e) => write!(f, "malformed Privy JWKS response: {e}"),
            Self::MissingKid => write!(f, "token header is missing 'kid'"),
            Self::UnknownKid(kid) => write!(f, "no JWKS key found for kid '{kid}'"),
            Self::UnsupportedAlgorithm => write!(f, "token uses an unsupported algorithm"),
            Self::InvalidKey(e) => write!(f, "unusable JWKS key: {e}"),
            Self::InvalidToken(e) => write!(f, "token verification failed: {e}"),
        }
    }
}

impl std::error::Error for PrivyAuthError {}

struct CachedJwks {
    keys: JwkSet,
    fetched_at: Instant,
}

/// Fetches Privy's JWKS over HTTP, caches it for `JWKS_CACHE_TTL`, and
/// verifies session tokens against it.
#[derive(Clone)]
pub struct PrivyJwksClient {
    http: reqwest::Client,
    jwks_url: String,
    cache: Arc<RwLock<Option<CachedJwks>>>,
}

impl PrivyJwksClient {
    pub fn new(jwks_url: String) -> Self {
        Self {
            http: reqwest::Client::new(),
            jwks_url,
            cache: Arc::new(RwLock::new(None)),
        }
    }

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

    /// Resolves the JWK matching `kid`, refreshing the cache if it's stale or
    /// missing the key (covers Privy rotating its signing key).
    async fn get_key(&self, kid: &str) -> Result<jsonwebtoken::jwk::Jwk, PrivyAuthError> {
        {
            let cache = self.cache.read().await;
            if let Some(cached) = cache.as_ref() {
                if cached.fetched_at.elapsed() < JWKS_CACHE_TTL {
                    if let Some(key) = cached.keys.find(kid) {
                        return Ok(key.clone());
                    }
                }
            }
        }

        // Cache miss, stale, or key not found under a stale cache: refetch.
        let jwks = self.fetch_jwks().await?;
        jwks.find(kid)
            .cloned()
            .ok_or_else(|| PrivyAuthError::UnknownKid(kid.to_string()))
    }

    /// Verifies a Privy-issued JWT's signature, expiry, issuer, and audience,
    /// returning the deserialized claims on success.
    ///
    /// `expected_app_id` is the Privy app ID, which Privy sets as the token's
    /// `aud` claim.
    pub async fn verify_token<T: DeserializeOwned>(
        &self,
        token: &str,
        expected_app_id: &str,
    ) -> Result<T, PrivyAuthError> {
        let header = decode_header(token).map_err(PrivyAuthError::InvalidToken)?;

        if !ALLOWED_ALGORITHMS.contains(&header.alg) {
            return Err(PrivyAuthError::UnsupportedAlgorithm);
        }

        let kid = header.kid.ok_or(PrivyAuthError::MissingKid)?;
        let jwk = self.get_key(&kid).await?;

        let decoding_key =
            DecodingKey::from_jwk(&jwk).map_err(|e| PrivyAuthError::InvalidKey(e.to_string()))?;

        let mut validation = Validation::new(header.alg);
        // Restricted to the token's own (already allowlisted) algorithm: the
        // resolved key only has one family (EC or RSA), and jsonwebtoken
        // requires every entry in `validation.algorithms` to match the key's
        // family, so listing both ES256 and RS256 here would always fail.
        validation.algorithms = vec![header.alg];
        validation.set_audience(&[expected_app_id]);
        validation.set_issuer(&["privy.io"]);
        // jsonwebtoken only enforces `aud`/`iss` when those claims are
        // *present* in the token; without this, a token that simply omits
        // them would skip audience/issuer checks entirely.
        validation.set_required_spec_claims(&["exp", "aud", "iss"]);

        let data = decode::<T>(token, &decoding_key, &validation)
            .map_err(PrivyAuthError::InvalidToken)?;

        Ok(data.claims)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn rejects_token_with_no_kid() {
        // A well-formed but kid-less HS256 token should fail before any HTTP
        // fetch even happens (caught by the algorithm allowlist check).
        let claims = serde_json::json!({ "sub": "did:privy:x", "exp": 9999999999u64 });
        let token = jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &claims,
            &jsonwebtoken::EncodingKey::from_secret(b"secret"),
        )
        .unwrap();

        let client = PrivyJwksClient::new("http://127.0.0.1:0/jwks.json".to_string());
        let result = client
            .verify_token::<serde_json::Value>(&token, "app-id")
            .await;
        assert!(matches!(result, Err(PrivyAuthError::UnsupportedAlgorithm)));
    }
}
