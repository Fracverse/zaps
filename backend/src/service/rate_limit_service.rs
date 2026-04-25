use crate::config::Config;
use crate::models::{EndpointRateLimitConfig, RateLimitConfig, RateLimitScope};
use dashmap::DashMap;
use redis::{aio::ConnectionManager, AsyncCommands};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;

#[derive(Debug, Clone)]
pub struct RateLimitDecision {
    pub allowed: bool,
    pub limit: u32,
    pub remaining: u32,
    pub reset_after_seconds: u64,
}

#[derive(Debug, Clone)]
struct WindowCounter {
    count: u32,
    reset_at_ms: u64,
}

#[derive(Clone)]
pub struct RateLimitService {
    config: RateLimitConfig,
    redis: Arc<Mutex<Option<ConnectionManager>>>,
    local_windows: Arc<DashMap<String, WindowCounter>>,
}

impl RateLimitService {
    pub async fn new(config: Config) -> Self {
        let redis = match redis::Client::open(config.queue_config.redis_url.clone()) {
            Ok(client) => match client.get_connection_manager().await {
                Ok(manager) => {
                    tracing::info!("Redis rate limiter connected");
                    Some(manager)
                }
                Err(error) => {
                    tracing::warn!(%error, "Redis rate limiter unavailable; using local limiter");
                    None
                }
            },
            Err(error) => {
                tracing::warn!(%error, "Invalid Redis URL; using local rate limiter");
                None
            }
        };

        Self {
            config: config.rate_limit,
            redis: Arc::new(Mutex::new(redis)),
            local_windows: Arc::new(DashMap::new()),
        }
    }

    pub async fn check_rate_limit(
        &self,
        scope_key: &str,
        path: &str,
        scope: &RateLimitScope,
    ) -> RateLimitDecision {
        let endpoint = self.endpoint_limit(path);
        let window_ms = endpoint
            .as_ref()
            .map(|limit| limit.window_ms)
            .unwrap_or(self.config.window_ms);
        let max_requests = endpoint
            .as_ref()
            .map(|limit| limit.max_requests)
            .unwrap_or(self.config.max_requests);
        let bucket = endpoint
            .as_ref()
            .map(|limit| limit.path_prefix.as_str())
            .unwrap_or("global");
        let redis_key = format!(
            "rate_limit:{}:{}:{}",
            scope_name(scope),
            bucket.replace('/', "_"),
            scope_key
        );

        if let Some(decision) = self.check_redis(&redis_key, max_requests, window_ms).await {
            return decision;
        }

        self.check_local(&redis_key, max_requests, window_ms)
    }

    fn endpoint_limit(&self, path: &str) -> Option<EndpointRateLimitConfig> {
        self.config
            .endpoint_limits
            .iter()
            .filter(|limit| path.starts_with(&limit.path_prefix))
            .max_by_key(|limit| limit.path_prefix.len())
            .cloned()
    }

    async fn check_redis(
        &self,
        key: &str,
        max_requests: u32,
        window_ms: u64,
    ) -> Option<RateLimitDecision> {
        let mut guard = self.redis.lock().await;
        let connection = guard.as_mut()?;

        let count: redis::RedisResult<u32> = connection.incr(key, 1_u32).await;
        let count = match count {
            Ok(count) => count,
            Err(error) => {
                tracing::warn!(%error, "Redis rate limit increment failed; falling back locally");
                *guard = None;
                return None;
            }
        };

        if count == 1 {
            let expire_result: redis::RedisResult<()> =
                connection.pexpire(key, window_ms as i64).await;
            if let Err(error) = expire_result {
                tracing::warn!(%error, "Redis rate limit expiry failed");
            }
        }

        let ttl_ms: i64 = connection.pttl(key).await.unwrap_or(window_ms as i64);
        let reset_after_seconds = ((ttl_ms.max(0) as u64) + 999) / 1000;
        Some(RateLimitDecision {
            allowed: count <= max_requests,
            limit: max_requests,
            remaining: max_requests.saturating_sub(count),
            reset_after_seconds,
        })
    }

    fn check_local(&self, key: &str, max_requests: u32, window_ms: u64) -> RateLimitDecision {
        let now = now_ms();
        let reset_at_ms = now.saturating_add(window_ms);
        let mut entry = self
            .local_windows
            .entry(key.to_string())
            .or_insert(WindowCounter {
                count: 0,
                reset_at_ms,
            });

        if now >= entry.reset_at_ms {
            entry.count = 0;
            entry.reset_at_ms = reset_at_ms;
        }

        entry.count = entry.count.saturating_add(1);
        let reset_after_seconds = (entry.reset_at_ms.saturating_sub(now) + 999) / 1000;

        RateLimitDecision {
            allowed: entry.count <= max_requests,
            limit: max_requests,
            remaining: max_requests.saturating_sub(entry.count),
            reset_after_seconds,
        }
    }
}

pub fn scope_name(scope: &RateLimitScope) -> &'static str {
    match scope {
        RateLimitScope::Ip => "ip",
        RateLimitScope::User => "user",
        RateLimitScope::ApiKey => "api_key",
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
