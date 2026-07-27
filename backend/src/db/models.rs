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

// ─── Bulk disbursement (BE-554 / BE-555) ─────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct PayoutBatch {
    pub id: Uuid,
    pub idempotency_key: String,
    pub created_by: Uuid,
    /// PENDING, PROCESSING, COMPLETED, PARTIALLY_FAILED, FAILED, CANCELLED
    pub status: String,
    pub currency: String,
    pub total_recipients: i32,
    pub total_amount: i64,
    pub succeeded_count: i32,
    pub failed_count: i32,
    pub started_at: Option<NaiveDateTime>,
    pub completed_at: Option<NaiveDateTime>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct BatchRecipient {
    pub id: Uuid,
    pub batch_id: Uuid,
    /// `None` when the payout targets a raw Stellar address rather than a
    /// registered Zaps user.
    pub user_id: Option<Uuid>,
    pub destination_address: Option<String>,
    pub amount: i64,
    /// PENDING, SUBMITTED, CONFIRMED, FAILED
    pub status: String,
    pub sdp_payment_id: Option<String>,
    pub tx_hash: Option<String>,
    pub attempt_count: i32,
    pub last_error: Option<String>,
    pub locked_at: Option<NaiveDateTime>,
    pub locked_by: Option<String>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct DispatchLog {
    pub id: Uuid,
    pub batch_id: Uuid,
    pub recipient_id: Option<Uuid>,
    pub attempt: i32,
    /// CLAIMED, SUBMITTED, CONFIRMED, FAILED, RETRY_SCHEDULED, CANCELLED
    pub event: String,
    pub sdp_response_code: Option<String>,
    pub detail: Option<String>,
    pub created_at: NaiveDateTime,
}
