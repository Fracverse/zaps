#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, Env, BytesN,
};

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Spin up a fresh environment with a registered escrow contract and a funded
/// SAC token.  Returns `(env, client, buyer, seller, token_address)`.
fn setup() -> (Env, EscrowContractClient<'static>, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, EscrowContract);
    let client = EscrowContractClient::new(&env, &contract_id);

    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let admin = Address::generate(&env);

    let sac_contract = env.register_stellar_asset_contract_v2(admin.clone());
    let token = sac_contract.address();
    let sac = token::StellarAssetClient::new(&env, &token);
    sac.mint(&buyer, &10_000_000);

    // SAFETY: the lifetime is tied to `env` which lives for the whole test.
    let client: EscrowContractClient<'static> = unsafe { core::mem::transmute(client) };

    (env, client, buyer, seller, token)
}

fn make_id(env: &Env, seed: u8) -> BytesN<32> {
    BytesN::from_array(env, &[seed; 32])
}

fn zero_memo(env: &Env) -> BytesN<32> {
    BytesN::from_array(env, &[0u8; 32])
}

// ─── lock_funds ───────────────────────────────────────────────────────────────

#[test]
fn test_lock_funds_success() {
    let (env, client, buyer, seller, token) = setup();

    let escrow_id = make_id(&env, 1);
    let amount: i128 = 1_000_000;

    client.lock_funds(&escrow_id, &buyer, &seller, &token, &amount, &1_000_000, &zero_memo(&env));

    let stored = client.get_escrow(&escrow_id);
    assert_eq!(stored.buyer, buyer);
    assert_eq!(stored.seller, seller);
    assert_eq!(stored.token, token);
    assert_eq!(stored.amount, amount);
    assert_eq!(stored.state, EscrowState::Locked);
    assert!(client.is_locked(&escrow_id));
}

#[test]
fn test_lock_funds_duplicate_id_fails() {
    let (env, client, buyer, seller, token) = setup();

    let escrow_id = make_id(&env, 2);
    let amount: i128 = 500_000;

    client.lock_funds(&escrow_id, &buyer, &seller, &token, &amount, &1_000_000, &zero_memo(&env));

    let result = client.try_lock_funds(
        &escrow_id, &buyer, &seller, &token, &amount, &1_000_000, &zero_memo(&env),
    );
    assert!(result.is_err());
}

#[test]
fn test_lock_funds_zero_amount_fails() {
    let (env, client, buyer, seller, token) = setup();

    let result = client.try_lock_funds(
        &make_id(&env, 3), &buyer, &seller, &token, &0, &1_000_000, &zero_memo(&env),
    );
    assert!(result.is_err());
}

// ─── release_funds ────────────────────────────────────────────────────────────

#[test]
fn test_release_funds_by_seller_success() {
    let (env, client, buyer, seller, token) = setup();

    let escrow_id = make_id(&env, 4);
    client.lock_funds(&escrow_id, &buyer, &seller, &token, &750_000, &1_000_000, &zero_memo(&env));
    client.release_funds(&escrow_id, &seller);

    let stored = client.get_escrow(&escrow_id);
    assert_eq!(stored.state, EscrowState::Released);
    assert!(!client.is_locked(&escrow_id));
}

#[test]
fn test_release_funds_unauthorized_fails() {
    let (env, client, buyer, seller, token) = setup();

    let random_caller = Address::generate(&env);
    let escrow_id = make_id(&env, 5);

    client.lock_funds(&escrow_id, &buyer, &seller, &token, &300_000, &1_000_000, &zero_memo(&env));

    let result = client.try_release_funds(&escrow_id, &random_caller);
    assert!(result.is_err());
}

#[test]
fn test_release_already_released_fails() {
    let (env, client, buyer, seller, token) = setup();

    let escrow_id = make_id(&env, 14);
    client.lock_funds(&escrow_id, &buyer, &seller, &token, &100_000, &1_000_000, &zero_memo(&env));
    client.release_funds(&escrow_id, &seller);

    // Second release must fail – funds are gone and state is Released.
    let result = client.try_release_funds(&escrow_id, &seller);
    assert!(result.is_err());
}

// ─── refund_funds ─────────────────────────────────────────────────────────────

#[test]
fn test_refund_funds_by_buyer_after_timeout_success() {
    let (env, client, buyer, seller, token) = setup();

    let escrow_id = make_id(&env, 6);
    client.lock_funds(&escrow_id, &buyer, &seller, &token, &1_200_000, &1_000_000, &zero_memo(&env));

    let creation_time = env.ledger().timestamp();
    env.ledger().set_timestamp(creation_time + 8 * 24 * 60 * 60);

    client.refund_funds(&escrow_id, &buyer);

    let stored = client.get_escrow(&escrow_id);
    assert_eq!(stored.state, EscrowState::Refunded);
    assert!(!client.is_locked(&escrow_id));
}

