use crate::{api_error::ApiError, config::Config, service::MetricsService};
use redis::{aio::ConnectionManager, AsyncCommands};
use serde::{de::DeserializeOwned, Serialize};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::Mutex;

/// Cache hit/miss counters for hit-rate monitoring.
/// These are process-local counters that complement the Prometheus metrics.
static CACHE_HITS: AtomicU64 = AtomicU64::new(0);
static CACHE_MISSES: AtomicU64 = AtomicU64::new(0);
static CACHE_ERRORS: AtomicU64 = AtomicU64::new(0);

/// Cache hit rate statistics snapshot.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub errors: u64,
    pub hit_rate_percent: f64,
    pub total_requests: u64,
}

impl CacheStats {
    fn snapshot() -> Self {
        let hits = CACHE_HITS.load(Ordering::Relaxed);
        let misses = CACHE_MISSES.load(Ordering::Relaxed);
        let errors = CACHE_ERRORS.load(Ordering::Relaxed);
        let total = hits + misses;
        let hit_rate_percent = if total == 0 {
            0.0
        } else {
            (hits as f64 / total as f64) * 100.0
        };
        CacheStats {
            hits,
            misses,
            errors,
            hit_rate_percent,
            total_requests: total,
        }
    }
}

#[derive(Clone)]
pub struct CacheService {
    connection: Arc<Mutex<Option<ConnectionManager>>>,
    default_ttl_seconds: u64,
    hot_data_ttl_seconds: u64,
}

impl CacheService {
    pub async fn new(config: Config) -> Self {
        let connection = match redis::Client::open(config.cache.redis_url.clone()) {
            Ok(client) => match client.get_connection_manager().await {
                Ok(manager) => {
                    tracing::info!("Redis cache connected");
                    Some(manager)
                }
                Err(error) => {
                    tracing::warn!(%error, "Redis cache unavailable");
                    None
                }
            },
            Err(error) => {
                tracing::warn!(%error, "Invalid Redis cache URL");
                None
            }
        };

        Self {
            connection: Arc::new(Mutex::new(connection)),
            default_ttl_seconds: config.cache.default_ttl_seconds,
            hot_data_ttl_seconds: config.cache.hot_data_ttl_seconds,
        }
    }

    // -------------------------------------------------------------------------
    // Core get/set/invalidate (backward-compatible with existing callers)
    // -------------------------------------------------------------------------

    pub async fn get_json<T>(&self, key: &str) -> Result<Option<T>, ApiError>
    where
        T: DeserializeOwned,
    {
        let mut guard = self.connection.lock().await;
        let Some(connection) = guard.as_mut() else {
            MetricsService::record_cache_event("get", "unavailable");
            return Ok(None);
        };

        let value: Option<String> = connection.get(key).await.map_err(|_error| {
            CACHE_ERRORS.fetch_add(1, Ordering::Relaxed);
            MetricsService::record_cache_event("get", "error");
            ApiError::InternalServerError
        })?;

        match value {
            Some(value) => {
                CACHE_HITS.fetch_add(1, Ordering::Relaxed);
                MetricsService::record_cache_event("get", "hit");
                MetricsService::record_cache_hit_rate(Self::hit_rate_percent());
                serde_json::from_str(&value)
                    .map(Some)
                    .map_err(ApiError::Json)
            }
            None => {
                CACHE_MISSES.fetch_add(1, Ordering::Relaxed);
                MetricsService::record_cache_event("get", "miss");
                MetricsService::record_cache_hit_rate(Self::hit_rate_percent());
                Ok(None)
            }
        }
    }

    pub async fn set_json<T>(
        &self,
        key: &str,
        value: &T,
        ttl_seconds: Option<u64>,
    ) -> Result<(), ApiError>
    where
        T: Serialize,
    {
        let mut guard = self.connection.lock().await;
        let Some(connection) = guard.as_mut() else {
            MetricsService::record_cache_event("set", "unavailable");
            return Ok(());
        };

        let payload = serde_json::to_string(value)?;
        let ttl = ttl_seconds.unwrap_or(self.default_ttl_seconds);
        connection
            .set_ex::<_, _, ()>(key, payload, ttl)
            .await
            .map_err(|_error| {
                MetricsService::record_cache_event("set", "error");
                ApiError::InternalServerError
            })?;
        MetricsService::record_cache_event("set", "ok");
        Ok(())
    }

    /// Set a value with the "hot data" TTL (shorter, for frequently accessed data).
    pub async fn set_hot<T>(&self, key: &str, value: &T) -> Result<(), ApiError>
    where
        T: Serialize,
    {
        self.set_json(key, value, Some(self.hot_data_ttl_seconds))
            .await
    }

    pub async fn invalidate(&self, key: &str) -> Result<(), ApiError> {
        let mut guard = self.connection.lock().await;
        let Some(connection) = guard.as_mut() else {
            MetricsService::record_cache_event("invalidate", "unavailable");
            return Ok(());
        };

        connection.del::<_, ()>(key).await.map_err(|_error| {
            MetricsService::record_cache_event("invalidate", "error");
            ApiError::InternalServerError
        })?;
        MetricsService::record_cache_event("invalidate", "ok");
        Ok(())
    }

    // -------------------------------------------------------------------------
    // Pattern-based invalidation
    // -------------------------------------------------------------------------

