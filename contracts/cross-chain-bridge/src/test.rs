#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Events, Ledger},
    token::StellarAssetClient,
    Bytes, Env, Error as SdkError, Symbol, TryFromVal,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn sdk_err(e: BridgeError) -> SdkError { SdkError::from_contract_error(e as u32) }
fn eth() -> &'static str { "ethereum" }
fn poly() -> &'static str { "polygon" }
fn evm_addr(env: &Env) -> Bytes { Bytes::from_slice(env, &[0xABu8; 20]) }
fn nonce(env: &Env, seed: u8) -> Bytes { Bytes::from_slice(env, &[seed; 32]) }
fn evm_tx(env: &Env, seed: u8) -> Bytes { Bytes::from_slice(env, &[seed; 32]) }

fn make_token(env: &Env, admin: &Address) -> Address {
    env.register_stellar_asset_contract_v2(admin.clone()).address()
}
fn mint(env: &Env, token: &Address, to: &Address, amount: i128) {
    StellarAssetClient::new(env, token).mint(to, &amount);
}
fn balance(env: &Env, token: &Address, who: &Address) -> i128 {
    soroban_sdk::token::Client::new(env, token).balance(who)
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
// Setup
// ---------------------------------------------------------------------------

/// Small timeout for timeout/refund tests — avoids advancing 120 960 ledgers
/// which would expire SAC balance entries in the test environment.
const TEST_TIMEOUT: u32 = 10;

struct Setup {
    env: Env,
    client: CrossChainBridgeClient<'static>,
    admin: Address,
    relayer: Address,
    token: Address,
    contract_id: Address,
}

impl Setup {
    /// Standard setup — uses OUTBOUND_TIMEOUT_LEDGERS (for non-timeout tests).
    fn new() -> Self { Self::with_timeout(OUTBOUND_TIMEOUT_LEDGERS) }

    /// Short-timeout setup — use for all timeout/refund tests.
    fn short() -> Self { Self::with_timeout(TEST_TIMEOUT) }

    fn with_timeout(timeout: u32) -> Self {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let relayer = Address::generate(&env);
        let token = make_token(&env, &admin);
        let contract_id = env.register_contract(None, CrossChainBridge);
        let client = CrossChainBridgeClient::new(&env, &contract_id);
        client.initialize(&admin, &token, &1_000, &10_000_000, &timeout);
        client.add_relayer(&relayer);
        // Pre-fund contract for inbound payouts.
        mint(&env, &token, &contract_id, 100_000_000);
        let client: CrossChainBridgeClient<'static> = unsafe { core::mem::transmute(client) };
        Setup { env, client, admin, relayer, token, contract_id }
    }

    fn user(&self) -> Address { Address::generate(&self.env) }
    fn advance(&self, n: u32) { self.env.ledger().with_mut(|l| l.sequence_number += n); }
    fn fund(&self, user: &Address, amount: i128) { mint(&self.env, &self.token, user, amount); }
}

// ---------------------------------------------------------------------------
// Initialisation
// ---------------------------------------------------------------------------

#[test]
fn test_initialize_stores_config() {
    let s = Setup::new();
    assert_eq!(s.client.get_admin(), s.admin);
    assert_eq!(s.client.get_token(), s.token);
    assert_eq!(s.client.get_limits(), (1_000, 10_000_000));
    assert!(!s.client.is_paused());
}

#[test]
fn test_initialize_twice_fails() {
    let s = Setup::new();
    assert_eq!(
        s.client.try_initialize(&s.admin, &s.token, &1_000, &10_000_000, &OUTBOUND_TIMEOUT_LEDGERS),
        Err(Ok(sdk_err(BridgeError::AlreadyInitialized)))
    );
}

#[test]
fn test_initialize_invalid_limits_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let token = make_token(&env, &admin);
    let id = env.register_contract(None, CrossChainBridge);
    let client = CrossChainBridgeClient::new(&env, &id);
    assert_eq!(
        client.try_initialize(&admin, &token, &5_000, &1_000, &100),
        Err(Ok(sdk_err(BridgeError::InvalidAmountLimits)))
    );
    assert_eq!(
        client.try_initialize(&admin, &token, &0, &1_000, &100),
        Err(Ok(sdk_err(BridgeError::InvalidAmountLimits)))
    );
}

// ---------------------------------------------------------------------------
// Relayer management
// ---------------------------------------------------------------------------

