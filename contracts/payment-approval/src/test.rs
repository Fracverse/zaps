#![cfg(test)]

use super::*;
use soroban_sdk::{testutils::Address as _, vec, Address, Bytes, Env};

fn make_config(env: &Env, approvers: Vec<Address>) -> ApprovalConfig {
    ApprovalConfig {
        threshold: 1_000_000,    // 1 XLM (in stroops)
        required_approvals: 2,
        timeout_ledgers: DEFAULT_TIMEOUT,
        approvers,
    }
}

fn setup() -> (
    Env,
    PaymentApprovalClient<'static>,
    Address,
    Address,
    Address,
    Address,
) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, PaymentApproval);
    let client = PaymentApprovalClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let approver1 = Address::generate(&env);
    let approver2 = Address::generate(&env);
    let requester = Address::generate(&env);
    let config = make_config(&env, vec![&env, approver1.clone(), approver2.clone()]);
    client.initialize(&admin, &config);
    (env, client, admin, approver1, approver2, requester)
}

fn pid(env: &Env, s: &[u8]) -> Bytes {
    Bytes::from_slice(env, s)
}

#[test]
fn test_initialize() {
    let (_, client, _, _, _, _) = setup();
    let cfg = client.get_config();
    assert_eq!(cfg.threshold, 1_000_000);
    assert_eq!(cfg.required_approvals, 2);
}

#[test]
#[should_panic(expected = "AlreadyInitialized")]
fn test_double_initialize() {
    let (env, client, admin, approver1, _, _) = setup();
    let config = make_config(&env, vec![&env, approver1]);
    client.initialize(&admin, &config);
}

#[test]
fn test_submit_and_approve() {
    let (env, client, _, approver1, approver2, requester) = setup();
    let payment_id = pid(&env, b"pay-001");
    client.submit_request(&requester, &payment_id, &5_000_000);
    let req = client.get_request(&payment_id);
    assert_eq!(req.status, ApprovalStatus::Pending);

    client.approve(&approver1, &payment_id);
    let req = client.get_request(&payment_id);
    assert_eq!(req.status, ApprovalStatus::Pending); // still needs 2nd

    client.approve(&approver2, &payment_id);
    let req = client.get_request(&payment_id);
    assert_eq!(req.status, ApprovalStatus::Approved);
}

#[test]
fn test_reject() {
    let (env, client, _, approver1, _, requester) = setup();
    let payment_id = pid(&env, b"pay-002");
    client.submit_request(&requester, &payment_id, &5_000_000);
    client.reject(&approver1, &payment_id);
    let req = client.get_request(&payment_id);
    assert_eq!(req.status, ApprovalStatus::Rejected);
}

#[test]
#[should_panic(expected = "BelowThreshold")]
fn test_below_threshold() {
    let (env, client, _, _, _, requester) = setup();
    client.submit_request(&requester, &pid(&env, b"pay-003"), &500_000);
}

#[test]
#[should_panic(expected = "NotApprover")]
fn test_non_approver_cannot_approve() {
    let (env, client, _, _, _, requester) = setup();
    let payment_id = pid(&env, b"pay-004");
    client.submit_request(&requester, &payment_id, &5_000_000);
    let rogue = Address::generate(&env);
    client.approve(&rogue, &payment_id);
}

#[test]
#[should_panic(expected = "AlreadyVoted")]
fn test_double_vote() {
    let (env, client, _, approver1, _, requester) = setup();
    let payment_id = pid(&env, b"pay-005");
    client.submit_request(&requester, &payment_id, &5_000_000);
    client.approve(&approver1, &payment_id);
    client.approve(&approver1, &payment_id);
}

#[test]
#[should_panic(expected = "AlreadyRejected")]
fn test_approve_after_reject() {
    let (env, client, _, approver1, approver2, requester) = setup();
    let payment_id = pid(&env, b"pay-006");
    client.submit_request(&requester, &payment_id, &5_000_000);
    client.reject(&approver1, &payment_id);
    client.approve(&approver2, &payment_id);
}

#[test]
fn test_expire_request() {
    let (env, client, _, _, _, requester) = setup();
    let payment_id = pid(&env, b"pay-007");
    client.submit_request(&requester, &payment_id, &5_000_000);
    // Advance past timeout
    env.ledger().set_sequence_number(DEFAULT_TIMEOUT + 10);
    client.expire_request(&payment_id);
    let req = client.get_request(&payment_id);
    assert_eq!(req.status, ApprovalStatus::Expired);
}

#[test]
fn test_update_config() {
    let (env, client, _, approver1, approver2, _) = setup();
    let new_config = ApprovalConfig {
        threshold: 2_000_000,
        required_approvals: 1,
        timeout_ledgers: 360,
        approvers: vec![&env, approver1, approver2],
    };
    client.update_config(&new_config);
    let cfg = client.get_config();
    assert_eq!(cfg.threshold, 2_000_000);
    assert_eq!(cfg.required_approvals, 1);
}

#[test]
#[should_panic(expected = "RequestNotFound")]
fn test_get_nonexistent_request() {
    let (env, client, _, _, _, _) = setup();
    client.get_request(&pid(&env, b"ghost"));
}
