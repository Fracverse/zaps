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
    pub(crate) address: String,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DeliveryStatus {
    SENT,
    DELIVERED,
    FAILED,
}

impl FromStr for DeliveryStatus {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "DELIVERED" => DeliveryStatus::DELIVERED,
            "FAILED" => DeliveryStatus::FAILED,
            _ => DeliveryStatus::SENT,
        })
    }
}

impl fmt::Display for DeliveryStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            DeliveryStatus::SENT => "SENT",
            DeliveryStatus::DELIVERED => "DELIVERED",
            DeliveryStatus::FAILED => "FAILED",
        };
        write!(f, "{}", s)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DeliveryChannel {
    EMAIL,
    SMS,
    PUSH,
    IN_APP,
}

impl FromStr for DeliveryChannel {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "SMS" => DeliveryChannel::SMS,
            "PUSH" => DeliveryChannel::PUSH,
            "IN_APP" => DeliveryChannel::IN_APP,
            _ => DeliveryChannel::EMAIL,
        })
    }
}

impl fmt::Display for DeliveryChannel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            DeliveryChannel::EMAIL => "EMAIL",
            DeliveryChannel::SMS => "SMS",
            DeliveryChannel::PUSH => "PUSH",
            DeliveryChannel::IN_APP => "IN_APP",
        };
        write!(f, "{}", s)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationPreference {
    pub id: String,
    pub user_id: String,
    pub email_enabled: bool,
    pub sms_enabled: bool,
    pub push_enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationTemplate {
    pub id: String,
    pub name: String,
    pub subject_template: Option<String>,
    pub body_template: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationDeliveryLog {
    pub id: String,
    pub notification_id: String,
    pub channel: DeliveryChannel,
    pub status: DeliveryStatus,
    pub error_message: Option<String>,
    pub delivered_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
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
    #[serde(default)]
    pub endpoint_limits: Vec<EndpointRateLimitConfig>,
    #[serde(default = "default_bypass_admin")]
    pub bypass_admin: bool,
}

fn default_bypass_admin() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndpointRateLimitConfig {
    pub path_prefix: String,
    pub window_ms: u64,
    pub max_requests: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Blocked,
}

impl fmt::Display for RiskLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            RiskLevel::Low => "low",
            RiskLevel::Medium => "medium",
            RiskLevel::High => "high",
            RiskLevel::Blocked => "blocked",
        };
        write!(f, "{}", value)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionRiskAssessment {
    pub user_id: String,
    pub address: String,
    pub amount: i64,
    pub risk_score: u8,
    pub risk_level: RiskLevel,
    pub sanctions_match: bool,
    pub velocity_limit_exceeded: bool,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserProfile {
    pub verification_status: String,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileUploadResponseDto {
    pub file_id: String,
    pub original_name: String,
    pub mime_type: String,
    pub size: u64,
    pub url: String,
}

// =============================================================================
// Payment Dispute Models
// =============================================================================

/// Payment dispute status lifecycle.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DisputeStatus {
    /// Dispute has been filed and is awaiting review.
    Open,
    /// Dispute is under active investigation.
    UnderReview,
    /// Dispute has been resolved in the customer's favour (refund issued).
    ResolvedCustomer,
    /// Dispute has been resolved in the merchant's favour (no refund).
    ResolvedMerchant,
    /// Dispute was closed without a resolution (e.g. withdrawn by customer).
    Closed,
}

impl FromStr for DisputeStatus {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "under_review" => DisputeStatus::UnderReview,
            "resolved_customer" => DisputeStatus::ResolvedCustomer,
            "resolved_merchant" => DisputeStatus::ResolvedMerchant,
            "closed" => DisputeStatus::Closed,
            _ => DisputeStatus::Open,
        })
    }
}

impl fmt::Display for DisputeStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            DisputeStatus::Open => "open",
            DisputeStatus::UnderReview => "under_review",
            DisputeStatus::ResolvedCustomer => "resolved_customer",
            DisputeStatus::ResolvedMerchant => "resolved_merchant",
            DisputeStatus::Closed => "closed",
        };
        write!(f, "{}", s)
    }
}

/// Reason category for a payment dispute.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DisputeReason {
    /// Customer did not authorise the transaction.
    Unauthorized,
    /// Goods or services were not delivered.
    NotDelivered,
    /// Goods or services were significantly not as described.
    NotAsDescribed,
    /// Customer was charged the wrong amount.
    IncorrectAmount,
    /// Duplicate charge for the same transaction.
    Duplicate,
    /// Any other reason (details in `description`).
    Other,
}

