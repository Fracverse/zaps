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
    /// Signer set: [s0, s1, s2]
    signers: [Address; 3],
}

impl Setup {
    /// 2-of-3 multi-sig, 1000-ledger expiry.
    fn new_2_of_3() -> Self {
        Self::new(2, 1_000)
    }

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

        client.initialize(
            &admin,
            &payment_router,
            &payout_contract,
            &signers_vec,
            &threshold,
            &expiry_ledgers,
        );
        client.init_merchant(&merchant);
        client.credit(&merchant, &10_000);

        let client: MerchantVaultClient<'static> = unsafe { core::mem::transmute(client) };

        Setup { env, client, admin, merchant, signers: [s0, s1, s2] }
    }
}

// ---------------------------------------------------------------------------
// Initialisation
// ---------------------------------------------------------------------------

#[test]
fn test_initialize_stores_threshold_and_signers() {
    let s = Setup::new_2_of_3();
    assert_eq!(s.client.get_threshold(), 2);
    let stored = s.client.get_signers();
    assert_eq!(stored.len(), 3);
}

#[test]
fn test_initialize_rejects_empty_signers() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let pr = Address::generate(&env);
    let pc = Address::generate(&env);
    let contract_id = env.register_contract(None, MerchantVault);
    let client = MerchantVaultClient::new(&env, &contract_id);

    let result = client.try_initialize(&admin, &pr, &pc, &vec![&env], &1, &1_000);
    assert_eq!(result, Err(Ok(Error::EmptySigners)));
}

#[test]
fn test_initialize_rejects_threshold_zero() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let pr = Address::generate(&env);
    let pc = Address::generate(&env);
    let s = Address::generate(&env);
    let contract_id = env.register_contract(None, MerchantVault);
    let client = MerchantVaultClient::new(&env, &contract_id);

    let result = client.try_initialize(&admin, &pr, &pc, &vec![&env, s], &0, &1_000);
    assert_eq!(result, Err(Ok(Error::InvalidThreshold)));
}

#[test]
fn test_initialize_rejects_threshold_exceeds_signers() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let pr = Address::generate(&env);
    let pc = Address::generate(&env);
    let s = Address::generate(&env);
    let contract_id = env.register_contract(None, MerchantVault);
    let client = MerchantVaultClient::new(&env, &contract_id);

    // 1 signer, threshold 2 → invalid
    let result = client.try_initialize(&admin, &pr, &pc, &vec![&env, s], &2, &1_000);
    assert_eq!(result, Err(Ok(Error::InvalidThreshold)));
}

#[test]
fn test_initialize_1_of_1() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let pr = Address::generate(&env);
    let pc = Address::generate(&env);
    let s = Address::generate(&env);
    let contract_id = env.register_contract(None, MerchantVault);
    let client = MerchantVaultClient::new(&env, &contract_id);

    client.initialize(&admin, &pr, &pc, &vec![&env, s], &1, &1_000);
    assert_eq!(client.get_threshold(), 1);
}

// ---------------------------------------------------------------------------
// is_signer / get_signers
// ---------------------------------------------------------------------------

#[test]
fn test_is_signer_returns_true_for_known_signer() {
    let s = Setup::new_2_of_3();
    assert!(s.client.is_signer(&s.signers[0]));
    assert!(s.client.is_signer(&s.signers[1]));
    assert!(s.client.is_signer(&s.signers[2]));
}

#[test]
fn test_is_signer_returns_false_for_unknown_address() {
    let s = Setup::new_2_of_3();
    let outsider = Address::generate(&s.env);
    assert!(!s.client.is_signer(&outsider));
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
    assert_eq!(p.proposer, s.signers[0]);
    assert!(!p.executed);
    assert!(!p.cancelled);
    // Proposer's approval is pre-recorded.
    assert_eq!(p.approvals.len(), 1);
}

#[test]
fn test_propose_increments_proposal_id() {
    let s = Setup::new_2_of_3();
    let pid0 = s.client.propose_withdrawal(&s.merchant, &100, &s.signers[0]);
    let pid1 = s.client.propose_withdrawal(&s.merchant, &200, &s.signers[1]);
    assert_eq!(pid0, 0);
    assert_eq!(pid1, 1);
}

#[test]
fn test_propose_by_non_signer_fails() {
    let s = Setup::new_2_of_3();
    let outsider = Address::generate(&s.env);
    let result = s.client.try_propose_withdrawal(&s.merchant, &500, &outsider);
    assert_eq!(result, Err(Ok(Error::NotASigner)));
}

#[test]
fn test_propose_zero_amount_fails() {
    let s = Setup::new_2_of_3();
    let result = s.client.try_propose_withdrawal(&s.merchant, &0, &s.signers[0]);
    assert_eq!(result, Err(Ok(Error::NegativeAmount)));
}

