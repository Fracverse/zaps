#![no_std]

//! # Payment Compliance Scoring Contract
//!
//! Computes and stores multi-dimensional compliance risk scores for payments
//! on the Stellar / Soroban network.
//!
//! ## Scoring Dimensions
//!
//! Each payment is evaluated across six independent dimensions (0–100 each,
//! higher = riskier):
//!
//! | Dimension       | What it measures                                      |
//! |-----------------|-------------------------------------------------------|
//! | `velocity`      | Transaction frequency relative to rolling window      |
//! | `amount`        | Payment size relative to configured thresholds        |
//! | `pattern`       | Structural anomalies (round numbers, repeated amounts)|
//! | `counterparty`  | Counterparty risk (blocklist, new address, etc.)      |
//! | `geo`           | Jurisdiction / geographic risk of the counterparty    |
//! | `history`       | Historical dispute / failure rate for the payer       |
//!
//! ## Weighted Aggregate
//!
//! The overall score is a weighted average of the six dimensions:
//!
//! ```text
//! overall = (velocity * W_VEL + amount * W_AMT + pattern * W_PAT
//!          + counterparty * W_CTR + geo * W_GEO + history * W_HIS) / W_TOTAL
//! ```
//!
//! Default weights are tunable by the admin via `set_weights`.
//!
//! ## Risk Levels
//!
//! | Score range | Level    | Action                          |
//! |-------------|----------|---------------------------------|
//! | 0–29        | Low      | Auto-approve                    |
//! | 30–59       | Medium   | Flag for review                 |
//! | 60–84       | High     | Require additional verification |
//! | 85–100      | Blocked  | Reject payment                  |
//!
//! ## Access Control
//!
//! Only authorised **scorers** (e.g. the payment router or backend oracle)
//! may call `score_payment`.  The admin manages the scorer whitelist.

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, symbol_short,
    Address, Env, Symbol,
};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Default weight for each scoring dimension (equal weighting).
pub const DEFAULT_WEIGHT: u32 = 1;

/// Score threshold above which a payment is considered blocked.
pub const BLOCKED_THRESHOLD: u32 = 85;
/// Score threshold above which a payment is considered high risk.
pub const HIGH_THRESHOLD: u32 = 60;
/// Score threshold above which a payment is considered medium risk.
pub const MEDIUM_THRESHOLD: u32 = 30;

/// Maximum score value.
pub const MAX_SCORE: u32 = 100;

/// Instance storage TTL (~1 year at 5 s/ledger).
const TTL_THRESHOLD: u32 = 100_000;
const TTL_EXTEND: u32 = 6_307_200;

/// Persistent storage TTL (~6 months).
const PERSISTENT_TTL_THRESHOLD: u32 = 50_000;
const PERSISTENT_TTL_EXTEND: u32 = 3_153_600;

// ---------------------------------------------------------------------------
// Storage keys
// ---------------------------------------------------------------------------

const KEY_ADMIN: Symbol = symbol_short!("admin");
const KEY_PAUSED: Symbol = symbol_short!("paused");
const KEY_VERSION: Symbol = symbol_short!("version");
const KEY_WEIGHTS: Symbol = symbol_short!("weights");

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    /// Authorised scorer whitelist.
    Scorer(Address),
    /// Blocklisted counterparty addresses.
    Blocklist(Address),
    /// Per-payer score history: payer → ScoreHistory.
    PayerHistory(Address),
    /// Per-payment score record: payment_id (u64) → ComplianceScore.
    PaymentScore(u64),
}

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Dimension weights used in the weighted-average calculation.
/// All weights must be > 0; the contract normalises by their sum.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ScoringWeights {
    pub velocity: u32,
    pub amount: u32,
    pub pattern: u32,
    pub counterparty: u32,
    pub geo: u32,
    pub history: u32,
}

impl ScoringWeights {
    pub fn default_weights() -> Self {
        ScoringWeights {
            velocity: DEFAULT_WEIGHT,
            amount: DEFAULT_WEIGHT,
            pattern: DEFAULT_WEIGHT,
            counterparty: DEFAULT_WEIGHT,
            geo: DEFAULT_WEIGHT,
            history: DEFAULT_WEIGHT,
        }
    }

