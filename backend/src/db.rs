use deadpool_postgres::{Manager, ManagerConfig, Pool, RecyclingMethod, Runtime};
use std::str::FromStr;
use tokio_postgres::NoTls;

pub type DbPool = Pool;

pub async fn create_pool(database_url: &str) -> Result<DbPool, Box<dyn std::error::Error>> {
    let pg_config = tokio_postgres::Config::from_str(database_url)?;
    let mgr_config = ManagerConfig {
        recycling_method: RecyclingMethod::Fast,
    };
    let mgr = Manager::from_config(pg_config, NoTls, mgr_config);
    let pool = Pool::builder(mgr)
        .max_size(16)
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

/// Reset migrations for testing purposes
/// This drops all tables, types, and the migration history to allow re-running migrations
/// WARNING: Only use this in test environments! This will destroy all data in the database.
pub async fn reset_migrations(database_url: &str) -> Result<(), Box<dyn std::error::Error>> {
    let pool = sqlx::PgPool::connect(database_url)
        .await
        .map_err(|e| format!("Failed to connect to database for migration reset: {}", e))?;
    
    // Use a transaction to ensure atomic cleanup
    let mut tx = pool.begin().await
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
    tx.commit().await
        .map_err(|e| format!("Failed to commit reset transaction: {}", e))?;
    
    // Verify _sqlx_migrations is gone (it should be after the above)
    let table_exists: bool = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (
            SELECT FROM information_schema.tables 
            WHERE table_schema = 'public' 
            AND table_name = '_sqlx_migrations'
        )"
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
