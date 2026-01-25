use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::role::Role;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub user_id: String,
    pub stellar_address: String,
    pub role: Role,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Wallet {
    pub user_id: String,
    pub address: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Merchant {
    pub id: String,
    pub merchant_id: String,
    pub vault_address: String,
    pub settlement_asset: String,
    pub active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PaymentStatus {
    Pending,
    Processing,
    Completed,
    Failed,
    Refunded,
}

impl PaymentStatus {
    pub fn from_str(s: &str) -> Self {
        match s {
            "completed" => PaymentStatus::Completed,
            "processing" => PaymentStatus::Processing,
            "failed" => PaymentStatus::Failed,
            "refunded" => PaymentStatus::Refunded,
            _ => PaymentStatus::Pending,
        }
    }

    pub fn to_string(&self) -> String {
        match self {
            PaymentStatus::Pending => "pending",
            PaymentStatus::Processing => "processing",
            PaymentStatus::Completed => "completed",
            PaymentStatus::Failed => "failed",
            PaymentStatus::Refunded => "refunded",
        }
        .to_string()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Payment {
    pub id: String,
    pub tx_hash: Option<String>,
    pub from_address: String,
    pub merchant_id: String,
    pub send_asset: String,
    pub send_amount: i64,
    pub receive_amount: Option<i64>,
    pub status: PaymentStatus,
    pub memo: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransferStatus {
    Pending,
    Processing,
    Completed,
    Failed,
}

impl TransferStatus {
    pub fn from_str(s: &str) -> Self {
        match s {
            "processing" => TransferStatus::Processing,
            "completed" => TransferStatus::Completed,
            "failed" => TransferStatus::Failed,
            _ => TransferStatus::Pending,
        }
    }

    pub fn to_string(&self) -> String {
        match self {
            TransferStatus::Pending => "pending",
            TransferStatus::Processing => "processing",
            TransferStatus::Completed => "completed",
            TransferStatus::Failed => "failed",
        }
        .to_string()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transfer {
    pub id: String,
    pub tx_hash: Option<String>,
    pub from_user_id: String,
    pub to_user_id: String,
    pub amount: i64,
    pub asset: String,
    pub status: TransferStatus,
    pub memo: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WithdrawalStatus {
    Pending,
    Processing,
    Completed,
    Failed,
}

impl WithdrawalStatus {
    pub fn from_str(s: &str) -> Self {
        match s {
            "processing" => WithdrawalStatus::Processing,
            "completed" => WithdrawalStatus::Completed,
            "failed" => WithdrawalStatus::Failed,
            _ => WithdrawalStatus::Pending,
        }
    }

    pub fn to_string(&self) -> String {
        match self {
            WithdrawalStatus::Pending => "pending",
            WithdrawalStatus::Processing => "processing",
            WithdrawalStatus::Completed => "completed",
            WithdrawalStatus::Failed => "failed",
        }
        .to_string()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Withdrawal {
    pub id: String,
    pub tx_hash: Option<String>,
    pub user_id: String,
    pub destination_address: String,
    pub amount: i64,
    pub asset: String,
    pub status: WithdrawalStatus,
    pub anchor_tx_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Balance {
    pub id: String,
    pub owner_id: String,
    pub asset: String,
    pub amount: i64,
    pub last_updated: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLog {
    pub id: String,
    pub event_type: String,
    pub ref_id: String,
    pub user_id: Option<String>,
    pub details: serde_json::Value,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BridgeTransactionStatus {
    Pending,
    Confirming,
    Completed,
    Failed,
}

impl BridgeTransactionStatus {
    pub fn from_str(s: &str) -> Self {
        match s {
            "confirming" => BridgeTransactionStatus::Confirming,
            "completed" => BridgeTransactionStatus::Completed,
            "failed" => BridgeTransactionStatus::Failed,
            _ => BridgeTransactionStatus::Pending,
        }
    }

    pub fn to_string(&self) -> String {
        match self {
            BridgeTransactionStatus::Pending => "pending",
            BridgeTransactionStatus::Confirming => "confirming",
            BridgeTransactionStatus::Completed => "completed",
            BridgeTransactionStatus::Failed => "failed",
        }
        .to_string()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeTransaction {
    pub id: String,
    pub from_chain: String,
    pub to_chain: String,
    pub asset: String,
    pub amount: u64,
    pub destination_address: String,
    pub user_id: String,
    pub status: BridgeTransactionStatus,
    pub tx_hash: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NotificationType {
    SYSTEM,
    ACTION,
    SECURITY,
}

impl NotificationType {
    pub fn from_str(s: &str) -> Self {
        match s {
            "SYSTEM" => NotificationType::SYSTEM,
            "ACTION" => NotificationType::ACTION,
            "SECURITY" => NotificationType::SECURITY,
            _ => NotificationType::SYSTEM,
        }
    }

    pub fn to_string(&self) -> String {
        match self {
            NotificationType::SYSTEM => "SYSTEM",
            NotificationType::ACTION => "ACTION",
            NotificationType::SECURITY => "SECURITY",
        }
        .to_string()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    pub id: String,
    pub user_id: String,
    pub notification_type: NotificationType,
    pub title: String,
    pub message: String,
    pub metadata: Option<serde_json::Value>,
    pub read: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RateLimitScope {
    Ip,
    User,
    ApiKey,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    pub window_ms: u64,
    pub max_requests: u32,
    pub scope: RateLimitScope,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserProfile {
    pub id: String,
    pub user_id: String,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub bio: Option<String>,
    pub country: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