#[test]
fn test_add_remove_relayer() {
    let s = Setup::new();
    let r2 = s.user();
    assert!(!s.client.is_relayer(&r2));
    s.client.add_relayer(&r2);
    assert!(s.client.is_relayer(&r2));
    s.client.remove_relayer(&r2);
    assert!(!s.client.is_relayer(&r2));
}

#[test]
fn test_add_duplicate_relayer_fails() {
    let s = Setup::new();
    assert_eq!(
        s.client.try_add_relayer(&s.relayer),
        Err(Ok(sdk_err(BridgeError::RelayerAlreadyAdded)))
    );
}

#[test]
fn test_remove_unknown_relayer_fails() {
    let s = Setup::new();
    assert_eq!(
        s.client.try_remove_relayer(&s.user()),
        Err(Ok(sdk_err(BridgeError::RelayerNotFound)))
    );
}

#[test]
fn test_non_relayer_cannot_claim_inbound() {
    let s = Setup::new();
    assert_eq!(
        s.client.try_claim_inbound(
            &s.user(), &nonce(&s.env, 1), &Symbol::new(&s.env, eth()),
            &evm_tx(&s.env, 1), &s.user(), &10_000,
        ),
        Err(Ok(sdk_err(BridgeError::Unauthorized)))
    );
}

// ---------------------------------------------------------------------------
// Pause / circuit breaker
// ---------------------------------------------------------------------------

#[test]
fn test_pause_blocks_inbound() {
    let s = Setup::new();
    s.client.pause();
    assert_eq!(
        s.client.try_claim_inbound(
            &s.relayer, &nonce(&s.env, 1), &Symbol::new(&s.env, eth()),
            &evm_tx(&s.env, 1), &s.user(), &10_000,
        ),
        Err(Ok(sdk_err(BridgeError::ContractPaused)))
    );
}

#[test]
fn test_pause_blocks_outbound() {
    let s = Setup::new();
    s.client.pause();
    let user = s.user();
    s.fund(&user, 50_000);
    assert_eq!(
        s.client.try_lock_outbound(&user, &Symbol::new(&s.env, eth()), &evm_addr(&s.env), &10_000),
        Err(Ok(sdk_err(BridgeError::ContractPaused)))
    );
}

#[test]
fn test_unpause_restores_operations() {
    let s = Setup::new();
    s.client.pause();
    s.client.unpause();
    assert!(!s.client.is_paused());
    let recipient = s.user();
    s.client.claim_inbound(
        &s.relayer, &nonce(&s.env, 1), &Symbol::new(&s.env, eth()),
        &evm_tx(&s.env, 1), &recipient, &10_000,
    );
    assert_eq!(balance(&s.env, &s.token, &recipient), 10_000);
}

#[test]
fn test_pause_emits_event() {
    let s = Setup::new();
    s.client.pause();
    assert!(has_event(&s.env, "bridge", "paused"));
}

#[test]
fn test_unpause_emits_event() {
    let s = Setup::new();
    s.client.pause();
    s.client.unpause();
    assert!(has_event(&s.env, "bridge", "unpaused"));
}

// ---------------------------------------------------------------------------
// Amount limits & chain validation
// ---------------------------------------------------------------------------

#[test]
fn test_inbound_below_min_fails() {
    let s = Setup::new();
    assert_eq!(
        s.client.try_claim_inbound(
            &s.relayer, &nonce(&s.env, 1), &Symbol::new(&s.env, eth()),
            &evm_tx(&s.env, 1), &s.user(), &999,
        ),
        Err(Ok(sdk_err(BridgeError::AmountTooLow)))
    );
}

#[test]
fn test_inbound_above_max_fails() {
    let s = Setup::new();
    assert_eq!(
        s.client.try_claim_inbound(
            &s.relayer, &nonce(&s.env, 1), &Symbol::new(&s.env, eth()),
            &evm_tx(&s.env, 1), &s.user(), &10_000_001,
        ),
        Err(Ok(sdk_err(BridgeError::AmountTooHigh)))
    );
}

#[test]
fn test_outbound_below_min_fails() {
    let s = Setup::new();
    let user = s.user();
    s.fund(&user, 50_000);
    assert_eq!(
        s.client.try_lock_outbound(&user, &Symbol::new(&s.env, eth()), &evm_addr(&s.env), &500),
        Err(Ok(sdk_err(BridgeError::AmountTooLow)))
    );
}

