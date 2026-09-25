/// Integration tests for Privy authentication endpoint with mock JWKS server
/// Issue #563 / Issue #945: Comprehensive Privy verification test suite
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

const TEST_APP_ID: &str = "test-privy-app-id";
const TEST_KID: &str = "test-key-1";

const TEST_EC_PRIVATE_KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----\n\
MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQgGQzKcoE6pm8bOcUe\n\
IaM8s+yui6U0IPs9K0zfQSc11iChRANCAATj48/t4zGBE/NGemlVh9NGTzYmxP7Z\n\
rRlMOMELosipYoxwGFRZqetbRSv0LHXerPoyOQZXYx/676/FQQyPlsxX\n\
-----END PRIVATE KEY-----\n";
const TEST_EC_X: &str = "4-PP7eMxgRPzRnppVYfTRk82JsT-2a0ZTDjBC6LIqWI";
const TEST_EC_Y: &str = "jHAYVFmp61tFK_Qsdd6s-jI5BldjH_rvr8VBDI-WzFc";

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

/// Helper: Generate a mock Privy JWT token for testing
fn create_mock_privy_token(did: &str, stellar_address: Option<&str>, expired: bool) -> String {
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
        aud: TEST_APP_ID.to_string(),
        iss: "privy.io".to_string(),
        exp,
        iat: now,
        linked_accounts,
    };

    let mut header = Header::new(Algorithm::ES256);
    header.kid = Some(TEST_KID.to_string());

    encode(
        &header,
        &payload,
        &EncodingKey::from_ec_pem(TEST_EC_PRIVATE_KEY_PEM.as_bytes()).expect("valid test EC key"),
    )
    .expect("Failed to encode mock JWT")
}

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
async fn setup_test_pool() -> Option<PgPool> {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost/zaps_test".to_string());

    match tokio::time::timeout(
        std::time::Duration::from_millis(500),
        PgPool::connect(&database_url),
    )
    .await
    {
        Ok(Ok(pool)) => Some(pool),
        _ => None,
    }
}

/// Helper: Clean up test user by address
async fn cleanup_test_user(pool: &PgPool, address: &str) {
    let _ = sqlx::query("DELETE FROM users WHERE address = $1")
        .bind(address)
        .execute(pool)
        .await;
}

/// Helper: Create test app router
fn create_test_app(pool: PgPool) -> Router {
    let state = AuthState {
        pool,
        privy: Arc::new(PrivyJwksClient::new(mock_jwks_url().to_string())),
        privy_app_id: TEST_APP_ID.to_string(),
        cache: None,
    };
    Router::new().nest(
        "/api/auth",
        zaps_backend::api::auth_routes_with_state(state),
    )
}

fn create_mock_auth_app() -> Router {
    let state = AuthState {
        pool: sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://localhost/dummy")
            .unwrap(),
        privy: Arc::new(PrivyJwksClient::new(mock_jwks_url().to_string())),
        privy_app_id: TEST_APP_ID.to_string(),
        cache: None,
    };
    Router::new().nest(
        "/api/auth",
        zaps_backend::api::auth_routes_with_state(state),
    )
}

#[cfg(test)]
mod privy_auth_integration_tests {
    use super::*;

    /// Issue #563: Test 1 - Valid Privy auth request creates user with DID linkage
    #[tokio::test]
    async fn test_privy_auth_creates_user_with_did() {
        let Some(pool) = setup_test_pool().await else {
            eprintln!("Skipping DB test: PostgreSQL not available");
            return;
        };
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

        assert!(
            json["token"].is_string(),
            "Response should contain JWT token"
        );
        assert_eq!(json["username"].as_str().unwrap(), "u_GBPK7THXDEPNBQB5K");
        assert_eq!(json["privy_did"].as_str().unwrap(), privy_did);

        cleanup_test_user(&pool, stellar_addr).await;
    }