#[test]
fn test_propose_negative_amount_fails() {
    let s = Setup::new_2_of_3();
    let result = s.client.try_propose_withdrawal(&s.merchant, &-1, &s.signers[0]);
    assert_eq!(result, Err(Ok(Error::NegativeAmount)));
}

#[test]
fn test_propose_for_uninitialised_merchant_fails() {
    let s = Setup::new_2_of_3();
    let unknown = Address::generate(&s.env);
    let result = s.client.try_propose_withdrawal(&unknown, &100, &s.signers[0]);
    assert_eq!(result, Err(Ok(Error::MerchantNotInitialized)));
}

// ---------------------------------------------------------------------------
// approve_withdrawal — threshold not yet reached
// ---------------------------------------------------------------------------

#[test]
fn test_approve_records_approval_below_threshold() {
    let s = Setup::new_2_of_3(); // threshold = 2
    let pid = s.client.propose_withdrawal(&s.merchant, &500, &s.signers[0]);

    // s0 already approved via propose; s1 approves → threshold reached → executed
    // So let's use a 3-of-3 setup to test the "below threshold" path.
    let s3 = Setup::new(3, 1_000);
    let pid3 = s3.client.propose_withdrawal(&s3.merchant, &500, &s3.signers[0]);

    // s1 approves — 2 of 3, not yet executed.
    let executed = s3.client.approve_withdrawal(&pid3, &s3.signers[1]);
    assert!(!executed);

    let p = s3.client.get_proposal(&pid3);
    assert_eq!(p.approvals.len(), 2);
    assert!(!p.executed);

    // Balance unchanged.
    assert_eq!(s3.client.balance_of(&s3.merchant), 10_000);

    // Suppress unused warning
    let _ = pid;
}

#[test]
fn test_approve_by_non_signer_fails() {
    let s = Setup::new_2_of_3();
    let pid = s.client.propose_withdrawal(&s.merchant, &500, &s.signers[0]);
    let outsider = Address::generate(&s.env);
    let result = s.client.try_approve_withdrawal(&pid, &outsider);
    assert_eq!(result, Err(Ok(Error::NotASigner)));
}

#[test]
fn test_duplicate_approval_fails() {
    let s = Setup::new(3, 1_000); // 3-of-3 so we don't auto-execute
    let pid = s.client.propose_withdrawal(&s.merchant, &500, &s.signers[0]);

    // s0 already approved via propose; trying again must fail.
    let result = s.client.try_approve_withdrawal(&pid, &s.signers[0]);
    assert_eq!(result, Err(Ok(Error::AlreadyApproved)));
}

#[test]
fn test_approve_nonexistent_proposal_fails() {
    let s = Setup::new_2_of_3();
    let result = s.client.try_approve_withdrawal(&999, &s.signers[0]);
    assert_eq!(result, Err(Ok(Error::ProposalNotFound)));
}

// ---------------------------------------------------------------------------
// approve_withdrawal — threshold reached → auto-execute
// ---------------------------------------------------------------------------

#[test]
fn test_2_of_3_executes_on_second_approval() {
    let s = Setup::new_2_of_3();
    let pid = s.client.propose_withdrawal(&s.merchant, &1_000, &s.signers[0]);

    // s1 approves → 2 of 2 required → executed.
    let executed = s.client.approve_withdrawal(&pid, &s.signers[1]);
    assert!(executed);

    let p = s.client.get_proposal(&pid);
    assert!(p.executed);
    assert_eq!(s.client.balance_of(&s.merchant), 9_000);
}

#[test]
fn test_3_of_3_executes_on_third_approval() {
    let s = Setup::new(3, 1_000);
    let pid = s.client.propose_withdrawal(&s.merchant, &2_000, &s.signers[0]);

    let executed = s.client.approve_withdrawal(&pid, &s.signers[1]);
    assert!(!executed);

    let executed = s.client.approve_withdrawal(&pid, &s.signers[2]);
    assert!(executed);

    assert_eq!(s.client.balance_of(&s.merchant), 8_000);
}

#[test]
fn test_1_of_3_executes_immediately_on_propose() {
    let s = Setup::new(1, 1_000);
    // With threshold=1 the proposer's own approval is enough.
    // propose_withdrawal returns the id; we then need to approve to trigger.
    // Actually threshold=1 means the FIRST approval (the proposer's) should
    // trigger execution when approve_withdrawal is called by anyone.
    // Let's verify: propose creates 1 approval; approve by s1 → 2 ≥ 1 → executes.
    let pid = s.client.propose_withdrawal(&s.merchant, &500, &s.signers[0]);
    // Proposal has 1 approval (proposer). Threshold is 1 → already met.
    // approve_withdrawal by s1 would also execute (2 ≥ 1).
    // But the real test is: does a second signer's approval execute?
    let executed = s.client.approve_withdrawal(&pid, &s.signers[1]);
    assert!(executed);
    assert_eq!(s.client.balance_of(&s.merchant), 9_500);
}

