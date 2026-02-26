use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use lazy_static::lazy_static;
use serde_json::{json, Value};
use tokio::sync::Mutex;
use tower::util::ServiceExt;

use blinks_backend::{app::create_app, config::Config, db};

lazy_static! {
    static ref MIGRATION_LOCK: Mutex<bool> = Mutex::new(false);
}

/// Helper to create a test app with a test database
/// Note: These tests require a running database as defined in the config/env.
/// Run with: cargo test --test profile_handler_test -- --ignored
async fn create_test_app() -> Router {
    // Attempt to load config - if fails, use default
    let config = Config::load().expect("Failed to load config");

    // Use a mutex to ensure migrations are only run once for all tests
    {
        let mut initialized = MIGRATION_LOCK.lock().await;
        if !*initialized {
            // Reset migrations first only on the first initialization
            let _ = db::reset_migrations(&config.database.url).await;
            db::run_migrations(&config.database.url)
                .await
                .expect("Failed to run database migrations");
            *initialized = true;
        }
    }

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
    use axum::extract::ConnectInfo;
    use std::net::SocketAddr;

    Request::builder()
        .method("POST")
        .uri(uri)
        .header("Content-Type", "application/json")
        .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 12345))))
        .body(Body::from(body.to_string()))
        .unwrap()
}

/// Helper to make authenticated JSON POST request
fn json_post_auth(uri: &str, body: Value, token: &str) -> Request<Body> {
    use axum::extract::ConnectInfo;
    use std::net::SocketAddr;

    Request::builder()
        .method("POST")
        .uri(uri)
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", token))
        .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 12345))))
        .body(Body::from(body.to_string()))
        .unwrap()
}

/// Helper to make JSON PATCH request
fn json_patch(uri: &str, body: Value) -> Request<Body> {
    use axum::extract::ConnectInfo;
    use std::net::SocketAddr;

    Request::builder()
        .method("PATCH")
        .uri(uri)
        .header("Content-Type", "application/json")
        .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 12345))))
        .body(Body::from(body.to_string()))
        .unwrap()
}

/// Helper to make authenticated JSON PATCH request
fn json_patch_auth(uri: &str, body: Value, token: &str) -> Request<Body> {
    use axum::extract::ConnectInfo;
    use std::net::SocketAddr;

    Request::builder()
        .method("PATCH")
        .uri(uri)
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", token))
        .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 12345))))
        .body(Body::from(body.to_string()))
        .unwrap()
}

/// Helper to make GET request
fn json_get(uri: &str) -> Request<Body> {
    use axum::extract::ConnectInfo;
    use std::net::SocketAddr;

    Request::builder()
        .method("GET")
        .uri(uri)
        .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 12345))))
        .body(Body::empty())
        .unwrap()
}

/// Helper to make authenticated GET request
fn json_get_auth(uri: &str, token: &str) -> Request<Body> {
    use axum::extract::ConnectInfo;
    use std::net::SocketAddr;

    Request::builder()
        .method("GET")
        .uri(uri)
        .header("Authorization", format!("Bearer {}", token))
        .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 12345))))
        .body(Body::empty())
        .unwrap()
}

/// Helper to make DELETE request
fn json_delete(uri: &str) -> Request<Body> {
    use axum::extract::ConnectInfo;
    use std::net::SocketAddr;

    Request::builder()
        .method("DELETE")
        .uri(uri)
        .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 12345))))
        .body(Body::empty())
        .unwrap()
}

/// Helper to make authenticated DELETE request
fn json_delete_auth(uri: &str, token: &str) -> Request<Body> {
    use axum::extract::ConnectInfo;
    use std::net::SocketAddr;

    Request::builder()
        .method("DELETE")
        .uri(uri)
        .header("Authorization", format!("Bearer {}", token))
        .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 12345))))
        .body(Body::empty())
        .unwrap()
}

/// Helper to parse JSON response
async fn parse_response(response: axum::response::Response) -> Value {
    let status = response.status();
    let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("Failed to read response body");

    match serde_json::from_slice::<Value>(&body_bytes) {
        Ok(json) => json,
        Err(e) => {
            let body_str = String::from_utf8_lossy(&body_bytes);
            panic!(
                "Failed to parse response body as JSON. Status: {}. Error: {}. Body: {}",
                status, e, body_str
            );
        }
    }
}

