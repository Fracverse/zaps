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
    let stake = s.client.remove_underwriter(&s.underwriter);
    assert_eq!(stake, 1000);
}

#[test]
fn test_remove_underwriter_decreases_reserve() {
    let s = Setup::new();
    mint(&s.env, &s.token, &s.underwriter, 5000);
    s.client.add_underwriter(&s.underwriter, &1000);
    s.client.remove_underwriter(&s.underwriter);
    assert_eq!(s.client.get_reserve(), 0);
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
}

// ---------------------------------------------------------------------------
// Premium calculation tests
// ---------------------------------------------------------------------------

#[test]
fn test_calculate_premium_basic() {
    let s = Setup::new();
    let premium = s.client.calculate_premium(&10000, &500);
    assert_eq!(premium, 500);
}

#[test]
fn test_calculate_premium_zero_coverage() {
    let s = Setup::new();
    let premium = s.client.calculate_premium(&0, &500);
    assert_eq!(premium, 0);
}

#[test]
fn test_calculate_premium_100_percent_risk() {
    let s = Setup::new();
    let premium = s.client.calculate_premium(&1000, &10000);
    assert_eq!(premium, 1000);
}

// ---------------------------------------------------------------------------
// Buy policy tests
// ---------------------------------------------------------------------------

#[test]
fn test_buy_policy_creates_policy() {
    let s = Setup::new();
    mint(&s.env, &s.token, &s.holder, 1000);
    let policy_id = s.client.buy_policy(&s.holder, &10000, &500);
    let policy = s.client.get_policy(&policy_id);
    assert_eq!(policy.holder, s.holder);
    assert_eq!(policy.coverage_amount, 10000);
    assert_eq!(policy.premium_paid, 500);
    assert!(policy.active);
}

#[test]
fn test_buy_policy_increases_reserve() {
    let s = Setup::new();
    mint(&s.env, &s.token, &s.holder, 1000);
    s.client.buy_policy(&s.holder, &10000, &500);
    assert_eq!(s.client.get_reserve(), 500);
}

#[test]
fn test_buy_policy_mints_incrementing_ids() {
    let s = Setup::new();
    mint(&s.env, &s.token, &s.holder, 1000);
    let id1 = s.client.buy_policy(&s.holder, &1000, &100);
    let id2 = s.client.buy_policy(&s.holder, &1000, &100);
    assert_eq!(id2, id1 + 1);
}

#[test]
fn test_buy_policy_zero_coverage_fails() {
    let s = Setup::new();
    let result = s.client.try_buy_policy(&s.holder, &0, &500);
    assert_eq!(result, Err(Ok(Error::ZeroAmount)));
}

#[test]
fn test_buy_policy_emits_event() {
    let s = Setup::new();
    mint(&s.env, &s.token, &s.holder, 1000);
    s.client.buy_policy(&s.holder, &10000, &500);
    assert!(has_event(&s.env, "pay_ins", "p_bought"));
}

// ---------------------------------------------------------------------------
// Claim tests
// ---------------------------------------------------------------------------

#[test]
fn test_submit_claim_marks_pending() {
    let s = Setup::new();
    mint(&s.env, &s.token, &s.holder, 1000);
    let policy_id = s.client.buy_policy(&s.holder, &10000, &500);
    s.client.submit_claim(&policy_id);
    let policy = s.client.get_policy(&policy_id);
    assert!(matches!(policy.claim_status, PolicyStatus::ClaimPending));
}

#[test]
fn test_submit_claim_emits_event() {
    let s = Setup::new();
    mint(&s.env, &s.token, &s.holder, 1000);
    let policy_id = s.client.buy_policy(&s.holder, &10000, &500);
    s.client.submit_claim(&policy_id);
    assert!(has_event(&s.env, "pay_ins", "c_sub"));
}

