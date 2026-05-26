#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token::{Client as TokenClient, StellarAssetClient},
    Address, Env, Error as SdkError,
};

fn sdk_err(e: SubError) -> SdkError {
    SdkError::from_contract_error(e as u32)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn create_token(env: &Env, admin: &Address) -> Address {
    env.register_stellar_asset_contract_v2(admin.clone())
        .address()
}

fn mint(env: &Env, token: &Address, to: &Address, amount: i128) {
    StellarAssetClient::new(env, token).mint(to, &amount);
}

/// Approve the subscription contract to pull `amount` from `owner`.
fn approve(env: &Env, token: &Address, owner: &Address, spender: &Address, amount: i128) {
    // Expire well within the max ledger (~6.3M). Use current + 500_000 (~1 month).
    let expiry = env.ledger().sequence() + 500_000u32;
    TokenClient::new(env, token).approve(owner, spender, &amount, &expiry);
}

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

struct Setup {
    env: Env,
    client: SubscriptionPaymentsClient<'static>,
    admin: Address,
    merchant: Address,
    subscriber: Address,
    token: Address,
    /// Plan created during setup: 100 tokens every 1000 ledgers.
    plan_id: u64,
}

impl Setup {
    fn new() -> Self {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let merchant = Address::generate(&env);
        let subscriber = Address::generate(&env);

        let token = create_token(&env, &admin);
        // Give subscriber plenty of tokens.
        mint(&env, &token, &subscriber, 1_000_000);

        let contract_id = env.register_contract(None, SubscriptionPayments);
        let client = SubscriptionPaymentsClient::new(&env, &contract_id);
        client.initialize(&admin);

        let plan_id = client.create_plan(&merchant, &token, &100i128, &1000u32);

        // Approve the contract to pull tokens on behalf of subscriber.
        approve(&env, &token, &subscriber, &contract_id, 1_000_000);

        // SAFETY: lifetime extension for test convenience (env outlives client).
        let client: SubscriptionPaymentsClient<'static> =
            unsafe { core::mem::transmute(client) };

        Setup {
            env,
            client,
            admin,
            merchant,
            subscriber,
            token,
            plan_id,
        }
    }
}

// ---------------------------------------------------------------------------
// Initialisation
// ---------------------------------------------------------------------------

#[test]
fn test_double_init_rejected() {
    let s = Setup::new();
    assert_eq!(
        s.client.try_initialize(&s.admin),
        Err(Ok(sdk_err(SubError::AlreadyInitialized)))
    );
}

// ---------------------------------------------------------------------------
// Plan management
// ---------------------------------------------------------------------------

#[test]
fn test_create_plan_stores_correctly() {
    let s = Setup::new();
    let plan = s.client.get_plan(&s.plan_id);
    assert_eq!(plan.merchant, s.merchant);
    assert_eq!(plan.amount, 100);
    assert_eq!(plan.interval_ledgers, 1000);
    assert!(plan.active);
}

#[test]
fn test_create_plan_invalid_amount_rejected() {
    let s = Setup::new();
    assert_eq!(
        s.client
            .try_create_plan(&s.merchant, &s.token, &0i128, &1000u32),
        Err(Ok(sdk_err(SubError::InvalidAmount)))
    );
}

#[test]
fn test_create_plan_invalid_interval_rejected() {
    let s = Setup::new();
    assert_eq!(
        s.client
            .try_create_plan(&s.merchant, &s.token, &100i128, &0u32),
        Err(Ok(sdk_err(SubError::InvalidInterval)))
    );
}

#[test]
fn test_deactivate_plan() {
    let s = Setup::new();
    s.client.deactivate_plan(&s.plan_id);
    let plan = s.client.get_plan(&s.plan_id);
    assert!(!plan.active);
}

#[test]
fn test_subscribe_to_inactive_plan_rejected() {
    let s = Setup::new();
    s.client.deactivate_plan(&s.plan_id);
    assert_eq!(
        s.client.try_subscribe(&s.subscriber, &s.plan_id),
        Err(Ok(sdk_err(SubError::PlanInactive)))
    );
}

// ---------------------------------------------------------------------------
// Subscribe — first payment
// ---------------------------------------------------------------------------

#[test]
fn test_subscribe_executes_first_payment() {
    let s = Setup::new();
    let sub_id = s.client.subscribe(&s.subscriber, &s.plan_id);

    // Merchant received first payment.
    assert_eq!(
        TokenClient::new(&s.env, &s.token).balance(&s.merchant),
        100
    );

    let sub = s.client.get_subscription(&sub_id);
    assert_eq!(sub.status, SubscriptionStatus::Active);
    assert_eq!(sub.payments_made, 1);
    assert_eq!(sub.failure_count, 0);
}

#[test]
fn test_subscribe_sets_next_payment_ledger() {
    let s = Setup::new();
    let current = s.env.ledger().sequence();
    let sub_id = s.client.subscribe(&s.subscriber, &s.plan_id);
    let sub = s.client.get_subscription(&sub_id);
    // next_payment_ledger = current + interval (1000)
    assert_eq!(sub.next_payment_ledger, current + 1000);
}

// ---------------------------------------------------------------------------
// Execute payment
// ---------------------------------------------------------------------------

#[test]
fn test_execute_payment_before_due_rejected() {
    let s = Setup::new();
    let sub_id = s.client.subscribe(&s.subscriber, &s.plan_id);
    // Ledger hasn't advanced past next_payment_ledger yet.
    assert_eq!(
        s.client.try_execute_payment(&sub_id),
        Err(Ok(sdk_err(SubError::PaymentNotDue)))
    );
}

#[test]
fn test_execute_payment_on_due_ledger_succeeds() {
    let s = Setup::new();
    let sub_id = s.client.subscribe(&s.subscriber, &s.plan_id);

    // Advance to the due ledger.
    s.env.ledger().with_mut(|l| l.sequence_number += 1000);

    let success = s.client.execute_payment(&sub_id);
    assert!(success);

    // Merchant received two payments total (subscribe + execute).
    assert_eq!(
        TokenClient::new(&s.env, &s.token).balance(&s.merchant),
        200
    );

    let sub = s.client.get_subscription(&sub_id);
    assert_eq!(sub.payments_made, 2);
    assert_eq!(sub.failure_count, 0);
    assert_eq!(sub.status, SubscriptionStatus::Active);
}

#[test]
fn test_execute_payment_advances_next_due_ledger() {
    let s = Setup::new();
    let sub_id = s.client.subscribe(&s.subscriber, &s.plan_id);

    s.env.ledger().with_mut(|l| l.sequence_number += 1000);
    s.client.execute_payment(&sub_id);

    let sub = s.client.get_subscription(&sub_id);
    // next_payment_ledger should now be current + 1000.
    let current = s.env.ledger().sequence();
    assert_eq!(sub.next_payment_ledger, current + 1000);
}

// ---------------------------------------------------------------------------
// Cancellation
// ---------------------------------------------------------------------------

#[test]
fn test_cancel_subscription() {
    let s = Setup::new();
    let sub_id = s.client.subscribe(&s.subscriber, &s.plan_id);
    s.client.cancel(&s.subscriber, &sub_id);

    let sub = s.client.get_subscription(&sub_id);
    assert_eq!(sub.status, SubscriptionStatus::Cancelled);
}

#[test]
fn test_cancel_already_cancelled_rejected() {
    let s = Setup::new();
    let sub_id = s.client.subscribe(&s.subscriber, &s.plan_id);
    s.client.cancel(&s.subscriber, &sub_id);
    assert_eq!(
        s.client.try_cancel(&s.subscriber, &sub_id),
        Err(Ok(sdk_err(SubError::SubscriptionNotActive)))
    );
}

#[test]
fn test_execute_payment_on_cancelled_subscription_rejected() {
    let s = Setup::new();
    let sub_id = s.client.subscribe(&s.subscriber, &s.plan_id);
    s.client.cancel(&s.subscriber, &sub_id);

    s.env.ledger().with_mut(|l| l.sequence_number += 1000);
    assert_eq!(
        s.client.try_execute_payment(&sub_id),
        Err(Ok(sdk_err(SubError::SubscriptionNotActive)))
    );
}

#[test]
fn test_cancel_by_wrong_address_rejected() {
    let s = Setup::new();
    let sub_id = s.client.subscribe(&s.subscriber, &s.plan_id);
    let other = Address::generate(&s.env);
    assert_eq!(
        s.client.try_cancel(&other, &sub_id),
        Err(Ok(sdk_err(SubError::Unauthorized)))
    );
}

// ---------------------------------------------------------------------------
// Payment failure handling
// ---------------------------------------------------------------------------

#[test]
fn test_payment_failure_increments_failure_count() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let merchant = Address::generate(&env);
    let subscriber = Address::generate(&env);
    let token = create_token(&env, &admin);

    // Give subscriber exactly enough for the first payment only.
    mint(&env, &token, &subscriber, 100);

    let contract_id = env.register_contract(None, SubscriptionPayments);
    let client = SubscriptionPaymentsClient::new(&env, &contract_id);
    client.initialize(&admin);
    let plan_id = client.create_plan(&merchant, &token, &100i128, &1000u32);

    // Approve contract to pull tokens (only 100 available).
    approve(&env, &token, &subscriber, &contract_id, 1_000_000);

    // First payment succeeds (uses the 100 tokens).
    let sub_id = client.subscribe(&subscriber, &plan_id);
    let sub = client.get_subscription(&sub_id);
    assert_eq!(sub.payments_made, 1);
    assert_eq!(sub.failure_count, 0);

    // Subscriber now has 0 tokens — next payment will fail.
    env.ledger().with_mut(|l| l.sequence_number += 1000);
    let success = client.execute_payment(&sub_id);
    assert!(!success);

    let sub = client.get_subscription(&sub_id);
    assert_eq!(sub.failure_count, 1);
    assert_eq!(sub.status, SubscriptionStatus::PaymentFailed);
}

