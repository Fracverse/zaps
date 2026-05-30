#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    vec, Address, Env,
};

// ---------------------------------------------------------------------------
// Setup helpers
// ---------------------------------------------------------------------------

struct Setup {
    env: Env,
    client: MerchantVaultClient<'static>,
    admin: Address,
    merchant: Address,
    signers: [Address; 3],
}

impl Setup {
    fn new_2_of_3() -> Self { Self::new(2, 1_000) }

    fn new(threshold: u32, expiry_ledgers: u32) -> Self {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let payment_router = Address::generate(&env);
        let payout_contract = Address::generate(&env);
        let merchant = Address::generate(&env);
        let s0 = Address::generate(&env);
        let s1 = Address::generate(&env);
        let s2 = Address::generate(&env);
        let signers_vec = vec![&env, s0.clone(), s1.clone(), s2.clone()];
        let contract_id = env.register_contract(None, MerchantVault);
        let client = MerchantVaultClient::new(&env, &contract_id);
        client.initialize(&admin, &payment_router, &payout_contract, &signers_vec, &threshold, &expiry_ledgers);
        client.init_merchant(&merchant);
        client.credit(&merchant, &10_000);
        let client: MerchantVaultClient<'static> = unsafe { core::mem::transmute(client) };
        Setup { env, client, admin, merchant, signers: [s0, s1, s2] }
    }
}

// ---------------------------------------------------------------------------
// Original contract tests (updated for new initialize signature)
// ---------------------------------------------------------------------------

#[test]
fn test_initialization() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let pr = Address::generate(&env);
    let pc = Address::generate(&env);
    let s = Address::generate(&env);
    let contract_id = env.register_contract(None, MerchantVault);
    let client = MerchantVaultClient::new(&env, &contract_id);
    client.initialize(&admin, &pr, &pc, &vec![&env, s], &1, &1_000);
    let result = client.try_initialize(&admin, &pr, &pc, &vec![&env, Address::generate(&env)], &1, &1_000);
    assert_eq!(result, Err(Ok(Error::AlreadyInitialized)));
}

#[test]
fn test_merchant_initialization() {
    let s = Setup::new_2_of_3();
    assert_eq!(s.client.balance_of(&s.merchant), 10_000);
}

#[test]
fn test_credit_flow() {
    let s = Setup::new_2_of_3();
    let bal = s.client.credit(&s.merchant, &1_000);
    assert_eq!(bal, 11_000);
}

#[test]
fn test_debit_flow() {
    let s = Setup::new_2_of_3();
    let bal = s.client.debit(&s.merchant, &300);
    assert_eq!(bal, 9_700);
}

#[test]
fn test_over_debit_rejection() {
    let s = Setup::new_2_of_3();
    let result = s.client.try_debit(&s.merchant, &99_999);
    assert_eq!(result, Err(Ok(Error::InsufficientBalance)));
}

#[test]
fn test_negative_amount_rejection() {
    let s = Setup::new_2_of_3();
    assert_eq!(s.client.try_credit(&s.merchant, &-1), Err(Ok(Error::NegativeAmount)));
    assert_eq!(s.client.try_debit(&s.merchant, &-1), Err(Ok(Error::NegativeAmount)));
}

// ---------------------------------------------------------------------------
// Initialisation — multi-sig validation
// ---------------------------------------------------------------------------

#[test]
fn test_initialize_stores_threshold_and_signers() {
    let s = Setup::new_2_of_3();
    assert_eq!(s.client.get_threshold(), 2);
    assert_eq!(s.client.get_signers().len(), 3);
}

#[test]
fn test_initialize_rejects_empty_signers() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, MerchantVault);
    let client = MerchantVaultClient::new(&env, &contract_id);
    let result = client.try_initialize(
        &Address::generate(&env), &Address::generate(&env), &Address::generate(&env),
        &vec![&env], &1, &1_000,
    );
    assert_eq!(result, Err(Ok(Error::EmptySigners)));
}

#[test]
fn test_initialize_rejects_threshold_zero() {
    let env = Env::default();
    env.mock_all_auths();
    let s = Address::generate(&env);
    let contract_id = env.register_contract(None, MerchantVault);
    let client = MerchantVaultClient::new(&env, &contract_id);
    let result = client.try_initialize(
        &Address::generate(&env), &Address::generate(&env), &Address::generate(&env),
        &vec![&env, s], &0, &1_000,
    );
    assert_eq!(result, Err(Ok(Error::InvalidThreshold)));
}

