#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Events},
    token::StellarAssetClient,
    Address, Env, Symbol, TryFromVal,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_token(env: &Env, admin: &Address) -> Address {
    env.register_stellar_asset_contract_v2(admin.clone()).address()
}

fn mint(env: &Env, token: &Address, to: &Address, amount: i128) {
    StellarAssetClient::new(env, token).mint(to, &amount);
}

fn token_balance(env: &Env, token: &Address, who: &Address) -> i128 {
    TokenClient::new(env, token).balance(who)
}

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

struct Setup {
    env: Env,
    client: PaymentInsuranceClient<'static>,
    admin: Address,
    token: Address,
    underwriter: Address,
    holder: Address,
}

impl Setup {
    fn new() -> Self {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let token = make_token(&env, &admin);

        let underwriter = Address::generate(&env);
        let holder = Address::generate(&env);

        let contract_id = env.register_contract(None, PaymentInsurance);
        let client = PaymentInsuranceClient::new(&env, &contract_id);

        client.initialize(&admin, &token);

        let client: PaymentInsuranceClient<'static> = unsafe { core::mem::transmute(client) };

        Setup { env, client, admin, token, underwriter, holder }
    }
}

fn has_event(env: &Env, t0: &str, t1: &str) -> bool {
    let events = env.events().all();
    events.iter().any(|(_, topics, _)| {
        if topics.len() != 2 { return false; }
        let a = <Symbol as TryFromVal<Env, _>>::try_from_val(env, &topics.get(0).unwrap());
        let b = <Symbol as TryFromVal<Env, _>>::try_from_val(env, &topics.get(1).unwrap());
        matches!((a, b), (Ok(x), Ok(y))
            if x == Symbol::new(env, t0) && y == Symbol::new(env, t1))
    })
}

// ---------------------------------------------------------------------------
// Initialisation tests
// ---------------------------------------------------------------------------

#[test]
fn test_initialize_stores_config() {
    let s = Setup::new();
    assert_eq!(s.client.get_reserve(), 0);
    assert!(s.client.get_underwriters().is_empty());
    assert_eq!(s.client.get_total_stakes(), 0);
}

#[test]
fn test_initialize_emits_event() {
    let s = Setup::new();
    assert!(has_event(&s.env, "pay_ins", "init"));
}

#[test]
fn test_initialize_twice_fails() {
    let s = Setup::new();
    let result = s.client.try_initialize(&s.admin, &s.token);
    assert_eq!(result, Err(Ok(Error::AlreadyInitialized)));
}

// ---------------------------------------------------------------------------
// Underwriter management tests
// ---------------------------------------------------------------------------

#[test]
fn test_add_underwriter_increases_reserve() {
    let s = Setup::new();
    mint(&s.env, &s.token, &s.underwriter, 5000);
    s.client.add_underwriter(&s.underwriter, &1000);
    assert_eq!(s.client.get_reserve(), 1000);
    assert_eq!(s.client.get_total_stakes(), 1000);
    assert_eq!(token_balance(&s.env, &s.token, &s.underwriter), 4000);
}

#[test]
fn test_add_underwriter_emits_event() {
    let s = Setup::new();
    mint(&s.env, &s.token, &s.underwriter, 1000);
    s.client.add_underwriter(&s.underwriter, &1000);
    assert!(has_event(&s.env, "pay_ins", "und_added"));
}

#[test]
fn test_add_underwriter_zero_fails() {
    let s = Setup::new();
    let result = s.client.try_add_underwriter(&s.underwriter, &0);
    assert_eq!(result, Err(Ok(Error::ZeroAmount)));
}

#[test]
fn test_remove_underwriter_returns_stake() {
    let s = Setup::new();
    mint(&s.env, &s.token, &s.underwriter, 5000);
    s.client.add_underwriter(&s.underwriter, &1000);
    assert_eq!(token_balance(&s.env, &s.token, &s.underwriter), 4000);

    let stake = s.client.remove_underwriter(&s.underwriter);
    assert_eq!(stake, 1000);
    assert_eq!(token_balance(&s.env, &s.token, &s.underwriter), 5000);
    assert_eq!(s.client.get_reserve(), 0);
    assert_eq!(s.client.get_total_stakes(), 0);
}

