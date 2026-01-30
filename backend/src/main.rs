use std::net::SocketAddr;
use tracing::info;
use blinks_backend::{app::create_app, config::Config, db, telemetry};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize telemetry
    telemetry::init_tracing()?;

    // Load configuration
    let config = Config::load()?;

    // Initialize database
    let db_pool = db::create_pool(&config.database.url).await?;

    // Run database migrations
    db::run_migrations(&config.database.url).await?;

    // Create application
    let app = create_app(db_pool, config.clone()).await?;

    // Start server
    let addr = SocketAddr::from(([0, 0, 0, 0], config.server.port));
    info!("Starting BLINKS backend server on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;

    Ok(())
}