impl FromStr for DisputeReason {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "unauthorized" => DisputeReason::Unauthorized,
            "not_delivered" => DisputeReason::NotDelivered,
            "not_as_described" => DisputeReason::NotAsDescribed,
            "incorrect_amount" => DisputeReason::IncorrectAmount,
            "duplicate" => DisputeReason::Duplicate,
            _ => DisputeReason::Other,
        })
    }
}

impl fmt::Display for DisputeReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            DisputeReason::Unauthorized => "unauthorized",
            DisputeReason::NotDelivered => "not_delivered",
            DisputeReason::NotAsDescribed => "not_as_described",
            DisputeReason::IncorrectAmount => "incorrect_amount",
            DisputeReason::Duplicate => "duplicate",
            DisputeReason::Other => "other",
        };
        write!(f, "{}", s)
    }
}

/// A payment dispute record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentDispute {
    pub id: String,
    pub payment_id: String,
    pub filed_by_user_id: String,
    pub reason: DisputeReason,
    pub description: String,
    pub status: DisputeStatus,
    /// Amount being disputed (may differ from full payment amount for partial disputes).
    pub disputed_amount: i64,
    /// Internal notes added by admins during review.
    pub resolution_notes: Option<String>,
    /// ID of the admin who resolved the dispute.
    pub resolved_by: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Evidence item attached to a dispute.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisputeEvidence {
    pub id: String,
    pub dispute_id: String,
    pub submitted_by_user_id: String,
    pub evidence_type: String,
    pub description: String,
    pub file_url: Option<String>,
    pub created_at: DateTime<Utc>,
}

// =============================================================================
// Payment Reconciliation Models
// =============================================================================

/// Payment reconciliation status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReconciliationStatus {
    Pending,
    Matched,
    Mismatched,
    ManualReview,
    Resolved,
}

impl std::fmt::Display for ReconciliationStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let s = match self {
            ReconciliationStatus::Pending => "pending",
            ReconciliationStatus::Matched => "matched",
            ReconciliationStatus::Mismatched => "mismatched",
            ReconciliationStatus::ManualReview => "manual_review",
            ReconciliationStatus::Resolved => "resolved",
        };
        write!(f, "{}", s)
    }
}

#[derive(Debug)]
pub struct ParseReconciliationStatusError;

impl std::fmt::Display for ParseReconciliationStatusError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "invalid reconciliation status")
    }
}

impl std::error::Error for ParseReconciliationStatusError {}

impl std::str::FromStr for ReconciliationStatus {
    type Err = ParseReconciliationStatusError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pending" => Ok(ReconciliationStatus::Pending),
            "matched" => Ok(ReconciliationStatus::Matched),
            "mismatched" => Ok(ReconciliationStatus::Mismatched),
            "manual_review" => Ok(ReconciliationStatus::ManualReview),
            "resolved" => Ok(ReconciliationStatus::Resolved),
            _ => Err(ParseReconciliationStatusError),
        }
    }
}

/// Reconciliation source
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReconciliationSource {
    Stellar,
    Bank,
    Manual,
}

/// Payment reconciliation record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentReconciliation {
    pub id: String,
    pub payment_id: Option<String>,
    pub external_id: Option<String>,
    pub source: ReconciliationSource,
    pub amount: i64,
    pub currency: String,
    pub status: ReconciliationStatus,
    pub discrepancy_notes: Option<String>,
    pub resolved_by: Option<String>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Audit log for reconciliation actions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconciliationAuditLog {
    pub id: String,
    pub reconciliation_id: String,
    pub actor_id: String,
    pub action: String,
    pub old_status: Option<ReconciliationStatus>,
    pub new_status: Option<ReconciliationStatus>,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
}

// =============================================================================
// Merchant Payout Models
// =============================================================================

/// Payout status lifecycle
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PayoutStatus {
    Pending,
    Scheduled,
    Processing,
    Completed,
    Failed,
    Cancelled,
}

impl FromStr for PayoutStatus {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "scheduled" => PayoutStatus::Scheduled,
            "processing" => PayoutStatus::Processing,
            "completed" => PayoutStatus::Completed,
            "failed" => PayoutStatus::Failed,
            "cancelled" => PayoutStatus::Cancelled,
            _ => PayoutStatus::Pending,
        })
    }
}

impl fmt::Display for PayoutStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            PayoutStatus::Pending => "pending",
            PayoutStatus::Scheduled => "scheduled",
            PayoutStatus::Processing => "processing",
            PayoutStatus::Completed => "completed",
            PayoutStatus::Failed => "failed",
            PayoutStatus::Cancelled => "cancelled",
        };
        write!(f, "{}", s)
    }
}

