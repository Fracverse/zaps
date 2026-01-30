use sqlx::PgPool;
use blinks_backend::config::Config;
use blinks_backend::db;

// Note: This test requires a running database using the config.
// We will write a simple test that tries to connect and check migrations.

#[tokio::test]
#[ignore] // Ignore by default to avoid breaking CI if no DB
async fn test_migrations_and_connection() {
    // Attempt to load config - if fails, skip
    let config = match Config::load() {
        Ok(c) => c,
        Err(_) => {
            println!("Skipping test: No config found");
            return;
        }
    };

    // Run migrations
    let result = db::run_migrations(&config.database.url).await;
    assert!(result.is_ok(), "Migrations failed: {:?}", result.err());

    // Connect to verify tables exist
    let pool = PgPool::connect(&config.database.url)
        .await
        .expect("Failed to connect");

    let row: (i64,) = sqlx::query_as("SELECT count(*) FROM users")
        .fetch_one(&pool)
        .await
        .expect("Failed to query users");

    println!("Users count: {}", row.0);
}
