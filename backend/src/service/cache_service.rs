use crate::{api_error::ApiError, config::Config, service::MetricsService};
use redis::{aio::ConnectionManager, AsyncCommands};
use serde::{de::DeserializeOwned, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct CacheService {
    connection: Arc<Mutex<Option<ConnectionManager>>>,
    default_ttl_seconds: u64,
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
        }
    }

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
            MetricsService::record_cache_event("get", "error");
            ApiError::InternalServerError
        })?;

        match value {
            Some(value) => {
                MetricsService::record_cache_event("get", "hit");
                serde_json::from_str(&value)
                    .map(Some)
                    .map_err(ApiError::Json)
            }
            None => {
                MetricsService::record_cache_event("get", "miss");
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
}