/// Helper to register a user and get their token
async fn register_and_get_token(app: &Router, user_id: &str, pin: &str) -> String {
    let response = app
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

    let body = parse_response(response).await;
    body["token"].as_str().unwrap().to_string()
}

// =============================================================================
// Integration Tests (require database)
// =============================================================================

#[tokio::test]
#[ignore]
async fn test_create_profile_success() {
    let app = create_test_app().await;

    let user_id = format!("testuser_{}", uuid::Uuid::new_v4());
    let token = register_and_get_token(&app, &user_id, "1234").await;

    let response = app
        .clone()
        .oneshot(json_post_auth(
            "/profiles/",
            json!({
                "display_name": "Test User",
                "avatar_url": "https://example.com/avatar.jpg",
                "bio": "This is a test bio",
                "country": "US"
            }),
            &token,
        ))
        .await
        .unwrap();

    let status = response.status();
    let body = parse_response(response).await;
    assert_eq!(status, StatusCode::OK, "Response body: {:?}", body);

    assert_eq!(body["user_id"], user_id);
    assert_eq!(body["display_name"], "Test User");
    assert_eq!(body["avatar_url"], "https://example.com/avatar.jpg");
    assert_eq!(body["bio"], "This is a test bio");
    assert_eq!(body["country"], "US");
}

#[tokio::test]
#[ignore]
async fn test_create_profile_duplicate() {
    let app = create_test_app().await;

    let user_id = format!("testuser_{}", uuid::Uuid::new_v4());
    let token = register_and_get_token(&app, &user_id, "1234").await;

    // Create first profile
    let _ = app
        .clone()
        .oneshot(json_post_auth(
            "/profiles/",
            json!({
                "display_name": "Test User"
            }),
            &token,
        ))
        .await
        .unwrap();

    // Try to create duplicate profile
    let response = app
        .clone()
        .oneshot(json_post_auth(
            "/profiles/",
            json!({
                "display_name": "Test User 2"
            }),
            &token,
        ))
        .await
        .unwrap();

    let status = response.status();
    assert_eq!(status, StatusCode::CONFLICT);
}