#[test]
fn test_refund_before_timeout_only_by_buyer_or_arbitrator() {
    let (env, client, buyer, seller, token) = setup();

    let random_caller = Address::generate(&env);
    let escrow_id = make_id(&env, 7);

    client.lock_funds(&escrow_id, &buyer, &seller, &token, &900_000, &1_000_000, &zero_memo(&env));

    // Random caller before timeout → should fail.
    let result = client.try_refund_funds(&escrow_id, &random_caller);
    assert!(result.is_err());

    // Buyer before timeout → should succeed.
    client.refund_funds(&escrow_id, &buyer);
    assert_eq!(client.get_escrow(&escrow_id).state, EscrowState::Refunded);
}

#[test]
fn test_refund_already_refunded_fails() {
    let (env, client, buyer, seller, token) = setup();

    let escrow_id = make_id(&env, 15);
    client.lock_funds(&escrow_id, &buyer, &seller, &token, &100_000, &1_000_000, &zero_memo(&env));
    client.refund_funds(&escrow_id, &buyer);

    let result = client.try_refund_funds(&escrow_id, &buyer);
    assert!(result.is_err());
}

// ─── initiate_dispute ─────────────────────────────────────────────────────────

#[test]
fn test_initiate_dispute_by_buyer() {
    let (env, client, buyer, seller, token) = setup();

    let resolver = Address::generate(&env);
    let escrow_id = make_id(&env, 8);

    client.lock_funds(&escrow_id, &buyer, &seller, &token, &500_000, &1_000_000, &zero_memo(&env));
    client.initiate_dispute(&escrow_id, &buyer, &resolver);

    assert_eq!(client.get_state(&escrow_id), EscrowState::Disputed);
}

#[test]
fn test_initiate_dispute_by_seller() {
    let (env, client, buyer, seller, token) = setup();

    let resolver = Address::generate(&env);
    let escrow_id = make_id(&env, 9);

    client.lock_funds(&escrow_id, &buyer, &seller, &token, &500_000, &1_000_000, &zero_memo(&env));
    client.initiate_dispute(&escrow_id, &seller, &resolver);

    assert_eq!(client.get_state(&escrow_id), EscrowState::Disputed);
}

#[test]
fn test_initiate_dispute_unauthorized_fails() {
    let (env, client, buyer, seller, token) = setup();

    let random = Address::generate(&env);
    let resolver = Address::generate(&env);
    let escrow_id = make_id(&env, 10);

    client.lock_funds(&escrow_id, &buyer, &seller, &token, &500_000, &1_000_000, &zero_memo(&env));

    let result = client.try_initiate_dispute(&escrow_id, &random, &resolver);
    assert!(result.is_err());
}

// ─── vote_resolution ──────────────────────────────────────────────────────────

#[test]
fn test_vote_resolution_agreement_releases_to_seller() {
    let (env, client, buyer, seller, token) = setup();

    let resolver = Address::generate(&env);
    let escrow_id = make_id(&env, 11);

    client.lock_funds(&escrow_id, &buyer, &seller, &token, &400_000, &1_000_000, &zero_memo(&env));
    client.initiate_dispute(&escrow_id, &buyer, &resolver);

    // Both vote to release to seller.
    client.vote_resolution(&escrow_id, &buyer, &true);
    client.vote_resolution(&escrow_id, &seller, &true);

    assert_eq!(client.get_state(&escrow_id), EscrowState::Released);
}

#[test]
fn test_vote_resolution_agreement_refunds_buyer() {
    let (env, client, buyer, seller, token) = setup();

    let resolver = Address::generate(&env);
    let escrow_id = make_id(&env, 12);

    client.lock_funds(&escrow_id, &buyer, &seller, &token, &400_000, &1_000_000, &zero_memo(&env));
    client.initiate_dispute(&escrow_id, &buyer, &resolver);

    // Both vote to refund buyer.
    client.vote_resolution(&escrow_id, &buyer, &false);
    client.vote_resolution(&escrow_id, &seller, &false);

    assert_eq!(client.get_state(&escrow_id), EscrowState::Refunded);
}

#[test]
fn test_vote_resolution_disagreement_stays_disputed() {
    let (env, client, buyer, seller, token) = setup();

    let resolver = Address::generate(&env);
    let escrow_id = make_id(&env, 13);

    client.lock_funds(&escrow_id, &buyer, &seller, &token, &400_000, &1_000_000, &zero_memo(&env));
    client.initiate_dispute(&escrow_id, &buyer, &resolver);

    // Votes disagree – no automatic resolution.
    client.vote_resolution(&escrow_id, &buyer, &true);
    client.vote_resolution(&escrow_id, &seller, &false);

    assert_eq!(client.get_state(&escrow_id), EscrowState::Disputed);
}

#[test]
fn test_vote_resolution_duplicate_vote_fails() {
    let (env, client, buyer, seller, token) = setup();

    let resolver = Address::generate(&env);
    let escrow_id = make_id(&env, 16);

    client.lock_funds(&escrow_id, &buyer, &seller, &token, &400_000, &1_000_000, &zero_memo(&env));
    client.initiate_dispute(&escrow_id, &buyer, &resolver);

    client.vote_resolution(&escrow_id, &buyer, &true);

    // Buyer tries to vote again.
    let result = client.try_vote_resolution(&escrow_id, &buyer, &false);
    assert!(result.is_err());
}