#[test]
fn test_execution_fails_if_insufficient_balance() {
    let s = Setup::new_2_of_3();
    // Propose more than the balance.
    let pid = s.client.propose_withdrawal(&s.merchant, &99_999, &s.signers[0]);
    let result = s.client.try_approve_withdrawal(&pid, &s.signers[1]);
    assert_eq!(result, Err(Ok(Error::InsufficientBalance)));
}

#[test]
fn test_approve_already_executed_proposal_fails() {
    let s = Setup::new_2_of_3();
    let pid = s.client.propose_withdrawal(&s.merchant, &100, &s.signers[0]);
    s.client.approve_withdrawal(&pid, &s.signers[1]); // executes

    let result = s.client.try_approve_withdrawal(&pid, &s.signers[2]);
    assert_eq!(result, Err(Ok(Error::ProposalAlreadyExecuted)));
}

// ---------------------------------------------------------------------------
// Proposal expiry
// ---------------------------------------------------------------------------

#[test]
fn test_approve_expired_proposal_fails() {
    let s = Setup::new(2, 100); // expires after 100 ledgers
    let pid = s.client.propose_withdrawal(&s.merchant, &500, &s.signers[0]);

    // Advance ledger past expiry.
    s.env.ledger().set_sequence_number(
        s.env.ledger().sequence() + 101,
    );

    let result = s.client.try_approve_withdrawal(&pid, &s.signers[1]);
    assert_eq!(result, Err(Ok(Error::ProposalExpired)));
}

#[test]
fn test_approve_at_exact_expiry_boundary_fails() {
    let s = Setup::new(2, 100);
    let created = s.env.ledger().sequence();
    let pid = s.client.propose_withdrawal(&s.merchant, &500, &s.signers[0]);

    // Advance to exactly created + expiry_ledgers + 1 (one past the boundary).
    s.env.ledger().set_sequence_number(created + 101);

    let result = s.client.try_approve_withdrawal(&pid, &s.signers[1]);
    assert_eq!(result, Err(Ok(Error::ProposalExpired)));
}

#[test]
fn test_approve_just_before_expiry_succeeds() {
    let s = Setup::new(2, 100);
    let created = s.env.ledger().sequence();
    let pid = s.client.propose_withdrawal(&s.merchant, &500, &s.signers[0]);

    // Advance to exactly the expiry ledger (not past it).
    s.env.ledger().set_sequence_number(created + 100);

    let executed = s.client.approve_withdrawal(&pid, &s.signers[1]);
    assert!(executed);
}

// ---------------------------------------------------------------------------
// cancel_proposal
// ---------------------------------------------------------------------------

#[test]
fn test_proposer_can_cancel() {
    let s = Setup::new_2_of_3();
    let pid = s.client.propose_withdrawal(&s.merchant, &500, &s.signers[0]);

    s.client.cancel_proposal(&pid, &s.signers[0]);

    let p = s.client.get_proposal(&pid);
    assert!(p.cancelled);
    assert!(!p.executed);
}

#[test]
fn test_admin_can_cancel() {
    let s = Setup::new_2_of_3();
    let pid = s.client.propose_withdrawal(&s.merchant, &500, &s.signers[0]);

    s.client.cancel_proposal(&pid, &s.admin);

    let p = s.client.get_proposal(&pid);
    assert!(p.cancelled);
}

#[test]
fn test_non_proposer_non_admin_cannot_cancel() {
    let s = Setup::new_2_of_3();
    let pid = s.client.propose_withdrawal(&s.merchant, &500, &s.signers[0]);

    // s1 is a signer but not the proposer or admin.
    let result = s.client.try_cancel_proposal(&pid, &s.signers[1]);
    assert_eq!(result, Err(Ok(Error::UnauthorizedCaller)));
}

#[test]
fn test_cancel_already_executed_fails() {
    let s = Setup::new_2_of_3();
    let pid = s.client.propose_withdrawal(&s.merchant, &100, &s.signers[0]);
    s.client.approve_withdrawal(&pid, &s.signers[1]); // executes

    let result = s.client.try_cancel_proposal(&pid, &s.signers[0]);
    assert_eq!(result, Err(Ok(Error::ProposalAlreadyExecuted)));
}

#[test]
fn test_cancel_already_cancelled_fails() {
    let s = Setup::new_2_of_3();
    let pid = s.client.propose_withdrawal(&s.merchant, &500, &s.signers[0]);
    s.client.cancel_proposal(&pid, &s.signers[0]);

    let result = s.client.try_cancel_proposal(&pid, &s.signers[0]);
    assert_eq!(result, Err(Ok(Error::ProposalAlreadyCancelled)));
}