#[test]
fn test_remove_underwriter_not_found_fails() {
    let s = Setup::new();
    let unknown = Address::generate(&s.env);
    let result = s.client.try_remove_underwriter(&unknown);
    assert_eq!(result, Err(Ok(Error::NotUnderwriter)));
}

#[test]
fn test_add_underwriter_to_existing_stake_adds() {
    let s = Setup::new();
    mint(&s.env, &s.token, &s.underwriter, 5000);
    s.client.add_underwriter(&s.underwriter, &1000);
    s.client.add_underwriter(&s.underwriter, &500);
    assert_eq!(s.client.get_underwriters().get(s.underwriter.clone()).unwrap(), 1500);
    assert_eq!(s.client.get_total_stakes(), 1500);
}

// ---------------------------------------------------------------------------
// Risk Profile Registry tests
// ---------------------------------------------------------------------------

#[test]
fn test_create_risk_profile() {
    let s = Setup::new();
    let id = s.client.create_risk_profile(&s.admin, &Symbol::new(&s.env, "LowRisk"), &500);
    assert_eq!(id, 1);

    let profile = s.client.get_risk_profile(&1);
    assert_eq!(profile.id, 1);
    assert_eq!(profile.name, Symbol::new(&s.env, "LowRisk"));
    assert_eq!(profile.risk_bps, 500);
    assert!(profile.active);
}

#[test]
fn test_create_risk_profile_underwriter_authorized() {
    let s = Setup::new();
    mint(&s.env, &s.token, &s.underwriter, 5000);
    s.client.add_underwriter(&s.underwriter, &1000);

    let id = s.client.create_risk_profile(&s.underwriter, &Symbol::new(&s.env, "MedRisk"), &1000);
    assert_eq!(id, 1);
}

#[test]
fn test_create_risk_profile_unauthorized_fails() {
    let s = Setup::new();
    let result = s.client.try_create_risk_profile(&s.holder, &Symbol::new(&s.env, "NoRisk"), &500);
    assert_eq!(result, Err(Ok(Error::Unauthorized)));
}

#[test]
fn test_create_risk_profile_invalid_bps_fails() {
    let s = Setup::new();
    let result = s.client.try_create_risk_profile(&s.admin, &Symbol::new(&s.env, "TooHigh"), &15000);
    assert_eq!(result, Err(Ok(Error::InvalidBps)));
}

#[test]
fn test_toggle_risk_profile() {
    let s = Setup::new();
    let id = s.client.create_risk_profile(&s.admin, &Symbol::new(&s.env, "LowRisk"), &500);
    s.client.toggle_risk_profile(&s.admin, &id, &false);

    let profile = s.client.get_risk_profile(&id);
    assert!(!profile.active);
}

// ---------------------------------------------------------------------------
// Premium calculation tests
// ---------------------------------------------------------------------------

#[test]
fn test_calculate_premium_basic() {
    let s = Setup::new();
    let id = s.client.create_risk_profile(&s.admin, &Symbol::new(&s.env, "LowRisk"), &500);
    let premium = s.client.calculate_premium(&10000, &id);
    assert_eq!(premium, 500);
}

#[test]
fn test_calculate_premium_zero_coverage() {
    let s = Setup::new();
    let id = s.client.create_risk_profile(&s.admin, &Symbol::new(&s.env, "LowRisk"), &500);
    let premium = s.client.calculate_premium(&0, &id);
    assert_eq!(premium, 0);
}

#[test]
fn test_calculate_premium_100_percent_risk() {
    let s = Setup::new();
    let id = s.client.create_risk_profile(&s.admin, &Symbol::new(&s.env, "MaxRisk"), &10000);
    let premium = s.client.calculate_premium(&1000, &id);
    assert_eq!(premium, 1000);
}