#[test]
fn test_set_limits_updates_correctly() {
    let s = Setup::new();
    s.client.set_limits(&500, &5_000_000);
    assert_eq!(s.client.get_limits(), (500, 5_000_000));
}

#[test]
fn test_set_limits_invalid_fails() {
    let s = Setup::new();
    assert_eq!(
        s.client.try_set_limits(&5_000, &1_000),
        Err(Ok(sdk_err(BridgeError::InvalidAmountLimits)))
    );
}

#[test]
fn test_unsupported_chain_inbound_fails() {
    let s = Setup::new();
    assert_eq!(
        s.client.try_claim_inbound(
            &s.relayer, &nonce(&s.env, 1), &Symbol::new(&s.env, "bitcoin"),
            &evm_tx(&s.env, 1), &s.user(), &10_000,
        ),
        Err(Ok(sdk_err(BridgeError::UnsupportedChain)))
    );
}

#[test]
fn test_unsupported_chain_outbound_fails() {
    let s = Setup::new();
    let user = s.user();
    s.fund(&user, 50_000);
    assert_eq!(
        s.client.try_lock_outbound(&user, &Symbol::new(&s.env, "bsc"), &evm_addr(&s.env), &10_000),
        Err(Ok(sdk_err(BridgeError::UnsupportedChain)))
    );
}

#[test]
fn test_ethereum_chain_accepted() {
    let s = Setup::new();
    let r = s.user();
    s.client.claim_inbound(&s.relayer, &nonce(&s.env, 1), &Symbol::new(&s.env, eth()), &evm_tx(&s.env, 1), &r, &10_000);
    assert_eq!(balance(&s.env, &s.token, &r), 10_000);
}

#[test]
fn test_polygon_chain_accepted() {
    let s = Setup::new();
    let r = s.user();
    s.client.claim_inbound(&s.relayer, &nonce(&s.env, 2), &Symbol::new(&s.env, poly()), &evm_tx(&s.env, 2), &r, &10_000);
    assert_eq!(balance(&s.env, &s.token, &r), 10_000);
}

// ---------------------------------------------------------------------------
// Inbound: claim_inbound
// ---------------------------------------------------------------------------

#[test]
fn test_claim_inbound_transfers_tokens_to_recipient() {
    let s = Setup::new();
    let r = s.user();
    s.client.claim_inbound(&s.relayer, &nonce(&s.env, 1), &Symbol::new(&s.env, eth()), &evm_tx(&s.env, 1), &r, &50_000);
    assert_eq!(balance(&s.env, &s.token, &r), 50_000);
}

#[test]
fn test_claim_inbound_increments_total_inbound() {
    let s = Setup::new();
    let r = s.user();
    s.client.claim_inbound(&s.relayer, &nonce(&s.env, 1), &Symbol::new(&s.env, eth()), &evm_tx(&s.env, 1), &r, &10_000);
    s.client.claim_inbound(&s.relayer, &nonce(&s.env, 2), &Symbol::new(&s.env, eth()), &evm_tx(&s.env, 2), &r, &20_000);
    assert_eq!(s.client.get_total_inbound(), 30_000);
}

#[test]
fn test_claim_inbound_marks_nonce_used() {
    let s = Setup::new();
    let n = nonce(&s.env, 1);
    assert!(!s.client.is_nonce_used(&n));
    s.client.claim_inbound(&s.relayer, &n, &Symbol::new(&s.env, eth()), &evm_tx(&s.env, 1), &s.user(), &10_000);
    assert!(s.client.is_nonce_used(&n));
}

#[test]
fn test_claim_inbound_replay_rejected() {
    let s = Setup::new();
    let n = nonce(&s.env, 1);
    let r = s.user();
    s.client.claim_inbound(&s.relayer, &n, &Symbol::new(&s.env, eth()), &evm_tx(&s.env, 1), &r, &10_000);
    assert_eq!(
        s.client.try_claim_inbound(&s.relayer, &n, &Symbol::new(&s.env, eth()), &evm_tx(&s.env, 1), &r, &10_000),
        Err(Ok(sdk_err(BridgeError::NonceAlreadyUsed)))
    );
}

#[test]
fn test_claim_inbound_different_nonces_both_succeed() {
    let s = Setup::new();
    let r = s.user();
    s.client.claim_inbound(&s.relayer, &nonce(&s.env, 1), &Symbol::new(&s.env, eth()), &evm_tx(&s.env, 1), &r, &10_000);
    s.client.claim_inbound(&s.relayer, &nonce(&s.env, 2), &Symbol::new(&s.env, eth()), &evm_tx(&s.env, 2), &r, &10_000);
    assert_eq!(balance(&s.env, &s.token, &r), 20_000);
}

