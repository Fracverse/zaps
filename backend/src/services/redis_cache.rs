use redis::{
    aio::{ConnectionManager, ConnectionManagerConfig},
    RedisError,
};
use std::time::Duration;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(2);

/// #544: 30-minute TTL for cached username -> address mappings, per the
/// issue's acceptance criteria.
const USERNAME_ADDRESS_TTL_SECS: u64 = 30 * 60;

fn cache_key(username: &str) -> String {
    format!("zaps:user:address:{username}")
}

/// Redis-backed cache for username -> Stellar address resolution, so
/// transfer/payout flows don't hit Postgres on every lookup.
///
/// Mirrors `api::yield::YieldCache`'s shape and failure handling: connects
/// lazily so a Redis outage at boot doesn't prevent the API from starting,
/// and every operation degrades to "treat as a cache miss" on error rather
/// than surfacing a Redis failure to the caller — Postgres is always the
/// source of truth.
#[derive(Clone)]
pub struct UsernameAddressCache {
    pool: ConnectionManager,
}

impl UsernameAddressCache {
    pub fn connect(redis_url: &str) -> Result<Self, RedisError> {
        let client = redis::Client::open(redis_url)?;
        let config = ConnectionManagerConfig::new()
            .set_connection_timeout(Some(CONNECT_TIMEOUT))
            .set_response_timeout(Some(RESPONSE_TIMEOUT));

        Ok(Self {
            pool: ConnectionManager::new_lazy_with_config(client, config)?,
        })
    }

    /// Retrieve a cached address for `username`. A miss — or any Redis
    /// error — is reported as `None` so the caller falls back to Postgres.
    pub async fn get_address(&self, username: &str) -> Option<String> {
        match redis::cmd("GET")
            .arg(cache_key(username))
            .query_async::<Option<String>>(&mut self.pool.clone())
            .await
        {
            Ok(cached) => cached,
            Err(e) => {
                tracing::warn!("username->address cache read failed: {e}");
                None
            }
        }
    }

    /// Cache `address` for `username` with a 30-minute expiration.
    pub async fn set_address(&self, username: &str, address: &str) {
        if let Err(e) = redis::cmd("SET")
            .arg(cache_key(username))
            .arg(address)
            .arg("EX")
            .arg(USERNAME_ADDRESS_TTL_SECS)
            .query_async::<()>(&mut self.pool.clone())
            .await
        {
            tracing::warn!("username->address cache write failed: {e}");
        }
    }

    /// Evict a cached mapping.
    ///
    /// Intended to be called by the indexer when a `UserRegistered` event is
    /// indexed for this username, per this issue's guidance ("Invalidate
    /// cache entries if user registry changes are index-logged"). Wire this
    /// into `indexer::worker::process_user_registered_event` once that
    /// lands, so a fresh registration is never masked by a stale cache
    /// entry for the remainder of its 30-minute TTL.
    pub async fn invalidate(&self, username: &str) {
        if let Err(e) = redis::cmd("DEL")
            .arg(cache_key(username))
            .query_async::<()>(&mut self.pool.clone())
            .await
        {
            tracing::warn!("username->address cache invalidation failed: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_key_is_namespaced_per_username() {
        assert_eq!(cache_key("ebube"), "zaps:user:address:ebube");
        assert_ne!(cache_key("ebube"), cache_key("chidi"));
    }

    #[test]
    fn connect_rejects_a_non_redis_url() {
        assert!(UsernameAddressCache::connect("postgres://localhost/zaps").is_err());
    }
}
