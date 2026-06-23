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
