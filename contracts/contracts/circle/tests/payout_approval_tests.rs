//! Soroban contract tests for the Circle Payout Approval Workflow.
//! Illustrative tests; they may not compile until the contract functions
//! and error variants are implemented.

use soroban_sdk::{Address, Env};
use contracts::circle::{CircleContract, Error, PayoutStatus};

fn env() -> Env {
    Env::default()
}

fn admin_address(e: &Env) -> Address {
    Address::random(e)
}

fn member_address(e: &Env) -> Address {
    Address::random(e)
}

#[test]
fn only_eligible_approvers_can_approve() {
    let e = env();
    let contract = CircleContract;
    let admin = admin_address(&e);
    let non_member = Address::random(&e);

    contract.initialize(&e, admin.clone()).unwrap();

    let result = contract.approve_payout(&e, 1, 100, &non_member);
    assert_eq!(result, Err(Error::NotEligibleApprover));
}

#[test]
fn rejections_include_a_reason() {
    let e = env();
    let contract = CircleContract;
    let admin = admin_address(&e);
    let approver = member_address(&e);

    contract.initialize(&e, admin.clone()).unwrap();
    let rid = contract
        .request_payout(&e, 1, &admin, 100, &Address::native(&e))
        .unwrap();

    contract
        .reject_payout(&e, 1, rid, &approver, "insufficient funds")
        .unwrap();

    let payout = contract.get_payout(&e, 1, rid).unwrap();
    assert_eq!(payout.status, PayoutStatus::Rejected);
    assert_eq!(
        payout.rejection_reason,
        Some("insufficient funds".try_into().unwrap())
    );
}

#[test]
fn approval_status_is_tracked() {
    let e = env();
    let contract = CircleContract;
    let admin = admin_address(&e);
    let approver = member_address(&e);

    contract.initialize(&e, admin.clone()).unwrap();
    let rid = contract
        .request_payout(&e, 1, &admin, 100, &Address::native(&e))
        .unwrap();

    contract.approve_payout(&e, 1, rid, &approver).unwrap();

    let payout = contract.get_payout(&e, 1, rid).unwrap();
    assert!(payout.approvals.contains(&approver));
    assert_eq!(payout.approval_count, 1);
    assert_eq!(payout.status, PayoutStatus::Pending);
}

#[test]
fn duplicate_approvals_are_prevented() {
    let e = env();
    let contract = CircleContract;
    let admin = admin_address(&e);
    let approver = member_address(&e);

    contract.initialize(&e, admin.clone()).unwrap();
    let rid = contract
        .request_payout(&e, 1, &admin, 100, &Address::native(&e))
        .unwrap();

    contract.approve_payout(&e, 1, rid, &approver).unwrap();
    let result = contract.approve_payout(&e, 1, rid, &approver);

    assert_eq!(result, Err(Error::DuplicateApproval));
}

#[test]
fn approved_requests_can_proceed_to_execution() {
    let e = env();
    let contract = CircleContract;
    let admin = admin_address(&e);
    let approver1 = member_address(&e);
    let approver2 = member_address(&e);

    contract.initialize(&e, admin.clone()).unwrap();
    let rid = contract
        .request_payout(&e, 1, &admin, 100, &Address::native(&e))
        .unwrap();

    contract.approve_payout(&e, 1, rid, &approver1).unwrap();
    contract.approve_payout(&e, 1, rid, &approver2).unwrap();

    let payout = contract.get_payout(&e, 1, rid).unwrap();
    assert_eq!(payout.status, PayoutStatus::Approved);

    contract.execute(&e, 1, rid).unwrap();

    let payout = contract.get_payout(&e, 1, rid).unwrap();
    assert_eq!(payout.status, PayoutStatus::Executed);
}