#[test]
fn test_process_claim_approve_transfers_coverage() {
    let s = Setup::new();
    
    // Fund contract with enough tokens for coverage
    // Underwriter adds stake which goes to reserve (but we need tokens in contract balance too)
    mint(&s.env, &s.token, &s.underwriter, 60000);
    s.client.add_underwriter(&s.underwriter, &50000);
    
    // Holder buys policy
    mint(&s.env, &s.token, &s.holder, 1000);
    let policy_id = s.client.buy_policy(&s.holder, &10000, &500);
    
    // Submit and approve claim
    s.client.submit_claim(&policy_id);
    s.client.process_claim(&policy_id, &true);
    
    let policy = s.client.get_policy(&policy_id);
    assert!(!policy.active);
    assert!(matches!(policy.claim_status, PolicyStatus::ClaimApproved));
    assert_eq!(token_balance(&s.env, &s.token, &s.holder), 10000);
}

#[test]
fn test_process_claim_approve_decreases_reserve() {
    let s = Setup::new();
    
    mint(&s.env, &s.token, &s.underwriter, 60000);
    s.client.add_underwriter(&s.underwriter, &50000);
    
    mint(&s.env, &s.token, &s.holder, 1000);
    let policy_id = s.client.buy_policy(&s.holder, &10000, &500);
    
    s.client.submit_claim(&policy_id);
    s.client.process_claim(&policy_id, &true);
    
    assert_eq!(s.client.get_reserve(), 40000);
}

#[test]
fn test_process_claim_approve_emits_event() {
    let s = Setup::new();
    
    mint(&s.env, &s.token, &s.underwriter, 60000);
    s.client.add_underwriter(&s.underwriter, &50000);
    
    mint(&s.env, &s.token, &s.holder, 1000);
    let policy_id = s.client.buy_policy(&s.holder, &10000, &500);
    
    s.client.submit_claim(&policy_id);
    s.client.process_claim(&policy_id, &true);
    
    assert!(has_event(&s.env, "pay_ins", "c_appr"));
}

#[test]
fn test_process_claim_deny_marks_denied() {
    let s = Setup::new();
    mint(&s.env, &s.token, &s.holder, 1000);
    let policy_id = s.client.buy_policy(&s.holder, &10000, &500);
    s.client.submit_claim(&policy_id);
    s.client.process_claim(&policy_id, &false);
    
    let policy = s.client.get_policy(&policy_id);
    assert!(matches!(policy.claim_status, PolicyStatus::ClaimDenied));
}

#[test]
fn test_process_claim_deny_emits_event() {
    let s = Setup::new();
    mint(&s.env, &s.token, &s.holder, 1000);
    let policy_id = s.client.buy_policy(&s.holder, &10000, &500);
    s.client.submit_claim(&policy_id);
    s.client.process_claim(&policy_id, &false);
    
    assert!(has_event(&s.env, "pay_ins", "c_den"));
}

#[test]
fn test_process_claim_insufficient_reserve_fails() {
    let s = Setup::new();
    
    // No underwriter - reserve is 0
    mint(&s.env, &s.token, &s.holder, 1000);
    let policy_id = s.client.buy_policy(&s.holder, &10000, &500);
    
    s.client.submit_claim(&policy_id);
    let result = s.client.try_process_claim(&policy_id, &true);
    assert_eq!(result, Err(Ok(Error::InsufficientReserve)));
}

#[test]
fn test_submit_claim_nonexistent_policy_fails() {
    let s = Setup::new();
    let result = s.client.try_submit_claim(&999);
    assert_eq!(result, Err(Ok(Error::PolicyNotFound)));
}

#[test]
fn test_process_claim_nonexistent_policy_fails() {
    let s = Setup::new();
    let result = s.client.try_process_claim(&999, &true);
    assert_eq!(result, Err(Ok(Error::PolicyNotFound)));
}

#[test]
fn test_get_policy_nonexistent_fails() {
    let s = Setup::new();
    let result = s.client.try_get_policy(&999);
    assert_eq!(result, Err(Ok(Error::PolicyNotFound)));
}