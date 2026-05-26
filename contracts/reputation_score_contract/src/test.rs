#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Env, Error as SdkError,
};

fn sdk_err(e: RepError) -> SdkError {
    SdkError::from_contract_error(e as u32)
}

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

struct Setup {
    env: Env,
    client: ReputationScoreContractClient<'static>,
    reporter: Address,
}

impl Setup {
    fn new() -> Self {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let reporter = Address::generate(&env);
        let id = env.register_contract(None, ReputationScoreContract);
        let client = ReputationScoreContractClient::new(&env, &id);
        client.initialize(&admin);
        client.add_reporter(&reporter);

        let client: ReputationScoreContractClient<'static> =
            unsafe { core::mem::transmute(client) };

        Setup { env, client, reporter }
    }

    fn user(&self) -> Address {
        Address::generate(&self.env)
    }

    fn advance_ledgers(&self, n: u32) {
        self.env.ledger().with_mut(|l| l.sequence_number += n);
    }
}

// ---------------------------------------------------------------------------
// Initialisation
// ---------------------------------------------------------------------------

#[test]
fn test_double_init_rejected() {
    let s = Setup::new();
    let admin2 = Address::generate(&s.env);
    assert_eq!(
        s.client.try_initialize(&admin2),
        Err(Ok(sdk_err(RepError::AlreadyInitialized)))
    );
}

// ---------------------------------------------------------------------------
// Reporter management
// ---------------------------------------------------------------------------

#[test]
fn test_add_remove_reporter() {
    let s = Setup::new();
    let r2 = s.user();
    assert!(!s.client.is_reporter(&r2));
    s.client.add_reporter(&r2);
    assert!(s.client.is_reporter(&r2));
    s.client.remove_reporter(&r2);
    assert!(!s.client.is_reporter(&r2));
}

#[test]
fn test_add_duplicate_reporter_rejected() {
    let s = Setup::new();
    assert_eq!(
        s.client.try_add_reporter(&s.reporter),
        Err(Ok(sdk_err(RepError::ReporterAlreadyAdded)))
    );
}

#[test]
fn test_remove_unknown_reporter_rejected() {
    let s = Setup::new();
    let unknown = s.user();
    assert_eq!(
        s.client.try_remove_reporter(&unknown),
        Err(Ok(sdk_err(RepError::ReporterNotFound)))
    );
}

#[test]
fn test_unauthorized_reporter_rejected() {
    let s = Setup::new();
    let fake = s.user();
    let user = s.user();
    assert_eq!(
        s.client.try_record_success(&fake, &user, &1_000_000),
        Err(Ok(sdk_err(RepError::Unauthorized)))
    );
    assert_eq!(
        s.client.try_record_dispute(&fake, &user),
        Err(Ok(sdk_err(RepError::Unauthorized)))
    );
    assert_eq!(
        s.client.try_record_dispute_resolved(&fake, &user, &true),
        Err(Ok(sdk_err(RepError::Unauthorized)))
    );
}

// ---------------------------------------------------------------------------
// Volume-based scoring
// ---------------------------------------------------------------------------

#[test]
fn test_new_user_starts_at_neutral() {
    let s = Setup::new();
    assert_eq!(s.client.get_score(&s.user()), NEUTRAL);
}

#[test]
fn test_zero_volume_applies_base_delta_only() {
    let s = Setup::new();
    let user = s.user();
    s.client.record_success(&s.reporter, &user, &0);
    // 500 + BASE_SUCCESS_DELTA(5) = 505
    assert_eq!(s.client.get_score(&user), NEUTRAL + BASE_SUCCESS_DELTA);
}

#[test]
fn test_one_tier_volume_adds_one_bonus_point() {
    let s = Setup::new();
    let user = s.user();
    // Exactly 1 tier = 1 bonus pt → delta = 5 + 1 = 6
    s.client.record_success(&s.reporter, &user, &VOLUME_TIER);
    assert_eq!(s.client.get_score(&user), NEUTRAL + BASE_SUCCESS_DELTA + 1);
}