#[test]
fn test_claim_inbound_emits_event() {
    let s = Setup::new();
    s.client.claim_inbound(&s.relayer, &nonce(&s.env, 1), &Symbol::new(&s.env, eth()), &evm_tx(&s.env, 1), &s.user(), &10_000);
    assert!(has_event(&s.env, "bridge", "inbound"));
}

// ---------------------------------------------------------------------------
// Outbound: lock_outbound
// ---------------------------------------------------------------------------

#[test]
fn test_lock_outbound_transfers_tokens_to_contract() {
    let s = Setup::new();
    let user = s.user();
    s.fund(&user, 50_000);
    let before = balance(&s.env, &s.token, &s.contract_id);
    s.client.lock_outbound(&user, &Symbol::new(&s.env, eth()), &evm_addr(&s.env), &10_000);
    assert_eq!(balance(&s.env, &s.token, &user), 40_000);
    assert_eq!(balance(&s.env, &s.token, &s.contract_id), before + 10_000);
}

#[test]
fn test_lock_outbound_returns_sequential_ids() {
    let s = Setup::new();
    let user = s.user();
    s.fund(&user, 100_000);
    let id0 = s.client.lock_outbound(&user, &Symbol::new(&s.env, eth()), &evm_addr(&s.env), &10_000);
    let id1 = s.client.lock_outbound(&user, &Symbol::new(&s.env, poly()), &evm_addr(&s.env), &10_000);
    assert_eq!(id0, 0);
    assert_eq!(id1, 1);
}

#[test]
fn test_lock_outbound_creates_pending_record() {
    let s = Setup::new();
    let user = s.user();
    s.fund(&user, 50_000);
    let id = s.client.lock_outbound(&user, &Symbol::new(&s.env, eth()), &evm_addr(&s.env), &10_000);
    let t = s.client.get_outbound(&id);
    assert_eq!(t.status, OutboundStatus::Pending);
    assert_eq!(t.amount, 10_000);
    assert_eq!(t.sender, user);
    assert_eq!(t.dest_chain, Chain::Ethereum);
}

#[test]
fn test_lock_outbound_increments_total_outbound() {
    let s = Setup::new();
    let user = s.user();
    s.fund(&user, 100_000);
    s.client.lock_outbound(&user, &Symbol::new(&s.env, eth()), &evm_addr(&s.env), &10_000);
    s.client.lock_outbound(&user, &Symbol::new(&s.env, eth()), &evm_addr(&s.env), &20_000);
    assert_eq!(s.client.get_total_outbound(), 30_000);
}

#[test]
fn test_lock_outbound_invalid_dest_address_fails() {
    let s = Setup::new();
    let user = s.user();
    s.fund(&user, 50_000);
    let bad = Bytes::from_slice(&s.env, &[0xABu8; 19]); // 19 bytes, not 20
    assert_eq!(
        s.client.try_lock_outbound(&user, &Symbol::new(&s.env, eth()), &bad, &10_000),
        Err(Ok(sdk_err(BridgeError::InvalidDestAddress)))
    );
}

#[test]
fn test_lock_outbound_emits_event() {
    let s = Setup::new();
    let user = s.user();
    s.fund(&user, 50_000);
    s.client.lock_outbound(&user, &Symbol::new(&s.env, eth()), &evm_addr(&s.env), &10_000);
    assert!(has_event(&s.env, "bridge", "locked"));
}

// ---------------------------------------------------------------------------
// Outbound: confirm_outbound
// ---------------------------------------------------------------------------

#[test]
fn test_confirm_outbound_marks_confirmed() {
    let s = Setup::new();
    let user = s.user();
    s.fund(&user, 50_000);
    let id = s.client.lock_outbound(&user, &Symbol::new(&s.env, eth()), &evm_addr(&s.env), &10_000);
    s.client.confirm_outbound(&s.relayer, &id, &evm_tx(&s.env, 1));
    assert_eq!(s.client.get_outbound(&id).status, OutboundStatus::Confirmed);
}

#[test]
fn test_confirm_nonexistent_transfer_fails() {
    let s = Setup::new();
    assert_eq!(
        s.client.try_confirm_outbound(&s.relayer, &999, &evm_tx(&s.env, 1)),
        Err(Ok(sdk_err(BridgeError::TransferNotFound)))
    );
}

