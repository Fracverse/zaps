use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use serde_json::{json, Value};
use tower::util::ServiceExt; // for oneshot

use zaps_backend::{app::create_app, auth, config::Config, db};

/// Helper to create a test app with a test database
/// Note: These tests require a running database as defined in the config/env.
/// Run with: cargo test --test auth_handler_test -- --ignored
async fn create_test_app() -> Router {
    // Attempt to load config - if fails, use default
    let config = Config::load().expect("Failed to load config");

    // Ensure migrations are run for the test database
    let _ = db::run_migrations(&config.database.url).await;

    // Create a database pool using the config URL
    let pool = db::create_pool(&config.database.url)
        .await
        .expect("Failed to create pool");

    create_app(pool, config)
        .await
        .expect("Failed to create app")
}

/// Helper to make JSON POST request
fn json_post(uri: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("Content-Type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

/// Helper to parse JSON response
async fn parse_response(response: axum::response::Response) -> Value {
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&body).unwrap_or(json!({}))
}

// =============================================================================
// Unit Tests
// =============================================================================

#[cfg(test)]
mod unit_tests {
    use zaps_backend::auth;

    #[test]
    fn test_pin_hash_and_verify_flow() {
        let pin = "1234";
        let hash = auth::hash_pin(pin).expect("Failed to hash");

        assert!(auth::verify_pin(pin, &hash).expect("Failed to verify"));
        assert!(!auth::verify_pin("wrong", &hash).expect("Failed to verify"));
    }

    #[test]
    fn test_access_token_cannot_refresh() {
        let secret = "test-secret";
        let access_token = auth::generate_access_token("user1", secret, 1).unwrap();

        // Access token should fail refresh validation
        let result = auth::validate_refresh_token(&access_token, secret);
        assert!(result.is_err());
    }

    #[test]
    fn test_refresh_token_cannot_access() {
        let secret = "test-secret";
        let refresh_token = auth::generate_refresh_token("user1", secret, 168).unwrap();

        // Refresh token should fail access validation
        let result = auth::validate_access_token(&refresh_token, secret);
        assert!(result.is_err());
    }

    #[test]
    fn test_token_pair_generation() {
        let secret = "test-secret";
        let user_id = "testuser";

        let access = auth::generate_access_token(user_id, secret, 24).unwrap();
        let refresh = auth::generate_refresh_token(user_id, secret, 168).unwrap();

        // Both tokens are valid
        let access_claims = auth::validate_access_token(&access, secret).unwrap();
        let refresh_claims = auth::validate_refresh_token(&refresh, secret).unwrap();

        assert_eq!(access_claims.sub, user_id);
        assert_eq!(refresh_claims.sub, user_id);

        // Tokens are different
        assert_ne!(access, refresh);
    }
}

// =============================================================================
// Integration Tests (require database)
// =============================================================================

#[tokio::test]
#[ignore] // Requires database - run with: cargo test --test auth_handler_test -- --ignored
async fn test_register_success() {
    let app = create_test_app().await;

    let user_id = format!("testuser_{}", uuid::Uuid::new_v4());
    let response = app
        .oneshot(json_post(
            "/auth/register",
            json!({
                "user_id": user_id,
                "pin": "1234"
            }),
        ))
        .await
        .unwrap();

    let status = response.status();
    let body = parse_response(response).await;
    assert_eq!(status, StatusCode::OK, "Response body: {:?}", body);

    assert_eq!(body["user_id"], user_id);
    assert!(body["token"].as_str().is_some());
    assert!(body["refresh_token"].as_str().is_some());
}

#[tokio::test]
#[ignore]
async fn test_register_duplicate_user() {
    let app = create_test_app().await;

    let user_id = format!("testuser_{}", uuid::Uuid::new_v4());

    // First registration should succeed
    let response = app
        .clone()
        .oneshot(json_post(
            "/auth/register",
            json!({
                "user_id": user_id,
                "pin": "1234"
            }),
        ))
        .await
        .unwrap();
    let status = response.status();
    let body = parse_response(response).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "First registration failed: {:?}",
        body
    );

    // Second registration should fail with conflict
    let response = app
        .oneshot(json_post(
            "/auth/register",
            json!({
                "user_id": user_id,
                "pin": "5678"
            }),
        ))
        .await
        .unwrap();

    let status = response.status();
    let body = parse_response(response).await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "Registration body: {:?}",
        body
    );
}

#[tokio::test]
#[ignore]
async fn test_login_success() {
    let app = create_test_app().await;

    let user_id = format!("testuser_{}", uuid::Uuid::new_v4());
    let pin = "1234";

    // Register first
    let _ = app
        .clone()
        .oneshot(json_post(
            "/auth/register",
            json!({
                "user_id": user_id,
                "pin": pin
            }),
        ))
        .await
        .unwrap();

    // Login should succeed
    let response = app
        .oneshot(json_post(
            "/auth/login",
            json!({
                "user_id": user_id,
                "pin": pin
            }),
        ))
        .await
        .unwrap();

    let status = response.status();
    let body = parse_response(response).await;
    assert_eq!(status, StatusCode::OK, "Login failed: {:?}", body);

    assert_eq!(body["user_id"], user_id);
    assert!(body["token"].as_str().is_some());
}

#[tokio::test]
#[ignore]
async fn test_login_wrong_pin() {
    let app = create_test_app().await;

    let user_id = format!("testuser_{}", uuid::Uuid::new_v4());

    // Register first
    let _ = app
        .clone()
        .oneshot(json_post(
            "/auth/register",
            json!({
                "user_id": user_id,
                "pin": "1234"
            }),
        ))
        .await
        .unwrap();

    // Login with wrong PIN should fail
    let response = app
        .oneshot(json_post(
            "/auth/login",
            json!({
                "user_id": user_id,
                "pin": "wrong"
            }),
        ))
        .await
        .unwrap();

    let status = response.status();
    let body = parse_response(response).await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "Login with wrong PIN body: {:?}",
        body
    );
}

#[tokio::test]
#[ignore]
async fn test_refresh_token_success() {
    let app = create_test_app().await;

    let user_id = format!("testuser_{}", uuid::Uuid::new_v4());

    // Register to get tokens
    let response = app
        .clone()
        .oneshot(json_post(
            "/auth/register",
            json!({
                "user_id": user_id,
                "pin": "1234"
            }),
        ))
        .await
        .unwrap();

    let body = parse_response(response).await;
    let refresh_token = body["refresh_token"].as_str().unwrap();

    // Use refresh token to get new tokens
    let response = app
        .oneshot(json_post(
            "/auth/refresh",
            json!({
                "token": refresh_token
            }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = parse_response(response).await;
    assert_eq!(body["user_id"], user_id);
    assert!(body["token"].as_str().is_some());
}
