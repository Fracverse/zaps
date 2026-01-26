use crate::config::Config;
use governor::{clock::DefaultClock, state::keyed::DashMapStateStore, Quota, RateLimiter};
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone)]
pub struct RateLimitService {
    pub limiter: Arc<RateLimiter<String, DashMapStateStore<String>, DefaultClock>>,
}

impl RateLimitService {
    pub fn new(config: Config) -> Self {
        let quota = Quota::with_period(Duration::from_millis(config.rate_limit.window_ms))
            .unwrap()
            .allow_burst(NonZeroU32::new(config.rate_limit.max_requests).unwrap());

        let limiter = RateLimiter::dashmap(quota);

        Self {
            limiter: Arc::new(limiter),
        }
    }

    pub fn check_rate_limit(&self, key: String) -> bool {
        self.limiter.check_key(&key).is_ok()
    }
}
