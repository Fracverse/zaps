#![allow(dead_code, unused_variables, unused_imports)]

use axum::{
    extract::State,
    http::{Request, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use chrono::Utc;
use serde::Serialize;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;
use tower_http::{classify::ServerErrorsFailureClass, trace::TraceLayer};
use tracing::Span;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

// Bring modules in from the library crate (defined in src/lib.rs)
use zaps_backend::api;
use zaps_backend::config;
use zaps_backend::db;
use zaps_backend::indexer;
use zaps_backend::services;

// ── Health check types ────────────────────────────────────────────────────────

#[derive(Clone)]
struct HealthState {
    pool: sqlx::PgPool,
    stellar_rpc_url: String,
}

#[derive(Serialize)]
struct DbHealth {
    status: &'static str,
    latency_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Serialize)]
struct YieldDbHealth {
    status: &'static str,
    latency_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    active_yield_accounts: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    yield_rate_bps: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Serialize)]
struct RpcHealth {
    status: &'static str,
    latency_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    latest_ledger: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Serialize)]
struct HealthComponents {
    database: DbHealth,
    yield_db: YieldDbHealth,
    soroban_rpc: RpcHealth,
}

#[derive(Serialize)]
struct HealthResponse {
    /// "ok" when all components are healthy; "degraded" otherwise.
    status: &'static str,
    components: HealthComponents,
    checked_at: String,
}

// ── Rate limiter state: token bucket per client (IP address) ──────────────────

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

    // Health check state: pool + Soroban RPC URL for live component probing.
    let health_state = HealthState {
        pool: pool.clone(),
        stellar_rpc_url: config.stellar_rpc_url.clone(),
    };

    // Setup routes
    let public_routes = Router::new()
        .route("/health", get(health_check))
        .with_state(health_state);

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

    // BE-029: Auto-sweep idle stablecoins for users with auto-earn enabled.
    let sweep_pool = pool.clone();
    let sweep_config = services::sweep_worker::SweepWorkerConfig::from_env();
    tokio::spawn(async move {
        services::sweep_worker::run(sweep_pool, sweep_config).await;
    });

    // BE-032: Daily / weekly yield report push notifications.
    let notification_pool = pool.clone();
    let notification_config = services::notifications::NotificationSchedulerConfig::from_env();
    tokio::spawn(async move {
        services::notifications::run(notification_pool, notification_config).await;
    });

    let addr = SocketAddr::from(([0, 0, 0, 0], 8080));
    tracing::info!("Listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

// ── /health handler ───────────────────────────────────────────────────────────

async fn health_check(State(state): State<HealthState>) -> impl IntoResponse {
    // Run all three probes concurrently so latencies don't stack.
    let (db, yield_db, rpc) = tokio::join!(
        probe_database(&state.pool),
        probe_yield_db(&state.pool),
        probe_soroban_rpc(&state.stellar_rpc_url),
    );

    let all_ok = db.status == "ok" && yield_db.status == "ok" && rpc.status == "ok";

    let body = HealthResponse {
        status: if all_ok { "ok" } else { "degraded" },
        components: HealthComponents {
            database: db,
            yield_db,
            soroban_rpc: rpc,
        },
        checked_at: Utc::now().to_rfc3339(),
    };

    let code = if all_ok {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (code, Json(body))
}

// ── Component probes ──────────────────────────────────────────────────────────

/// Basic Postgres connectivity: a single round-trip to the DB pool.
async fn probe_database(pool: &sqlx::PgPool) -> DbHealth {
    let start = Instant::now();
    match sqlx::query_scalar::<_, i64>("SELECT 1")
        .fetch_one(pool)
        .await
    {
        Ok(_) => DbHealth {
            status: "ok",
            latency_ms: start.elapsed().as_millis() as u64,
            error: None,
        },
        Err(e) => DbHealth {
            status: "error",
            latency_ms: start.elapsed().as_millis() as u64,
            error: Some(e.to_string()),
        },
    }
}

/// Yield-specific DB probe: verifies `user_yield_balances` and
/// `yield_rates_history` are reachable and returns live metrics.
async fn probe_yield_db(pool: &sqlx::PgPool) -> YieldDbHealth {
    let start = Instant::now();

    let result: Result<(i64, Option<i32>), sqlx::Error> = async {
        let active_yield_accounts: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM user_yield_balances")
                .fetch_one(pool)
                .await?;

        let yield_rate_bps: Option<i32> = sqlx::query_scalar(
            "SELECT apy FROM yield_rates_history ORDER BY created_at DESC LIMIT 1",
        )
        .fetch_optional(pool)
        .await?;

        Ok((active_yield_accounts, yield_rate_bps))
    }
    .await;

    let latency_ms = start.elapsed().as_millis() as u64;

    match result {
        Ok((count, rate)) => YieldDbHealth {
            status: "ok",
            latency_ms,
            active_yield_accounts: Some(count),
            yield_rate_bps: rate,
            error: None,
        },
        Err(e) => YieldDbHealth {
            status: "error",
            latency_ms,
            active_yield_accounts: None,
            yield_rate_bps: None,
            error: Some(e.to_string()),
        },
    }
}

/// Soroban RPC probe: issues a real `getLatestLedger` JSON-RPC call.
/// Fails if the node is unreachable, returns a non-2xx status, or the
/// response shape is unexpected.
async fn probe_soroban_rpc(rpc_url: &str) -> RpcHealth {
    let start = Instant::now();

    let result: Result<u32, String> = async {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .map_err(|e| e.to_string())?;

        let payload = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getLatestLedger",
            "params": {}
        });

        let res = client
            .post(rpc_url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        if !res.status().is_success() {
            return Err(format!("RPC returned HTTP {}", res.status()));
        }

        let body: serde_json::Value = res.json().await.map_err(|e| e.to_string())?;

        body["result"]["sequence"]
            .as_u64()
            .map(|n| n as u32)
            .ok_or_else(|| "unexpected RPC response shape".to_string())
    }
    .await;

    let latency_ms = start.elapsed().as_millis() as u64;

    match result {
        Ok(ledger) => RpcHealth {
            status: "ok",
            latency_ms,
            latest_ledger: Some(ledger),
            error: None,
        },
        Err(e) => RpcHealth {
            status: "error",
            latency_ms,
            latest_ledger: None,
            error: Some(e),
        },
    }
}