    pub fn total(&self) -> u32 {
        self.velocity + self.amount + self.pattern + self.counterparty + self.geo + self.history
    }
}

/// Raw per-dimension scores supplied by the scorer oracle.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct DimensionScores {
    /// Frequency / velocity risk (0–100).
    pub velocity: u32,
    /// Amount-size risk (0–100).
    pub amount: u32,
    /// Structural pattern risk (0–100).
    pub pattern: u32,
    /// Counterparty risk (0–100).
    pub counterparty: u32,
    /// Geographic / jurisdiction risk (0–100).
    pub geo: u32,
    /// Historical dispute / failure risk (0–100).
    pub history: u32,
}

/// Risk level derived from the overall score.
#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RiskLevel {
    Low = 0,
    Medium = 1,
    High = 2,
    Blocked = 3,
}

/// Full compliance score record stored on-chain for a single payment.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ComplianceScore {
    /// Opaque payment identifier (assigned by the caller).
    pub payment_id: u64,
    /// Address of the payer.
    pub payer: Address,
    /// Payment amount in token base units.
    pub amount: i128,
    /// Per-dimension raw scores.
    pub dimensions: DimensionScores,
    /// Weighted aggregate score (0–100).
    pub overall_score: u32,
    /// Derived risk level.
    pub risk_level: RiskLevel,
    /// Whether the payer's address was on the blocklist at scoring time.
    pub blocklisted: bool,
    /// Ledger sequence at which the score was recorded.
    pub scored_at: u32,
}

/// Aggregated scoring history for a payer address.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct PayerHistory {
    /// Total payments scored.
    pub total_scored: u32,
    /// Payments that resulted in a Blocked decision.
    pub total_blocked: u32,
    /// Payments that resulted in a High risk decision.
    pub total_high: u32,
    /// Payments that resulted in a Medium risk decision.
    pub total_medium: u32,
    /// Running sum of overall scores (for average calculation).
    pub score_sum: u64,
    /// Ledger of the most recent scoring event.
    pub last_scored_at: u32,
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[contracterror]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum ComplianceError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    ContractPaused = 4,
    InvalidScore = 5,
    InvalidWeight = 6,
    ScorerAlreadyAdded = 7,
    ScorerNotFound = 8,
    PaymentNotFound = 9,
    AddressAlreadyBlocklisted = 10,
    AddressNotBlocklisted = 11,
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn bump_instance(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(TTL_THRESHOLD, TTL_EXTEND);
}

fn bump_persistent<K>(env: &Env, key: &K)
where
    K: soroban_sdk::IntoVal<Env, soroban_sdk::Val>,
{
    env.storage()
        .persistent()
        .extend_ttl(key, PERSISTENT_TTL_THRESHOLD, PERSISTENT_TTL_EXTEND);
}

fn require_admin(env: &Env) -> Address {
    let admin: Address = env
        .storage()
        .instance()
        .get(&KEY_ADMIN)
        .unwrap_or_else(|| panic_with_error!(env, ComplianceError::NotInitialized));
    admin.require_auth();
    admin
}

fn require_not_paused(env: &Env) {
    if env
        .storage()
        .instance()
        .get::<Symbol, bool>(&KEY_PAUSED)
        .unwrap_or(false)
    {
        panic_with_error!(env, ComplianceError::ContractPaused);
    }
}

fn require_scorer(env: &Env, scorer: &Address) {
    scorer.require_auth();
    if !env
        .storage()
        .persistent()
        .get::<DataKey, bool>(&DataKey::Scorer(scorer.clone()))
        .unwrap_or(false)
    {
        panic_with_error!(env, ComplianceError::Unauthorized);
    }
}

fn validate_dimension(env: &Env, score: u32) {
    if score > MAX_SCORE {
        panic_with_error!(env, ComplianceError::InvalidScore);
    }
}

fn load_weights(env: &Env) -> ScoringWeights {
    env.storage()
        .instance()
        .get(&KEY_WEIGHTS)
        .unwrap_or_else(ScoringWeights::default_weights)
}

