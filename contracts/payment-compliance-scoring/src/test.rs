#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Env, Error as SdkError,
};

fn sdk_err(e: ComplianceError) -> SdkError {
    SdkError::from_contract_error(e as u32)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn low_dims() -> DimensionScores {
    DimensionScores {
        velocity: 10,
        amount: 5,
        pattern: 0,
        counterparty: 5,
        geo: 10,
        history: 0,
    }
}

fn high_dims() -> DimensionScores {
    DimensionScores {
        velocity: 90,
        amount: 95,
        pattern: 80,
        counterparty: 85,
        geo: 70,
        history: 90,
    }
}

fn blocked_dims() -> DimensionScores {
    DimensionScores {
        velocity: 90,
        amount: 90,
        pattern: 90,
        counterparty: 90,
        geo: 90,
        history: 90,
    }
}

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

struct Setup {
    env: Env,
    client: PaymentComplianceScoringClient<'static>,
    scorer: Address,
}

impl Setup {
    fn new() -> Self {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let scorer = Address::generate(&env);

        let id = env.register_contract(None, PaymentComplianceScoring);
        let client = PaymentComplianceScoringClient::new(&env, &id);
        client.initialize(&admin);
        client.add_scorer(&scorer);

        let client: PaymentComplianceScoringClient<'static> =
            unsafe { core::mem::transmute(client) };

        Setup { env, client, scorer }
    }

    fn payer(&self) -> Address {
        Address::generate(&self.env)
    }
}

// ---------------------------------------------------------------------------
// Initialisation
// ---------------------------------------------------------------------------

#[test]
fn test_initialize() {
    let s = Setup::new();
    assert!(!s.client.is_paused());
    assert_eq!(s.client.get_version(), 1);
}

#[test]
fn test_double_initialize_rejected() {
    let s = Setup::new();
    let admin2 = Address::generate(&s.env);
    assert_eq!(
        s.client.try_initialize(&admin2),
        Err(Ok(sdk_err(ComplianceError::AlreadyInitialized)))
    );
}

// ---------------------------------------------------------------------------
// Scorer management
// ---------------------------------------------------------------------------

#[test]
fn test_add_remove_scorer() {
    let s = Setup::new();
    let new_scorer = s.payer();
    assert!(!s.client.is_scorer(&new_scorer));
    s.client.add_scorer(&new_scorer);
    assert!(s.client.is_scorer(&new_scorer));
    s.client.remove_scorer(&new_scorer);
    assert!(!s.client.is_scorer(&new_scorer));
}

#[test]
fn test_add_duplicate_scorer_rejected() {
    let s = Setup::new();
    assert_eq!(
        s.client.try_add_scorer(&s.scorer),
        Err(Ok(sdk_err(ComplianceError::ScorerAlreadyAdded)))
    );
}

#[test]
fn test_remove_unknown_scorer_rejected() {
    let s = Setup::new();
    let unknown = s.payer();
    assert_eq!(
        s.client.try_remove_scorer(&unknown),
        Err(Ok(sdk_err(ComplianceError::ScorerNotFound)))
    );
}

#[test]
fn test_unauthorized_scorer_rejected() {
    let s = Setup::new();
    let fake = s.payer();
    let payer = s.payer();
    assert_eq!(
        s.client
            .try_score_payment(&fake, &1u64, &payer, &1_000_000, &low_dims()),
        Err(Ok(sdk_err(ComplianceError::Unauthorized)))
    );
}

// ---------------------------------------------------------------------------
// Blocklist management
// ---------------------------------------------------------------------------

#[test]
fn test_add_remove_blocklist() {
    let s = Setup::new();
    let addr = s.payer();
    assert!(!s.client.is_blocklisted(&addr));
    s.client.add_to_blocklist(&addr);
    assert!(s.client.is_blocklisted(&addr));
    s.client.remove_from_blocklist(&addr);
    assert!(!s.client.is_blocklisted(&addr));
}

#[test]
fn test_add_duplicate_blocklist_rejected() {
    let s = Setup::new();
    let addr = s.payer();
    s.client.add_to_blocklist(&addr);
    assert_eq!(
        s.client.try_add_to_blocklist(&addr),
        Err(Ok(sdk_err(ComplianceError::AddressAlreadyBlocklisted)))
    );
}

#[test]
fn test_remove_unknown_blocklist_rejected() {
    let s = Setup::new();
    let addr = s.payer();
    assert_eq!(
        s.client.try_remove_from_blocklist(&addr),
        Err(Ok(sdk_err(ComplianceError::AddressNotBlocklisted)))
    );
}

#[test]
fn test_blocklisted_payer_gets_max_score() {
    let s = Setup::new();
    let payer = s.payer();
    s.client.add_to_blocklist(&payer);

    // Even with low dimension scores, blocklisted → overall = 100, Blocked.
    let score = s
        .client
        .score_payment(&s.scorer, &1u64, &payer, &500_000, &low_dims());

    assert_eq!(score.overall_score, MAX_SCORE);
    assert_eq!(score.risk_level, RiskLevel::Blocked);
    assert!(score.blocklisted);
}

// ---------------------------------------------------------------------------
// Scoring
// ---------------------------------------------------------------------------

#[test]
fn test_low_risk_score() {
    let s = Setup::new();
    let payer = s.payer();
    let score = s
        .client
        .score_payment(&s.scorer, &1u64, &payer, &100_000, &low_dims());

    // (10+5+0+5+10+0)/6 = 5 → Low
    assert!(score.overall_score < MEDIUM_THRESHOLD);
    assert_eq!(score.risk_level, RiskLevel::Low);
    assert!(!score.blocklisted);
}

#[test]
fn test_high_risk_score() {
    let s = Setup::new();
    let payer = s.payer();
    let score = s
        .client
        .score_payment(&s.scorer, &2u64, &payer, &5_000_000, &high_dims());

    // (90+95+80+85+70+90)/6 = 85 → Blocked
    assert!(score.overall_score >= HIGH_THRESHOLD);
    assert!(matches!(
        score.risk_level,
        RiskLevel::High | RiskLevel::Blocked
    ));
}

#[test]
fn test_blocked_risk_score() {
    let s = Setup::new();
    let payer = s.payer();
    let score = s
        .client
        .score_payment(&s.scorer, &3u64, &payer, &10_000_000, &blocked_dims());

    assert!(score.overall_score >= BLOCKED_THRESHOLD);
    assert_eq!(score.risk_level, RiskLevel::Blocked);
}

#[test]
fn test_medium_risk_score() {
    let s = Setup::new();
    let payer = s.payer();
    let dims = DimensionScores {
        velocity: 40,
        amount: 35,
        pattern: 30,
        counterparty: 40,
        geo: 35,
        history: 30,
    };
    let score = s
        .client
        .score_payment(&s.scorer, &4u64, &payer, &200_000, &dims);

    // (40+35+30+40+35+30)/6 = 35 → Medium
    assert!(score.overall_score >= MEDIUM_THRESHOLD);
    assert!(score.overall_score < HIGH_THRESHOLD);
    assert_eq!(score.risk_level, RiskLevel::Medium);
}

#[test]
fn test_invalid_dimension_score_rejected() {
    let s = Setup::new();
    let payer = s.payer();
    let bad_dims = DimensionScores {
        velocity: 101, // > MAX_SCORE
        amount: 0,
        pattern: 0,
        counterparty: 0,
        geo: 0,
        history: 0,
    };
    assert_eq!(
        s.client
            .try_score_payment(&s.scorer, &5u64, &payer, &100_000, &bad_dims),
        Err(Ok(sdk_err(ComplianceError::InvalidScore)))
    );
}

#[test]
fn test_score_stored_and_retrievable() {
    let s = Setup::new();
    let payer = s.payer();
    let payment_id = 42u64;
    let score = s
        .client
        .score_payment(&s.scorer, &payment_id, &payer, &300_000, &low_dims());

    let retrieved = s.client.get_payment_score(&payment_id);
    assert_eq!(retrieved.payment_id, score.payment_id);
    assert_eq!(retrieved.overall_score, score.overall_score);
    assert_eq!(retrieved.risk_level, score.risk_level);
}

#[test]
fn test_get_nonexistent_payment_score_panics() {
    let s = Setup::new();
    assert_eq!(
        s.client.try_get_payment_score(&9999u64),
        Err(Ok(sdk_err(ComplianceError::PaymentNotFound)))
    );
}

// ---------------------------------------------------------------------------
// Payer history
// ---------------------------------------------------------------------------

#[test]
fn test_payer_history_accumulates() {
    let s = Setup::new();
    let payer = s.payer();

    s.client
        .score_payment(&s.scorer, &1u64, &payer, &100_000, &low_dims());
    s.client
        .score_payment(&s.scorer, &2u64, &payer, &200_000, &low_dims());

    let history = s.client.get_payer_history(&payer);
    assert_eq!(history.total_scored, 2);
}

#[test]
fn test_payer_history_blocked_count() {
    let s = Setup::new();
    let payer = s.payer();

    s.client
        .score_payment(&s.scorer, &1u64, &payer, &100_000, &blocked_dims());
    s.client
        .score_payment(&s.scorer, &2u64, &payer, &100_000, &low_dims());

    let history = s.client.get_payer_history(&payer);
    assert_eq!(history.total_blocked, 1);
    assert_eq!(history.total_scored, 2);
}

#[test]
fn test_payer_average_score() {
    let s = Setup::new();
    let payer = s.payer();

    let s1 = s
        .client
        .score_payment(&s.scorer, &1u64, &payer, &100_000, &low_dims());
    let s2 = s
        .client
        .score_payment(&s.scorer, &2u64, &payer, &100_000, &low_dims());

    let avg = s.client.get_payer_average_score(&payer);
    let expected = (s1.overall_score + s2.overall_score) / 2;
    // Allow ±1 for integer division rounding.
    assert!(avg.abs_diff(expected) <= 1);
}

#[test]
fn test_new_payer_average_score_is_zero() {
    let s = Setup::new();
    let payer = s.payer();
    assert_eq!(s.client.get_payer_average_score(&payer), 0);
}

// ---------------------------------------------------------------------------
// Weight configuration
// ---------------------------------------------------------------------------

#[test]
fn test_custom_weights_change_overall_score() {
    let s = Setup::new();
    let payer1 = s.payer();
    let payer2 = s.payer();

    let dims = DimensionScores {
        velocity: 80,
        amount: 10,
        pattern: 10,
        counterparty: 10,
        geo: 10,
        history: 10,
    };

    // Default equal weights: (80+10+10+10+10+10)/6 = 23
    let score_default = s
        .client
        .score_payment(&s.scorer, &1u64, &payer1, &100_000, &dims);

    // Heavy velocity weight: velocity dominates → higher overall
    let heavy_velocity = ScoringWeights {
        velocity: 10,
        amount: 1,
        pattern: 1,
        counterparty: 1,
        geo: 1,
        history: 1,
    };
    s.client.set_weights(&heavy_velocity);

    let score_heavy = s
        .client
        .score_payment(&s.scorer, &2u64, &payer2, &100_000, &dims);

    assert!(score_heavy.overall_score > score_default.overall_score);
}

#[test]
fn test_zero_weight_rejected() {
    let s = Setup::new();
    let bad_weights = ScoringWeights {
        velocity: 0,
        amount: 1,
        pattern: 1,
        counterparty: 1,
        geo: 1,
        history: 1,
    };
    assert_eq!(
        s.client.try_set_weights(&bad_weights),
        Err(Ok(sdk_err(ComplianceError::InvalidWeight)))
    );
}

// ---------------------------------------------------------------------------
// Compute helpers (pure functions)
// ---------------------------------------------------------------------------

#[test]
fn test_compute_overall_equal_weights() {
    let dims = DimensionScores {
        velocity: 60,
        amount: 60,
        pattern: 60,
        counterparty: 60,
        geo: 60,
        history: 60,
    };
    let weights = ScoringWeights::default_weights();
    assert_eq!(compute_overall(&dims, &weights), 60);
}

#[test]
fn test_compute_overall_mixed() {
    let dims = DimensionScores {
        velocity: 0,
        amount: 100,
        pattern: 0,
        counterparty: 0,
        geo: 0,
        history: 0,
    };
    let weights = ScoringWeights::default_weights();
    // (0+100+0+0+0+0)/6 = 16
    assert_eq!(compute_overall(&dims, &weights), 16);
}

#[test]
fn test_score_to_risk_level_boundaries() {
    assert_eq!(score_to_risk_level(0), RiskLevel::Low);
    assert_eq!(score_to_risk_level(29), RiskLevel::Low);
    assert_eq!(score_to_risk_level(30), RiskLevel::Medium);
    assert_eq!(score_to_risk_level(59), RiskLevel::Medium);
    assert_eq!(score_to_risk_level(60), RiskLevel::High);
    assert_eq!(score_to_risk_level(84), RiskLevel::High);
    assert_eq!(score_to_risk_level(85), RiskLevel::Blocked);
    assert_eq!(score_to_risk_level(100), RiskLevel::Blocked);
}

// ---------------------------------------------------------------------------
// Circuit breaker
// ---------------------------------------------------------------------------

#[test]
fn test_pause_blocks_scoring() {
    let s = Setup::new();
    s.client.pause();
    let payer = s.payer();
    assert_eq!(
        s.client
            .try_score_payment(&s.scorer, &1u64, &payer, &100_000, &low_dims()),
        Err(Ok(sdk_err(ComplianceError::ContractPaused)))
    );
}

#[test]
fn test_unpause_restores_scoring() {
    let s = Setup::new();
    s.client.pause();
    s.client.unpause();
    let payer = s.payer();
    // Should not panic.
    s.client
        .score_payment(&s.scorer, &1u64, &payer, &100_000, &low_dims());
}

// ---------------------------------------------------------------------------
// Admin transfer
// ---------------------------------------------------------------------------

#[test]
fn test_transfer_admin() {
    let s = Setup::new();
    let new_admin = s.payer();
    s.client.transfer_admin(&new_admin);
    assert_eq!(s.client.get_admin(), new_admin);
}