/// Individual payout record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Payout {
    pub id: String,
    pub merchant_id: String,
    pub batch_id: Option<String>,
    pub amount: i64,
    pub currency: String,
    pub destination_address: String,
    pub status: PayoutStatus,
    pub tx_hash: Option<String>,
    pub failure_reason: Option<String>,
    pub retry_count: i32,
    pub scheduled_at: Option<DateTime<Utc>>,
    pub processed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Payout batch for bulk processing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PayoutBatch {
    pub id: String,
    pub merchant_id: String,
    pub total_amount: i64,
    pub currency: String,
    pub payout_count: i32,
    pub status: PayoutStatus,
    pub scheduled_at: DateTime<Utc>,
    pub processed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Payout reconciliation record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PayoutReconciliation {
    pub id: String,
    pub payout_id: String,
    pub anchor_tx_id: Option<String>,
    pub status: String,
    pub discrepancy: Option<String>,
    pub reconciled_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

// =============================================================================
// ML-Based Compliance & Risk Assessment Models
// =============================================================================

/// Behavioral profile tracking user transaction patterns
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BehavioralProfile {
    pub user_id: String,
    pub average_transaction_amount: f64,
    pub transaction_frequency: f64, // transactions per day
    pub total_transactions: i64,
    pub high_risk_transaction_count: i64,
    pub geographic_diversity_score: f64, // 0.0 to 1.0
    pub time_pattern_score: f64,         // 0.0 to 1.0 (1.0 = unusual time patterns)
    pub device_diversity_score: f64,     // 0.0 to 1.0
    pub merchant_category_diversity: f64, // 0.0 to 1.0
    pub last_update: DateTime<Utc>,
}

/// Multiple sanctions database provider configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SanctionsProvider {
    pub id: String,
    pub name: String,
    pub provider_type: String, // e.g., "ofac", "un", "eu", "fca"
    pub api_url: String,
    pub api_key: String,
    pub enabled: bool,
    pub priority: i32, // higher = checked first
    pub timeout_seconds: u32,
    pub created_at: DateTime<Utc>,
}

/// ML-based risk score calculation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MLRiskScore {
    pub assessment_id: String,
    pub model_version: String,
    pub base_risk_score: f64,     // 0-100
    pub behavioral_risk: f64,     // 0-100
    pub network_risk: f64,        // 0-100
    pub geographic_risk: f64,     // 0-100
    pub temporal_risk: f64,       // 0-100
    pub device_risk: f64,         // 0-100
    pub final_ml_score: f64,      // 0-100 (weighted combination)
    pub confidence_level: f64,    // 0-1.0 (model confidence)
    pub risk_factors: Vec<String>,
    pub created_at: DateTime<Utc>,
}

/// Risk indicator for specific transaction behaviors
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskIndicator {
    pub id: String,
    pub assessment_id: String,
    pub indicator_type: String, // e.g., "structured_transaction", "circular_flow", "layering"
    pub severity: String,        // "low", "medium", "high", "critical"
    pub description: String,
    pub detected_at: DateTime<Utc>,
}

/// Compliance case for manual review and investigation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceCase {
    pub id: String,
    pub user_id: String,
    pub assessment_id: Option<String>,
    pub case_type: String, // e.g., "high_risk_transaction", "behavioral_anomaly", "sanctions_hit"
    pub status: String,    // "open", "under_investigation", "escalated", "resolved", "closed"
    pub priority: String,  // "low", "medium", "high", "critical"
    pub risk_score: f64,
    pub assigned_analyst: Option<String>,
    pub description: String,
    pub findings: Option<String>,
    pub resolution: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
}

/// Case activity log
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseActivityLog {
    pub id: String,
    pub case_id: String,
    pub activity_type: String, // "created", "assigned", "updated", "commented", "escalated", "resolved"
    pub performed_by: String,
    pub details: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

/// Compliance report aggregating metrics and findings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceReport {
    pub id: String,
    pub report_period: String, // "daily", "weekly", "monthly"
    pub start_date: DateTime<Utc>,
    pub end_date: DateTime<Utc>,
    pub total_transactions_screened: i64,
    pub high_risk_transactions: i64,
    pub blocked_transactions: i64,
    pub sanctioned_addresses_detected: i64,
    pub suspicious_patterns_detected: i64,
    pub cases_opened: i64,
    pub cases_resolved: i64,
    pub cases_escalated: i64,
    pub total_amount_flagged: i64,
    pub ml_model_performance: serde_json::Value, // precision, recall, f1-score
    pub recommendations: Vec<String>,
    pub created_at: DateTime<Utc>,
}
