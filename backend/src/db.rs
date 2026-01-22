use deadpool_postgres::{Config, Pool, Runtime};
use tokio_postgres::NoTls;

pub type DbPool = Pool;

pub async fn create_pool(database_url: &str) -> Result<DbPool, Box<dyn std::error::Error>> {
    let mut cfg = Config::new();
    // Parse the URL to set config fields
    if let Ok(config) = database_url.parse::<tokio_postgres::Config>() {
            cfg.user = config.get_user().map(|s| s.to_string());
            cfg.password = config.get_password().map(|s| String::from_utf8_lossy(s).to_string());
            cfg.dbname = config.get_dbname().map(|s| s.to_string());
        cfg.host = config.get_hosts().first().map(|h| match h {
            tokio_postgres::config::Host::Tcp(s) => s.to_string(),
            tokio_postgres::config::Host::Unix(s) => s.to_string_lossy().to_string(),
        });
        cfg.port = config.get_ports().first().copied();
    }
    
    let pool = cfg.create_pool(Some(Runtime::Tokio1), NoTls)?;
    Ok(pool)
}

pub async fn run_migrations(database_url: &str) -> Result<(), Box<dyn std::error::Error>> {
    let pool = sqlx::PgPool::connect(database_url).await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    pool.close().await;
    Ok(())
}