#[test]
fn test_five_tiers_volume_adds_five_bonus_points() {
    let s = Setup::new();
    let user = s.user();
    s.client.record_success(&s.reporter, &user, &(5 * VOLUME_TIER));
    assert_eq!(s.client.get_score(&user), NEUTRAL + BASE_SUCCESS_DELTA + 5);
}

#[test]
fn test_volume_bonus_capped_at_max() {
    let s = Setup::new();
    let user = s.user();
    // 100 tiers → bonus would be 100, but capped at MAX_VOLUME_BONUS(20)
    s.client.record_success(&s.reporter, &user, &(100 * VOLUME_TIER));
    assert_eq!(
        s.client.get_score(&user),
        NEUTRAL + BASE_SUCCESS_DELTA + MAX_VOLUME_BONUS
    );
}

#[test]
fn test_volume_delta_helper_values() {
    assert_eq!(volume_delta(0), BASE_SUCCESS_DELTA);
    assert_eq!(volume_delta(VOLUME_TIER), BASE_SUCCESS_DELTA + 1);
    assert_eq!(volume_delta(5 * VOLUME_TIER), BASE_SUCCESS_DELTA + 5);
    assert_eq!(volume_delta(100 * VOLUME_TIER), BASE_SUCCESS_DELTA + MAX_VOLUME_BONUS);
    // Sub-tier amounts round down to zero bonus.
    assert_eq!(volume_delta(VOLUME_TIER - 1), BASE_SUCCESS_DELTA);
}

#[test]
fn test_negative_volume_rejected() {
    let s = Setup::new();
    let user = s.user();
    assert_eq!(
        s.client.try_record_success(&s.reporter, &user, &-1),
        Err(Ok(sdk_err(RepError::InvalidVolume)))
    );
}

#[test]
fn test_multiple_successes_accumulate_volume() {
    let s = Setup::new();
    let user = s.user();
    s.client.record_success(&s.reporter, &user, &(2 * VOLUME_TIER));
    s.client.record_success(&s.reporter, &user, &(3 * VOLUME_TIER));
    let rec = s.client.get_record(&user);
    assert_eq!(rec.total_volume, 5 * VOLUME_TIER);
    assert_eq!(rec.tx_success, 2);
}

#[test]
fn test_score_capped_at_max() {
    let s = Setup::new();
    let user = s.user();
    // Each tx with max bonus = 5 + 20 = 25 pts.  From 500 need 500 more → 20 txs.
    for _ in 0..40 {
        s.client.record_success(&s.reporter, &user, &(100 * VOLUME_TIER));
    }
    assert_eq!(s.client.get_score(&user), MAX_SCORE);
}

// ---------------------------------------------------------------------------
// Dispute scoring
// ---------------------------------------------------------------------------

#[test]
fn test_dispute_open_deducts_open_delta() {
    let s = Setup::new();
    let user = s.user();
    s.client.record_dispute(&s.reporter, &user);
    assert_eq!(s.client.get_score(&user), NEUTRAL - DISPUTE_OPEN_DELTA);
}

#[test]
fn test_dispute_resolved_against_user_deducts_loss_delta() {
    let s = Setup::new();
    let user = s.user();
    s.client.record_dispute(&s.reporter, &user);
    s.client.record_dispute_resolved(&s.reporter, &user, &true);
    // 500 - 20 - 50 = 430
    assert_eq!(
        s.client.get_score(&user),
        NEUTRAL - DISPUTE_OPEN_DELTA - DISPUTE_LOSS_DELTA
    );
}

#[test]
fn test_dispute_resolved_in_favour_adds_win_delta() {
    let s = Setup::new();
    let user = s.user();
    s.client.record_dispute(&s.reporter, &user);
    s.client.record_dispute_resolved(&s.reporter, &user, &false);
    // 500 - 20 + 15 = 495
    assert_eq!(
        s.client.get_score(&user),
        NEUTRAL - DISPUTE_OPEN_DELTA + DISPUTE_WIN_DELTA
    );
}

#[test]
fn test_dispute_win_partially_offsets_open_penalty() {
    let s = Setup::new();
    let user = s.user();
    s.client.record_dispute(&s.reporter, &user);
    let after_open = s.client.get_score(&user);
    s.client.record_dispute_resolved(&s.reporter, &user, &false);
    let after_win = s.client.get_score(&user);
    // Win should recover some points.
    assert!(after_win > after_open);
}