#[test]
fn test_confirm_already_confirmed_fails() {
    let s = Setup::new();
    let user = s.user();
    s.fund(&user, 50_000);
    let id = s.client.lock_outbound(&user, &Symbol::new(&s.env, eth()), &evm_addr(&s.env), &10_000);
    s.client.confirm_outbound(&s.relayer, &id, &evm_tx(&s.env, 1));
    assert_eq!(
        s.client.try_confirm_outbound(&s.relayer, &id, &evm_tx(&s.env, 2)),
        Err(Ok(sdk_err(BridgeError::TransferNotPending)))
    );
}

/// Uses short timeout so we only advance 11 ledgers — avoids SAC TTL expiry.
#[test]
fn test_confirm_after_timeout_fails() {
    let s = Setup::short();
    let user = s.user();
    s.fund(&user, 50_000);
    let id = s.client.lock_outbound(&user, &Symbol::new(&s.env, eth()), &evm_addr(&s.env), &10_000);
    s.advance(TEST_TIMEOUT + 1);
    assert_eq!(
        s.client.try_confirm_outbound(&s.relayer, &id, &evm_tx(&s.env, 1)),
        Err(Ok(sdk_err(BridgeError::TimeoutAlreadyExpired)))
    );
}

#[test]
fn test_non_relayer_cannot_confirm() {
    let s = Setup::new();
    let user = s.user();
    s.fund(&user, 50_000);
    let id = s.client.lock_outbound(&user, &Symbol::new(&s.env, eth()), &evm_addr(&s.env), &10_000);
    assert_eq!(
        s.client.try_confirm_outbound(&s.user(), &id, &evm_tx(&s.env, 1)),
        Err(Ok(sdk_err(BridgeError::Unauthorized)))
    );
}

#[test]
fn test_confirm_emits_event() {
    let s = Setup::new();
    let user = s.user();
    s.fund(&user, 50_000);
    let id = s.client.lock_outbound(&user, &Symbol::new(&s.env, eth()), &evm_addr(&s.env), &10_000);
    s.client.confirm_outbound(&s.relayer, &id, &evm_tx(&s.env, 1));
    assert!(has_event(&s.env, "bridge", "confirmed"));
}

// ---------------------------------------------------------------------------
// Outbound: refund_outbound — all use Setup::short() (TEST_TIMEOUT = 10)
// ---------------------------------------------------------------------------

#[test]
fn test_refund_returns_tokens_to_sender() {
    let s = Setup::short();
    let user = s.user();
    s.fund(&user, 50_000);
    let id = s.client.lock_outbound(&user, &Symbol::new(&s.env, eth()), &evm_addr(&s.env), &10_000);
    s.advance(TEST_TIMEOUT + 1);
    s.client.refund_outbound(&id);
    assert_eq!(balance(&s.env, &s.token, &user), 50_000);
    assert_eq!(s.client.get_outbound(&id).status, OutboundStatus::Refunded);
}

#[test]
fn test_refund_before_timeout_fails() {
    let s = Setup::short();
    let user = s.user();
    s.fund(&user, 50_000);
    let id = s.client.lock_outbound(&user, &Symbol::new(&s.env, eth()), &evm_addr(&s.env), &10_000);
    assert_eq!(
        s.client.try_refund_outbound(&id),
        Err(Ok(sdk_err(BridgeError::TimeoutNotReached)))
    );
}

#[test]
fn test_refund_at_exact_timeout_boundary_fails() {
    let s = Setup::short();
    let user = s.user();
    s.fund(&user, 50_000);
    let created = s.env.ledger().sequence();
    let id = s.client.lock_outbound(&user, &Symbol::new(&s.env, eth()), &evm_addr(&s.env), &10_000);
    s.env.ledger().with_mut(|l| l.sequence_number = created + TEST_TIMEOUT);
    assert_eq!(
        s.client.try_refund_outbound(&id),
        Err(Ok(sdk_err(BridgeError::TimeoutNotReached)))
    );
}

#[test]
fn test_refund_one_past_timeout_succeeds() {
    let s = Setup::short();
    let user = s.user();
    s.fund(&user, 50_000);
    let created = s.env.ledger().sequence();
    let id = s.client.lock_outbound(&user, &Symbol::new(&s.env, eth()), &evm_addr(&s.env), &10_000);
    s.env.ledger().with_mut(|l| l.sequence_number = created + TEST_TIMEOUT + 1);
    s.client.refund_outbound(&id);
    assert_eq!(s.client.get_outbound(&id).status, OutboundStatus::Refunded);
}