#[test]
fn test_initialize_rejects_threshold_exceeds_signers() {
    let env = Env::default();
    env.mock_all_auths();
    let s = Address::generate(&env);
    let contract_id = env.register_contract(None, MerchantVault);
    let client = MerchantVaultClient::new(&env, &contract_id);
    let result = client.try_initialize(
        &Address::generate(&env), &Address::generate(&env), &Address::generate(&env),
        &vec![&env, s], &2, &1_000,
    );
    assert_eq!(result, Err(Ok(Error::InvalidThreshold)));
}

// ---------------------------------------------------------------------------
// is_signer
// ---------------------------------------------------------------------------

#[test]
fn test_is_signer_returns_true_for_known_signer() {
    let s = Setup::new_2_of_3();
    assert!(s.client.is_signer(&s.signers[0]));
    assert!(s.client.is_signer(&s.signers[2]));
}

#[test]
fn test_is_signer_returns_false_for_unknown_address() {
    let s = Setup::new_2_of_3();
    assert!(!s.client.is_signer(&Address::generate(&s.env)));
}

// ---------------------------------------------------------------------------
// propose_withdrawal
// ---------------------------------------------------------------------------

#[test]
fn test_propose_withdrawal_creates_proposal() {
    let s = Setup::new_2_of_3();
    let pid = s.client.propose_withdrawal(&s.merchant, &500, &s.signers[0]);
    assert_eq!(pid, 0);
    let p = s.client.get_proposal(&pid);
    assert_eq!(p.merchant_id, s.merchant);
    assert_eq!(p.amount, 500);
    assert!(!p.executed);
    assert!(!p.cancelled);
    assert_eq!(p.approvals.len(), 1); // proposer pre-approved
}

#[test]
fn test_propose_increments_proposal_id() {
    let s = Setup::new_2_of_3();
    assert_eq!(s.client.propose_withdrawal(&s.merchant, &100, &s.signers[0]), 0);
    assert_eq!(s.client.propose_withdrawal(&s.merchant, &200, &s.signers[1]), 1);
}

#[test]
fn test_propose_by_non_signer_fails() {
    let s = Setup::new_2_of_3();
    let result = s.client.try_propose_withdrawal(&s.merchant, &500, &Address::generate(&s.env));
    assert_eq!(result, Err(Ok(Error::NotASigner)));
}

#[test]
fn test_propose_zero_amount_fails() {
    let s = Setup::new_2_of_3();
    assert_eq!(s.client.try_propose_withdrawal(&s.merchant, &0, &s.signers[0]), Err(Ok(Error::NegativeAmount)));
}

#[test]
fn test_propose_negative_amount_fails() {
    let s = Setup::new_2_of_3();
    assert_eq!(s.client.try_propose_withdrawal(&s.merchant, &-1, &s.signers[0]), Err(Ok(Error::NegativeAmount)));
}

#[test]
fn test_propose_for_uninitialised_merchant_fails() {
    let s = Setup::new_2_of_3();
    let result = s.client.try_propose_withdrawal(&Address::generate(&s.env), &100, &s.signers[0]);
    assert_eq!(result, Err(Ok(Error::MerchantNotInitialized)));
}

// ---------------------------------------------------------------------------
// approve_withdrawal
// ---------------------------------------------------------------------------

#[test]
fn test_approve_records_approval_below_threshold() {
    let s = Setup::new(3, 1_000); // 3-of-3
    let pid = s.client.propose_withdrawal(&s.merchant, &500, &s.signers[0]);
    let executed = s.client.approve_withdrawal(&pid, &s.signers[1]);
    assert!(!executed);
    assert_eq!(s.client.get_proposal(&pid).approvals.len(), 2);
    assert_eq!(s.client.balance_of(&s.merchant), 10_000); // unchanged
}

#[test]
fn test_approve_by_non_signer_fails() {
    let s = Setup::new_2_of_3();
    let pid = s.client.propose_withdrawal(&s.merchant, &500, &s.signers[0]);
    assert_eq!(
        s.client.try_approve_withdrawal(&pid, &Address::generate(&s.env)),
        Err(Ok(Error::NotASigner))
    );
}

#[test]
fn test_duplicate_approval_fails() {
    let s = Setup::new(3, 1_000);
    let pid = s.client.propose_withdrawal(&s.merchant, &500, &s.signers[0]);
    assert_eq!(s.client.try_approve_withdrawal(&pid, &s.signers[0]), Err(Ok(Error::AlreadyApproved)));
}