#[test]
fn test_auto_cancel_after_max_retries() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let merchant = Address::generate(&env);
    let subscriber = Address::generate(&env);
    let token = create_token(&env, &admin);

    // Enough for first payment only.
    mint(&env, &token, &subscriber, 100);

    let contract_id = env.register_contract(None, SubscriptionPayments);
    let client = SubscriptionPaymentsClient::new(&env, &contract_id);
    client.initialize(&admin);
    let plan_id = client.create_plan(&merchant, &token, &100i128, &1000u32);

    // Approve contract to pull tokens.
    approve(&env, &token, &subscriber, &contract_id, 1_000_000);

    let sub_id = client.subscribe(&subscriber, &plan_id);

    // Exhaust MAX_RETRIES (3) failed payments.
    for i in 1..=MAX_RETRIES {
        env.ledger()
            .with_mut(|l| l.sequence_number += 1000);
        let success = client.execute_payment(&sub_id);
        assert!(!success);
        let sub = client.get_subscription(&sub_id);
        if i < MAX_RETRIES {
            assert_eq!(sub.status, SubscriptionStatus::PaymentFailed);
        } else {
            assert_eq!(sub.status, SubscriptionStatus::Cancelled);
        }
    }
}

#[test]
fn test_failure_count_resets_on_success() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let merchant = Address::generate(&env);
    let subscriber = Address::generate(&env);
    let token = create_token(&env, &admin);

    // Enough for first payment + one failure + one recovery.
    mint(&env, &token, &subscriber, 200);

    let contract_id = env.register_contract(None, SubscriptionPayments);
    let client = SubscriptionPaymentsClient::new(&env, &contract_id);
    client.initialize(&admin);
    let plan_id = client.create_plan(&merchant, &token, &100i128, &1000u32);

    // Approve contract to pull tokens.
    approve(&env, &token, &subscriber, &contract_id, 1_000_000);

    let sub_id = client.subscribe(&subscriber, &plan_id);
    // subscriber now has 100 tokens left.

    // Drain subscriber so next payment fails.
    TokenClient::new(&env, &token).transfer(&subscriber, &merchant, &100i128);
    // subscriber now has 0.

    env.ledger().with_mut(|l| l.sequence_number += 1000);
    let fail = client.execute_payment(&sub_id);
    assert!(!fail);
    assert_eq!(client.get_subscription(&sub_id).failure_count, 1);

    // Refund subscriber so recovery succeeds.
    mint(&env, &token, &subscriber, 100);

    env.ledger().with_mut(|l| l.sequence_number += 1000);
    let success = client.execute_payment(&sub_id);
    assert!(success);

    let sub = client.get_subscription(&sub_id);
    assert_eq!(sub.failure_count, 0);
    assert_eq!(sub.status, SubscriptionStatus::Active);
}