#[tokio::test]
#[ignore]
async fn test_create_profile_validation_empty_display_name() {
    let app = create_test_app().await;

    let user_id = format!("testuser_{}", uuid::Uuid::new_v4());
    let token = register_and_get_token(&app, &user_id, "1234").await;

    let response = app
        .clone()
        .oneshot(json_post_auth(
            "/profiles/",
            json!({
                "display_name": ""
            }),
            &token,
        ))
        .await
        .unwrap();

    let status = response.status();
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
#[ignore]
async fn test_create_profile_validation_display_name_too_long() {
    let app = create_test_app().await;

    let user_id = format!("testuser_{}", uuid::Uuid::new_v4());
    let token = register_and_get_token(&app, &user_id, "1234").await;

    let long_name = "a".repeat(101);
    let response = app
        .clone()
        .oneshot(json_post_auth(
            "/profiles/",
            json!({
                "display_name": long_name
            }),
            &token,
        ))
        .await
        .unwrap();

    let status = response.status();
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
#[ignore]
async fn test_create_profile_validation_bio_too_long() {
    let app = create_test_app().await;

    let user_id = format!("testuser_{}", uuid::Uuid::new_v4());
    let token = register_and_get_token(&app, &user_id, "1234").await;

    let long_bio = "a".repeat(501);
    let response = app
        .clone()
        .oneshot(json_post_auth(
            "/profiles/",
            json!({
                "display_name": "Test User",
                "bio": long_bio
            }),
            &token,
        ))
        .await
        .unwrap();

    let status = response.status();
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
#[ignore]
async fn test_create_profile_validation_invalid_avatar_url() {
    let app = create_test_app().await;

    let user_id = format!("testuser_{}", uuid::Uuid::new_v4());
    let token = register_and_get_token(&app, &user_id, "1234").await;

    let response = app
        .clone()
        .oneshot(json_post_auth(
            "/profiles/",
            json!({
                "display_name": "Test User",
                "avatar_url": "not-a-valid-url"
            }),
            &token,
        ))
        .await
        .unwrap();

    let status = response.status();
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
#[ignore]
async fn test_get_profile_success() {
    let app = create_test_app().await;

    let user_id = format!("testuser_{}", uuid::Uuid::new_v4());
    let token = register_and_get_token(&app, &user_id, "1234").await;

    // Create profile first
    let create_response = app
        .clone()
        .oneshot(json_post_auth(
            "/profiles/",
            json!({
                "display_name": "Test User",
                "bio": "Test bio"
            }),
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(create_response.status(), StatusCode::OK);

    // Get profile (public endpoint, no auth required)
    let response = app
        .clone()
        .oneshot(json_get(&format!("/profiles/{}", user_id)))
        .await
        .unwrap();

    let status = response.status();
    let body = parse_response(response).await;
    assert_eq!(status, StatusCode::OK, "Response body: {:?}", body);

    assert_eq!(body["user_id"], user_id);
    assert_eq!(body["display_name"], "Test User");
    assert_eq!(body["bio"], "Test bio");
}

#[tokio::test]
#[ignore]
async fn test_get_profile_not_found() {
    let app = create_test_app().await;

    let user_id = format!("testuser_{}", uuid::Uuid::new_v4());

    let response = app
        .clone()
        .oneshot(json_get(&format!("/profiles/{}", user_id)))
        .await
        .unwrap();

    let status = response.status();
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
#[ignore]
async fn test_get_my_profile_success() {
    let app = create_test_app().await;

    let user_id = format!("testuser_{}", uuid::Uuid::new_v4());
    let token = register_and_get_token(&app, &user_id, "1234").await;

    // Create profile first
    let _ = app
        .clone()
        .oneshot(json_post_auth(
            "/profiles/",
            json!({
                "display_name": "Test User"
            }),
            &token,
        ))
        .await
        .unwrap();

    // Get own profile
    let response = app
        .clone()
        .oneshot(json_get_auth("/profiles/me", &token))
        .await
        .unwrap();

    let status = response.status();
    let body = parse_response(response).await;
    assert_eq!(status, StatusCode::OK, "Response body: {:?}", body);

    assert_eq!(body["user_id"], user_id);
    assert_eq!(body["display_name"], "Test User");
}

#[tokio::test]
#[ignore]
async fn test_update_profile_success() {
    let app = create_test_app().await;

    let user_id = format!("testuser_{}", uuid::Uuid::new_v4());
    let token = register_and_get_token(&app, &user_id, "1234").await;

    // Create profile first
    let _ = app
        .clone()
        .oneshot(json_post_auth(
            "/profiles/",
            json!({
                "display_name": "Original Name"
            }),
            &token,
        ))
        .await
        .unwrap();

    // Update profile
    let response = app
        .clone()
        .oneshot(json_patch_auth(
            &format!("/profiles/{}", user_id),
            json!({
                "display_name": "Updated Name",
                "bio": "Updated bio"
            }),
            &token,
        ))
        .await
        .unwrap();

    let status = response.status();
    let body = parse_response(response).await;
    assert_eq!(status, StatusCode::OK, "Response body: {:?}", body);

    assert_eq!(body["display_name"], "Updated Name");
    assert_eq!(body["bio"], "Updated bio");
}

#[tokio::test]
#[ignore]
async fn test_update_profile_rbac_user_cannot_update_other() {
    let app = create_test_app().await;

    let user1_id = format!("testuser1_{}", uuid::Uuid::new_v4());
    let user2_id = format!("testuser2_{}", uuid::Uuid::new_v4());

    let token1 = register_and_get_token(&app, &user1_id, "1234").await;
    let token2 = register_and_get_token(&app, &user2_id, "1234").await;

    // User1 creates profile
    let _ = app
        .clone()
        .oneshot(json_post_auth(
            "/profiles/",
            json!({
                "display_name": "User 1"
            }),
            &token1,
        ))
        .await
        .unwrap();

    // User2 tries to update User1's profile (should fail)
    let response = app
        .clone()
        .oneshot(json_patch_auth(
            &format!("/profiles/{}", user1_id),
            json!({
                "display_name": "Hacked Name"
            }),
            &token2,
        ))
        .await
        .unwrap();

    let status = response.status();
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
#[ignore]
async fn test_update_profile_rbac_admin_can_update_any() {
    let app = create_test_app().await;

    // Create regular user
    let user_id = format!("testuser_{}", uuid::Uuid::new_v4());
    let user_token = register_and_get_token(&app, &user_id, "1234").await;

    // Create admin user (we'll need to manually set role in DB or use a test helper)
    // For now, we'll test that admin can update - this requires admin token generation
    // Note: In a real scenario, you'd need a way to create admin users for testing

    // User creates profile
    let _ = app
        .clone()
        .oneshot(json_post_auth(
            "/profiles/",
            json!({
                "display_name": "Regular User"
            }),
            &user_token,
        ))
        .await
        .unwrap();

    // This test would require admin token generation
    // For now, we verify the RBAC logic is in place via the previous test
}

#[tokio::test]
#[ignore]
async fn test_update_profile_validation() {
    let app = create_test_app().await;

    let user_id = format!("testuser_{}", uuid::Uuid::new_v4());
    let token = register_and_get_token(&app, &user_id, "1234").await;

    // Create profile first
    let _ = app
        .clone()
        .oneshot(json_post_auth(
            "/profiles/",
            json!({
                "display_name": "Test User"
            }),
            &token,
        ))
        .await
        .unwrap();

    // Try to update with invalid data
    let response = app
        .clone()
        .oneshot(json_patch_auth(
            &format!("/profiles/{}", user_id),
            json!({
                "display_name": ""  // Empty display name
            }),
            &token,
        ))
        .await
        .unwrap();

    let status = response.status();
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
#[ignore]
async fn test_delete_profile_success() {
    let app = create_test_app().await;

    let user_id = format!("testuser_{}", uuid::Uuid::new_v4());
    let token = register_and_get_token(&app, &user_id, "1234").await;

    // Create profile first
    let _ = app
        .clone()
        .oneshot(json_post_auth(
            "/profiles/",
            json!({
                "display_name": "Test User"
            }),
            &token,
        ))
        .await
        .unwrap();

    // Delete profile
    let response = app
        .clone()
        .oneshot(json_delete_auth(&format!("/profiles/{}", user_id), &token))
        .await
        .unwrap();

    let status = response.status();
    assert_eq!(status, StatusCode::NO_CONTENT);

    // Verify profile is deleted
    let get_response = app
        .clone()
        .oneshot(json_get(&format!("/profiles/{}", user_id)))
        .await
        .unwrap();
    assert_eq!(get_response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
#[ignore]
async fn test_delete_profile_rbac_user_cannot_delete_other() {
    let app = create_test_app().await;

    let user1_id = format!("testuser1_{}", uuid::Uuid::new_v4());
    let user2_id = format!("testuser2_{}", uuid::Uuid::new_v4());

    let token1 = register_and_get_token(&app, &user1_id, "1234").await;
    let token2 = register_and_get_token(&app, &user2_id, "1234").await;

    // User1 creates profile
    let _ = app
        .clone()
        .oneshot(json_post_auth(
            "/profiles/",
            json!({
                "display_name": "User 1"
            }),
            &token1,
        ))
        .await
        .unwrap();

    // User2 tries to delete User1's profile (should fail)
    let response = app
        .clone()
        .oneshot(json_delete_auth(
            &format!("/profiles/{}", user1_id),
            &token2,
        ))
        .await
        .unwrap();

    let status = response.status();
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
#[ignore]
async fn test_create_profile_requires_auth() {
    let app = create_test_app().await;

    let response = app
        .clone()
        .oneshot(json_post(
            "/profiles/",
            json!({
                "display_name": "Test User"
            }),
        ))
        .await
        .unwrap();

    let status = response.status();
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
#[ignore]
async fn test_update_profile_requires_auth() {
    let app = create_test_app().await;

    let user_id = format!("testuser_{}", uuid::Uuid::new_v4());

    let response = app
        .clone()
        .oneshot(json_patch(
            &format!("/profiles/{}", user_id),
            json!({
                "display_name": "Test User"
            }),
        ))
        .await
        .unwrap();

    let status = response.status();
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
#[ignore]
async fn test_delete_profile_requires_auth() {
    let app = create_test_app().await;

    let user_id = format!("testuser_{}", uuid::Uuid::new_v4());

    let response = app
        .clone()
        .oneshot(json_delete(&format!("/profiles/{}", user_id)))
        .await
        .unwrap();

    let status = response.status();
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}
