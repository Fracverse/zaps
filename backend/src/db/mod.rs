pub mod models;

pub async fn get_pool(database_url: &str) -> Result<sqlx::PgPool, sqlx::Error> {
    sqlx::PgPool::connect(database_url).await
}

pub async fn run_migrations(pool: &sqlx::PgPool) -> Result<(), sqlx::migrate::MigrateError> {
    sqlx::migrate!().run(pool).await
}

