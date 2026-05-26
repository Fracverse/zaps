use std::time::Duration;
use blinks_backend::config::Config;
use blinks_backend::models::{RateLimitConfig, RateLimitScope};
use blinks_backend::service::RateLimitService;

#[tokio::test]
async fn test_rate_limit_enforcement() {
    let mut config = Config::default();
    config.rate_limit = RateLimitConfig {
        window_ms: 1000,
        max_requests: 2,
        scope: RateLimitScope::Ip,
        endpoint_limits: vec![],
        bypass_admin: true,
    };

    let rate_limit_service = RateLimitService::new(config).await;
    let key = "127.0.0.1";
    let path = "/test";
    let scope = RateLimitScope::Ip;

    // First request should pass
    let decision1 = rate_limit_service.check_rate_limit(key, path, &scope).await;
    assert!(decision1.allowed);
    assert_eq!(decision1.limit, 2);
    assert_eq!(decision1.remaining, 1);

    // Second request should pass
    let decision2 = rate_limit_service.check_rate_limit(key, path, &scope).await;
    assert!(decision2.allowed);
    assert_eq!(decision2.remaining, 0);

    // Third request should fail
    let decision3 = rate_limit_service.check_rate_limit(key, path, &scope).await;
    assert!(!decision3.allowed);
}

#[tokio::test]
async fn test_rate_limit_expiry() {
    let mut config = Config::default();
    config.rate_limit = RateLimitConfig {
        window_ms: 200,
        max_requests: 1,
        scope: RateLimitScope::Ip,
        endpoint_limits: vec![],
        bypass_admin: true,
    };

    let rate_limit_service = RateLimitService::new(config).await;
    let key = "127.0.0.1";
    let path = "/test";
    let scope = RateLimitScope::Ip;

    // First request passes
    let decision1 = rate_limit_service.check_rate_limit(key, path, &scope).await;
    assert!(decision1.allowed);

    // Immediate second request fails
    let decision2 = rate_limit_service.check_rate_limit(key, path, &scope).await;
    assert!(!decision2.allowed);

    // Wait for window to expire
    tokio::time::sleep(Duration::from_millis(250)).await;

    // Request should pass again
    let decision3 = rate_limit_service.check_rate_limit(key, path, &scope).await;
    assert!(decision3.allowed);
}

#[tokio::test]
async fn test_independent_limits() {
    let mut config = Config::default();
    config.rate_limit = RateLimitConfig {
        window_ms: 1000,
        max_requests: 1,
        scope: RateLimitScope::Ip,
        endpoint_limits: vec![],
        bypass_admin: true,
    };

    let rate_limit_service = RateLimitService::new(config).await;
    let key1 = "127.0.0.1";
    let key2 = "192.168.1.1";
    let path = "/test";
    let scope = RateLimitScope::Ip;

    // Key1 uses its quota
    let decision1 = rate_limit_service.check_rate_limit(key1, path, &scope).await;
    assert!(decision1.allowed);
    
    let decision2 = rate_limit_service.check_rate_limit(key1, path, &scope).await;
    assert!(!decision2.allowed);

    // Key2 should still be allowed
    let decision3 = rate_limit_service.check_rate_limit(key2, path, &scope).await;
    assert!(decision3.allowed);
}

#[tokio::test]
async fn test_sliding_window_burst() {
    let mut config = Config::default();
    config.rate_limit = RateLimitConfig {
        window_ms: 300,
        max_requests: 2,
        scope: RateLimitScope::Ip,
        endpoint_limits: vec![],
        bypass_admin: true,
    };

    let rate_limit_service = RateLimitService::new(config).await;
    let key = "127.0.0.1";
    let path = "/test";
    let scope = RateLimitScope::Ip;

    // Request 1 at t=0ms (allowed)
    assert!(rate_limit_service.check_rate_limit(key, path, &scope).await.allowed);

    // Sleep 100ms
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Request 2 at t=100ms (allowed)
    assert!(rate_limit_service.check_rate_limit(key, path, &scope).await.allowed);

    // Request 3 at t=120ms (blocked - limit of 2 reached)
    assert!(!rate_limit_service.check_rate_limit(key, path, &scope).await.allowed);

    // Sleep 250ms (total time ~350ms).
    // Request 1 (at t=0ms) has fallen out of the 300ms window, but Request 2 (at t=100ms) is still inside (100ms + 300ms = 400ms expiry).
    tokio::time::sleep(Duration::from_millis(250)).await;

    // Request 4 at t=350ms should pass since only Request 2 is active in the window
    assert!(rate_limit_service.check_rate_limit(key, path, &scope).await.allowed);

    // Request 5 at t=360ms should fail since both Request 2 and Request 4 are active
    assert!(!rate_limit_service.check_rate_limit(key, path, &scope).await.allowed);
}