#[test]
fn test_approve_nonexistent_proposal_fails() {
    let s = Setup::new_2_of_3();
    assert_eq!(s.client.try_approve_withdrawal(&999, &s.signers[0]), Err(Ok(Error::ProposalNotFound)));
}

#[test]
fn test_2_of_3_executes_on_second_approval() {
    let s = Setup::new_2_of_3();
    let pid = s.client.propose_withdrawal(&s.merchant, &1_000, &s.signers[0]);
    let executed = s.client.approve_withdrawal(&pid, &s.signers[1]);
    assert!(executed);
    assert!(s.client.get_proposal(&pid).executed);
    assert_eq!(s.client.balance_of(&s.merchant), 9_000);
}

#[test]
fn test_3_of_3_executes_on_third_approval() {
    let s = Setup::new(3, 1_000);
    let pid = s.client.propose_withdrawal(&s.merchant, &2_000, &s.signers[0]);
    assert!(!s.client.approve_withdrawal(&pid, &s.signers[1]));
    assert!(s.client.approve_withdrawal(&pid, &s.signers[2]));
    assert_eq!(s.client.balance_of(&s.merchant), 8_000);
}

#[test]
fn test_execution_fails_if_insufficient_balance() {
    let s = Setup::new_2_of_3();
    let pid = s.client.propose_withdrawal(&s.merchant, &99_999, &s.signers[0]);
    assert_eq!(s.client.try_approve_withdrawal(&pid, &s.signers[1]), Err(Ok(Error::InsufficientBalance)));
}

#[test]
fn test_approve_already_executed_proposal_fails() {
    let s = Setup::new_2_of_3();
    let pid = s.client.propose_withdrawal(&s.merchant, &100, &s.signers[0]);
    s.client.approve_withdrawal(&pid, &s.signers[1]);
    assert_eq!(s.client.try_approve_withdrawal(&pid, &s.signers[2]), Err(Ok(Error::ProposalAlreadyExecuted)));
}

// ---------------------------------------------------------------------------
// Proposal expiry
// ---------------------------------------------------------------------------

#[test]
fn test_approve_expired_proposal_fails() {
    let s = Setup::new(2, 100);
    let pid = s.client.propose_withdrawal(&s.merchant, &500, &s.signers[0]);
    s.env.ledger().set_sequence_number(s.env.ledger().sequence() + 101);
    assert_eq!(s.client.try_approve_withdrawal(&pid, &s.signers[1]), Err(Ok(Error::ProposalExpired)));
}

#[test]
fn test_approve_just_before_expiry_succeeds() {
    let s = Setup::new(2, 100);
    let created = s.env.ledger().sequence();
    let pid = s.client.propose_withdrawal(&s.merchant, &500, &s.signers[0]);
    s.env.ledger().set_sequence_number(created + 100);
    assert!(s.client.approve_withdrawal(&pid, &s.signers[1]));
}

// ---------------------------------------------------------------------------
// cancel_proposal
// ---------------------------------------------------------------------------

#[test]
fn test_proposer_can_cancel() {
    let s = Setup::new_2_of_3();
    let pid = s.client.propose_withdrawal(&s.merchant, &500, &s.signers[0]);
    s.client.cancel_proposal(&pid, &s.signers[0]);
    assert!(s.client.get_proposal(&pid).cancelled);
}

#[test]
fn test_admin_can_cancel() {
    let s = Setup::new_2_of_3();
    let pid = s.client.propose_withdrawal(&s.merchant, &500, &s.signers[0]);
    s.client.cancel_proposal(&pid, &s.admin);
    assert!(s.client.get_proposal(&pid).cancelled);
}

#[test]
fn test_non_proposer_non_admin_cannot_cancel() {
    let s = Setup::new_2_of_3();
    let pid = s.client.propose_withdrawal(&s.merchant, &500, &s.signers[0]);
    assert_eq!(s.client.try_cancel_proposal(&pid, &s.signers[1]), Err(Ok(Error::UnauthorizedCaller)));
}

#[test]
fn test_cancel_already_executed_fails() {
    let s = Setup::new_2_of_3();
    let pid = s.client.propose_withdrawal(&s.merchant, &100, &s.signers[0]);
    s.client.approve_withdrawal(&pid, &s.signers[1]);
    assert_eq!(s.client.try_cancel_proposal(&pid, &s.signers[0]), Err(Ok(Error::ProposalAlreadyExecuted)));
}

