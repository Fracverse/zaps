#![allow(dead_code, unused_variables, unused_imports)]

use axum::{
    extract::State,
    http::{Request, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::Mutex;
use tower_http::{classify::ServerErrorsFailureClass, trace::TraceLayer};
use tracing::Span;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod api;
mod config;
mod db;
mod indexer;
mod services;

// Rate limiter state: token bucket per client (IP address)
#[derive(Clone)]
struct RateLimiter {
    buckets: Arc<Mutex<HashMap<String, (i64, std::time::Instant)>>>,
    tokens_per_second: i64,
    max_tokens: i64,
}

impl RateLimiter {
    fn new(tokens_per_second: i64, max_tokens: i64) -> Self {
        Self {
            buckets: Arc::new(Mutex::new(HashMap::new())),
            tokens_per_second,
            max_tokens,
        }
    }

    async fn check_rate(&self, key: String) -> bool {
        let mut buckets = self.buckets.lock().await;
        let now = std::time::Instant::now();

        let (tokens, last_refill) = buckets.entry(key).or_insert((self.max_tokens, now));

        // Refill tokens based on time passed
        let elapsed = now.duration_since(*last_refill).as_secs() as i64;
        if elapsed > 0 {
            *tokens = std::cmp::min(*tokens + elapsed * self.tokens_per_second, self.max_tokens);
            *last_refill = now;
        }

        if *tokens > 0 {
            *tokens -= 1;
            true
        } else {
            false
        }
    }
}

async fn rate_limiter_middleware(
    State(rate_limiter): State<RateLimiter>,
    request: Request<axum::body::Body>,
    next: Next,
) -> impl IntoResponse {
    // Get client IP address
    let ip = request
        .headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .or_else(|| {
            request
                .extensions()
                .get::<axum::extract::ConnectInfo<SocketAddr>>()
                .map(|info| info.to_string())
        })
        .unwrap_or_else(|| "unknown".to_string());

    if rate_limiter.check_rate(ip.clone()).await {
        Ok(next.run(request).await)
    } else {
        Err((
            StatusCode::TOO_MANY_REQUESTS,
            "Too many requests, please try again later.",
        ))
    }
}

#[tokio::main]
async fn main() {
    // Initialize logging
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "zaps-backend=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer().json())
        .init();

    tracing::info!("Initializing Zaps Social Backend...");

    let config = config::Config::from_env();
    let pool = db::get_pool(&config.database_url)
        .await
        .expect("Failed to connect to database");

    // Run schema migrations/initialization
    db::run_migrations(&pool)
        .await
        .expect("Failed to run database migrations");

    // Initialize rate limiter: 5 requests per second, max 10 tokens
    let rate_limiter = RateLimiter::new(5, 10);

    // Bridge state: shares the DB pool and the Allbridge API client.
    let bridge_state =
        api::bridge::BridgeState::new(pool.clone(), config.allbridge_api_url.clone());

    // Setup routes
    let public_routes = Router::new().route("/health", get(health_check));

    let sensitive_routes = Router::new()
        .nest("/api/auth", api::auth_routes(pool.clone()))
        .nest("/api/users", api::user_routes(pool.clone()));

    let other_routes = Router::new()
        .nest("/api/feed", api::feed_routes(pool.clone()))
        .nest("/api/social", api::social_routes(pool.clone()))
        .nest("/api/bridge", api::bridge_routes(bridge_state.clone()))
        .nest("/api/yield", api::yield_routes(pool.clone()));

    let app = Router::new()
        .merge(public_routes)
        .merge(sensitive_routes.layer(middleware::from_fn_with_state(
            rate_limiter.clone(),
            rate_limiter_middleware,
        )))
        .merge(other_routes)
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(|request: &Request<_>| {
                    tracing::info_span!(
                        "http_request",
                        method = %request.method(),
                        path = %request.uri().path(),
                        status_code = tracing::field::Empty,
                        duration_ms = tracing::field::Empty,
                    )
                })
                .on_response(
                    |response: &Response, duration: std::time::Duration, span: &Span| {
                        let status_code = response.status().as_u16();
                        let duration_ms = duration.as_millis() as u64;
                        span.record("status_code", status_code);
                        span.record("duration_ms", duration_ms);
                        tracing::info!(
                            parent: span,
                            status_code,
                            duration_ms,
                            "request completed"
                        );
                    },
                )
                .on_failure(
                    |error: ServerErrorsFailureClass,
                     duration: std::time::Duration,
                     span: &Span| {
                        let duration_ms = duration.as_millis() as u64;
                        span.record("duration_ms", duration_ms);
                        tracing::error!(
                            parent: span,
                            error = %error,
                            duration_ms,
                            "request failed"
                        );
                    },
                ),
        );

    // Spawn indexer in the background
    let indexer_pool = pool.clone();
    let indexer_rpc_url = config.stellar_rpc_url.clone();
    tokio::spawn(async move {
        if let Err(e) = indexer::worker::run(indexer_pool, indexer_rpc_url).await {
            tracing::error!("Stellar Indexer background worker failed: {:?}", e);
        }
    });

    // Spawn the bridge status poller to periodically refresh pending cross-chain deposits.
    tokio::spawn(async move {
        api::bridge::run_status_poller(bridge_state).await;
    });

    let addr = SocketAddr::from(([0, 0, 0, 0], 8080));
    tracing::info!("Listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn health_check() -> &'static str {
    "OK"
}