#[test]
fn test_refund_nonexistent_transfer_fails() {
    let s = Setup::short();
    assert_eq!(
        s.client.try_refund_outbound(&999),
        Err(Ok(sdk_err(BridgeError::TransferNotFound)))
    );
}

#[test]
fn test_refund_already_confirmed_fails() {
    let s = Setup::short();
    let user = s.user();
    s.fund(&user, 50_000);
    let id = s.client.lock_outbound(&user, &Symbol::new(&s.env, eth()), &evm_addr(&s.env), &10_000);
    s.client.confirm_outbound(&s.relayer, &id, &evm_tx(&s.env, 1));
    s.advance(TEST_TIMEOUT + 1);
    assert_eq!(
        s.client.try_refund_outbound(&id),
        Err(Ok(sdk_err(BridgeError::TransferNotPending)))
    );
}

#[test]
fn test_refund_already_refunded_fails() {
    let s = Setup::short();
    let user = s.user();
    s.fund(&user, 50_000);
    let id = s.client.lock_outbound(&user, &Symbol::new(&s.env, eth()), &evm_addr(&s.env), &10_000);
    s.advance(TEST_TIMEOUT + 1);
    s.client.refund_outbound(&id);
    assert_eq!(
        s.client.try_refund_outbound(&id),
        Err(Ok(sdk_err(BridgeError::TransferNotPending)))
    );
}

#[test]
fn test_refund_emits_event() {
    let s = Setup::short();
    let user = s.user();
    s.fund(&user, 50_000);
    let id = s.client.lock_outbound(&user, &Symbol::new(&s.env, eth()), &evm_addr(&s.env), &10_000);
    s.advance(TEST_TIMEOUT + 1);
    s.client.refund_outbound(&id);
    assert!(has_event(&s.env, "bridge", "refunded"));
}

// ---------------------------------------------------------------------------
// Admin: transfer_admin
// ---------------------------------------------------------------------------

#[test]
fn test_transfer_admin() {
    let s = Setup::new();
    let new_admin = s.user();
    s.client.transfer_admin(&new_admin);
    assert_eq!(s.client.get_admin(), new_admin);
}

#[test]
fn test_non_admin_cannot_transfer_admin() {
    let s = Setup::new();
    s.env.mock_auths(&[]);
    assert!(s.client.try_transfer_admin(&s.user()).is_err());
}

// ---------------------------------------------------------------------------
// Reentrancy guard — lock released after each operation
// ---------------------------------------------------------------------------

#[test]
fn test_reentrancy_lock_released_after_claim_inbound() {
    let s = Setup::new();
    let r = s.user();
    s.client.claim_inbound(&s.relayer, &nonce(&s.env, 1), &Symbol::new(&s.env, eth()), &evm_tx(&s.env, 1), &r, &10_000);
    s.client.claim_inbound(&s.relayer, &nonce(&s.env, 2), &Symbol::new(&s.env, eth()), &evm_tx(&s.env, 2), &r, &10_000);
    assert_eq!(balance(&s.env, &s.token, &r), 20_000);
}

#[test]
fn test_reentrancy_lock_released_after_lock_outbound() {
    let s = Setup::new();
    let user = s.user();
    s.fund(&user, 100_000);
    let id0 = s.client.lock_outbound(&user, &Symbol::new(&s.env, eth()), &evm_addr(&s.env), &10_000);
    let id1 = s.client.lock_outbound(&user, &Symbol::new(&s.env, eth()), &evm_addr(&s.env), &10_000);
    assert_eq!(id0, 0);
    assert_eq!(id1, 1);
}

#[test]
fn test_reentrancy_lock_released_after_refund() {
    let s = Setup::short();
    let user = s.user();
    s.fund(&user, 100_000);
    let id0 = s.client.lock_outbound(&user, &Symbol::new(&s.env, eth()), &evm_addr(&s.env), &10_000);
    let id1 = s.client.lock_outbound(&user, &Symbol::new(&s.env, eth()), &evm_addr(&s.env), &10_000);
    s.advance(TEST_TIMEOUT + 1);
    s.client.refund_outbound(&id0);
    s.client.refund_outbound(&id1);
    assert_eq!(balance(&s.env, &s.token, &user), 100_000);
}
