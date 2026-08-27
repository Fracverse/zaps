/// Integration tests for the Privy authentication endpoint, exercised against
/// a real (locally hosted) mock JWKS server so the tests cover actual
/// signature/audience/issuer verification rather than a placeholder.
use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use chrono::Utc;
use http_body_util::BodyExt;
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::PgPool;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};
use tower::ServiceExt;
use uuid::Uuid;
use zaps_backend::api::auth::AuthState;
use zaps_backend::api::privy_jwks::PrivyJwksClient;

/// The Privy app ID these tests act as; tokens must carry it as `aud`.
const TEST_APP_ID: &str = "test-privy-app-id";
const TEST_KID: &str = "test-key-1";

// Test-only EC (P-256) keypair used to sign valid tokens. Its public half is
// what the mock JWKS server below publishes under `TEST_KID`.
const TEST_EC_PRIVATE_KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----\n\
MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQgGQzKcoE6pm8bOcUe\n\
IaM8s+yui6U0IPs9K0zfQSc11iChRANCAATj48/t4zGBE/NGemlVh9NGTzYmxP7Z\n\
rRlMOMELosipYoxwGFRZqetbRSv0LHXerPoyOQZXYx/676/FQQyPlsxX\n\
-----END PRIVATE KEY-----\n";
const TEST_EC_X: &str = "4-PP7eMxgRPzRnppVYfTRk82JsT-2a0ZTDjBC6LIqWI";
const TEST_EC_Y: &str = "jHAYVFmp61tFK_Qsdd6s-jI5BldjH_rvr8VBDI-WzFc";

// A second, *unregistered* EC keypair (its public key is never published in
// the mock JWKS). Used to prove that a token signed by a key other than the
// one on file for its `kid` is rejected, not just accepted on payload shape.
const OTHER_EC_PRIVATE_KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----\n\
MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQgfFXfMKHKhOUXwdQw\n\
rgCShgJ/VHgpo9YPKxw1/ckqT32hRANCAARgqT0Cmzt5co8HYXAazcJp3VDRY/aS\n\
i9nvMUXoPE0iRxHx2W/DKDPGHlngUtL22jr/1AficmcPFgjzR43n11JW\n\
-----END PRIVATE KEY-----\n";

/// Mock Privy JWT payload structure for testing
#[derive(Debug, Serialize, Deserialize)]
struct MockPrivyPayload {
    sub: String, // Privy DID
    aud: String,
    iss: String,
    exp: usize,
    iat: usize,
    #[serde(default)]
    linked_accounts: Vec<MockLinkedAccount>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct MockLinkedAccount {
    #[serde(rename = "type")]
    account_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    chain_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    verified_at: Option<String>,
}

/// Helper: Generate a mock Privy JWT token for testing, ES256-signed with
/// `signing_key_pem` and stamped with `kid` so the server can resolve it
/// against the mock JWKS.
fn create_mock_privy_token_with_key(
    did: &str,
    stellar_address: Option<&str>,
    expired: bool,
    signing_key_pem: &str,
    kid: &str,
    aud: &str,
) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as usize;

    let exp = if expired {
        now - 3600 // 1 hour ago
    } else {
        now + 86400 // 24 hours from now
    };

    let mut linked_accounts = vec![];
    if let Some(addr) = stellar_address {
        linked_accounts.push(MockLinkedAccount {
            account_type: "wallet".to_string(),
            address: Some(addr.to_string()),
            chain_type: Some("stellar".to_string()),
            verified_at: Some(Utc::now().to_rfc3339()),
        });
    }

    let payload = MockPrivyPayload {
        sub: did.to_string(),
        aud: aud.to_string(),
        iss: "privy.io".to_string(),
        exp,
        iat: now,
        linked_accounts,
    };

    let mut header = Header::new(Algorithm::ES256);
    header.kid = Some(kid.to_string());

    encode(
        &header,
        &payload,
        &EncodingKey::from_ec_pem(signing_key_pem.as_bytes()).expect("valid test EC key"),
    )
    .expect("Failed to encode mock JWT")
}

/// Convenience wrapper: signs with the registered test key / kid and the
/// correct audience, which is what most tests want.
fn create_mock_privy_token(did: &str, stellar_address: Option<&str>, expired: bool) -> String {
    create_mock_privy_token_with_key(
        did,
        stellar_address,
        expired,
        TEST_EC_PRIVATE_KEY_PEM,
        TEST_KID,
        TEST_APP_ID,
    )
}

