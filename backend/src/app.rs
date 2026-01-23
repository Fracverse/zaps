use axum::{
    middleware,
    routing::{get, post},
    Router,
};
use deadpool_postgres::Pool;
use std::sync::Arc;
use tower_http::{cors::CorsLayer, trace::TraceLayer};

use crate::{
    config::Config,
    http::{admin, auth, health, identity, jobs, payments, transfers, withdrawals},
    job_worker::JobWorker,
    middleware::{auth as auth_middleware, metrics, request_id},
    service::ServiceContainer,
};

pub async fn create_app(
    db_pool: Pool,
    config: Config,
) -> Result<Router, Box<dyn std::error::Error>> {
    // Create service container
    let services = Arc::new(ServiceContainer::new(db_pool, config.clone()).await?);

    // Create job worker
    let job_worker = Arc::new(JobWorker::new(config.clone()).await?);
    
    // Spawn job workers in the background
    let worker_clone = Arc::clone(&job_worker);
    tokio::spawn(async move {
        if let Err(e) = worker_clone.start_workers().await {
            tracing::error!("Job workers failed: {}", e);
        }
    });

    // Health check routes
    let health_routes = Router::new()
        .route("/health", get(health::health_check))
        .route("/ready", get(health::readiness_check));

    // Auth routes
    let auth_routes = Router::new()
        .route("/login", post(auth::login))
        .route("/register", post(auth::register))
        .route("/refresh", post(auth::refresh_token));

    // Identity & Wallet routes
    let identity_routes = Router::new()
        .route("/users", post(identity::create_user))
        .route("/users/:user_id", get(identity::get_user))
        .route("/users/:user_id/wallet", get(identity::get_wallet))
        .route("/resolve/:user_id", get(identity::resolve_user_id));

    // Payment routes
    let payment_routes = Router::new()
        .route("/payments", post(payments::create_payment))
        .route("/payments/:id", get(payments::get_payment))
        .route("/payments/:id/status", get(payments::get_payment_status))
        .route("/qr/generate", post(payments::generate_qr))
        .route("/nfc/validate", post(payments::validate_nfc));

    // Transfer routes
    let transfer_routes = Router::new()
        .route("/transfers", post(transfers::create_transfer))
        .route("/transfers/:id", get(transfers::get_transfer))
        .route("/transfers/:id/status", get(transfers::get_transfer_status));

    // Withdrawal routes
    let withdrawal_routes = Router::new()
        .route("/withdrawals", post(withdrawals::create_withdrawal))
        .route("/withdrawals/:id", get(withdrawals::get_withdrawal))
        .route(
            "/withdrawals/:id/status",
            get(withdrawals::get_withdrawal_status),
        );

    // Admin routes (protected)
    let admin_routes = Router::new()
        .route("/dashboard/stats", get(admin::get_dashboard_stats))
        .route("/transactions", get(admin::get_transactions))
        .route("/users/:user_id/activity", get(admin::get_user_activity))
        .route("/system/health", get(admin::get_system_health))
        .layer(middleware::from_fn(auth_middleware::admin_only));

    // Job management routes (protected)
    let job_routes = jobs::create_job_routes()
        .layer(middleware::from_fn(auth_middleware::authenticate));

    // Protected routes (require authentication)
    let protected_routes = Router::new()
        .nest("/identity", identity_routes)
        .nest("/payments", payment_routes)
        .nest("/transfers", transfer_routes)
        .nest("/withdrawals", withdrawal_routes)
        .nest("/admin", admin_routes)
        .nest("/jobs", job_routes)
        .layer(middleware::from_fn(auth_middleware::authenticate));

    // Public routes
    let public_routes = Router::new()
        .nest("/auth", auth_routes)
        .nest("/health", health_routes);

    // Combine all routes
    let app = Router::new()
        .merge(public_routes)
        .merge(protected_routes)
        .with_state(services)
        .layer(middleware::from_fn(request_id::request_id))
        .layer(middleware::from_fn(metrics::track_metrics))
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(job_worker);

    Ok(app)
}
