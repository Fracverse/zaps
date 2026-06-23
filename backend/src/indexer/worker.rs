use serde::Deserialize;
use sqlx::{PgPool, Row};
use std::time::Duration;
use uuid::Uuid;

#[derive(Deserialize, Debug)]
pub struct SocialPaymentEvent {
    pub sender: String,
    pub receiver: String,
    pub amount: i64,
    pub memo: String,
    pub visibility: String,
    pub tx_hash: String,
}

pub async fn run() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing::info!("Starting Stellar event indexer background worker...");
    // We'll get the pool from config when implemented properly
    // For now, this is a placeholder with the parser logic
    loop {
        // TODO: Implement BE-013 (Poll/Subscribe to Soroban RPC payment events)
        // TODO: Implement BE-015 (Stellar cursor tracker to avoid double indexing)
        tokio::time::sleep(Duration::from_secs(10)).await;
        tracing::debug!("Stellar Indexer heartbeats... polling Soroban RPC for new events.");
    }
}

// Parse and process a SocialPaymentEvent into the database
pub async fn process_social_payment_event(
    event: SocialPaymentEvent,
    pool: &PgPool,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Look up sender and receiver UUIDs from their addresses
    let sender_id = get_or_create_user_id(&event.sender, pool).await?;
    let receiver_id = get_or_create_user_id(&event.receiver, pool).await?;

    // Insert into payments table
    sqlx::query(
        r#"
        INSERT INTO payments (tx_hash, sender_id, receiver_id, amount, currency, memo, visibility)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        ON CONFLICT (tx_hash) DO NOTHING
        "#,
    )
    .bind(&event.tx_hash)
    .bind(sender_id)
    .bind(receiver_id)
    .bind(event.amount)
    .bind("NGN") // Default to NGN for now
    .bind(&event.memo)
    .bind(event.visibility.to_uppercase())
    .execute(pool)
    .await?;

    Ok(())
}

// Helper function to get user ID by address, creating if not exists
async fn get_or_create_user_id(
    address: &str,
    pool: &PgPool,
) -> Result<Uuid, Box<dyn std::error::Error + Send + Sync>> {
    let username = format!("u_{}", &address[1..15].to_lowercase());
    let row = sqlx::query(
        r#"
        INSERT INTO users (address, username, display_name)
        VALUES ($1, $2, $3)
        ON CONFLICT (address)
        DO UPDATE SET username = COALESCE(users.username, EXCLUDED.username)
        RETURNING id
        "#,
    )
    .bind(address)
    .bind(&username)
    .bind(Some(&username))
    .fetch_one(pool)
    .await?;

    Ok(row.get("id"))
}