// ---------------------------------------------------------------------------
// Pause / unpause
// ---------------------------------------------------------------------------

#[test]
fn test_pause_blocks_subscribe_and_execute() {
    let s = Setup::new();
    let sub_id = s.client.subscribe(&s.subscriber, &s.plan_id);

    s.client.pause();
    assert!(s.client.is_paused());

    assert_eq!(
        s.client.try_subscribe(&s.subscriber, &s.plan_id),
        Err(Ok(sdk_err(SubError::ContractPaused)))
    );

    s.env.ledger().with_mut(|l| l.sequence_number += 1000);
    assert_eq!(
        s.client.try_execute_payment(&sub_id),
        Err(Ok(sdk_err(SubError::ContractPaused)))
    );

    s.client.unpause();
    assert!(!s.client.is_paused());
}

// ---------------------------------------------------------------------------
// Admin transfer
// ---------------------------------------------------------------------------

#[test]
fn test_transfer_admin() {
    let s = Setup::new();
    let new_admin = Address::generate(&s.env);
    s.client.transfer_admin(&new_admin);
    assert_eq!(s.client.get_admin(), new_admin);
}

// ---------------------------------------------------------------------------
// Counters
// ---------------------------------------------------------------------------

#[test]
fn test_plan_and_sub_counters() {
    let s = Setup::new();
    assert_eq!(s.client.plan_count(), 1);
    s.client.subscribe(&s.subscriber, &s.plan_id);
    assert_eq!(s.client.sub_count(), 1);
}
