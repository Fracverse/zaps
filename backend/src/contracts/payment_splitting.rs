use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Payment splitting contract for distributing payments across multiple recipients
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentSplittingContract {
    pub id: String,
    pub contract_address: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub status: SplittingStatus,
    pub owner: String,
    pub total_splits: i32,
    pub is_immutable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SplittingStatus {
    Active,
    Paused,
    Archived,
    Disabled,
}

/// Split recipient configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SplitRecipient {
    pub id: String,
    pub contract_id: String,
    pub recipient_address: String,
    pub percentage: f64,
    pub fixed_amount: Option<i64>,
    pub priority: i32,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Payment split execution record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentSplit {
    pub id: String,
    pub contract_id: String,
    pub original_payment_id: String,
    pub total_amount: i64,
    pub currency: String,
    pub status: SplitStatus,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub splits: Vec<SplitDistribution>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SplitStatus {
    Pending,
    Processing,
    Completed,
    Failed,
    Reversed,
}

/// Individual distribution within a payment split
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SplitDistribution {
    pub id: String,
    pub payment_split_id: String,
    pub recipient_id: String,
    pub recipient_address: String,
    pub amount: i64,
    pub percentage_applied: f64,
    pub status: DistributionStatus,
    pub transaction_hash: Option<String>,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DistributionStatus {
    Pending,
    Processing,
    Completed,
    Failed,
    Retrying,
}

/// Split configuration template
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SplitTemplate {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub created_by: String,
    pub recipients: Vec<TemplateRecipient>,
    pub created_at: DateTime<Utc>,
    pub is_public: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateRecipient {
    pub recipient_address: String,
    pub percentage: f64,
    pub fixed_amount: Option<i64>,
    pub priority: i32,
}

/// Split rule for conditional distributions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SplitRule {
    pub id: String,
    pub contract_id: String,
    pub rule_name: String,
    pub condition: RuleCondition,
    pub recipients: Vec<ConditionalRecipient>,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RuleCondition {
    AmountRange { min: i64, max: i64 },
    PaymentMethod(String),
    TimeWindow { start_hour: i32, end_hour: i32 },
    DayOfWeek(Vec<String>),
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConditionalRecipient {
    pub recipient_address: String,
    pub percentage: f64,
    pub priority: i32,
}

/// Split analytics and reporting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SplitAnalytics {
    pub contract_id: String,
    pub total_payments_split: i64,
    pub total_amount_distributed: i64,
    pub total_recipients: i32,
    pub average_split_amount: i64,
    pub successful_splits: i64,
    pub failed_splits: i64,
    pub success_rate: f64,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
}

/// Recipient performance metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecipientMetrics {
    pub recipient_id: String,
    pub recipient_address: String,
    pub total_received: i64,
    pub payment_count: i64,
    pub average_payment: i64,
    pub last_payment_at: Option<DateTime<Utc>>,
    pub success_rate: f64,
}

/// Request to create a payment splitting contract
#[derive(Debug, Deserialize)]
pub struct CreatePaymentSplittingRequest {
    pub contract_address: String,
    pub owner: String,
    pub recipients: Vec<CreateSplitRecipientRequest>,
    pub is_immutable: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct CreateSplitRecipientRequest {
    pub recipient_address: String,
    pub percentage: f64,
    pub fixed_amount: Option<i64>,
    pub priority: Option<i32>,
}

/// Request to execute a payment split
#[derive(Debug, Deserialize)]
pub struct ExecutePaymentSplitRequest {
    pub contract_id: String,
    pub original_payment_id: String,
    pub total_amount: i64,
    pub currency: String,
}

/// Request to update split recipients
#[derive(Debug, Deserialize)]
pub struct UpdateSplitRecipientsRequest {
    pub contract_id: String,
    pub recipients: Vec<CreateSplitRecipientRequest>,
}

/// Request to add a split rule
#[derive(Debug, Deserialize)]
pub struct AddSplitRuleRequest {
    pub contract_id: String,
    pub rule_name: String,
    pub condition: RuleCondition,
    pub recipients: Vec<ConditionalRecipient>,
}

/// Split execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SplitExecutionResult {
    pub payment_split_id: String,
    pub success: bool,
    pub total_amount: i64,
    pub distributions: Vec<DistributionResult>,
    pub message: String,
    pub executed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistributionResult {
    pub recipient_address: String,
    pub amount: i64,
    pub success: bool,
    pub transaction_hash: Option<String>,
}

/// Validation result for split configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SplitValidationResult {
    pub is_valid: bool,
    pub total_percentage: f64,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

impl PaymentSplittingContract {
    pub fn new(
        contract_address: String,
        owner: String,
        total_splits: i32,
        is_immutable: bool,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            contract_address,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            status: SplittingStatus::Active,
            owner,
            total_splits,
            is_immutable,
        }
    }

    pub fn is_active(&self) -> bool {
        self.status == SplittingStatus::Active
    }

    pub fn pause(&mut self) {
        self.status = SplittingStatus::Paused;
        self.updated_at = Utc::now();
    }

    pub fn resume(&mut self) {
        self.status = SplittingStatus::Active;
        self.updated_at = Utc::now();
    }

    pub fn disable(&mut self) {
        self.status = SplittingStatus::Disabled;
        self.updated_at = Utc::now();
    }
}

impl SplitRecipient {
    pub fn new(
        contract_id: String,
        recipient_address: String,
        percentage: f64,
        priority: i32,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            contract_id,
            recipient_address,
            percentage,
            fixed_amount: None,
            priority,
            is_active: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    pub fn with_fixed_amount(mut self, amount: i64) -> Self {
        self.fixed_amount = Some(amount);
        self
    }

    pub fn deactivate(&mut self) {
        self.is_active = false;
        self.updated_at = Utc::now();
    }

    pub fn activate(&mut self) {
        self.is_active = true;
        self.updated_at = Utc::now();
    }
}

impl PaymentSplit {
    pub fn new(
        contract_id: String,
        original_payment_id: String,
        total_amount: i64,
        currency: String,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            contract_id,
            original_payment_id,
            total_amount,
            currency,
            status: SplitStatus::Pending,
            created_at: Utc::now(),
            completed_at: None,
            splits: Vec::new(),
        }
    }

    pub fn add_distribution(&mut self, distribution: SplitDistribution) {
        self.splits.push(distribution);
    }

    pub fn mark_processing(&mut self) {
        self.status = SplitStatus::Processing;
    }

    pub fn mark_completed(&mut self) {
        self.status = SplitStatus::Completed;
        self.completed_at = Some(Utc::now());
    }

    pub fn mark_failed(&mut self) {
        self.status = SplitStatus::Failed;
    }

    pub fn mark_reversed(&mut self) {
        self.status = SplitStatus::Reversed;
        self.completed_at = Some(Utc::now());
    }

    pub fn get_total_distributed(&self) -> i64 {
        self.splits.iter().map(|d| d.amount).sum()
    }

    pub fn get_pending_distributions(&self) -> Vec<&SplitDistribution> {
        self.splits
            .iter()
            .filter(|d| d.status == DistributionStatus::Pending)
            .collect()
    }

    pub fn get_failed_distributions(&self) -> Vec<&SplitDistribution> {
        self.splits
            .iter()
            .filter(|d| d.status == DistributionStatus::Failed)
            .collect()
    }
}

impl SplitDistribution {
    pub fn new(
        payment_split_id: String,
        recipient_id: String,
        recipient_address: String,
        amount: i64,
        percentage_applied: f64,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            payment_split_id,
            recipient_id,
            recipient_address,
            amount,
            percentage_applied,
            status: DistributionStatus::Pending,
            transaction_hash: None,
            created_at: Utc::now(),
            completed_at: None,
        }
    }

    pub fn mark_processing(&mut self) {
        self.status = DistributionStatus::Processing;
    }

    pub fn mark_completed(&mut self, transaction_hash: Option<String>) {
        self.status = DistributionStatus::Completed;
        self.transaction_hash = transaction_hash;
        self.completed_at = Some(Utc::now());
    }

    pub fn mark_failed(&mut self) {
        self.status = DistributionStatus::Failed;
    }

    pub fn mark_retrying(&mut self) {
        self.status = DistributionStatus::Retrying;
    }
}

impl SplitTemplate {
    pub fn new(
        name: String,
        created_by: String,
        recipients: Vec<TemplateRecipient>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name,
            description: None,
            created_by,
            recipients,
            created_at: Utc::now(),
            is_public: false,
        }
    }

    pub fn validate(&self) -> SplitValidationResult {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        let total_percentage: f64 = self.recipients.iter().map(|r| r.percentage).sum();

        if (total_percentage - 100.0).abs() > 0.01 {
            errors.push(format!(
                "Total percentage must equal 100%, got {}%",
                total_percentage
            ));
        }

        if self.recipients.is_empty() {
            errors.push("At least one recipient is required".to_string());
        }

        for recipient in &self.recipients {
            if recipient.percentage < 0.0 || recipient.percentage > 100.0 {
                errors.push(format!(
                    "Invalid percentage for {}: {}%",
                    recipient.recipient_address, recipient.percentage
                ));
            }

            if recipient.recipient_address.is_empty() {
                errors.push("Recipient address cannot be empty".to_string());
            }
        }

        SplitValidationResult {
            is_valid: errors.is_empty(),
            total_percentage,
            errors,
            warnings,
        }
    }
}

impl SplitRule {
    pub fn new(
        contract_id: String,
        rule_name: String,
        condition: RuleCondition,
        recipients: Vec<ConditionalRecipient>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            contract_id,
            rule_name,
            condition,
            recipients,
            is_active: true,
            created_at: Utc::now(),
        }
    }

    pub fn deactivate(&mut self) {
        self.is_active = false;
    }

    pub fn activate(&mut self) {
        self.is_active = true;
    }

    pub fn matches_condition(&self, amount: i64, payment_method: Option<&str>) -> bool {
        match &self.condition {
            RuleCondition::AmountRange { min, max } => amount >= *min && amount <= *max,
            RuleCondition::PaymentMethod(method) => {
                payment_method.map_or(false, |pm| pm == method)
            }
            RuleCondition::TimeWindow { start_hour, end_hour } => {
                let now = Utc::now();
                let current_hour = now.hour() as i32;
                current_hour >= *start_hour && current_hour < *end_hour
            }
            RuleCondition::DayOfWeek(days) => {
                let now = Utc::now();
                let weekday = now.weekday().to_string();
                days.contains(&weekday)
            }
            RuleCondition::Custom(_) => true,
        }
    }
}

