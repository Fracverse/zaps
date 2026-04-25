#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Env, Error as SdkError,
};

fn sdk_err(e: RepError) -> SdkError {
    SdkError::from_contract_error(e as u32)
}

fn setup() -> (
    Env,
    ReputationScoreContractClient<'static>,
    Address,
    Address,
) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let reporter = Address::generate(&env);
    let id = env.register_contract(None, ReputationScoreContract);
    let client = ReputationScoreContractClient::new(&env, &id);
    client.initialize(&admin);
    client.add_reporter(&reporter);
    let client: ReputationScoreContractClient<'static> = unsafe { core::mem::transmute(client) };
    (env, client, admin, reporter)
}

// ---------------------------------------------------------------------------
// Init
// ---------------------------------------------------------------------------

#[test]
fn test_double_init_rejected() {
    let (_, client, admin, _) = setup();
    assert_eq!(
        client.try_initialize(&admin),
        Err(Ok(sdk_err(RepError::AlreadyInitialized)))
    );
}

// ---------------------------------------------------------------------------
// Reporter management
// ---------------------------------------------------------------------------

#[test]
fn test_add_remove_reporter() {
    let (env, client, _, _) = setup();
    let r2 = Address::generate(&env);
    assert!(!client.is_reporter(&r2));
    client.add_reporter(&r2);
    assert!(client.is_reporter(&r2));
    client.remove_reporter(&r2);
    assert!(!client.is_reporter(&r2));
}

#[test]
fn test_add_reporter_duplicate_rejected() {
    let (_, client, _, reporter) = setup();
    assert_eq!(
        client.try_add_reporter(&reporter),
        Err(Ok(sdk_err(RepError::ReporterAlreadyAdded)))
    );
}

#[test]
fn test_remove_reporter_missing_rejected() {
    let (env, client, _, _) = setup();
    let unknown = Address::generate(&env);
    assert_eq!(
        client.try_remove_reporter(&unknown),
        Err(Ok(sdk_err(RepError::ReporterNotFound)))
    );
}

// ---------------------------------------------------------------------------
// Scoring
// ---------------------------------------------------------------------------

#[test]
fn test_new_user_starts_at_neutral() {
    let (env, client, _, _) = setup();
    let user = Address::generate(&env);
    assert_eq!(client.get_score(&user), 500);
}

#[test]
fn test_success_increases_score() {
    let (env, client, _, reporter) = setup();
    let user = Address::generate(&env);
    client.record_success(&reporter, &user);
    assert_eq!(client.get_score(&user), 510);
    client.record_success(&reporter, &user);
    assert_eq!(client.get_score(&user), 520);
}

#[test]
fn test_dispute_decreases_score() {
    let (env, client, _, reporter) = setup();
    let user = Address::generate(&env);
    client.record_dispute(&reporter, &user);
    assert_eq!(client.get_score(&user), 450);
}

#[test]
fn test_score_capped_at_max() {
    let (env, client, _, reporter) = setup();
    let user = Address::generate(&env);
    // 50 successes from 500 = 500 + 50*10 = 1000
    for _ in 0..60 {
        client.record_success(&reporter, &user);
    }
    assert_eq!(client.get_score(&user), 1000);
}

#[test]
fn test_score_floored_at_zero() {
    let (env, client, _, reporter) = setup();
    let user = Address::generate(&env);
    // 10 disputes from 500 = 500 - 10*50 = 0
    for _ in 0..15 {
        client.record_dispute(&reporter, &user);
    }
    assert_eq!(client.get_score(&user), 0);
}

#[test]
fn test_tx_counts_tracked() {
    let (env, client, _, reporter) = setup();
    let user = Address::generate(&env);
    client.record_success(&reporter, &user);
    client.record_success(&reporter, &user);
    client.record_dispute(&reporter, &user);
    let rec = client.get_record(&user);
    assert_eq!(rec.tx_success, 2);
    assert_eq!(rec.tx_disputed, 1);
}

