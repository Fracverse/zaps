use sqlx::postgres::PgPoolOptions;
use zaps_backend::config::Config;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Loading configuration...");
    let config = Config::load()?;

    println!("Running migrations...");
    zaps_backend::db::run_migrations(&config.database.url).await?;

    println!("Connecting to database...");
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&config.database.url)
        .await?;

    println!("Seeding database...");

    // Seed Users
    let user_id = "user_123";
    let stellar_address = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

    // We use raw SQL query here since we might not have access to specific macros if sqlx-data.json is not set up
    // or if we want generic execution. usage of sqlx::query! requires compile-time DB connection or offline mode.
    // For simplicity in a script without expecting a running DB during compilation, we can use sqlx::query().

    sqlx::query(
        r#"
        INSERT INTO users (user_id, stellar_address)
        VALUES ($1, $2)
        ON CONFLICT (user_id) DO NOTHING
        "#,
    )
    .bind(user_id)
    .bind(stellar_address)
    .execute(&pool)
    .await?;

    // Seed Merchants
    let merchant_id = "merchant_abc";
    sqlx::query(
        r#"
        INSERT INTO merchants (merchant_id, vault_address, settlement_asset, active)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (merchant_id) DO NOTHING
        "#,
    )
    .bind(merchant_id)
    .bind("GBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB")
    .bind("USDC")
    .bind(true)
    .execute(&pool)
    .await?;

    // Seed Balances
    sqlx::query(
        r#"
        INSERT INTO balances (owner_id, asset, amount)
        VALUES ($1, $2, $3)
        ON CONFLICT (owner_id, asset) DO NOTHING
        "#,
    )
    .bind(user_id)
    .bind("USDC")
    .bind(1000_000_000_i64) // 1000 USDC
    .execute(&pool)
    .await?;

    println!("Database seeded successfully!");

    Ok(())
}