impl SplitAnalytics {
    pub fn new(contract_id: String) -> Self {
        let now = Utc::now();
        Self {
            contract_id,
            total_payments_split: 0,
            total_amount_distributed: 0,
            total_recipients: 0,
            average_split_amount: 0,
            successful_splits: 0,
            failed_splits: 0,
            success_rate: 0.0,
            period_start: now,
            period_end: now,
        }
    }

    pub fn calculate_success_rate(&mut self) {
        let total = self.successful_splits + self.failed_splits;
        if total > 0 {
            self.success_rate = (self.successful_splits as f64 / total as f64) * 100.0;
        }
    }

    pub fn calculate_average(&mut self) {
        if self.total_payments_split > 0 {
            self.average_split_amount = self.total_amount_distributed / self.total_payments_split;
        }
    }
}

impl RecipientMetrics {
    pub fn new(recipient_id: String, recipient_address: String) -> Self {
        Self {
            recipient_id,
            recipient_address,
            total_received: 0,
            payment_count: 0,
            average_payment: 0,
            last_payment_at: None,
            success_rate: 0.0,
        }
    }

    pub fn record_payment(&mut self, amount: i64) {
        self.total_received += amount;
        self.payment_count += 1;
        self.last_payment_at = Some(Utc::now());
        self.average_payment = self.total_received / self.payment_count;
    }

    pub fn update_success_rate(&mut self, successful: i64, total: i64) {
        if total > 0 {
            self.success_rate = (successful as f64 / total as f64) * 100.0;
        }
    }
}