// ─── Reentrancy guard tests ───────────────────────────────────────────────────
//
// Soroban's execution model is single-threaded and atomic within a transaction,
// so true cross-contract reentrancy requires a malicious contract to call back
// into the escrow during a token transfer hook.  The tests below verify the
// guard mechanism directly by inspecting the error code and confirming that
// the lock is properly released after a successful call (allowing subsequent
// legitimate calls to proceed).

#[test]
fn test_reentrancy_lock_released_after_successful_lock_funds() {
    // After a successful lock_funds the guard must be cleared so that a second
    // independent escrow can be created in the same test (simulating two
    // sequential transactions).
    let (env, client, buyer, seller, token) = setup();

    let id1 = make_id(&env, 20);
    let id2 = make_id(&env, 21);

    client.lock_funds(&id1, &buyer, &seller, &token, &100_000, &1_000_000, &zero_memo(&env));
    // If the lock were not released this second call would panic with Reentrant.
    client.lock_funds(&id2, &buyer, &seller, &token, &100_000, &1_000_000, &zero_memo(&env));

    assert_eq!(client.get_state(&id1), EscrowState::Locked);
    assert_eq!(client.get_state(&id2), EscrowState::Locked);
}

#[test]
fn test_reentrancy_lock_released_after_successful_release_funds() {
    let (env, client, buyer, seller, token) = setup();

    let id1 = make_id(&env, 22);
    let id2 = make_id(&env, 23);

    client.lock_funds(&id1, &buyer, &seller, &token, &100_000, &1_000_000, &zero_memo(&env));
    client.lock_funds(&id2, &buyer, &seller, &token, &100_000, &1_000_000, &zero_memo(&env));

    client.release_funds(&id1, &seller);
    // Guard must be clear for this second release to succeed.
    client.release_funds(&id2, &seller);

    assert_eq!(client.get_state(&id1), EscrowState::Released);
    assert_eq!(client.get_state(&id2), EscrowState::Released);
}

#[test]
fn test_reentrancy_lock_released_after_successful_refund_funds() {
    let (env, client, buyer, seller, token) = setup();

    let id1 = make_id(&env, 24);
    let id2 = make_id(&env, 25);

    client.lock_funds(&id1, &buyer, &seller, &token, &100_000, &1_000_000, &zero_memo(&env));
    client.lock_funds(&id2, &buyer, &seller, &token, &100_000, &1_000_000, &zero_memo(&env));

    client.refund_funds(&id1, &buyer);
    // Guard must be clear for this second refund to succeed.
    client.refund_funds(&id2, &buyer);

    assert_eq!(client.get_state(&id1), EscrowState::Refunded);
    assert_eq!(client.get_state(&id2), EscrowState::Refunded);
}

#[test]
fn test_reentrancy_lock_released_after_successful_initiate_dispute() {
    let (env, client, buyer, seller, token) = setup();

    let resolver = Address::generate(&env);
    let id1 = make_id(&env, 26);
    let id2 = make_id(&env, 27);

    client.lock_funds(&id1, &buyer, &seller, &token, &100_000, &1_000_000, &zero_memo(&env));
    client.lock_funds(&id2, &buyer, &seller, &token, &100_000, &1_000_000, &zero_memo(&env));

    client.initiate_dispute(&id1, &buyer, &resolver);
    // Guard must be clear for this second dispute to succeed.
    client.initiate_dispute(&id2, &buyer, &resolver);

    assert_eq!(client.get_state(&id1), EscrowState::Disputed);
    assert_eq!(client.get_state(&id2), EscrowState::Disputed);
}

#[test]
fn test_reentrancy_lock_released_after_vote_resolution() {
    let (env, client, buyer, seller, token) = setup();

    let resolver = Address::generate(&env);
    let id1 = make_id(&env, 28);
    let id2 = make_id(&env, 29);

    client.lock_funds(&id1, &buyer, &seller, &token, &100_000, &1_000_000, &zero_memo(&env));
    client.lock_funds(&id2, &buyer, &seller, &token, &100_000, &1_000_000, &zero_memo(&env));
    client.initiate_dispute(&id1, &buyer, &resolver);
    client.initiate_dispute(&id2, &buyer, &resolver);

    // First vote on id1.
    client.vote_resolution(&id1, &buyer, &true);
    // Guard must be clear to vote on id2.
    client.vote_resolution(&id2, &buyer, &true);

    // Both still disputed (only one vote each).
    assert_eq!(client.get_state(&id1), EscrowState::Disputed);
    assert_eq!(client.get_state(&id2), EscrowState::Disputed);
}

#[test]
fn test_reentrancy_error_code_is_eleven() {
    // Verify the Reentrant error variant has the expected discriminant (11)
    // so downstream clients can identify it unambiguously.
    assert_eq!(EscrowError::Reentrant as u32, 11);
}
