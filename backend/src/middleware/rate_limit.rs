use crate::api_error::ApiError;
use crate::middleware::auth::AuthenticatedUser;
use crate::models::RateLimitScope;
use crate::service::{rate_limit_service::scope_name, MetricsService, ServiceContainer};
use axum::{
    extract::{ConnectInfo, Request, State},
    http::header::HeaderName,
    http::HeaderValue,
    middleware::Next,
    response::Response,
};
use std::net::SocketAddr;
use std::sync::Arc;

pub async fn rate_limit(
    State(services): State<Arc<ServiceContainer>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let config = &services.config.rate_limit;
    let path = request.uri().path().to_string();

    let key = match config.scope {
        RateLimitScope::Ip => addr.ip().to_string(),
        RateLimitScope::User => request
            .extensions()
            .get::<AuthenticatedUser>()
            .map(|user| user.user_id.clone())
            .unwrap_or_else(|| addr.ip().to_string()),
        RateLimitScope::ApiKey => request
            .headers()
            .get("X-API-KEY")
            .and_then(|h| h.to_str().ok())
            .map(|s| s.to_string())
            .unwrap_or_else(|| addr.ip().to_string()),
    };

    let decision = services
        .rate_limit
        .check_rate_limit(&key, &path, &config.scope)
        .await;

    MetricsService::record_rate_limit_event(scope_name(&config.scope), decision.allowed);

    if !decision.allowed {
        tracing::warn!(
            rate_limit.scope = scope_name(&config.scope),
            rate_limit.path = %path,
            rate_limit.limit = decision.limit,
            rate_limit.reset_after_seconds = decision.reset_after_seconds,
            "Rate limit blocked request"
        );
        return Err(ApiError::RateLimit("Too many requests".to_string()));
    }

    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(
        HeaderName::from_static("x-ratelimit-limit"),
        HeaderValue::from_str(&decision.limit.to_string())
            .unwrap_or_else(|_| HeaderValue::from_static("0")),
    );
    headers.insert(
        HeaderName::from_static("x-ratelimit-remaining"),
        HeaderValue::from_str(&decision.remaining.to_string())
            .unwrap_or_else(|_| HeaderValue::from_static("0")),
    );
    headers.insert(
        HeaderName::from_static("x-ratelimit-reset"),
        HeaderValue::from_str(&decision.reset_after_seconds.to_string())
            .unwrap_or_else(|_| HeaderValue::from_static("0")),
    );

    Ok(response)
}