#[test]
fn test_calculate_premium_inactive_profile_fails() {
    let s = Setup::new();
    let id = s.client.create_risk_profile(&s.admin, &Symbol::new(&s.env, "Low"), &500);
    s.client.toggle_risk_profile(&s.admin, &id, &false);

    let result = s.client.try_calculate_premium(&10000, &id);
    assert_eq!(result, Err(Ok(Error::RiskProfileInactive)));
}

#[test]
fn test_calculate_premium_nonexistent_fails() {
    let s = Setup::new();
    let result = s.client.try_calculate_premium(&10000, &999);
    assert_eq!(result, Err(Ok(Error::RiskProfileNotFound)));
}

// ---------------------------------------------------------------------------
// Buy policy tests
// ---------------------------------------------------------------------------

#[test]
fn test_buy_policy_creates_policy_and_splits_premium() {
    let s = Setup::new();
    let id = s.client.create_risk_profile(&s.admin, &Symbol::new(&s.env, "LowRisk"), &500);

    mint(&s.env, &s.token, &s.holder, 1000);
    let policy_id = s.client.buy_policy(&s.holder, &10000, &id);

    let policy = s.client.get_policy(&policy_id);
    assert_eq!(policy.holder, s.holder);
    assert_eq!(policy.coverage_amount, 10000);
    assert_eq!(policy.premium_paid, 500);
    assert!(policy.active);

    // Premium is 500. 10% admin fee = 50, 90% reserve share = 450.
    // Holder balance should be 1000 - 500 = 500
    assert_eq!(token_balance(&s.env, &s.token, &s.holder), 500);
    // Reserve should be 450
    assert_eq!(s.client.get_reserve(), 450);
    // Admin should receive 50 fee
    assert_eq!(token_balance(&s.env, &s.token, &s.admin), 50);
}

#[test]
fn test_buy_policy_mints_incrementing_ids() {
    let s = Setup::new();
    let id = s.client.create_risk_profile(&s.admin, &Symbol::new(&s.env, "Low"), &500);
    mint(&s.env, &s.token, &s.holder, 2000);
    let id1 = s.client.buy_policy(&s.holder, &1000, &id);
    let id2 = s.client.buy_policy(&s.holder, &1000, &id);
    assert_eq!(id2, id1 + 1);
}

#[test]
fn test_buy_policy_zero_coverage_fails() {
    let s = Setup::new();
    let id = s.client.create_risk_profile(&s.admin, &Symbol::new(&s.env, "LowRisk"), &500);
    let result = s.client.try_buy_policy(&s.holder, &0, &id);
    assert_eq!(result, Err(Ok(Error::ZeroAmount)));
}

#[test]
fn test_buy_policy_emits_event() {
    let s = Setup::new();
    let id = s.client.create_risk_profile(&s.admin, &Symbol::new(&s.env, "Low"), &500);
    mint(&s.env, &s.token, &s.holder, 1000);
    s.client.buy_policy(&s.holder, &10000, &id);
    assert!(has_event(&s.env, "pay_ins", "p_bought"));
}

// ---------------------------------------------------------------------------
// Claim tests
// ---------------------------------------------------------------------------

#[test]
fn test_submit_claim_marks_pending() {
    let s = Setup::new();
    let id = s.client.create_risk_profile(&s.admin, &Symbol::new(&s.env, "LowRisk"), &500);
    
    mint(&s.env, &s.token, &s.holder, 1000);
    let policy_id = s.client.buy_policy(&s.holder, &10000, &id);

    let evidence = Symbol::new(&s.env, "evidence_hash_123");
    s.client.submit_claim(&policy_id, &evidence);

    let policy = s.client.get_policy(&policy_id);
    assert!(matches!(policy.claim_status, PolicyStatus::ClaimPending));
    assert_eq!(policy.evidence_hash, evidence);
}

