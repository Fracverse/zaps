use deadpool_postgres::{Manager, ManagerConfig, Pool, RecyclingMethod, Runtime};
use std::str::FromStr;
use tokio_postgres::NoTls;
use tokio::time::{sleep, Duration};
use crate::service::MetricsService;
use std::sync::Arc;
use std::cmp;

pub type DbPool = Pool;

pub async fn create_pool(database_url: &str) -> Result<DbPool, Box<dyn std::error::Error>> {
    create_pool_with_max_size(database_url, 16).await
}

pub async fn create_pool_with_max_size(
    database_url: &str,
    max_size: usize,
) -> Result<DbPool, Box<dyn std::error::Error>> {
    let pg_config = tokio_postgres::Config::from_str(database_url)?;
    let mgr_config = ManagerConfig {
        recycling_method: RecyclingMethod::Fast,
    };
    let mgr = Manager::from_config(pg_config, NoTls, mgr_config);
    let pool = Pool::builder(mgr)
        .max_size(max_size)
        .runtime(Runtime::Tokio1)
        .build()?;
    Ok(pool)
}

pub async fn run_migrations(database_url: &str) -> Result<(), Box<dyn std::error::Error>> {
    let pool = sqlx::PgPool::connect(database_url)
        .await
        .map_err(|e| format!("Failed to connect to database for migrations: {}", e))?;

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .map_err(|e| format!("Failed to run database migrations: {}", e))?;

    pool.close().await;
    Ok(())
}

/// Start a background task that monitors database connections and updates metrics.
///
/// - `database_url`: Postgres connection string used for monitoring queries.
/// - `configured_max_size`: the configured pool max size from configuration.
/// - `check_interval_secs`: how often to poll Postgres for connection counts.
///
/// Returns a JoinHandle for the spawned task. The task will run until cancelled.
pub fn start_db_pool_monitoring(
    database_url: String,
    configured_max_size: usize,
    check_interval_secs: u64,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            match tokio_postgres::Config::from_str(&database_url)
                .and_then(|cfg| cfg.connect(NoTls))
            {
                Ok((client, connection)) => {
                    // detach connection handling
                    tokio::spawn(async move {
                        if let Err(e) = connection.await {
                            tracing::warn!(error = %e, "Postgres monitor connection error");
                        }
                    });

                    // Query active connections for this database
                    match client
                        .query_one(
                            "SELECT count(*) FROM pg_stat_activity WHERE datname = current_database()",
                            &[],
                        )
                        .await
                    {
                        Ok(row) => {
                            let active: i64 = row.get(0);
                            let active_usize = cmp::min(active as usize, 1_000_000);
                            MetricsService::update_db_pool_status(configured_max_size, active_usize);
                        }
                        Err(e) => tracing::warn!(error = %e, "Failed to query pg_stat_activity"),
                    }

                    let _ = client.close().await;
                }
                Err(e) => tracing::error!(error = %e, "Failed to connect to Postgres for monitoring"),
            }

            sleep(Duration::from_secs(check_interval_secs)).await;
        }
    })
}

/// Health check for database connectivity.
pub async fn health_check_db(database_url: &str) -> bool {
    match tokio_postgres::Config::from_str(database_url)
        .and_then(|cfg| cfg.connect(NoTls))
        .await
    {
        Ok((mut client, connection)) => {
            // drive connection
            tokio::spawn(async move {
                let _ = connection.await;
            });

            let res = client.query_one("SELECT 1", &[]).await.is_ok();
            let _ = client.close().await;
            res
        }
        Err(_) => false,
    }
}

/// Recommend a new pool size based on current utilization and configuration.
/// This function does not mutate or rebuild the pool; it only suggests a size.
pub fn recommend_pool_size(
    current_max: usize,
    active_connections: usize,
    min_pool_size: usize,
    resize_step: usize,
    high_threshold: f64,
    low_threshold: f64,
) -> usize {
    if current_max == 0 {
        return current_max;
    }

    let utilization = (active_connections as f64 / current_max as f64) * 100.0;

    if utilization >= high_threshold {
        // scale up
        current_max.saturating_add(resize_step)
    } else if utilization <= low_threshold && current_max > min_pool_size {
        // scale down but not below min
        current_max.saturating_sub(resize_step).max(min_pool_size)
    } else {
        current_max
    }
}

/// Reset migrations for testing purposes
/// This drops all tables, types, and the migration history to allow re-running migrations
/// WARNING: Only use this in test environments! This will destroy all data in the database.
pub async fn reset_migrations(database_url: &str) -> Result<(), Box<dyn std::error::Error>> {
    let pool = sqlx::PgPool::connect(database_url)
        .await
        .map_err(|e| format!("Failed to connect to database for migration reset: {}", e))?;

    // Use a transaction to ensure atomic cleanup
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| format!("Failed to begin transaction: {}", e))?;

    // Drop all tables in public schema (CASCADE will also drop dependent objects)
    // This ensures a clean state for re-running migrations
    sqlx::query(
        r#"
        DO $$ 
        DECLARE 
            r RECORD;
        BEGIN
            -- Drop all tables (including _sqlx_migrations)
            FOR r IN (SELECT tablename FROM pg_tables WHERE schemaname = 'public') 
            LOOP
                EXECUTE 'DROP TABLE IF EXISTS public.' || quote_ident(r.tablename) || ' CASCADE';
            END LOOP;
            
            -- Drop all custom types
            FOR r IN (SELECT typname FROM pg_type WHERE typnamespace = (SELECT oid FROM pg_namespace WHERE nspname = 'public') AND typtype = 'e')
            LOOP
                EXECUTE 'DROP TYPE IF EXISTS public.' || quote_ident(r.typname) || ' CASCADE';
            END LOOP;
        END $$;
        "#,
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| format!("Failed to reset database: {}", e))?;

    // Commit the transaction to ensure all drops are applied
    tx.commit()
        .await
        .map_err(|e| format!("Failed to commit reset transaction: {}", e))?;

    // Verify _sqlx_migrations is gone (it should be after the above)
    let table_exists: bool = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (
            SELECT FROM information_schema.tables 
            WHERE table_schema = 'public' 
            AND table_name = '_sqlx_migrations'
        )",
    )
    .fetch_one(&pool)
    .await
    .unwrap_or(false);

    if table_exists {
        // Force drop if it still exists
        sqlx::query("DROP TABLE _sqlx_migrations CASCADE")
            .execute(&pool)
            .await
            .ok();
    }

    pool.close().await;
    Ok(())
}
