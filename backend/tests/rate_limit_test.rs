use std::thread;
use std::time::Duration;
use zaps_backend::config::Config;
use zaps_backend::service::RateLimitService;

// #[test]
// fn test_rate_limit_enforcement() {
//     // Setup config with strict limits: 2 requests per 1 second
//     let config = Config::default();
//     // config.rate_limit = RateLimitConfig {
//     //     window_ms: 1000,
//     //     max_requests: 2,
//     //     scope: RateLimitScope::Ip,
//     // };

//     let rate_limit_service = RateLimitService::new(config);
//     let key = "127.0.0.1".to_string();

//     // First request should pass
//     assert!(rate_limit_service.check_rate_limit(key.clone()));

//     // Second request should pass
//     assert!(rate_limit_service.check_rate_limit(key.clone()));

//     // Third request should fail
//     assert!(!rate_limit_service.check_rate_limit(key.clone()));
// }

// #[test]
// fn test_rate_limit_expiry() {
//     // Setup config: 1 request per 100ms
//     let config = Config::default();
//     // config.rate_limit = RateLimitConfig {
//     //     window_ms: 100,
//     //     max_requests: 1,
//     //     scope: RateLimitScope::Ip,
//     // };

//     let rate_limit_service = RateLimitService::new(config);
//     let key = "127.0.0.1".to_string();

//     // First request passes
//     assert!(rate_limit_service.check_rate_limit(key.clone()));

//     // Immediate second request fails
//     assert!(!rate_limit_service.check_rate_limit(key.clone()));

//     // Wait for window to expire
//     thread::sleep(Duration::from_millis(150));

//     // Request should pass again
//     assert!(rate_limit_service.check_rate_limit(key.clone()));
// }

// #[test]
// fn test_independent_limits() {
//     // Setup config: 1 request per second
//     let config = Config::default();
//     // config.rate_limit = RateLimitConfig {
//     //     window_ms: 1000,
//     //     max_requests: 1,
//     //     scope: RateLimitScope::Ip,
//     // };

//     let rate_limit_service = RateLimitService::new(config);
//     let key1 = "127.0.0.1".to_string();
//     let key2 = "192.168.1.1".to_string();

//     // Key1 uses its quota
//     assert!(rate_limit_service.check_rate_limit(key1.clone()));
//     assert!(!rate_limit_service.check_rate_limit(key1.clone()));

//     // Key2 should still be allowed
//     assert!(rate_limit_service.check_rate_limit(key2.clone()));
// }