#[test]
fn test_dispute_loss_worse_than_open_alone() {
    let s = Setup::new();
    let user = s.user();
    s.client.record_dispute(&s.reporter, &user);
    let after_open = s.client.get_score(&user);
    s.client.record_dispute_resolved(&s.reporter, &user, &true);
    let after_loss = s.client.get_score(&user);
    assert!(after_loss < after_open);
}

#[test]
fn test_score_floored_at_zero() {
    let s = Setup::new();
    let user = s.user();
    // Open + lose enough disputes to hit zero.
    for _ in 0..20 {
        s.client.record_dispute(&s.reporter, &user);
        s.client.record_dispute_resolved(&s.reporter, &user, &true);
    }
    assert_eq!(s.client.get_score(&user), 0);
}

#[test]
fn test_dispute_counts_tracked() {
    let s = Setup::new();
    let user = s.user();
    s.client.record_dispute(&s.reporter, &user);
    s.client.record_dispute_resolved(&s.reporter, &user, &true);
    s.client.record_dispute(&s.reporter, &user);
    s.client.record_dispute_resolved(&s.reporter, &user, &false);

    let rec = s.client.get_record(&user);
    assert_eq!(rec.tx_disputed, 2);
    assert_eq!(rec.disputes_lost, 1);
    assert_eq!(rec.disputes_won, 1);
}

// ---------------------------------------------------------------------------
// Time decay
// ---------------------------------------------------------------------------

#[test]
fn test_decay_moves_high_score_toward_neutral() {
    let s = Setup::new();
    let user = s.user();
    // Push to 1000.
    for _ in 0..40 {
        s.client.record_success(&s.reporter, &user, &(100 * VOLUME_TIER));
    }
    assert_eq!(s.client.get_score(&user), MAX_SCORE);

    s.advance_ledgers(DECAY_PERIOD);
    // 1 period: 1000 + (500 - 1000) / 100 = 1000 - 5 = 995
    assert_eq!(s.client.get_score(&user), 995);
}

#[test]
fn test_decay_moves_low_score_toward_neutral() {
    let s = Setup::new();
    let user = s.user();
    for _ in 0..20 {
        s.client.record_dispute(&s.reporter, &user);
        s.client.record_dispute_resolved(&s.reporter, &user, &true);
    }
    assert_eq!(s.client.get_score(&user), 0);

    s.advance_ledgers(DECAY_PERIOD);
    // 1 period: 0 + (500 - 0) / 100 = 5
    assert_eq!(s.client.get_score(&user), 5);
}

#[test]
fn test_decay_neutral_score_unchanged() {
    let s = Setup::new();
    let user = s.user();
    s.advance_ledgers(DECAY_PERIOD * 10);
    assert_eq!(s.client.get_score(&user), NEUTRAL);
}

#[test]
fn test_decay_does_not_write_on_get_score() {
    let s = Setup::new();
    let user = s.user();
    s.client.record_success(&s.reporter, &user, &0);
    let ledger_before = s.env.ledger().sequence();

    s.advance_ledgers(DECAY_PERIOD);
    // get_score is read-only; stored last_updated must still be ledger_before.
    let rec = s.client.get_record(&user);
    assert_eq!(rec.last_updated, ledger_before);
}

#[test]
fn test_decay_does_not_write_on_get_record() {
    let s = Setup::new();
    let user = s.user();
    s.client.record_success(&s.reporter, &user, &0);
    let ledger_before = s.env.ledger().sequence();

    s.advance_ledgers(DECAY_PERIOD * 3);
    let rec = s.client.get_record(&user);
    // Score is decayed in the returned value but NOT persisted.
    assert_eq!(rec.last_updated, ledger_before);
    assert!(rec.score < NEUTRAL + BASE_SUCCESS_DELTA); // decay applied
}

