//! #1223 — Rate limiting for circle-related APIs.
//!
//! A small configurable fixed-window limiter keyed by the authenticated user
//! (falling back to the request IP for unauthenticated diagnostics). Financial
//! operations use a stricter, separately configurable bucket. Every rejection
//! is logged as a structured monitoring event and returned as a clear `429`
//! response with a `Retry-After` header.

use axum::{
    extract::{ConnectInfo, Request, State},
    http::{header::RETRY_AFTER, HeaderMap, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use std::{
    collections::HashMap,
    env,
    net::SocketAddr,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

use super::auth_middleware::AuthenticatedUser;

/// Limits and window, all configurable through environment variables.
#[derive(Debug, Clone)]
pub struct CircleRateLimitConfig {
    pub window: Duration,
    pub general: u32,
    pub search: u32,
    pub social: u32,
    pub financial: u32,
}

impl Default for CircleRateLimitConfig {
    fn default() -> Self {
        Self {
            window: Duration::from_secs(60),
            general: 120,
            search: 30,
            social: 20,
            financial: 5,
        }
    }
}

fn env_u32(name: &str, default: u32) -> u32 {
    env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
        .max(1)
}

impl CircleRateLimitConfig {
    pub fn from_env() -> Self {
        let defaults = Self::default();
        Self {
            window: Duration::from_secs(env_u32("CIRCLE_RATE_LIMIT_WINDOW_SECS", 60) as u64),
            general: env_u32("CIRCLE_RATE_LIMIT_GENERAL", defaults.general),
            search: env_u32("CIRCLE_RATE_LIMIT_SEARCH", defaults.search),
            social: env_u32("CIRCLE_RATE_LIMIT_SOCIAL", defaults.social),
            financial: env_u32("CIRCLE_RATE_LIMIT_FINANCIAL", defaults.financial),
        }
    }

    fn for_path(&self, path: &str) -> (&'static str, u32) {
        if path.contains("/contribute")
            || path.contains("/payout")
            || path.contains("/approve")
            || path.contains("/transaction")
        {
            ("financial", self.financial)
        } else if path.contains("/search") || path.contains("/discover") {
            ("search", self.search)
        } else if path.contains("/invitation")
            || path.contains("/comment")
            || path.contains("/reaction")
            || path.contains("/membership")
            || path.contains("/join")
            || path.contains("/request")
        {
            ("social", self.social)
        } else {
            ("general", self.general)
        }
    }
}

#[derive(Clone)]
pub struct CircleRateLimiter {
    config: Arc<CircleRateLimitConfig>,
    buckets: Arc<Mutex<HashMap<String, (Instant, u32)>>>,
}

impl CircleRateLimiter {
    pub fn new(config: CircleRateLimitConfig) -> Self {
        Self {
            config: Arc::new(config),
            buckets: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn from_env() -> Self {
        Self::new(CircleRateLimitConfig::from_env())
    }
}

fn client_key(headers: &HeaderMap, peer: Option<ConnectInfo<SocketAddr>>) -> String {
    if let Some(user) = headers.get("x-circle-user").and_then(|v| v.to_str().ok()) {
        if !user.is_empty() {
            return format!("user:{user}");
        }
    }
    if let Some(forwarded) = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
    {
        if !forwarded.trim().is_empty() {
            return format!("ip:{}", forwarded.trim());
        }
    }
    format!("ip:{}", peer.map(|p| p.0.to_string()).unwrap_or_default())
}

/// Middleware: reject excessive requests with a clear, actionable response.
pub async fn circle_rate_limit(
    State(limiter): State<CircleRateLimiter>,
    req: Request,
    next: Next,
) -> Response {
    // The auth middleware runs outside this router and attaches the verified
    // principal; use it as the identity whenever available.
    let user = req.extensions().get::<AuthenticatedUser>().map(|u| u.address.clone());
    let path = req.uri().path().to_string();
    let (operation, limit) = limiter.config.for_path(&path);
    let key = match user {
        Some(address) if !address.is_empty() => format!("user:{address}"),
        _ => client_key(req.headers(), req.extensions().get::<ConnectInfo<SocketAddr>>().cloned()),
    };
    let bucket = format!("{operation}:{key}");

    let (allowed, retry_after) = {
        let mut buckets = limiter.buckets.lock().await;
        let now = Instant::now();
        let entry = buckets.entry(bucket.clone()).or_insert((now, 0));
        if now.duration_since(entry.0) >= limiter.config.window {
            *entry = (now, 0);
        }
        if entry.1 < limit {
            entry.1 += 1;
            (true, 0u64)
        } else {
            let elapsed = now.duration_since(entry.0);
            let remaining = limiter.config.window.saturating_sub(elapsed).as_secs().max(1);
            (false, remaining)
        }
    };

    if allowed {
        next.run(req).await
    } else {
        tracing::warn!(
            event = "circle_rate_limit_exceeded",
            operation,
            key = %bucket,
            limit,
            retry_after,
            path = %path,
            "circle API request rejected"
        );
        (
            StatusCode::TOO_MANY_REQUESTS,
            [(RETRY_AFTER, retry_after.to_string())],
            Json(serde_json::json!({
                "error": "rate_limited",
                "message": "Too many circle requests. Please retry later.",
                "operation": operation,
                "limit": limit,
                "retry_after": retry_after,
            })),
        )
            .into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn financial_limits_are_stricter_and_configurable() {
        let config = CircleRateLimitConfig::from_env();
        assert!(config.financial <= config.general);
        assert_eq!(config.for_path("/api/circles/1/contribute").0, "financial");
        assert_eq!(config.for_path("/api/circles/search").0, "search");
        assert_eq!(config.for_path("/api/circles/1/comments").0, "social");
        assert_eq!(config.for_path("/api/circles/1").0, "general");
    }
}
