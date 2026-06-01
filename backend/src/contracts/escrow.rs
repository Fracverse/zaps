use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Advanced escrow contract with dispute resolution capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EscrowContract {
    pub id: String,
    pub contract_address: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub status: EscrowStatus,
    pub owner: String,
    pub total_escrows: i64,
    pub total_amount_held: i64,
    pub dispute_resolution_enabled: bool,
    pub arbitrator_address: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum EscrowStatus {
    Active,
    Paused,
    Archived,
    Suspended,
}

/// Individual escrow transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Escrow {
    pub id: String,
    pub contract_id: String,
    pub payer: String,
    pub payee: String,
    pub amount: i64,
    pub currency: String,
    pub status: EscrowItemStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub release_date: Option<DateTime<Utc>>,
    pub released_at: Option<DateTime<Utc>>,
    pub refunded_at: Option<DateTime<Utc>>,
    pub description: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub dispute_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum EscrowItemStatus {
    Pending,
    Held,
    Released,
    Refunded,
    Disputed,
    Resolved,
    Cancelled,
}

/// Dispute raised against an escrow
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dispute {
    pub id: String,
    pub escrow_id: String,
    pub initiator: String,
    pub respondent: String,
    pub dispute_type: DisputeType,
    pub status: DisputeStatus,
    pub severity: DisputeSeverity,
    pub title: String,
    pub description: String,
    pub evidence_count: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub resolution_deadline: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub resolution_method: Option<ResolutionMethod>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DisputeType {
    NonDelivery,
    QualityIssue,
    PartialDelivery,
    Fraud,
    Unauthorized,
    Other(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DisputeStatus {
    Opened,
    UnderReview,
    EvidenceGathering,
    Mediation,
    Arbitration,
    Resolved,
    Closed,
    Appealed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DisputeSeverity {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ResolutionMethod {
    AutomatedResolution,
    MediatorReview,
    ArbitratorDecision,
    MutualAgreement,
    EscalatedArbitration,
}

/// Evidence submitted in a dispute
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisputeEvidence {
    pub id: String,
    pub dispute_id: String,
    pub submitted_by: String,
    pub evidence_type: EvidenceType,
    pub title: String,
    pub description: String,
    pub file_hash: Option<String>,
    pub file_url: Option<String>,
    pub created_at: DateTime<Utc>,
    pub verified: bool,
    pub verified_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum EvidenceType {
    Document,
    Image,
    Video,
    Audio,
    Message,
    Transaction,
    Other(String),
}

/// Mediation session for dispute resolution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediationSession {
    pub id: String,
    pub dispute_id: String,
    pub mediator: String,
    pub status: MediationStatus,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub proposed_resolution: Option<ProposedResolution>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MediationStatus {
    Scheduled,
    InProgress,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposedResolution {
    pub payer_amount: i64,
    pub payee_amount: i64,
    pub description: String,
    pub proposed_by: String,
    pub proposed_at: DateTime<Utc>,
}

/// Arbitration decision
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArbitrationDecision {
    pub id: String,
    pub dispute_id: String,
    pub arbitrator: String,
    pub decision_type: DecisionType,
    pub payer_amount: i64,
    pub payee_amount: i64,
    pub reasoning: String,
    pub created_at: DateTime<Utc>,
    pub is_final: bool,
    pub appeal_deadline: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DecisionType {
    FullRefund,
    FullRelease,
    PartialSplit,
    CustomDistribution,
}

/// Appeal of an arbitration decision
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisputeAppeal {
    pub id: String,
    pub original_decision_id: String,
    pub dispute_id: String,
    pub appellant: String,
    pub appeal_reason: String,
    pub status: AppealStatus,
    pub created_at: DateTime<Utc>,
    pub reviewed_at: Option<DateTime<Utc>>,
    pub appeal_decision: Option<AppealDecision>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AppealStatus {
    Pending,
    UnderReview,
    Approved,
    Rejected,
    Escalated,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppealDecision {
    pub decision_type: DecisionType,
    pub payer_amount: i64,
    pub payee_amount: i64,
    pub reasoning: String,
    pub decided_by: String,
    pub decided_at: DateTime<Utc>,
}

/// Dispute resolution rule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolutionRule {
    pub id: String,
    pub contract_id: String,
    pub rule_name: String,
    pub condition: RuleCondition,
    pub auto_resolution: Option<AutoResolution>,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RuleCondition {
    AmountThreshold { min: i64, max: i64 },
    DisputeType(DisputeType),
    TimeElapsed { days: i32 },
    EvidenceCount { min: i32 },
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoResolution {
    pub resolution_type: ResolutionType,
    pub payer_percentage: f64,
    pub payee_percentage: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ResolutionType {
    AutoRefund,
    AutoRelease,
    AutoSplit,
    RequireMediation,
    RequireArbitration,
}

/// Dispute analytics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisputeAnalytics {
    pub contract_id: String,
    pub total_disputes: i64,
    pub open_disputes: i64,
    pub resolved_disputes: i64,
    pub average_resolution_time_hours: i64,
    pub payer_win_rate: f64,
    pub payee_win_rate: f64,
    pub mutual_agreement_rate: f64,
    pub appeal_rate: f64,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
}

/// Mediator profile
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mediator {
    pub id: String,
    pub address: String,
    pub name: String,
    pub specialization: Vec<String>,
    pub rating: f64,
    pub total_cases: i64,
    pub successful_resolutions: i64,
    pub is_active: bool,
    pub joined_at: DateTime<Utc>,
}

/// Arbitrator profile
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Arbitrator {
    pub id: String,
    pub address: String,
    pub name: String,
    pub expertise: Vec<String>,
    pub rating: f64,
    pub total_cases: i64,
    pub appeal_rate: f64,
    pub is_active: bool,
    pub joined_at: DateTime<Utc>,
}

/// Request to create an escrow
#[derive(Debug, Deserialize)]
pub struct CreateEscrowRequest {
    pub contract_id: String,
    pub payer: String,
    pub payee: String,
    pub amount: i64,
    pub currency: String,
    pub release_date: Option<DateTime<Utc>>,
    pub description: Option<String>,
}

/// Request to release escrow
#[derive(Debug, Deserialize)]
pub struct ReleaseEscrowRequest {
    pub escrow_id: String,
    pub released_by: String,
}

/// Request to refund escrow
#[derive(Debug, Deserialize)]
pub struct RefundEscrowRequest {
    pub escrow_id: String,
    pub refunded_by: String,
    pub reason: Option<String>,
}

/// Request to open a dispute
#[derive(Debug, Deserialize)]
pub struct OpenDisputeRequest {
    pub escrow_id: String,
    pub initiator: String,
    pub dispute_type: DisputeType,
    pub severity: DisputeSeverity,
    pub title: String,
    pub description: String,
}

/// Request to submit evidence
#[derive(Debug, Deserialize)]
pub struct SubmitEvidenceRequest {
    pub dispute_id: String,
    pub submitted_by: String,
    pub evidence_type: EvidenceType,
    pub title: String,
    pub description: String,
    pub file_hash: Option<String>,
    pub file_url: Option<String>,
}

/// Request to propose resolution
#[derive(Debug, Deserialize)]
pub struct ProposeResolutionRequest {
    pub mediation_session_id: String,
    pub payer_amount: i64,
    pub payee_amount: i64,
    pub description: String,
    pub proposed_by: String,
}

/// Request to make arbitration decision
#[derive(Debug, Deserialize)]
pub struct MakeArbitrationDecisionRequest {
    pub dispute_id: String,
    pub arbitrator: String,
    pub decision_type: DecisionType,
    pub payer_amount: i64,
    pub payee_amount: i64,
    pub reasoning: String,
}

/// Request to appeal a decision
#[derive(Debug, Deserialize)]
pub struct AppealDecisionRequest {
    pub decision_id: String,
    pub appellant: String,
    pub appeal_reason: String,
}

/// Dispute resolution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisputeResolutionResult {
    pub dispute_id: String,
    pub resolution_method: ResolutionMethod,
    pub payer_amount: i64,
    pub payee_amount: i64,
    pub status: DisputeStatus,
    pub resolved_at: DateTime<Utc>,
    pub message: String,
}

impl EscrowContract {
    pub fn new(
        contract_address: String,
        owner: String,
        dispute_resolution_enabled: bool,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            contract_address,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            status: EscrowStatus::Active,
            owner,
            total_escrows: 0,
            total_amount_held: 0,
            dispute_resolution_enabled,
            arbitrator_address: None,
        }
    }

    pub fn is_active(&self) -> bool {
        self.status == EscrowStatus::Active
    }

    pub fn pause(&mut self) {
        self.status = EscrowStatus::Paused;
        self.updated_at = Utc::now();
    }

    pub fn resume(&mut self) {
        self.status = EscrowStatus::Active;
        self.updated_at = Utc::now();
    }

    pub fn set_arbitrator(&mut self, arbitrator_address: String) {
        self.arbitrator_address = Some(arbitrator_address);
        self.updated_at = Utc::now();
    }
}

impl Escrow {
    pub fn new(
        contract_id: String,
        payer: String,
        payee: String,
        amount: i64,
        currency: String,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            contract_id,
            payer,
            payee,
            amount,
            currency,
            status: EscrowItemStatus::Pending,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            release_date: None,
            released_at: None,
            refunded_at: None,
            description: None,
            metadata: None,
            dispute_id: None,
        }
    }

    pub fn hold(&mut self) {
        self.status = EscrowItemStatus::Held;
        self.updated_at = Utc::now();
    }

    pub fn release(&mut self) {
        self.status = EscrowItemStatus::Released;
        self.released_at = Some(Utc::now());
        self.updated_at = Utc::now();
    }

    pub fn refund(&mut self) {
        self.status = EscrowItemStatus::Refunded;
        self.refunded_at = Some(Utc::now());
        self.updated_at = Utc::now();
    }

    pub fn mark_disputed(&mut self, dispute_id: String) {
        self.status = EscrowItemStatus::Disputed;
        self.dispute_id = Some(dispute_id);
        self.updated_at = Utc::now();
    }

    pub fn mark_resolved(&mut self) {
        self.status = EscrowItemStatus::Resolved;
        self.updated_at = Utc::now();
    }

    pub fn can_be_released(&self) -> bool {
        if let Some(release_date) = self.release_date {
            Utc::now() >= release_date && self.status == EscrowItemStatus::Held
        } else {
            self.status == EscrowItemStatus::Held
        }
    }

    pub fn is_disputed(&self) -> bool {
        self.status == EscrowItemStatus::Disputed
    }
}

impl Dispute {
    pub fn new(
        escrow_id: String,
        initiator: String,
        respondent: String,
        dispute_type: DisputeType,
        severity: DisputeSeverity,
        title: String,
        description: String,
    ) -> Self {
        let now = Utc::now();
        let resolution_deadline = now + chrono::Duration::days(30);

        Self {
            id: Uuid::new_v4().to_string(),
            escrow_id,
            initiator,
            respondent,
            dispute_type,
            status: DisputeStatus::Opened,
            severity,
            title,
            description,
            evidence_count: 0,
            created_at: now,
            updated_at: now,
            resolution_deadline,
            resolved_at: None,
            resolution_method: None,
        }
    }

    pub fn start_review(&mut self) {
        self.status = DisputeStatus::UnderReview;
        self.updated_at = Utc::now();
    }

    pub fn start_evidence_gathering(&mut self) {
        self.status = DisputeStatus::EvidenceGathering;
        self.updated_at = Utc::now();
    }

    pub fn start_mediation(&mut self) {
        self.status = DisputeStatus::Mediation;
        self.updated_at = Utc::now();
    }

    pub fn start_arbitration(&mut self) {
        self.status = DisputeStatus::Arbitration;
        self.updated_at = Utc::now();
    }

    pub fn resolve(&mut self, resolution_method: ResolutionMethod) {
        self.status = DisputeStatus::Resolved;
        self.resolution_method = Some(resolution_method);
        self.resolved_at = Some(Utc::now());
        self.updated_at = Utc::now();
    }

    pub fn appeal(&mut self) {
        self.status = DisputeStatus::Appealed;
        self.updated_at = Utc::now();
    }

    pub fn is_overdue(&self) -> bool {
        Utc::now() > self.resolution_deadline && self.status != DisputeStatus::Resolved
    }

    pub fn add_evidence(&mut self) {
        self.evidence_count += 1;
        self.updated_at = Utc::now();
    }
}

impl DisputeEvidence {
    pub fn new(
        dispute_id: String,
        submitted_by: String,
        evidence_type: EvidenceType,
        title: String,
        description: String,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            dispute_id,
            submitted_by,
            evidence_type,
            title,
            description,
            file_hash: None,
            file_url: None,
            created_at: Utc::now(),
            verified: false,
            verified_at: None,
        }
    }

    pub fn verify(&mut self) {
        self.verified = true;
        self.verified_at = Some(Utc::now());
    }
}

impl MediationSession {
    pub fn new(dispute_id: String, mediator: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            dispute_id,
            mediator,
            status: MediationStatus::Scheduled,
            created_at: Utc::now(),
            started_at: None,
            completed_at: None,
            proposed_resolution: None,
            notes: None,
        }
    }

    pub fn start(&mut self) {
        self.status = MediationStatus::InProgress;
        self.started_at = Some(Utc::now());
    }

    pub fn pause(&mut self) {
        self.status = MediationStatus::Paused;
    }

    pub fn complete(&mut self) {
        self.status = MediationStatus::Completed;
        self.completed_at = Some(Utc::now());
    }

    pub fn fail(&mut self) {
        self.status = MediationStatus::Failed;
        self.completed_at = Some(Utc::now());
    }

    pub fn propose_resolution(&mut self, resolution: ProposedResolution) {
        self.proposed_resolution = Some(resolution);
    }
}

impl ArbitrationDecision {
    pub fn new(
        dispute_id: String,
        arbitrator: String,
        decision_type: DecisionType,
        payer_amount: i64,
        payee_amount: i64,
        reasoning: String,
    ) -> Self {
        let now = Utc::now();
        let appeal_deadline = now + chrono::Duration::days(14);

        Self {
            id: Uuid::new_v4().to_string(),
            dispute_id,
            arbitrator,
            decision_type,
            payer_amount,
            payee_amount,
            reasoning,
            created_at: now,
            is_final: false,
            appeal_deadline: Some(appeal_deadline),
        }
    }

    pub fn finalize(&mut self) {
        self.is_final = true;
        self.appeal_deadline = None;
    }

    pub fn can_be_appealed(&self) -> bool {
        if let Some(deadline) = self.appeal_deadline {
            !self.is_final && Utc::now() <= deadline
        } else {
            false
        }
    }
}

impl DisputeAppeal {
    pub fn new(
        original_decision_id: String,
        dispute_id: String,
        appellant: String,
        appeal_reason: String,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            original_decision_id,
            dispute_id,
            appellant,
            appeal_reason,
            status: AppealStatus::Pending,
            created_at: Utc::now(),
            reviewed_at: None,
            appeal_decision: None,
        }
    }

    pub fn start_review(&mut self) {
        self.status = AppealStatus::UnderReview;
    }

    pub fn approve(&mut self, decision: AppealDecision) {
        self.status = AppealStatus::Approved;
        self.appeal_decision = Some(decision);
        self.reviewed_at = Some(Utc::now());
    }

    pub fn reject(&mut self) {
        self.status = AppealStatus::Rejected;
        self.reviewed_at = Some(Utc::now());
    }

    pub fn escalate(&mut self) {
        self.status = AppealStatus::Escalated;
    }
}

impl ResolutionRule {
    pub fn new(
        contract_id: String,
        rule_name: String,
        condition: RuleCondition,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            contract_id,
            rule_name,
            condition,
            auto_resolution: None,
            is_active: true,
            created_at: Utc::now(),
        }
    }

    pub fn set_auto_resolution(&mut self, auto_resolution: AutoResolution) {
        self.auto_resolution = Some(auto_resolution);
    }

    pub fn deactivate(&mut self) {
        self.is_active = false;
    }

    pub fn activate(&mut self) {
        self.is_active = true;
    }

    pub fn matches_condition(&self, amount: i64, dispute_type: &DisputeType, evidence_count: i32) -> bool {
        match &self.condition {
            RuleCondition::AmountThreshold { min, max } => amount >= *min && amount <= *max,
            RuleCondition::DisputeType(dt) => dt == dispute_type,
            RuleCondition::TimeElapsed { days } => {
                // This would need context about when dispute was created
                true
            }
            RuleCondition::EvidenceCount { min } => evidence_count >= *min,
            RuleCondition::Custom(_) => true,
        }
    }
}

impl DisputeAnalytics {
    pub fn new(contract_id: String) -> Self {
        let now = Utc::now();
        Self {
            contract_id,
            total_disputes: 0,
            open_disputes: 0,
            resolved_disputes: 0,
            average_resolution_time_hours: 0,
            payer_win_rate: 0.0,
            payee_win_rate: 0.0,
            mutual_agreement_rate: 0.0,
            appeal_rate: 0.0,
            period_start: now,
            period_end: now,
        }
    }

    pub fn calculate_rates(&mut self, payer_wins: i64, payee_wins: i64, mutual: i64, appeals: i64) {
        let total = self.resolved_disputes;
        if total > 0 {
            self.payer_win_rate = (payer_wins as f64 / total as f64) * 100.0;
            self.payee_win_rate = (payee_wins as f64 / total as f64) * 100.0;
            self.mutual_agreement_rate = (mutual as f64 / total as f64) * 100.0;
            self.appeal_rate = (appeals as f64 / total as f64) * 100.0;
        }
    }
}

impl Mediator {
    pub fn new(address: String, name: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            address,
            name,
            specialization: Vec::new(),
            rating: 5.0,
            total_cases: 0,
            successful_resolutions: 0,
            is_active: true,
            joined_at: Utc::now(),
        }
    }

    pub fn update_rating(&mut self, new_rating: f64) {
        self.rating = new_rating.max(0.0).min(5.0);
    }

    pub fn record_case(&mut self, successful: bool) {
        self.total_cases += 1;
        if successful {
            self.successful_resolutions += 1;
        }
    }

    pub fn get_success_rate(&self) -> f64 {
        if self.total_cases > 0 {
            (self.successful_resolutions as f64 / self.total_cases as f64) * 100.0
        } else {
            0.0
        }
    }
}

impl Arbitrator {
    pub fn new(address: String, name: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            address,
            name,
            expertise: Vec::new(),
            rating: 5.0,
            total_cases: 0,
            appeal_rate: 0.0,
            is_active: true,
            joined_at: Utc::now(),
        }
    }

    pub fn update_rating(&mut self, new_rating: f64) {
        self.rating = new_rating.max(0.0).min(5.0);
    }

    pub fn record_case(&mut self, appeals: i64) {
        self.total_cases += 1;
        if self.total_cases > 0 {
            self.appeal_rate = (appeals as f64 / self.total_cases as f64) * 100.0;
        }
    }
}
