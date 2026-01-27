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
    http::{
        admin, auth, health, identity, metrics as metrics_http, notifications, payments, transfers,
        withdrawals,
    },
    middleware::{auth as auth_middleware, metrics, rate_limit, request_id, role_guard},
    role::Role,
    service::{MetricsService, ServiceContainer},
};

pub async fn create_app(
    db_pool: Pool,
    config: Config,
) -> Result<Router, Box<dyn std::error::Error>> {
    // Initialize metrics service
    MetricsService::init();

    // Create service container
    let services = Arc::new(ServiceContainer::new(db_pool, config.clone()).await?);

    // Health check routes
    let health_routes = Router::new()
        .route("/health", get(health::health_check))
        .route("/ready", get(health::readiness_check))
        .route("/live", get(health::liveness_check));

    // Metrics routes (Prometheus-compatible)
    let metrics_routes = Router::new()
        .route("/metrics", get(metrics_http::prometheus_metrics))
        .route("/metrics/json", get(metrics_http::json_metrics))
        .route("/metrics/alerts", get(metrics_http::check_alerts));

    // Auth routes
    let auth_routes = Router::new()
        .route("/login", post(auth::login))
        .route("/register", post(auth::register))
        .route("/refresh", post(auth::refresh_token));

    // Identity & Wallet routes
    let identity_routes = Router::new()
        .route("/users", post(identity::create_user))
        .route("/users/me", get(identity::get_user))
        .route("/users/me/wallet", get(identity::get_wallet))
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

    // Notification routes
    let notification_routes = Router::new()
        .route("/notifications", post(notifications::create_notification))
        .route("/notifications", get(notifications::get_notifications))
        .route(
            "/notifications/:id/read",
            axum::routing::patch(notifications::mark_notification_read),
        );

    // Admin routes (protected)
    let admin_routes = Router::new()
        .route("/dashboard/stats", get(admin::get_dashboard_stats))
        .route("/transactions", get(admin::get_transactions))
        .route("/users/:user_id/activity", get(admin::get_user_activity))
        .route("/system/health", get(admin::get_system_health))
        .layer(middleware::from_fn(role_guard::require_role(Role::Admin)));

    // Audit log routes (admin-only)
    let audit_routes = Router::new()
        .route("/audit-logs", get(audit::list_audit_logs))
        .route("/audit-logs/:id", get(audit::get_audit_log))
        .layer(middleware::from_fn(auth_middleware::admin_only));

    // Protected routes (require authentication)
    let protected_routes = Router::new()
        .nest("/identity", identity_routes)
        .nest("/payments", payment_routes)
        .nest("/transfers", transfer_routes)
        .nest("/withdrawals", withdrawal_routes)
        .nest("/notifications", notification_routes)
        .nest("/admin", admin_routes)
        .merge(audit_routes) // Audit routes at root level under /audit-logs
        .layer(middleware::from_fn_with_state(
            services.clone(),
            audit_logging,
        )) // Audit middleware for automatic logging
        .layer(middleware::from_fn(auth_middleware::authenticate));
        .layer(middleware::from_fn_with_state(
            services.clone(),
            auth_middleware::authenticate,
        ));

    // Public routes
    let public_routes = Router::new()
        .nest("/auth", auth_routes)
        .nest("/health", health_routes)
        .merge(metrics_routes);

    // Combine all routes
    let app = Router::new()
        .merge(public_routes)
        .merge(protected_routes)
        .layer(middleware::from_fn_with_state(
            services.clone(),
            rate_limit::rate_limit,
        ))
        .layer(middleware::from_fn(request_id::request_id))
        .layer(middleware::from_fn(metrics::track_metrics))
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(services);

    Ok(app)
}