    /// Invalidate all keys matching a Redis glob pattern (e.g. "payment:*").
    ///
    /// Uses SCAN + DEL to avoid blocking the server with KEYS on large datasets.
    pub async fn invalidate_pattern(&self, pattern: &str) -> Result<u64, ApiError> {
        let mut guard = self.connection.lock().await;
        let Some(connection) = guard.as_mut() else {
            MetricsService::record_cache_event("invalidate_pattern", "unavailable");
            return Ok(0);
        };

        let mut cursor: u64 = 0;
        let mut deleted: u64 = 0;

        loop {
            // SCAN cursor MATCH pattern COUNT 100
            let (next_cursor, keys): (u64, Vec<String>) = redis::cmd("SCAN")
                .arg(cursor)
                .arg("MATCH")
                .arg(pattern)
                .arg("COUNT")
                .arg(100u64)
                .query_async(connection)
                .await
                .map_err(|_| {
                    MetricsService::record_cache_event("invalidate_pattern", "error");
                    ApiError::InternalServerError
                })?;

            if !keys.is_empty() {
                let count = keys.len() as u64;
                connection
                    .del::<_, ()>(keys)
                    .await
                    .map_err(|_| {
                        MetricsService::record_cache_event("invalidate_pattern", "error");
                        ApiError::InternalServerError
                    })?;
                deleted += count;
            }

            cursor = next_cursor;
            if cursor == 0 {
                break;
            }
        }

        tracing::debug!(
            pattern = %pattern,
            deleted_keys = deleted,
            "Pattern-based cache invalidation complete"
        );
        MetricsService::record_cache_event("invalidate_pattern", "ok");
        Ok(deleted)
    }

    // -------------------------------------------------------------------------
    // Tag-based invalidation
    //
    // Tags are stored as Redis Sets: "tag:{tag_name}" → {key1, key2, ...}
    // When a tagged key is set, its key is added to each tag's set.
    // Invalidating a tag deletes all keys in the set, then the set itself.
    // -------------------------------------------------------------------------

    /// Set a JSON value and associate it with one or more cache tags.
    pub async fn set_json_tagged<T>(
        &self,
        key: &str,
        value: &T,
        ttl_seconds: Option<u64>,
        tags: &[&str],
    ) -> Result<(), ApiError>
    where
        T: Serialize,
    {
        // Store the value first
        self.set_json(key, value, ttl_seconds).await?;

        if tags.is_empty() {
            return Ok(());
        }

        let mut guard = self.connection.lock().await;
        let Some(connection) = guard.as_mut() else {
            return Ok(());
        };

        // Register the key under each tag set
        for tag in tags {
            let tag_key = format!("tag:{}", tag);
            connection
                .sadd::<_, _, ()>(&tag_key, key)
                .await
                .map_err(|_| ApiError::InternalServerError)?;

            // Give the tag set the same TTL as the value (plus a small buffer)
            let ttl = ttl_seconds.unwrap_or(self.default_ttl_seconds) + 60;
            connection
                .expire::<_, ()>(&tag_key, ttl as i64)
                .await
                .map_err(|_| ApiError::InternalServerError)?;
        }

        Ok(())
    }

    /// Invalidate all cache keys associated with a tag.
    pub async fn invalidate_tag(&self, tag: &str) -> Result<u64, ApiError> {
        let mut guard = self.connection.lock().await;
        let Some(connection) = guard.as_mut() else {
            MetricsService::record_cache_event("invalidate_tag", "unavailable");
            return Ok(0);
        };

        let tag_key = format!("tag:{}", tag);

        // Fetch all keys registered under this tag
        let keys: Vec<String> = connection
            .smembers(&tag_key)
            .await
            .map_err(|_| {
                MetricsService::record_cache_event("invalidate_tag", "error");
                ApiError::InternalServerError
            })?;

        let count = keys.len() as u64;

        if !keys.is_empty() {
            connection
                .del::<_, ()>(keys)
                .await
                .map_err(|_| {
                    MetricsService::record_cache_event("invalidate_tag", "error");
                    ApiError::InternalServerError
                })?;
        }

        // Remove the tag set itself
        connection
            .del::<_, ()>(&tag_key)
            .await
            .map_err(|_| ApiError::InternalServerError)?;

        tracing::debug!(
            tag = %tag,
            invalidated_keys = count,
            "Tag-based cache invalidation complete"
        );
        MetricsService::record_cache_event("invalidate_tag", "ok");
        Ok(count)
    }

    // -------------------------------------------------------------------------
    // Hit rate monitoring
    // -------------------------------------------------------------------------

    /// Returns a snapshot of cache hit/miss statistics for this process.
    pub fn stats() -> CacheStats {
        CacheStats::snapshot()
    }

    /// Returns the current hit rate as a percentage (0.0–100.0).
    pub fn hit_rate_percent() -> f64 {
        CacheStats::snapshot().hit_rate_percent
    }

    // -------------------------------------------------------------------------
    // Convenience helpers for common domain objects
    // -------------------------------------------------------------------------

    /// Cache key for a payment by UUID string.
    pub fn payment_key(payment_id: &str) -> String {
        format!("payment:{}", payment_id)
    }

    /// Cache key for a user profile.
    pub fn profile_key(user_id: &str) -> String {
        format!("profile:{}", user_id)
    }

    /// Cache key for a dispute.
    pub fn dispute_key(dispute_id: &str) -> String {
        format!("dispute:{}", dispute_id)
    }

    /// Invalidate all cached data for a specific payment (payment + its disputes).
    pub async fn invalidate_payment(&self, payment_id: &str) -> Result<(), ApiError> {
        self.invalidate(&Self::payment_key(payment_id)).await?;
        // Also invalidate any dispute keys associated with this payment
        self.invalidate_pattern(&format!("dispute:payment:{}:*", payment_id))
            .await?;
        Ok(())
    }
}