#[test]
fn test_submit_claim_emits_event() {
    let s = Setup::new();
    let id = s.client.create_risk_profile(&s.admin, &Symbol::new(&s.env, "Low"), &500);
    mint(&s.env, &s.token, &s.holder, 1000);
    let policy_id = s.client.buy_policy(&s.holder, &10000, &id);
    s.client.submit_claim(&policy_id, &Symbol::new(&s.env, "ev"));
    assert!(has_event(&s.env, "pay_ins", "c_sub"));
}

#[test]
fn test_process_claim_approve_transfers_coverage() {
    let s = Setup::new();
    let id = s.client.create_risk_profile(&s.admin, &Symbol::new(&s.env, "LowRisk"), &500);

    // Underwriter adds 50000 stake to reserve
    mint(&s.env, &s.token, &s.underwriter, 60000);
    s.client.add_underwriter(&s.underwriter, &50000);

    // Holder buys policy. Premium = 500. Reserve increases by 450 (90%). Total reserve = 50450.
    mint(&s.env, &s.token, &s.holder, 1000);
    let policy_id = s.client.buy_policy(&s.holder, &10000, &id);
    assert_eq!(token_balance(&s.env, &s.token, &s.holder), 500);

    // Submit claim and approve
    let evidence = Symbol::new(&s.env, "ev");
    s.client.submit_claim(&policy_id, &evidence);
    s.client.process_claim(&policy_id, &true, &10000);

    let policy = s.client.get_policy(&policy_id);
    assert!(!policy.active);
    assert!(matches!(policy.claim_status, PolicyStatus::ClaimApproved));
    
    // Holder balance becomes 500 + 10000 = 10500
    assert_eq!(token_balance(&s.env, &s.token, &s.holder), 10500);
    // Reserve becomes 50450 - 10000 = 40450
    assert_eq!(s.client.get_reserve(), 40450);
}

#[test]
fn test_process_claim_approve_decreases_reserve() {
    let s = Setup::new();
    let id = s.client.create_risk_profile(&s.admin, &Symbol::new(&s.env, "Low"), &500);
    
    mint(&s.env, &s.token, &s.underwriter, 60000);
    s.client.add_underwriter(&s.underwriter, &50000);
    
    mint(&s.env, &s.token, &s.holder, 1000);
    let policy_id = s.client.buy_policy(&s.holder, &10000, &id);
    
    s.client.submit_claim(&policy_id, &Symbol::new(&s.env, "ev"));
    s.client.process_claim(&policy_id, &true, &10000);
    
    assert_eq!(s.client.get_reserve(), 40450); // 50000 stake + 450 premium share - 10000 payout
}

#[test]
fn test_process_claim_approve_emits_event() {
    let s = Setup::new();
    let id = s.client.create_risk_profile(&s.admin, &Symbol::new(&s.env, "Low"), &500);
    
    mint(&s.env, &s.token, &s.underwriter, 60000);
    s.client.add_underwriter(&s.underwriter, &50000);
    
    mint(&s.env, &s.token, &s.holder, 1000);
    let policy_id = s.client.buy_policy(&s.holder, &10000, &id);
    
    s.client.submit_claim(&policy_id, &Symbol::new(&s.env, "ev"));
    s.client.process_claim(&policy_id, &true, &10000);
    
    assert!(has_event(&s.env, "pay_ins", "c_appr"));
}

#[test]
fn test_process_claim_deny_marks_denied() {
    let s = Setup::new();
    let id = s.client.create_risk_profile(&s.admin, &Symbol::new(&s.env, "Low"), &500);

    mint(&s.env, &s.token, &s.holder, 1000);
    let policy_id = s.client.buy_policy(&s.holder, &10000, &id);
    
    s.client.submit_claim(&policy_id, &Symbol::new(&s.env, "ev"));
    s.client.process_claim(&policy_id, &false, &10000);

    let policy = s.client.get_policy(&policy_id);
    assert!(matches!(policy.claim_status, PolicyStatus::ClaimDenied));
    assert!(policy.active); // Policy is still active if denied
}