    /// Issue #563: Test 2 - Reject if Stellar address already linked to different DID
    #[tokio::test]
    async fn test_privy_auth_rejects_address_linked_to_different_did() {
        let Some(pool) = setup_test_pool().await else {
            eprintln!("Skipping DB test: PostgreSQL not available");
            return;
        };
        let app = create_test_app(pool.clone());

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

        let resp_2 = app.oneshot(req_2).await.unwrap();
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

    /// Issue #563: Test 3 - Reject if Privy DID already linked to different address
    #[tokio::test]
    async fn test_privy_auth_rejects_did_linked_to_different_address() {
        let Some(pool) = setup_test_pool().await else {
            eprintln!("Skipping DB test: PostgreSQL not available");
            return;
        };
        let app = create_test_app(pool.clone());

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

        let resp_2 = app.oneshot(req_2).await.unwrap();
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

    /// Issue #563: Test 4 - Allow re-authentication with same DID and address
    #[tokio::test]
    async fn test_privy_auth_allows_same_did_address_pair() {
        let Some(pool) = setup_test_pool().await else {
            eprintln!("Skipping DB test: PostgreSQL not available");
            return;
        };
        let app = create_test_app(pool.clone());

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

        let resp_2 = app.oneshot(req_2).await.unwrap();
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

    /// Issue #563: Test 5 - Invalid Stellar address format rejected
    #[tokio::test]
    async fn test_privy_auth_rejects_invalid_stellar_address() {
        let app = create_mock_auth_app();

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

    /// Issue #563: Test 6 - Invalid Privy DID format rejected
    #[tokio::test]
    async fn test_privy_auth_rejects_invalid_did_format() {
        let app = create_mock_auth_app();

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

    /// Issue #563: Test 7 - Expired Privy token rejected
    #[tokio::test]
    async fn test_privy_auth_rejects_expired_token() {
        let app = create_mock_auth_app();

        let stellar_addr = "GBPK7THXDEPNBQB5K3EMQL5FZAQLHJ4XPBWJFNV3EPJN7CVPQGJZ6PBN";
        let privy_did = format!("did:privy:test_{}", Uuid::new_v4());

        // Create an expired token
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
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    /// Issue #562: Test 8 - Reject if Stellar address not in Privy token's linked_accounts
    #[tokio::test]
    async fn test_privy_auth_rejects_mismatched_wallet() {
        let app = create_mock_auth_app();

        let token_addr = "GBPK7THXDEPNBQB5K3EMQL5FZAQLHJ4XPBWJFNV3EPJN7CVPQGJZ6PBN";
        let submitted_addr = "GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN";
        let privy_did = format!("did:privy:test_{}", Uuid::new_v4());

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
    }

    /// Issue #562: Test 9 - Reject if token has no linked Stellar wallets
    #[tokio::test]
    async fn test_privy_auth_rejects_token_without_stellar_wallet() {
        let app = create_mock_auth_app();

        let stellar_addr = "GBPK7THXDEPNBQB5K3EMQL5FZAQLHJ4XPBWJFNV3EPJN7CVPQGJZ6PBN";
        let privy_did = format!("did:privy:test_{}", Uuid::new_v4());

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
    }

    #[tokio::test]
    async fn test_refresh_session_rejects_missing_token() {
        let app = create_mock_auth_app();
        let request = Request::builder()
            .method("POST")
            .uri("/api/auth/refresh")
            .header("content-type", "application/json")
            .body(Body::from("{}"))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_refresh_session_accepts_cached_session() {
        let cache = zaps_backend::api::AuthTokenCache::new();
        let user_id = Uuid::new_v4();
        let address = "GBBD47IF6LWK7P7MDEVSCWR7DPUWV3NY3DTQEVFL4NAT4AQH3ZLLFLA5".to_string();
        let username = "testuser".to_string();
        let session = zaps_backend::api::auth_middleware::CachedSession::new(
            user_id,
            address.clone(),
            username.clone(),
        );
        cache.insert("my-cached-token".to_string(), session).await;

        let state = AuthState {
            pool: sqlx::postgres::PgPoolOptions::new()
                .connect_lazy("postgres://localhost/dummy")
                .unwrap(),
            privy: Arc::new(PrivyJwksClient::new(mock_jwks_url().to_string())),
            privy_app_id: TEST_APP_ID.to_string(),
            cache: Some(cache),
        };
        let app = Router::new().nest(
            "/api/auth",
            zaps_backend::api::auth_routes_with_state(state),
        );

        let request = Request::builder()
            .method("POST")
            .uri("/api/auth/refresh")
            .header("Authorization", "Bearer my-cached-token")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert!(json["token"].as_str().is_some());
        assert_eq!(json["username"].as_str().unwrap(), "testuser");
        assert_eq!(json["address"].as_str().unwrap(), address);
        assert_eq!(json["user_id"].as_str().unwrap(), user_id.to_string());
    }

    #[tokio::test]
    async fn test_refresh_session_rejects_invalid_token() {
        let app = create_mock_auth_app();
        let request = Request::builder()
            .method("POST")
            .uri("/api/auth/refresh")
            .header("Authorization", "Bearer invalid-garbage-token")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}