/// Serves the fixed test JWKS (just `TEST_KID`'s public key) at `/jwks.json`
/// on a background thread, once per test binary. A plain `std::thread` (not
/// a tokio task) is used so the server outlives each `#[tokio::test]`'s
/// short-lived per-test runtime.
fn mock_jwks_url() -> &'static str {
    static URL: OnceLock<String> = OnceLock::new();
    URL.get_or_init(|| {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock JWKS listener");
        let addr = listener.local_addr().unwrap();

        std::thread::spawn(move || {
            let body = json!({
                "keys": [{
                    "kty": "EC",
                    "crv": "P-256",
                    "x": TEST_EC_X,
                    "y": TEST_EC_Y,
                    "kid": TEST_KID,
                    "alg": "ES256",
                    "use": "sig"
                }]
            })
            .to_string();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );

            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut buf = [0u8; 1024];
                let _ = stream.read(&mut buf);
                let _ = stream.write_all(response.as_bytes());
            }
        });

        format!("http://{addr}/jwks.json")
    })
}

/// Helper: Setup test database pool
async fn setup_test_pool() -> PgPool {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost/zaps_test".to_string());

    PgPool::connect(&database_url)
        .await
        .expect("Failed to connect to test database")
}

/// Helper: Clean up test user by address
async fn cleanup_test_user(pool: &PgPool, address: &str) {
    let _ = sqlx::query("DELETE FROM users WHERE address = $1")
        .bind(address)
        .execute(pool)
        .await;
}

/// Helper: Create test app router, wired to the real auth router (including
/// the per-IP rate limiter) with a `PrivyJwksClient` pointed at the mock
/// JWKS server instead of Privy's production endpoint.
fn create_test_app(pool: PgPool) -> Router {
    let state = AuthState {
        pool,
        privy: Arc::new(PrivyJwksClient::new(mock_jwks_url().to_string())),
        privy_app_id: TEST_APP_ID.to_string(),
    };
    Router::new().nest("/api/auth", zaps_backend::api::auth_routes_with_state(state))
}

#[cfg(test)]
mod privy_auth_integration_tests {
    use super::*;