#[test]
fn test_process_claim_deny_emits_event() {
    let s = Setup::new();
    let id = s.client.create_risk_profile(&s.admin, &Symbol::new(&s.env, "Low"), &500);
    mint(&s.env, &s.token, &s.holder, 1000);
    let policy_id = s.client.buy_policy(&s.holder, &10000, &id);
    s.client.submit_claim(&policy_id, &Symbol::new(&s.env, "ev"));
    s.client.process_claim(&policy_id, &false, &10000);
    assert!(has_event(&s.env, "pay_ins", "c_den"));
}

#[test]
fn test_process_claim_insufficient_reserve_fails() {
    let s = Setup::new();
    let id = s.client.create_risk_profile(&s.admin, &Symbol::new(&s.env, "Low"), &500);

    // Premium goes to reserve (450), but coverage is 10000
    mint(&s.env, &s.token, &s.holder, 1000);
    let policy_id = s.client.buy_policy(&s.holder, &10000, &id);

    s.client.submit_claim(&policy_id, &Symbol::new(&s.env, "ev"));
    let result = s.client.try_process_claim(&policy_id, &true, &10000);
    assert_eq!(result, Err(Ok(Error::InsufficientReserve)));
}

#[test]
fn test_submit_claim_nonexistent_policy_fails() {
    let s = Setup::new();
    let result = s.client.try_submit_claim(&999, &Symbol::new(&s.env, "ev"));
    assert_eq!(result, Err(Ok(Error::PolicyNotFound)));
}

#[test]
fn test_process_claim_nonexistent_policy_fails() {
    let s = Setup::new();
    let result = s.client.try_process_claim(&999, &true, &10000);
    assert_eq!(result, Err(Ok(Error::PolicyNotFound)));
}

#[test]
fn test_get_policy_nonexistent_fails() {
    let s = Setup::new();
    let result = s.client.try_get_policy(&999);
    assert_eq!(result, Err(Ok(Error::PolicyNotFound)));
}

// ---------------------------------------------------------------------------
// Proportional Loss-Sharing tests
// ---------------------------------------------------------------------------

#[test]
fn test_proportional_loss_sharing_underwriter_withdrawal() {
    let s = Setup::new();
    let id = s.client.create_risk_profile(&s.admin, &Symbol::new(&s.env, "LowRisk"), &500);

    // Mint underwriter stakes and add them
    mint(&s.env, &s.token, &s.underwriter, 50000);
    s.client.add_underwriter(&s.underwriter, &50000); // Reserve = 50000

    let underwriter2 = Address::generate(&s.env);
    mint(&s.env, &s.token, &underwriter2, 50000);
    s.client.add_underwriter(&underwriter2, &50000); // Reserve = 100000

    // Holder buys policy. Premium = 5000. 10% fee = 500, 90% reserve share = 4500.
    // Total reserve = 104500. Total stakes = 100000.
    mint(&s.env, &s.token, &s.holder, 6000);
    let policy_id = s.client.buy_policy(&s.holder, &100000, &id);

    // A large claim of 50000 is approved and paid out
    s.client.submit_claim(&policy_id, &Symbol::new(&s.env, "ev"));
    s.client.process_claim(&policy_id, &true, &50000);

    // Payout reduces reserve from 104500 to 54500.
    // Reserve (54500) < Total Stakes (100000). Underwriters share the loss!
    // Underwriter 1 withdraws. Their share is 50000 / 100000 = 50%.
    // Withdrawable = 50% * 54500 = 27250.
    let withdrawn = s.client.remove_underwriter(&s.underwriter);
    assert_eq!(withdrawn, 27250);
    assert_eq!(token_balance(&s.env, &s.token, &s.underwriter), 27250);
}