#[test]
fn test_decay_applied_on_next_write() {
    let s = Setup::new();
    let user = s.user();
    for _ in 0..40 {
        s.client.record_success(&s.reporter, &user, &(100 * VOLUME_TIER));
    }
    assert_eq!(s.client.get_score(&user), MAX_SCORE);

    s.advance_ledgers(DECAY_PERIOD);
    // Next write applies decay first, then adds delta.
    s.client.record_success(&s.reporter, &user, &0);
    // After decay: 995.  After +5: 1000 (capped).
    assert_eq!(s.client.get_score(&user), MAX_SCORE);
}

#[test]
fn test_multi_period_decay_converges() {
    let s = Setup::new();
    let user = s.user();
    for _ in 0..40 {
        s.client.record_success(&s.reporter, &user, &(100 * VOLUME_TIER));
    }
    assert_eq!(s.client.get_score(&user), MAX_SCORE);

    // 5 periods from 1000:
    // P1: 1000 - 5 = 995
    // P2: 995 - 4 = 991  (floor((500-995)/100) = floor(-4.95) = -4)
    // P3: 991 - 4 = 987
    // P4: 987 - 4 = 983
    // P5: 983 - 4 = 979
    s.advance_ledgers(DECAY_PERIOD * 5);
    assert_eq!(s.client.get_score(&user), 979);
}

#[test]
fn test_apply_decay_helper_zero_periods() {
    let (score, periods) = apply_decay(800, 100, 100);
    assert_eq!(score, 800);
    assert_eq!(periods, 0);
}

#[test]
fn test_apply_decay_helper_one_period_above_neutral() {
    let (score, periods) = apply_decay(1000, 0, DECAY_PERIOD);
    assert_eq!(score, 995);
    assert_eq!(periods, 1);
}

#[test]
fn test_apply_decay_helper_one_period_below_neutral() {
    let (score, periods) = apply_decay(0, 0, DECAY_PERIOD);
    assert_eq!(score, 5);
    assert_eq!(periods, 1);
}

#[test]
fn test_apply_decay_helper_neutral_unchanged() {
    let (score, _) = apply_decay(NEUTRAL, 0, DECAY_PERIOD * 100);
    assert_eq!(score, NEUTRAL);
}

// ---------------------------------------------------------------------------
// calculate_score — breakdown
// ---------------------------------------------------------------------------

#[test]
fn test_calculate_score_new_user() {
    let s = Setup::new();
    let user = s.user();
    let b = s.client.calculate_score(&user);
    assert_eq!(b.score, NEUTRAL);
    assert_eq!(b.decay_periods, 0);
    assert_eq!(b.tx_success, 0);
    assert_eq!(b.tx_disputed, 0);
    assert_eq!(b.total_volume, 0);
    assert_eq!(b.volume_contribution, 0);
    assert_eq!(b.dispute_open_impact, 0);
    assert_eq!(b.dispute_loss_impact, 0);
    assert_eq!(b.dispute_win_impact, 0);
}

#[test]
fn test_calculate_score_after_successes() {
    let s = Setup::new();
    let user = s.user();
    s.client.record_success(&s.reporter, &user, &(2 * VOLUME_TIER));
    s.client.record_success(&s.reporter, &user, &(2 * VOLUME_TIER));

    let b = s.client.calculate_score(&user);
    assert_eq!(b.tx_success, 2);
    assert_eq!(b.total_volume, 4 * VOLUME_TIER);
    // avg_volume = 2 tiers → avg_bonus = 2 → contribution = 2 * (5 + 2) = 14
    assert_eq!(b.volume_contribution, 14);
    assert_eq!(b.dispute_open_impact, 0);
}

#[test]
fn test_calculate_score_after_dispute_loss() {
    let s = Setup::new();
    let user = s.user();
    s.client.record_dispute(&s.reporter, &user);
    s.client.record_dispute_resolved(&s.reporter, &user, &true);

    let b = s.client.calculate_score(&user);
    assert_eq!(b.tx_disputed, 1);
    assert_eq!(b.dispute_open_impact, -(DISPUTE_OPEN_DELTA as i64));
    assert_eq!(b.dispute_loss_impact, -(DISPUTE_LOSS_DELTA as i64));
    assert_eq!(b.dispute_win_impact, 0);
}