/// Compute the weighted overall score from dimension scores and weights.
pub fn compute_overall(dims: &DimensionScores, weights: &ScoringWeights) -> u32 {
    let total_weight = weights.total();
    if total_weight == 0 {
        return 0;
    }
    let weighted_sum = dims.velocity as u64 * weights.velocity as u64
        + dims.amount as u64 * weights.amount as u64
        + dims.pattern as u64 * weights.pattern as u64
        + dims.counterparty as u64 * weights.counterparty as u64
        + dims.geo as u64 * weights.geo as u64
        + dims.history as u64 * weights.history as u64;

    (weighted_sum / total_weight as u64) as u32
}

/// Derive a `RiskLevel` from an overall score.
pub fn score_to_risk_level(score: u32) -> RiskLevel {
    if score >= BLOCKED_THRESHOLD {
        RiskLevel::Blocked
    } else if score >= HIGH_THRESHOLD {
        RiskLevel::High
    } else if score >= MEDIUM_THRESHOLD {
        RiskLevel::Medium
    } else {
        RiskLevel::Low
    }
}

fn update_payer_history(env: &Env, payer: &Address, risk_level: RiskLevel, overall_score: u32) {
    let key = DataKey::PayerHistory(payer.clone());
    let mut history: PayerHistory = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or(PayerHistory {
            total_scored: 0,
            total_blocked: 0,
            total_high: 0,
            total_medium: 0,
            score_sum: 0,
            last_scored_at: 0,
        });

    history.total_scored = history.total_scored.saturating_add(1);
    history.score_sum = history.score_sum.saturating_add(overall_score as u64);
    history.last_scored_at = env.ledger().sequence();

    match risk_level {
        RiskLevel::Blocked => history.total_blocked = history.total_blocked.saturating_add(1),
        RiskLevel::High => history.total_high = history.total_high.saturating_add(1),
        RiskLevel::Medium => history.total_medium = history.total_medium.saturating_add(1),
        RiskLevel::Low => {}
    }

    env.storage().persistent().set(&key, &history);
    bump_persistent(env, &key);
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct PaymentComplianceScoring;

#[contractimpl]
impl PaymentComplianceScoring {
    // -----------------------------------------------------------------------
    // Initialisation
    // -----------------------------------------------------------------------

    /// Initialise the compliance scoring contract.
    ///
    /// * `admin` – address that manages scorers, weights, and the blocklist.
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&KEY_ADMIN) {
            panic_with_error!(env, ComplianceError::AlreadyInitialized);
        }
        admin.require_auth();

        env.storage().instance().set(&KEY_ADMIN, &admin);
        env.storage().instance().set(&KEY_PAUSED, &false);
        env.storage().instance().set(&KEY_VERSION, &1u32);
        env.storage()
            .instance()
            .set(&KEY_WEIGHTS, &ScoringWeights::default_weights());
        bump_instance(&env);

        env.events().publish(
            (symbol_short!("comply"), symbol_short!("init")),
            admin,
        );
    }

    // -----------------------------------------------------------------------
    // Scorer management (admin only)
    // -----------------------------------------------------------------------

    /// Authorise an address to submit compliance scores.
    pub fn add_scorer(env: Env, scorer: Address) {
        require_admin(&env);
        bump_instance(&env);

        let key = DataKey::Scorer(scorer.clone());
        if env
            .storage()
            .persistent()
            .get::<DataKey, bool>(&key)
            .unwrap_or(false)
        {
            panic_with_error!(env, ComplianceError::ScorerAlreadyAdded);
        }

        env.storage().persistent().set(&key, &true);
        bump_persistent(&env, &key);

        env.events().publish(
            (symbol_short!("comply"), symbol_short!("scr_add")),
            scorer,
        );
    }

    /// Remove a scorer from the whitelist.
    pub fn remove_scorer(env: Env, scorer: Address) {
        require_admin(&env);
        bump_instance(&env);

        let key = DataKey::Scorer(scorer.clone());
        if !env
            .storage()
            .persistent()
            .get::<DataKey, bool>(&key)
            .unwrap_or(false)
        {
            panic_with_error!(env, ComplianceError::ScorerNotFound);
        }

        env.storage().persistent().remove(&key);

        env.events().publish(
            (symbol_short!("comply"), symbol_short!("scr_rm")),
            scorer,
        );
    }

    // -----------------------------------------------------------------------
    // Blocklist management (admin only)
    // -----------------------------------------------------------------------

    /// Add an address to the compliance blocklist.
    pub fn add_to_blocklist(env: Env, address: Address) {
        require_admin(&env);
        bump_instance(&env);

        let key = DataKey::Blocklist(address.clone());
        if env
            .storage()
            .persistent()
            .get::<DataKey, bool>(&key)
            .unwrap_or(false)
        {
            panic_with_error!(env, ComplianceError::AddressAlreadyBlocklisted);
        }

        env.storage().persistent().set(&key, &true);
        bump_persistent(&env, &key);

        env.events().publish(
            (symbol_short!("comply"), symbol_short!("blk_add")),
            address,
        );
    }

    /// Remove an address from the blocklist.
    pub fn remove_from_blocklist(env: Env, address: Address) {
        require_admin(&env);
        bump_instance(&env);

        let key = DataKey::Blocklist(address.clone());
        if !env
            .storage()
            .persistent()
            .get::<DataKey, bool>(&key)
            .unwrap_or(false)
        {
            panic_with_error!(env, ComplianceError::AddressNotBlocklisted);
        }

        env.storage().persistent().remove(&key);

        env.events().publish(
            (symbol_short!("comply"), symbol_short!("blk_rm")),
            address,
        );
    }

    // -----------------------------------------------------------------------
    // Weight configuration (admin only)
    // -----------------------------------------------------------------------

    /// Update the dimension weights used in the weighted-average calculation.
    /// All weights must be ≥ 1.
    pub fn set_weights(env: Env, weights: ScoringWeights) {
        require_admin(&env);
        bump_instance(&env);

        if weights.velocity == 0
            || weights.amount == 0
            || weights.pattern == 0
            || weights.counterparty == 0
            || weights.geo == 0
            || weights.history == 0
        {
            panic_with_error!(env, ComplianceError::InvalidWeight);
        }

        env.storage().instance().set(&KEY_WEIGHTS, &weights);

        env.events().publish(
            (symbol_short!("comply"), symbol_short!("wt_upd")),
            (),
        );
    }

    // -----------------------------------------------------------------------
    // Core: score a payment
    // -----------------------------------------------------------------------

    /// Record a compliance score for a payment.
    ///
    /// * `scorer`     – authorised scorer (must sign)
    /// * `payment_id` – opaque payment identifier (caller-assigned, must be unique)
    /// * `payer`      – address initiating the payment
    /// * `amount`     – payment amount in token base units
    /// * `dimensions` – per-dimension risk scores (0–100 each)
    ///
    /// Returns the computed `ComplianceScore`.
    pub fn score_payment(
        env: Env,
        scorer: Address,
        payment_id: u64,
        payer: Address,
        amount: i128,
        dimensions: DimensionScores,
    ) -> ComplianceScore {
        require_not_paused(&env);
        bump_instance(&env);
        require_scorer(&env, &scorer);

        // Validate all dimension scores are in range.
        validate_dimension(&env, dimensions.velocity);
        validate_dimension(&env, dimensions.amount);
        validate_dimension(&env, dimensions.pattern);
        validate_dimension(&env, dimensions.counterparty);
        validate_dimension(&env, dimensions.geo);
        validate_dimension(&env, dimensions.history);

        // Check blocklist.
        let blocklisted = env
            .storage()
            .persistent()
            .get::<DataKey, bool>(&DataKey::Blocklist(payer.clone()))
            .unwrap_or(false);

        // Compute weighted overall score.
        let weights = load_weights(&env);
        let mut overall = compute_overall(&dimensions, &weights);

        // Blocklisted addresses are always scored at maximum risk.
        if blocklisted {
            overall = MAX_SCORE;
        }

        let risk_level = score_to_risk_level(overall);

        let record = ComplianceScore {
            payment_id,
            payer: payer.clone(),
            amount,
            dimensions: dimensions.clone(),
            overall_score: overall,
            risk_level,
            blocklisted,
            scored_at: env.ledger().sequence(),
        };

        // Persist the score.
        let score_key = DataKey::PaymentScore(payment_id);
        env.storage().persistent().set(&score_key, &record);
        bump_persistent(&env, &score_key);

        // Update payer history.
        update_payer_history(&env, &payer, risk_level, overall);

        env.events().publish(
            (symbol_short!("comply"), symbol_short!("scored")),
            (payment_id, payer, overall, risk_level as u32),
        );

        record
    }

    // -----------------------------------------------------------------------
    // Admin: circuit breaker / upgrade / transfer
    // -----------------------------------------------------------------------

    pub fn pause(env: Env) {
        require_admin(&env);
        env.storage().instance().set(&KEY_PAUSED, &true);
        env.events()
            .publish((symbol_short!("comply"), symbol_short!("paused")), ());
    }

    pub fn unpause(env: Env) {
        require_admin(&env);
        env.storage().instance().set(&KEY_PAUSED, &false);
        env.events()
            .publish((symbol_short!("comply"), symbol_short!("unpaused")), ());
    }

    pub fn upgrade(env: Env, new_wasm_hash: soroban_sdk::BytesN<32>) {
        require_admin(&env);
        let version: u32 = env.storage().instance().get(&KEY_VERSION).unwrap_or(1);
        env.deployer().update_current_contract_wasm(new_wasm_hash);
        env.storage().instance().set(&KEY_VERSION, &(version + 1));
    }

    pub fn transfer_admin(env: Env, new_admin: Address) {
        require_admin(&env);
        env.storage().instance().set(&KEY_ADMIN, &new_admin);
        env.events().publish(
            (symbol_short!("comply"), symbol_short!("adm_xfer")),
            new_admin,
        );
    }

    // -----------------------------------------------------------------------
    // Views
    // -----------------------------------------------------------------------

    /// Retrieve the compliance score for a specific payment.
    pub fn get_payment_score(env: Env, payment_id: u64) -> ComplianceScore {
        env.storage()
            .persistent()
            .get(&DataKey::PaymentScore(payment_id))
            .unwrap_or_else(|| panic_with_error!(env, ComplianceError::PaymentNotFound))
    }

    /// Retrieve the scoring history for a payer address.
    pub fn get_payer_history(env: Env, payer: Address) -> PayerHistory {
        env.storage()
            .persistent()
            .get(&DataKey::PayerHistory(payer))
            .unwrap_or(PayerHistory {
                total_scored: 0,
                total_blocked: 0,
                total_high: 0,
                total_medium: 0,
                score_sum: 0,
                last_scored_at: 0,
            })
    }

    /// Returns the average overall score for a payer (0 if no history).
    pub fn get_payer_average_score(env: Env, payer: Address) -> u32 {
        let history: PayerHistory = env
            .storage()
            .persistent()
            .get(&DataKey::PayerHistory(payer))
            .unwrap_or(PayerHistory {
                total_scored: 0,
                total_blocked: 0,
                total_high: 0,
                total_medium: 0,
                score_sum: 0,
                last_scored_at: 0,
            });

        if history.total_scored == 0 {
            return 0;
        }
        (history.score_sum / history.total_scored as u64) as u32
    }

    /// Returns `true` if the address is on the blocklist.
    pub fn is_blocklisted(env: Env, address: Address) -> bool {
        env.storage()
            .persistent()
            .get::<DataKey, bool>(&DataKey::Blocklist(address))
            .unwrap_or(false)
    }

    /// Returns `true` if the address is an authorised scorer.
    pub fn is_scorer(env: Env, scorer: Address) -> bool {
        env.storage()
            .persistent()
            .get::<DataKey, bool>(&DataKey::Scorer(scorer))
            .unwrap_or(false)
    }

    pub fn get_weights(env: Env) -> ScoringWeights {
        load_weights(&env)
    }

    pub fn get_admin(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&KEY_ADMIN)
            .unwrap_or_else(|| panic_with_error!(env, ComplianceError::NotInitialized))
    }

    pub fn is_paused(env: Env) -> bool {
        env.storage()
            .instance()
            .get::<Symbol, bool>(&KEY_PAUSED)
            .unwrap_or(false)
    }

    pub fn get_version(env: Env) -> u32 {
        env.storage().instance().get(&KEY_VERSION).unwrap_or(1)
    }
}

mod test;
