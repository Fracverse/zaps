use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

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

impl FromStr for PaymentStatus {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "completed" => PaymentStatus::Completed,
            "processing" => PaymentStatus::Processing,
            "failed" => PaymentStatus::Failed,
            "refunded" => PaymentStatus::Refunded,
            _ => PaymentStatus::Pending,
        })
    }
}

impl fmt::Display for PaymentStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            PaymentStatus::Pending => "pending",
            PaymentStatus::Processing => "processing",
            PaymentStatus::Completed => "completed",
            PaymentStatus::Failed => "failed",
            PaymentStatus::Refunded => "refunded",
        };
        write!(f, "{}", s)
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

impl FromStr for TransferStatus {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "processing" => TransferStatus::Processing,
            "completed" => TransferStatus::Completed,
            "failed" => TransferStatus::Failed,
            _ => TransferStatus::Pending,
        })
    }
}

impl fmt::Display for TransferStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            TransferStatus::Pending => "pending",
            TransferStatus::Processing => "processing",
            TransferStatus::Completed => "completed",
            TransferStatus::Failed => "failed",
        };
        write!(f, "{}", s)
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

impl FromStr for WithdrawalStatus {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "processing" => WithdrawalStatus::Processing,
            "completed" => WithdrawalStatus::Completed,
            "failed" => WithdrawalStatus::Failed,
            _ => WithdrawalStatus::Pending,
        })
    }
}

impl fmt::Display for WithdrawalStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            WithdrawalStatus::Pending => "pending",
            WithdrawalStatus::Processing => "processing",
            WithdrawalStatus::Completed => "completed",
            WithdrawalStatus::Failed => "failed",
        };
        write!(f, "{}", s)
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
pub struct AuditLogEntry {
    pub id: String,
    pub actor_id: String,
    pub action: String,
    pub resource: String,
    pub resource_id: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub timestamp: DateTime<Utc>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateAuditLogParams {
    pub actor_id: String,
    pub action: String,
    pub resource: String,
    pub resource_id: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
}

// DTOs for Audit Log API
#[derive(Debug, Deserialize)]
pub struct AuditLogQueryParams {
    pub actor_id: Option<String>,
    pub action: Option<String>,
    pub from_date: Option<DateTime<Utc>>,
    pub to_date: Option<DateTime<Utc>>,
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

fn default_limit() -> i64 {
    50
}

#[derive(Debug, Serialize)]
pub struct AuditLogResponse {
    pub id: String,
    pub actor_id: String,
    pub action: String,
    pub resource: String,
    pub resource_id: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct AuditLogListResponse {
    pub logs: Vec<AuditLogResponse>,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BridgeTransactionStatus {
    Pending,
    Confirming,
    Completed,
    Failed,
}

impl FromStr for BridgeTransactionStatus {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "confirming" => BridgeTransactionStatus::Confirming,
            "completed" => BridgeTransactionStatus::Completed,
            "failed" => BridgeTransactionStatus::Failed,
            _ => BridgeTransactionStatus::Pending,
        })
    }
}

impl fmt::Display for BridgeTransactionStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            BridgeTransactionStatus::Pending => "pending",
            BridgeTransactionStatus::Confirming => "confirming",
            BridgeTransactionStatus::Completed => "completed",
            BridgeTransactionStatus::Failed => "failed",
        };
        write!(f, "{}", s)
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

impl FromStr for NotificationType {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "SYSTEM" => NotificationType::SYSTEM,
            "ACTION" => NotificationType::ACTION,
            "SECURITY" => NotificationType::SECURITY,
            _ => NotificationType::SYSTEM,
        })
    }
}

impl fmt::Display for NotificationType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            NotificationType::SYSTEM => "SYSTEM",
            NotificationType::ACTION => "ACTION",
            NotificationType::SECURITY => "SECURITY",
        };
        write!(f, "{}", s)
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
pub struct BuildTransactionDto {
    pub contract_id: String,
    pub method: String,
    pub args: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransactionStatus {
    PENDING,
    CONFIRMED,
    FAILED,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedTransactionResponse {
    pub tx_hash: String,
    pub status: TransactionStatus,
}
