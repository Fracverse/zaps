use axum::{
    extract::{ConnectInfo, Request, State},
    middleware::Next,
    response::Response,
};
use std::net::SocketAddr;
use std::sync::Arc;
use crate::models::RateLimitScope;
use crate::service::ServiceContainer;
use crate::api_error::ApiError;

pub async fn rate_limit(
    State(services): State<Arc<ServiceContainer>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let config = &services.config.rate_limit;
    
    let key = match config.scope {
        RateLimitScope::Ip => addr.ip().to_string(),
        RateLimitScope::User => {
            // Try to get user_id from extensions (set by auth middleware)
            request.extensions().get::<String>().cloned().unwrap_or_else(|| addr.ip().to_string())
        }
        RateLimitScope::ApiKey => {
            request.headers()
                .get("X-API-KEY")
                .and_then(|h| h.to_str().ok())
                .map(|s| s.to_string())
                .unwrap_or_else(|| addr.ip().to_string())
        }
    };

    if !services.rate_limit.check_rate_limit(key) {
        return Err(ApiError::RateLimit("Too many requests".to_string()));
    }
    
    Ok(next.run(request).await)
}