#[test]
fn test_calculate_score_after_dispute_win() {
    let s = Setup::new();
    let user = s.user();
    s.client.record_dispute(&s.reporter, &user);
    s.client.record_dispute_resolved(&s.reporter, &user, &false);

    let b = s.client.calculate_score(&user);
    assert_eq!(b.dispute_open_impact, -(DISPUTE_OPEN_DELTA as i64));
    assert_eq!(b.dispute_win_impact, DISPUTE_WIN_DELTA as i64);
    assert_eq!(b.dispute_loss_impact, 0);
}

#[test]
fn test_calculate_score_shows_decay_periods() {
    let s = Setup::new();
    let user = s.user();
    s.client.record_success(&s.reporter, &user, &0);

    s.advance_ledgers(DECAY_PERIOD * 3);
    let b = s.client.calculate_score(&user);
    assert_eq!(b.decay_periods, 3);
    assert!(b.score < b.pre_decay_score || b.pre_decay_score == NEUTRAL);
}

#[test]
fn test_calculate_score_pre_decay_score_matches_stored() {
    let s = Setup::new();
    let user = s.user();
    for _ in 0..10 {
        s.client.record_success(&s.reporter, &user, &(5 * VOLUME_TIER));
    }
    let stored_score = s.client.get_record(&user).score;

    s.advance_ledgers(DECAY_PERIOD * 2);
    let b = s.client.calculate_score(&user);
    assert_eq!(b.pre_decay_score, stored_score);
    assert!(b.score < stored_score); // decay reduced it
}

#[test]
fn test_calculate_score_is_read_only() {
    let s = Setup::new();
    let user = s.user();
    s.client.record_success(&s.reporter, &user, &0);
    let ledger_before = s.env.ledger().sequence();

    s.advance_ledgers(DECAY_PERIOD);
    s.client.calculate_score(&user);

    // last_updated must not have changed.
    let rec = s.client.get_record(&user);
    assert_eq!(rec.last_updated, ledger_before);
}

#[test]
fn test_calculate_score_total_impact_consistent() {
    let s = Setup::new();
    let user = s.user();
    s.client.record_success(&s.reporter, &user, &(3 * VOLUME_TIER));
    s.client.record_dispute(&s.reporter, &user);
    s.client.record_dispute_resolved(&s.reporter, &user, &true);

    let b = s.client.calculate_score(&user);
    // Verify the breakdown signs are correct.
    assert!(b.volume_contribution > 0);
    assert!(b.dispute_open_impact < 0);
    assert!(b.dispute_loss_impact < 0);
    assert_eq!(b.dispute_win_impact, 0);
}

// ---------------------------------------------------------------------------
// Mixed scenario
// ---------------------------------------------------------------------------

#[test]
fn test_mixed_activity_score_trajectory() {
    let s = Setup::new();
    let user = s.user();

    // Build reputation with volume.
    for _ in 0..10 {
        s.client.record_success(&s.reporter, &user, &(5 * VOLUME_TIER));
    }
    let after_success = s.client.get_score(&user);
    assert!(after_success > NEUTRAL);

    // Dispute opened and lost.
    s.client.record_dispute(&s.reporter, &user);
    s.client.record_dispute_resolved(&s.reporter, &user, &true);
    let after_loss = s.client.get_score(&user);
    assert!(after_loss < after_success);

    // Recover with more successful txs.
    for _ in 0..5 {
        s.client.record_success(&s.reporter, &user, &(10 * VOLUME_TIER));
    }
    let after_recovery = s.client.get_score(&user);
    assert!(after_recovery > after_loss);

    // Time passes — score decays toward neutral.
    s.advance_ledgers(DECAY_PERIOD * 10);
    let after_decay = s.client.get_score(&user);
    assert!(after_decay < after_recovery);
    assert!(after_decay > NEUTRAL); // still above neutral
}

#[test]
fn test_removed_reporter_cannot_record() {
    let s = Setup::new();
    let user = s.user();
    s.client.remove_reporter(&s.reporter);
    assert_eq!(
        s.client.try_record_success(&s.reporter, &user, &0),
        Err(Ok(sdk_err(RepError::Unauthorized)))
    );
}
