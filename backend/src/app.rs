use axum::{
    middleware,
    routing::{delete, get, patch, post},
    Router,
};
use deadpool_postgres::Pool;
use std::sync::Arc;
use tower_http::{cors::CorsLayer, trace::TraceLayer};

use crate::{
    config::Config,
    http::{
        admin, audit, auth, disputes, files, health, identity, jobs, metrics as metrics_http,
        notifications, payments, profiles, transfers, version as version_http, withdrawals,
    },
    job_worker::JobWorker,
    middleware::{
        audit_logging, auth as auth_middleware, metrics, rate_limit, request_id, role_guard,
        version_middleware,
    },
    role::Role,
    service::{MetricsService, ServiceContainer},
};

pub async fn create_app(
    db_pool: Pool,
    config: Config,
) -> Result<Router, Box<dyn std::error::Error>> {
    MetricsService::init();

    let services = Arc::new(ServiceContainer::new(db_pool, config.clone()).await?);

    // Start background job workers
    let job_worker = Arc::new(JobWorker::new(config.clone()).await?);
    let worker_clone = Arc::clone(&job_worker);
    tokio::spawn(async move {
        if let Err(e) = worker_clone.start_workers().await {
            tracing::error!("Job workers failed: {}", e);
        }
    });

    // =========================================================================
    // Route definitions
    // =========================================================================

    // -------------------- Health --------------------
    let health_routes = Router::new()
        .route("/health", get(health::health_check))
        .route("/ready", get(health::readiness_check))
        .route("/live", get(health::liveness_check));

    // -------------------- Metrics --------------------
    let metrics_routes = Router::new()
        .route("/metrics", get(metrics_http::prometheus_metrics))
        .route("/metrics/json", get(metrics_http::json_metrics))
        .route("/metrics/alerts", get(metrics_http::check_alerts));

    // -------------------- Auth --------------------
    let auth_routes = Router::new()
        .route("/login", post(auth::login))
        .route("/register", post(auth::register))
        .route("/refresh", post(auth::refresh_token));

    // -------------------- User --------------------
    let user_routes = Router::new().route("/register", post(auth::user_register));

    // -------------------- Identity --------------------
    let identity_routes = Router::new()
        .route("/users", post(identity::create_user))
        .route("/users/me", get(identity::get_user))
        .route("/users/me/wallet", get(identity::get_wallet))
        .route("/resolve/:user_id", get(identity::resolve_user_id));

    // -------------------- Payments --------------------
    let payment_routes = Router::new()
        .route("/payments", post(payments::create_payment))
        .route("/payments/:id", get(payments::get_payment))
        .route("/payments/:id/status", get(payments::get_payment_status))
        .route("/qr/generate", post(payments::generate_qr))
        .route("/nfc/validate", post(payments::validate_nfc));

    // -------------------- Transfers --------------------
    let transfer_routes = Router::new()
        .route("/transfers", post(transfers::create_transfer))
        .route("/transfers/:id", get(transfers::get_transfer))
        .route("/transfers/:id/status", get(transfers::get_transfer_status));

    // -------------------- Withdrawals --------------------
    let withdrawal_routes = Router::new()
        .route("/withdrawals", post(withdrawals::create_withdrawal))
        .route("/withdrawals/:id", get(withdrawals::get_withdrawal))
        .route(
            "/withdrawals/:id/status",
            get(withdrawals::get_withdrawal_status),
        );

    // -------------------- Notifications --------------------
    let notification_routes = Router::new()
        .route("/notifications", post(notifications::create_notification))
        .route("/notifications", get(notifications::get_notifications))
        .route(
            "/notifications/:id/read",
            patch(notifications::mark_notification_read),
        );

    // -------------------- Profiles --------------------
    let profile_routes = Router::new()
        .route("/", post(profiles::create_profile))
        .route("/me", get(profiles::get_my_profile))
        .route("/:user_id", get(profiles::get_profile))
        .route("/:user_id", patch(profiles::update_profile))
        .route("/:user_id", delete(profiles::delete_profile));

    // -------------------- Files --------------------
    let files_routes = Router::new()
        .route("/upload", post(files::upload_file))
        .route("/:id", get(files::get_file))
        .route("/:id/meta", get(files::get_file_metadata))
        .route("/:id", delete(files::delete_file));

    // -------------------- Admin --------------------
    let admin_routes = Router::new()
        .route("/dashboard/stats", get(admin::get_dashboard_stats))
        .route("/transactions", get(admin::get_transactions))
        .route("/users/:user_id/activity", get(admin::get_user_activity))
        .route("/system/health", get(admin::get_system_health))
        .layer(middleware::from_fn(role_guard::require_role(Role::Admin)));

    // -------------------- Audit --------------------
    let audit_routes = Router::new()
        .route("/audit-logs", get(audit::list_audit_logs))
        .route("/audit-logs/:id", get(audit::get_audit_log))
        .layer(middleware::from_fn(role_guard::admin_only()));

    // -------------------- Disputes (v2 only) --------------------
    // Payment-scoped dispute routes
    let payment_dispute_routes = Router::new()
        .route(
            "/payments/:payment_id/disputes",
            post(disputes::file_dispute),
        )
        .route(
            "/payments/:payment_id/disputes",
            get(disputes::list_payment_disputes),
        );

    // Standalone dispute routes
    let dispute_routes = Router::new()
        .route("/disputes", get(disputes::list_all_disputes))
        .route("/disputes/me", get(disputes::list_my_disputes))
        .route("/disputes/:dispute_id", get(disputes::get_dispute))
        .route(
            "/disputes/:dispute_id/status",
            patch(disputes::update_dispute_status),
        )
        .route(
            "/disputes/:dispute_id/evidence",
            post(disputes::add_evidence),
        )
        .route(
            "/disputes/:dispute_id/evidence",
            get(disputes::list_evidence),
        );

    // -------------------- Jobs --------------------
    let _job_routes = jobs::create_job_routes();

    // =========================================================================
    // Protected route bundles (auth + rate-limit + audit middleware applied)
    // =========================================================================

    // Shared protected routes (available on both /api/v1 and /api/v2)
    let shared_protected = Router::new()
        .nest("/identity", identity_routes)
        .nest("/payments", payment_routes)
        .nest("/transfers", transfer_routes)
        .nest("/withdrawals", withdrawal_routes)
        .nest("/notifications", notification_routes)
        .nest("/profiles", profile_routes)
        .nest("/files", files_routes)
        .nest("/admin", admin_routes)
        .nest("/audit", audit_routes);

    // v2-only protected routes (disputes)
    let v2_only_protected = Router::new()
        .merge(payment_dispute_routes)
        .merge(dispute_routes);

    // Apply auth/rate-limit/audit middleware to shared routes
    let shared_protected_with_middleware = shared_protected
        .layer(middleware::from_fn_with_state(
            services.clone(),
            audit_logging,
        ))
        .layer(middleware::from_fn_with_state(
            services.clone(),
            auth_middleware::authenticate,
        ))
        .layer(middleware::from_fn_with_state(
            services.clone(),
            rate_limit::rate_limit,
        ));

    // Apply auth/rate-limit/audit middleware to v2-only routes
    let v2_only_protected_with_middleware = v2_only_protected
        .layer(middleware::from_fn_with_state(
            services.clone(),
            audit_logging,
        ))
        .layer(middleware::from_fn_with_state(
            services.clone(),
            auth_middleware::authenticate,
        ))
        .layer(middleware::from_fn_with_state(
            services.clone(),
            rate_limit::rate_limit,
        ));

    // =========================================================================
    // Versioned API routes
    // =========================================================================

    // /api/v1 — all shared routes, no dispute endpoints
    let api_v1 = Router::new()
        .merge(shared_protected_with_middleware.clone())
        .layer(middleware::from_fn(version_middleware));

    // /api/v2 — shared routes + dispute endpoints
    let api_v2 = Router::new()
        .merge(shared_protected_with_middleware)
        .merge(v2_only_protected_with_middleware)
        .layer(middleware::from_fn(version_middleware));

    // =========================================================================
    // Version documentation routes (public, no auth)
    // =========================================================================
    let version_doc_routes = Router::new()
        .route("/versions", get(version_http::list_versions))
        .route("/versions/:version", get(version_http::get_version))
        .route(
            "/versions/:version/migration",
            get(version_http::get_migration_guide),
        );

    // =========================================================================
    // Anchor & public routes
    // =========================================================================
    let anchor_routes =
        Router::new().route("/webhook", post(crate::http::anchor::anchor_webhook));

    let public_routes = Router::new()
        .nest("/anchor", anchor_routes)
        .nest("/auth", auth_routes)
        .nest("/user", user_routes)
        .nest("/health", health_routes)
        .merge(metrics_routes);

    // =========================================================================
    // Assemble the full router
    // =========================================================================
    let app = Router::new()
        // Versioned API namespaces
        .nest("/api/v1", api_v1)
        .nest("/api/v2", api_v2)
        // Version documentation (public)
        .nest("/api", version_doc_routes)
        // Legacy unversioned public routes (backward compat)
        .merge(public_routes)
        .with_state(services)
        .layer(middleware::from_fn(request_id::request_id))
        .layer(middleware::from_fn(metrics::track_metrics))
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive());

    Ok(app)
}