// ---------------------------------------------------------------------------
// Unauthorized reporter
// ---------------------------------------------------------------------------

#[test]
fn test_unauthorized_reporter_rejected() {
    let (env, client, _, _) = setup();
    let fake_reporter = Address::generate(&env);
    let user = Address::generate(&env);
    assert_eq!(
        client.try_record_success(&fake_reporter, &user),
        Err(Ok(sdk_err(RepError::Unauthorized)))
    );
    assert_eq!(
        client.try_record_dispute(&fake_reporter, &user),
        Err(Ok(sdk_err(RepError::Unauthorized)))
    );
}

// ---------------------------------------------------------------------------
// Time decay
// ---------------------------------------------------------------------------

#[test]
fn test_decay_moves_high_score_toward_neutral() {
    let (env, client, _, reporter) = setup();
    let user = Address::generate(&env);

    // Push score to 1000
    for _ in 0..50 {
        client.record_success(&reporter, &user);
    }
    assert_eq!(client.get_score(&user), 1000);

    // Advance ledger by one decay period (17_280 ledgers)
    env.ledger().with_mut(|l| l.sequence_number += 17_280);

    // After 1 period: 1000 + (500 - 1000) / 100 = 1000 - 5 = 995
    assert_eq!(client.get_score(&user), 995);
}

#[test]
fn test_decay_moves_low_score_toward_neutral() {
    let (env, client, _, reporter) = setup();
    let user = Address::generate(&env);

    // Push score to 0
    for _ in 0..10 {
        client.record_dispute(&reporter, &user);
    }
    assert_eq!(client.get_score(&user), 0);

    env.ledger().with_mut(|l| l.sequence_number += 17_280);

    // After 1 period: 0 + (500 - 0) / 100 = 5
    assert_eq!(client.get_score(&user), 5);
}

#[test]
fn test_decay_neutral_score_unchanged() {
    let (env, client, _, _) = setup();
    let user = Address::generate(&env);

    // Score is neutral (500) — decay should not change it
    env.ledger().with_mut(|l| l.sequence_number += 17_280 * 10);
    assert_eq!(client.get_score(&user), 500);
}

#[test]
fn test_decay_does_not_write_on_get() {
    let (env, client, _, reporter) = setup();
    let user = Address::generate(&env);
    client.record_success(&reporter, &user);

    let ledger_before = env.ledger().sequence();
    env.ledger().with_mut(|l| l.sequence_number += 17_280);

    // get_score is read-only; stored last_updated should still be ledger_before
    let rec = client.get_record(&user);
    assert_eq!(rec.last_updated, ledger_before);
}

#[test]
fn test_multi_period_decay() {
    let (env, client, _, reporter) = setup();
    let user = Address::generate(&env);

    for _ in 0..50 {
        client.record_success(&reporter, &user);
    }
    assert_eq!(client.get_score(&user), 1000);

    // 5 periods
    env.ledger().with_mut(|l| l.sequence_number += 17_280 * 5);

    // Manual: 1000 → 995 → 990 → 985 → 980 → 975 (each period -5 at start, but delta changes)
    // Period 1: 1000 + (500-1000)/100 = 1000 - 5 = 995
    // Period 2: 995 + (500-995)/100 = 995 - 4 = 991  (integer: -4.95 → -4)
    // Period 3: 991 + (500-991)/100 = 991 - 4 = 987
    // Period 4: 987 + (500-987)/100 = 987 - 4 = 983
    // Period 5: 983 + (500-983)/100 = 983 - 4 = 979
    assert_eq!(client.get_score(&user), 979);
}

// ---------------------------------------------------------------------------
// Admin transfer
// ---------------------------------------------------------------------------

#[test]
fn test_transfer_admin() {
    let (env, client, _, _) = setup();
    let new_admin = Address::generate(&env);
    client.transfer_admin(&new_admin);
    // Old admin can no longer add reporters (new_admin is now admin)
    // Just verify the call succeeded without panic
}
