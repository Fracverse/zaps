use crate::api_error::ApiError;
use crate::middleware::auth::AuthenticatedUser;
use crate::models::RateLimitScope;
use crate::role::Role;
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

    // Check if the user is an admin and bypass is enabled
    let mut is_admin = false;
    if config.bypass_admin {
        if let Some(user) = request.extensions().get::<AuthenticatedUser>() {
            is_admin = user.role == Role::Admin;
        } else if let Some(auth_header) = request.headers().get("authorization") {
            if let Ok(auth_str) = auth_header.to_str() {
                if let Some(token) = auth_str.strip_prefix("Bearer ") {
                    if let Ok(claims) = crate::auth::validate_access_token(token, &services.config.jwt.secret) {
                        is_admin = claims.role == Role::Admin;
                    }
                }
            }
        }
    }

    if is_admin {
        tracing::debug!("Admin user bypassing rate limiting");
        return Ok(next.run(request).await);
    }

    let key = match config.scope {
        RateLimitScope::Ip => addr.ip().to_string(),
        RateLimitScope::User => {
            if let Some(user) = request.extensions().get::<AuthenticatedUser>() {
                user.user_id.clone()
            } else if let Some(auth_header) = request.headers().get("authorization")
                .and_then(|h| h.to_str().ok())
                .and_then(|s| s.strip_prefix("Bearer "))
            {
                crate::auth::validate_access_token(auth_header, &services.config.jwt.secret)
                    .map(|claims| claims.sub)
                    .unwrap_or_else(|_| addr.ip().to_string())
            } else {
                addr.ip().to_string()
            }
        }
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

    MetricsService::record_rate_limit_event(scope_name(&config.scope), decision.allowed, &path);

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