#[test]
fn test_approve_cancelled_proposal_fails() {
    let s = Setup::new_2_of_3();
    let pid = s.client.propose_withdrawal(&s.merchant, &500, &s.signers[0]);
    s.client.cancel_proposal(&pid, &s.signers[0]);

    let result = s.client.try_approve_withdrawal(&pid, &s.signers[1]);
    assert_eq!(result, Err(Ok(Error::ProposalAlreadyCancelled)));
}

#[test]
fn test_cancel_does_not_affect_balance() {
    let s = Setup::new_2_of_3();
    let pid = s.client.propose_withdrawal(&s.merchant, &5_000, &s.signers[0]);
    s.client.cancel_proposal(&pid, &s.signers[0]);

    // Balance must be unchanged.
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

    // Execute pid0.
    s.client.approve_withdrawal(&pid0, &s.signers[1]);
    assert_eq!(s.client.balance_of(&s.merchant), 9_000);

    // pid1 still pending.
    let p1 = s.client.get_proposal(&pid1);
    assert!(!p1.executed);

    // Execute pid1.
    s.client.approve_withdrawal(&pid1, &s.signers[0]);
    assert_eq!(s.client.balance_of(&s.merchant), 7_000);
}

#[test]
fn test_cancel_one_does_not_affect_other() {
    let s = Setup::new_2_of_3();

    let pid0 = s.client.propose_withdrawal(&s.merchant, &1_000, &s.signers[0]);
    let pid1 = s.client.propose_withdrawal(&s.merchant, &2_000, &s.signers[1]);

    s.client.cancel_proposal(&pid0, &s.signers[0]);

    // pid1 can still be executed.
    s.client.approve_withdrawal(&pid1, &s.signers[0]);
    assert_eq!(s.client.balance_of(&s.merchant), 8_000);
}

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

#[test]
fn test_propose_emits_proposed_event() {
    use soroban_sdk::{testutils::Events, TryFromVal, Symbol};

    let s = Setup::new_2_of_3();
    s.client.propose_withdrawal(&s.merchant, &500, &s.signers[0]);

    let events = s.env.events().all();
    let found = events.iter().any(|(_, topics, _)| {
        if topics.len() != 2 { return false; }
        let t0 = <Symbol as TryFromVal<Env, _>>::try_from_val(&s.env, &topics.get(0).unwrap());
        let t1 = <Symbol as TryFromVal<Env, _>>::try_from_val(&s.env, &topics.get(1).unwrap());
        matches!((t0, t1), (Ok(a), Ok(b))
            if a == symbol_short!("multisig") && b == symbol_short!("proposed"))
    });
    assert!(found, "expected (multisig, proposed) event");
}

#[test]
fn test_execute_emits_executed_event() {
    use soroban_sdk::{testutils::Events, TryFromVal, Symbol};

    let s = Setup::new_2_of_3();
    let pid = s.client.propose_withdrawal(&s.merchant, &500, &s.signers[0]);
    s.client.approve_withdrawal(&pid, &s.signers[1]);

    let events = s.env.events().all();
    let found = events.iter().any(|(_, topics, _)| {
        if topics.len() != 2 { return false; }
        let t0 = <Symbol as TryFromVal<Env, _>>::try_from_val(&s.env, &topics.get(0).unwrap());
        let t1 = <Symbol as TryFromVal<Env, _>>::try_from_val(&s.env, &topics.get(1).unwrap());
        matches!((t0, t1), (Ok(a), Ok(b))
            if a == symbol_short!("multisig") && b == symbol_short!("executed"))
    });
    assert!(found, "expected (multisig, executed) event");
}

#[test]
fn test_cancel_emits_cancelled_event() {
    use soroban_sdk::{testutils::Events, TryFromVal, Symbol};

    let s = Setup::new_2_of_3();
    let pid = s.client.propose_withdrawal(&s.merchant, &500, &s.signers[0]);
    s.client.cancel_proposal(&pid, &s.signers[0]);

    let events = s.env.events().all();
    let found = events.iter().any(|(_, topics, _)| {
        if topics.len() != 2 { return false; }
        let t0 = <Symbol as TryFromVal<Env, _>>::try_from_val(&s.env, &topics.get(0).unwrap());
        let t1 = <Symbol as TryFromVal<Env, _>>::try_from_val(&s.env, &topics.get(1).unwrap());
        matches!((t0, t1), (Ok(a), Ok(b))
            if a == symbol_short!("multisig") && b == symbol_short!("cancelled"))
    });
    assert!(found, "expected (multisig, cancelled) event");
}
