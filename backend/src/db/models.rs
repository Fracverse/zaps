use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize)]
pub struct User {
    pub id: Uuid,
    pub address: String,
    pub username: String,
    pub display_name: Option<String>,
    pub bio: Option<String>,
    pub avatar_url: Option<String>,
    pub auto_earn_enabled: bool,
    pub created_at: NaiveDateTime,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Payment {
    pub id: Uuid,
    pub tx_hash: String,
    pub sender_id: Uuid,
    pub receiver_id: Uuid,
    pub amount: i64,      // represented in lowest currency unit
    pub currency: String, // e.g. "NGN" or "USDC"
    pub memo: String,
    pub visibility: String, // "PUBLIC", "FRIENDS", "PRIVATE"
    pub created_at: NaiveDateTime,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Like {
    pub id: Uuid,
    pub payment_id: Uuid,
    pub user_id: Uuid,
    pub created_at: NaiveDateTime,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Comment {
    pub id: Uuid,
    pub payment_id: Uuid,
    pub user_id: Uuid,
    pub content: String,
    pub created_at: NaiveDateTime,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Friendship {
    pub id: Uuid,
    pub user_id: Uuid,
    pub friend_id: Uuid,
    pub status: String, // "PENDING", "ACCEPTED"
    pub created_at: NaiveDateTime,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BridgeTransaction {
    pub id: Uuid,
    pub source_tx_hash: String,
    pub source_chain: String,
    pub destination_chain: Option<String>,
    pub destination_address: Option<String>,
    pub amount: Option<String>,
    pub status: String, // "PENDING", "SUCCESS", "FAILED"
    pub confirmations: i32,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UserYieldBalance {
    pub user_id: Uuid,
    pub available_balance: i64,
    pub earning_balance: i64,
    pub last_yield_sync_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct YieldTransaction {
    pub id: Uuid,
    pub user_id: Uuid,
    pub tx_hash: String,
    pub r#type: String, // "DEPOSIT", "WITHDRAW", "EARNED"
    pub amount: i64,
    pub created_at: NaiveDateTime,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct YieldRateHistory {
    pub id: Uuid,
    pub apy: i32,
    pub created_at: NaiveDateTime,
}
