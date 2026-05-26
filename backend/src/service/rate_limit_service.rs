use crate::config::Config;
use crate::models::{EndpointRateLimitConfig, RateLimitConfig, RateLimitScope};
use dashmap::DashMap;
use redis::{aio::ConnectionManager, AsyncCommands};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;
use uuid::Uuid;

const REDIS_SLIDING_WINDOW_LUA: &str = r#"
    local key = KEYS[1]
    local now = tonumber(ARGV[1])
    local window = tonumber(ARGV[2])
    local limit = tonumber(ARGV[3])
    local member = ARGV[4]
    
    -- Prune expired entries
    redis.call('zremrangebyscore', key, 0, now - window)
    
    -- Count remaining entries
    local count = redis.call('zcard', key)
    
    local allowed = 0
    if count < limit then
        redis.call('zadd', key, now, member)
        redis.call('pexpire', key, window)
        allowed = 1
        count = count + 1
    end
    
    -- Get oldest timestamp to calculate reset
    local oldest = redis.call('zrange', key, 0, 0, 'withscores')
    local oldest_ts = now
    if #oldest > 0 then
        oldest_ts = tonumber(oldest[2])
    end
    
    return {allowed, count, oldest_ts}
"#;

#[derive(Debug, Clone)]
pub struct RateLimitDecision {
    pub allowed: bool,
    pub limit: u32,
    pub remaining: u32,
    pub reset_after_seconds: u64,
}

#[derive(Debug, Clone)]
struct LocalSlidingWindow {
    timestamps: Vec<u64>,
}

#[derive(Clone)]
pub struct RateLimitService {
    config: RateLimitConfig,
    redis: Arc<Mutex<Option<ConnectionManager>>>,
    local_windows: Arc<DashMap<String, LocalSlidingWindow>>,
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

        let now = now_ms();
        let member = Uuid::new_v4().to_string();

        let script = redis::Script::new(REDIS_SLIDING_WINDOW_LUA);
        let result: redis::RedisResult<(u8, u32, u64)> = script
            .key(key)
            .arg(now)
            .arg(window_ms)
            .arg(max_requests)
            .arg(&member)
            .invoke_async(connection)
            .await;

        let (allowed, count, oldest_ts) = match result {
            Ok(values) => values,
            Err(error) => {
                tracing::warn!(%error, "Redis sliding window rate limit script failed; falling back locally");
                *guard = None;
                return None;
            }
        };

        let reset_after_seconds = if count > 0 {
            let oldest_plus_window = oldest_ts.saturating_add(window_ms);
            let time_remaining_ms = oldest_plus_window.saturating_sub(now);
            (time_remaining_ms + 999) / 1000
        } else {
            (window_ms + 999) / 1000
        };

        Some(RateLimitDecision {
            allowed: allowed == 1,
            limit: max_requests,
            remaining: max_requests.saturating_sub(count),
            reset_after_seconds,
        })
    }

    fn check_local(&self, key: &str, max_requests: u32, window_ms: u64) -> RateLimitDecision {
        let now = now_ms();
        let mut entry = self
            .local_windows
            .entry(key.to_string())
            .or_insert(LocalSlidingWindow {
                timestamps: Vec::new(),
            });

        // Retain timestamps within the window
        let window_start = now.saturating_sub(window_ms);
        entry.timestamps.retain(|&ts| ts > window_start);

        let allowed = entry.timestamps.len() < max_requests as usize;
        if allowed {
            entry.timestamps.push(now);
        }

        let count = entry.timestamps.len() as u32;
        let oldest_ts = entry.timestamps.first().cloned().unwrap_or(now);

        let reset_after_seconds = if count > 0 {
            let oldest_plus_window = oldest_ts.saturating_add(window_ms);
            let time_remaining_ms = oldest_plus_window.saturating_sub(now);
            (time_remaining_ms + 999) / 1000
        } else {
            (window_ms + 999) / 1000
        };

        RateLimitDecision {
            allowed,
            limit: max_requests,
            remaining: max_requests.saturating_sub(count),
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