#[test]
fn test_cancel_already_cancelled_fails() {
    let s = Setup::new_2_of_3();
    let pid = s.client.propose_withdrawal(&s.merchant, &500, &s.signers[0]);
    s.client.cancel_proposal(&pid, &s.signers[0]);
    assert_eq!(s.client.try_cancel_proposal(&pid, &s.signers[0]), Err(Ok(Error::ProposalAlreadyCancelled)));
}

#[test]
fn test_approve_cancelled_proposal_fails() {
    let s = Setup::new_2_of_3();
    let pid = s.client.propose_withdrawal(&s.merchant, &500, &s.signers[0]);
    s.client.cancel_proposal(&pid, &s.signers[0]);
    assert_eq!(s.client.try_approve_withdrawal(&pid, &s.signers[1]), Err(Ok(Error::ProposalAlreadyCancelled)));
}

#[test]
fn test_cancel_does_not_affect_balance() {
    let s = Setup::new_2_of_3();
    let pid = s.client.propose_withdrawal(&s.merchant, &5_000, &s.signers[0]);
    s.client.cancel_proposal(&pid, &s.signers[0]);
    assert_eq!(s.client.balance_of(&s.merchant), 10_000);
}

// ---------------------------------------------------------------------------
// Multiple concurrent proposals
// ---------------------------------------------------------------------------

#[test]
fn test_multiple_proposals_independent() {
    let s = Setup::new_2_of_3();
    let pid0 = s.client.propose_withdrawal(&s.merchant, &1_000, &s.signers[0]);
    let pid1 = s.client.propose_withdrawal(&s.merchant, &2_000, &s.signers[1]);
    s.client.approve_withdrawal(&pid0, &s.signers[1]);
    assert_eq!(s.client.balance_of(&s.merchant), 9_000);
    assert!(!s.client.get_proposal(&pid1).executed);
    s.client.approve_withdrawal(&pid1, &s.signers[0]);
    assert_eq!(s.client.balance_of(&s.merchant), 7_000);
}

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

#[test]
fn test_propose_emits_proposed_event() {
    use soroban_sdk::{testutils::Events, TryFromVal, Symbol};
    let s = Setup::new_2_of_3();
    s.client.propose_withdrawal(&s.merchant, &500, &s.signers[0]);
    let found = s.env.events().all().iter().any(|(_, topics, _)| {
        if topics.len() != 2 { return false; }
        let t0 = <Symbol as TryFromVal<Env, _>>::try_from_val(&s.env, &topics.get(0).unwrap());
        let t1 = <Symbol as TryFromVal<Env, _>>::try_from_val(&s.env, &topics.get(1).unwrap());
        matches!((t0, t1), (Ok(a), Ok(b)) if a == symbol_short!("multisig") && b == symbol_short!("proposed"))
    });
    assert!(found);
}

#[test]
fn test_execute_emits_executed_event() {
    use soroban_sdk::{testutils::Events, TryFromVal, Symbol};
    let s = Setup::new_2_of_3();
    let pid = s.client.propose_withdrawal(&s.merchant, &500, &s.signers[0]);
    s.client.approve_withdrawal(&pid, &s.signers[1]);
    let found = s.env.events().all().iter().any(|(_, topics, _)| {
        if topics.len() != 2 { return false; }
        let t0 = <Symbol as TryFromVal<Env, _>>::try_from_val(&s.env, &topics.get(0).unwrap());
        let t1 = <Symbol as TryFromVal<Env, _>>::try_from_val(&s.env, &topics.get(1).unwrap());
        matches!((t0, t1), (Ok(a), Ok(b)) if a == symbol_short!("multisig") && b == symbol_short!("executed"))
    });
    assert!(found);
}

#[test]
fn test_cancel_emits_cancelled_event() {
    use soroban_sdk::{testutils::Events, TryFromVal, Symbol};
    let s = Setup::new_2_of_3();
    let pid = s.client.propose_withdrawal(&s.merchant, &500, &s.signers[0]);
    s.client.cancel_proposal(&pid, &s.signers[0]);
    let found = s.env.events().all().iter().any(|(_, topics, _)| {
        if topics.len() != 2 { return false; }
        let t0 = <Symbol as TryFromVal<Env, _>>::try_from_val(&s.env, &topics.get(0).unwrap());
        let t1 = <Symbol as TryFromVal<Env, _>>::try_from_val(&s.env, &topics.get(1).unwrap());
        matches!((t0, t1), (Ok(a), Ok(b)) if a == symbol_short!("multisig") && b == symbol_short!("cancelled"))
    });
    assert!(found);
}