    /// Test 1 - Valid Privy auth request creates user with DID linkage
    #[tokio::test]
    async fn test_privy_auth_creates_user_with_did() {
        let pool = setup_test_pool().await;
        let app = create_test_app(pool.clone());

        let stellar_addr = "GBPK7THXDEPNBQB5K3EMQL5FZAQLHJ4XPBWJFNV3EPJN7CVPQGJZ6PBN";
        let privy_did = format!("did:privy:test_{}", Uuid::new_v4());

        cleanup_test_user(&pool, stellar_addr).await;

        let token = create_mock_privy_token(&privy_did, Some(stellar_addr), false);

        let request = Request::builder()
            .method("POST")
            .uri("/api/auth/privy")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({
                    "privy_token": token,
                    "privy_did": privy_did,
                    "stellar_address": stellar_addr
                })
                .to_string(),
            ))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();

        assert_eq!(
            response.status(),
            StatusCode::CREATED,
            "Expected 201 CREATED for valid Privy auth"
        );

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: Value = serde_json::from_slice(&body).unwrap();

        assert!(json["token"].is_string(), "Response should contain JWT token");
        assert_eq!(json["username"].as_str().unwrap(), "u_GBPK7THXDEPNBQB5K");
        assert_eq!(json["privy_did"].as_str().unwrap(), privy_did);

        cleanup_test_user(&pool, stellar_addr).await;
    }

    /// Test 2 - Reject if Stellar address already linked to different DID
    #[tokio::test]
    async fn test_privy_auth_rejects_address_linked_to_different_did() {
        let pool = setup_test_pool().await;

        let stellar_addr = "GBPK7THXDEPNBQB5K3EMQL5FZAQLHJ4XPBWJFNV3EPJN7CVPQGJZ6PBN";
        let did_1 = format!("did:privy:test_{}", Uuid::new_v4());
        let did_2 = format!("did:privy:test_{}", Uuid::new_v4());

        cleanup_test_user(&pool, stellar_addr).await;

        // First request: Link address to DID 1
        let token_1 = create_mock_privy_token(&did_1, Some(stellar_addr), false);
        let req_1 = Request::builder()
            .method("POST")
            .uri("/api/auth/privy")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({
                    "privy_token": token_1,
                    "privy_did": did_1,
                    "stellar_address": stellar_addr
                })
                .to_string(),
            ))
            .unwrap();

        let resp_1 = create_test_app(pool.clone()).oneshot(req_1).await.unwrap();
        assert_eq!(resp_1.status(), StatusCode::CREATED);

        // Second request: Try to link same address to DID 2
        let token_2 = create_mock_privy_token(&did_2, Some(stellar_addr), false);
        let req_2 = Request::builder()
            .method("POST")
            .uri("/api/auth/privy")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({
                    "privy_token": token_2,
                    "privy_did": did_2,
                    "stellar_address": stellar_addr
                })
                .to_string(),
            ))
            .unwrap();

        let resp_2 = create_test_app(pool.clone()).oneshot(req_2).await.unwrap();
        assert_eq!(
            resp_2.status(),
            StatusCode::CONFLICT,
            "Should reject address linked to different DID"
        );

        let body = resp_2.into_body().collect().await.unwrap().to_bytes();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert!(json["error"]
            .as_str()
            .unwrap()
            .contains("already linked to a different Privy identity"));

        cleanup_test_user(&pool, stellar_addr).await;
    }

    /// Test 3 - Reject if Privy DID already linked to different address
    #[tokio::test]
    async fn test_privy_auth_rejects_did_linked_to_different_address() {
        let pool = setup_test_pool().await;

        let addr_1 = "GBPK7THXDEPNBQB5K3EMQL5FZAQLHJ4XPBWJFNV3EPJN7CVPQGJZ6PBN";
        let addr_2 = "GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN";
        let privy_did = format!("did:privy:test_{}", Uuid::new_v4());

        cleanup_test_user(&pool, addr_1).await;
        cleanup_test_user(&pool, addr_2).await;

        // First request: Link DID to address 1
        let token_1 = create_mock_privy_token(&privy_did, Some(addr_1), false);
        let req_1 = Request::builder()
            .method("POST")
            .uri("/api/auth/privy")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({
                    "privy_token": token_1,
                    "privy_did": privy_did,
                    "stellar_address": addr_1
                })
                .to_string(),
            ))
            .unwrap();

        let resp_1 = create_test_app(pool.clone()).oneshot(req_1).await.unwrap();
        assert_eq!(resp_1.status(), StatusCode::CREATED);

        // Second request: Try to link same DID to address 2
        let token_2 = create_mock_privy_token(&privy_did, Some(addr_2), false);
        let req_2 = Request::builder()
            .method("POST")
            .uri("/api/auth/privy")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({
                    "privy_token": token_2,
                    "privy_did": privy_did,
                    "stellar_address": addr_2
                })
                .to_string(),
            ))
            .unwrap();

        let resp_2 = create_test_app(pool.clone()).oneshot(req_2).await.unwrap();
        assert_eq!(
            resp_2.status(),
            StatusCode::CONFLICT,
            "Should reject DID linked to different address"
        );

        let body = resp_2.into_body().collect().await.unwrap().to_bytes();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert!(json["error"]
            .as_str()
            .unwrap()
            .contains("already linked to a different Stellar address"));

        cleanup_test_user(&pool, addr_1).await;
        cleanup_test_user(&pool, addr_2).await;
    }

    /// Test 4 - Allow re-authentication with same DID and address
    #[tokio::test]
    async fn test_privy_auth_allows_same_did_address_pair() {
        let pool = setup_test_pool().await;

        let stellar_addr = "GBPK7THXDEPNBQB5K3EMQL5FZAQLHJ4XPBWJFNV3EPJN7CVPQGJZ6PBN";
        let privy_did = format!("did:privy:test_{}", Uuid::new_v4());

        cleanup_test_user(&pool, stellar_addr).await;

        let token = create_mock_privy_token(&privy_did, Some(stellar_addr), false);

        // First authentication
        let req_1 = Request::builder()
            .method("POST")
            .uri("/api/auth/privy")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({
                    "privy_token": token,
                    "privy_did": privy_did,
                    "stellar_address": stellar_addr
                })
                .to_string(),
            ))
            .unwrap();

        let resp_1 = create_test_app(pool.clone()).oneshot(req_1).await.unwrap();
        assert_eq!(resp_1.status(), StatusCode::CREATED);

        // Second authentication with same credentials
        let token_2 = create_mock_privy_token(&privy_did, Some(stellar_addr), false);
        let req_2 = Request::builder()
            .method("POST")
            .uri("/api/auth/privy")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({
                    "privy_token": token_2,
                    "privy_did": privy_did,
                    "stellar_address": stellar_addr
                })
                .to_string(),
            ))
            .unwrap();

        let resp_2 = create_test_app(pool.clone()).oneshot(req_2).await.unwrap();
        assert_eq!(
            resp_2.status(),
            StatusCode::CREATED,
            "Should allow re-authentication with same DID/address"
        );

        let body = resp_2.into_body().collect().await.unwrap().to_bytes();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert!(json["token"].is_string());

        cleanup_test_user(&pool, stellar_addr).await;
    }

    /// Test 5 - Invalid Stellar address format rejected
    #[tokio::test]
    async fn test_privy_auth_rejects_invalid_stellar_address() {
        let pool = setup_test_pool().await;
        let app = create_test_app(pool.clone());

        let privy_did = format!("did:privy:test_{}", Uuid::new_v4());
        let invalid_addr = "invalid_address_123";

        let token = create_mock_privy_token(&privy_did, Some(invalid_addr), false);

        let request = Request::builder()
            .method("POST")
            .uri("/api/auth/privy")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({
                    "privy_token": token,
                    "privy_did": privy_did,
                    "stellar_address": invalid_addr
                })
                .to_string(),
            ))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(
            response.status(),
            StatusCode::BAD_REQUEST,
            "Should reject invalid Stellar address format"
        );

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert!(json["error"]
            .as_str()
            .unwrap()
            .contains("Invalid Stellar address format"));
    }

    /// Test 6 - Invalid Privy DID format rejected
    #[tokio::test]
    async fn test_privy_auth_rejects_invalid_did_format() {
        let pool = setup_test_pool().await;
        let app = create_test_app(pool.clone());

        let stellar_addr = "GBPK7THXDEPNBQB5K3EMQL5FZAQLHJ4XPBWJFNV3EPJN7CVPQGJZ6PBN";
        let invalid_did = "invalid_did";

        let token = create_mock_privy_token(invalid_did, Some(stellar_addr), false);

        let request = Request::builder()
            .method("POST")
            .uri("/api/auth/privy")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({
                    "privy_token": token,
                    "privy_did": invalid_did,
                    "stellar_address": stellar_addr
                })
                .to_string(),
            ))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(
            response.status(),
            StatusCode::BAD_REQUEST,
            "Should reject invalid Privy DID format"
        );

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert!(json["error"]
            .as_str()
            .unwrap()
            .contains("Invalid Privy DID format"));
    }

    /// Test 7 - Expired Privy token is rejected. Now that verification is
    /// backed by real JWKS signature + claim checks, this must be a strict
    /// 401 (previously this was a placeholder that accepted anything).
    #[tokio::test]
    async fn test_privy_auth_rejects_expired_token() {
        let pool = setup_test_pool().await;
        let app = create_test_app(pool.clone());

        let stellar_addr = "GBPK7THXDEPNBQB5K3EMQL5FZAQLHJ4XPBWJFNV3EPJN7CVPQGJZ6PBN";
        let privy_did = format!("did:privy:test_{}", Uuid::new_v4());

        cleanup_test_user(&pool, stellar_addr).await;

        let expired_token = create_mock_privy_token(&privy_did, Some(stellar_addr), true);

        let request = Request::builder()
            .method("POST")
            .uri("/api/auth/privy")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({
                    "privy_token": expired_token,
                    "privy_did": privy_did,
                    "stellar_address": stellar_addr
                })
                .to_string(),
            ))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "Expired tokens must be rejected"
        );

        cleanup_test_user(&pool, stellar_addr).await;
    }

    /// Test 8 - Reject if Stellar address not in Privy token's linked_accounts
    #[tokio::test]
    async fn test_privy_auth_rejects_mismatched_wallet() {
        let pool = setup_test_pool().await;
        let app = create_test_app(pool.clone());

        let token_addr = "GBPK7THXDEPNBQB5K3EMQL5FZAQLHJ4XPBWJFNV3EPJN7CVPQGJZ6PBN";
        let submitted_addr = "GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN";
        let privy_did = format!("did:privy:test_{}", Uuid::new_v4());

        cleanup_test_user(&pool, submitted_addr).await;

        // Create token with token_addr but submit different address
        let token = create_mock_privy_token(&privy_did, Some(token_addr), false);

        let request = Request::builder()
            .method("POST")
            .uri("/api/auth/privy")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({
                    "privy_token": token,
                    "privy_did": privy_did,
                    "stellar_address": submitted_addr
                })
                .to_string(),
            ))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(
            response.status(),
            StatusCode::BAD_REQUEST,
            "Should reject when submitted address not in token's linked_accounts"
        );

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert!(
            json["error"]
                .as_str()
                .unwrap()
                .contains("does not match any wallet linked to your Privy identity"),
            "Error should mention wallet mismatch"
        );

        cleanup_test_user(&pool, submitted_addr).await;
    }

    /// Test 9 - Reject if token has no linked Stellar wallets
    #[tokio::test]
    async fn test_privy_auth_rejects_token_without_stellar_wallet() {
        let pool = setup_test_pool().await;
        let app = create_test_app(pool.clone());

        let stellar_addr = "GBPK7THXDEPNBQB5K3EMQL5FZAQLHJ4XPBWJFNV3EPJN7CVPQGJZ6PBN";
        let privy_did = format!("did:privy:test_{}", Uuid::new_v4());

        cleanup_test_user(&pool, stellar_addr).await;

        // Create token with NO Stellar address
        let token = create_mock_privy_token(&privy_did, None, false);

        let request = Request::builder()
            .method("POST")
            .uri("/api/auth/privy")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({
                    "privy_token": token,
                    "privy_did": privy_did,
                    "stellar_address": stellar_addr
                })
                .to_string(),
            ))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(
            response.status(),
            StatusCode::BAD_REQUEST,
            "Should reject when token has no Stellar wallet"
        );

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert!(json["error"]
            .as_str()
            .unwrap()
            .contains("does not match any wallet"));

        cleanup_test_user(&pool, stellar_addr).await;
    }

    /// Test 10 - A token signed by a key other than the one on file for its
    /// `kid` (e.g. an attacker forging a token) must fail signature
    /// verification, not merely be accepted because the payload looks right.
    #[tokio::test]
    async fn test_privy_auth_rejects_token_signed_by_wrong_key() {
        let pool = setup_test_pool().await;
        let app = create_test_app(pool.clone());

        let stellar_addr = "GBPK7THXDEPNBQB5K3EMQL5FZAQLHJ4XPBWJFNV3EPJN7CVPQGJZ6PBN";
        let privy_did = format!("did:privy:test_{}", Uuid::new_v4());

        // Signed with a key that was never published to the mock JWKS, but
        // still claims `kid: TEST_KID` so the server resolves the *real*
        // public key -- which won't match this signature.
        let forged_token = create_mock_privy_token_with_key(
            &privy_did,
            Some(stellar_addr),
            false,
            OTHER_EC_PRIVATE_KEY_PEM,
            TEST_KID,
            TEST_APP_ID,
        );

        let request = Request::builder()
            .method("POST")
            .uri("/api/auth/privy")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({
                    "privy_token": forged_token,
                    "privy_did": privy_did,
                    "stellar_address": stellar_addr
                })
                .to_string(),
            ))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "Token signed by an unregistered key must fail verification"
        );
    }

    /// Test 11 - A token with the wrong audience (i.e. issued for a
    /// different Privy app) must be rejected.
    #[tokio::test]
    async fn test_privy_auth_rejects_wrong_audience() {
        let pool = setup_test_pool().await;
        let app = create_test_app(pool.clone());

        let stellar_addr = "GBPK7THXDEPNBQB5K3EMQL5FZAQLHJ4XPBWJFNV3EPJN7CVPQGJZ6PBN";
        let privy_did = format!("did:privy:test_{}", Uuid::new_v4());

        let token = create_mock_privy_token_with_key(
            &privy_did,
            Some(stellar_addr),
            false,
            TEST_EC_PRIVATE_KEY_PEM,
            TEST_KID,
            "some-other-privy-app-id",
        );

        let request = Request::builder()
            .method("POST")
            .uri("/api/auth/privy")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({
                    "privy_token": token,
                    "privy_did": privy_did,
                    "stellar_address": stellar_addr
                })
                .to_string(),
            ))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "Token issued for a different Privy app must be rejected"
        );
    }
}
