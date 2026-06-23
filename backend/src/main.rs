#![allow(dead_code, unused_variables, unused_imports)]

use axum::{
    routing::{get, post},
    Router,
};
use std::net::SocketAddr;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod api;
mod config;
mod db;
mod indexer;
mod services;

#[tokio::main]
async fn main() {
    // Initialize logging
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "zaps_backend=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
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

    // Setup routes
    let app = Router::new()
        .route("/health", get(health_check))
        .nest("/api/auth", api::auth_routes(pool.clone()))
        .nest("/api/users", api::user_routes(pool.clone()))
        .nest("/api/feed", api::feed_routes(pool.clone()))
        .nest("/api/social", api::social_routes())
        .nest("/api/bridge", api::bridge_routes());

    // Spawn indexer in the background
    tokio::spawn(async {
        if let Err(e) = indexer::worker::run().await {
            tracing::error!("Stellar Indexer background worker failed: {:?}", e);
        }
    });

    let addr = SocketAddr::from(([0, 0, 0, 0], 8080));
    tracing::info!("Listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn health_check() -> &'static str {
    "OK"
}
