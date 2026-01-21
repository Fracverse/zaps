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
    let pool = sqlx::PgPool::connect(database_url).await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    pool.close().await;
    Ok(())
}
