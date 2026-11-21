use super::*;
use soroban_sdk::testutils::{Address as _, Events};
use soroban_sdk::token::{Client as TokenClient, StellarAssetClient};
use soroban_sdk::{symbol_short, vec, Env, IntoVal};

struct Ctx {
    env: Env,
    client: CircleContractClient<'static>,
    contract: Address,
    admin: Address,
    alice: Address,
    bob: Address,
    token: Address,
}

fn asset(env: &Env, holders: &[&Address]) -> Address {
    let t = env.register_stellar_asset_contract(Address::generate(env));
    for h in holders {
        StellarAssetClient::new(env, &t).mint(h, &1_000);
    }
    t
}

fn setup() -> Ctx {
    let env = Env::default();
    env.mock_all_auths();
    let contract = env.register_contract(None, CircleContract);
    let client = CircleContractClient::new(&env, &contract);
    let (admin, alice, bob) = (Address::generate(&env), Address::generate(&env), Address::generate(&env));
    let token = asset(&env, &[&admin, &alice, &bob]);
    let client: CircleContractClient<'static> = unsafe { core::mem::transmute(client) };
    Ctx { env, client, contract, admin, alice, bob, token }
}

fn funded_circle(c: &Ctx, limit: i128) -> u32 {
    let id = c.client.create_circle(&c.admin, &c.token, &limit);
    c.client.join(&id, &c.alice);
    c.client.join(&id, &c.bob);
    c.client.contribute(&id, &c.alice, &300);
    c.client.contribute(&id, &c.bob, &200);
    id
}

#[test]
fn full_lifecycle_moves_balances_and_emits_events() {
    let c = setup();
    let id = funded_circle(&c, 0);
    let tok = TokenClient::new(&c.env, &c.token);
    assert_eq!(tok.balance(&c.contract), 500);
    assert_eq!(c.client.contribution_of(&id, &c.alice), 300);

    let rid = c.client.request_payout(&id, &c.alice, &c.bob, &400);
    c.client.approve(&id, &rid, &c.admin);
    c.client.execute(&id, &rid);

    assert_eq!(tok.balance(&c.contract), 100);
    assert_eq!(tok.balance(&c.bob), 1000 - 200 + 400);
    let circle = c.client.get_circle(&id);
    assert_eq!((circle.balance, circle.reserved), (100, 0));
    let (emitter, topics, data) = c.env.events().all().last().unwrap();
    assert_eq!(emitter, c.contract);
    let name: soroban_sdk::Symbol = topics.get(0).unwrap().into_val(&c.env);
    let amount: i128 = data.into_val(&c.env);
    assert_eq!((name, amount), (symbol_short!("paid"), 400));
    assert_eq!((topics.len(), id, rid), (3, 0, 0));
}

#[test]
fn multiple_assets_are_isolated_per_circle() {
    let c = setup();
    let other = asset(&c.env, &[&c.alice]);
    let a = c.client.create_circle(&c.admin, &c.token, &0);
    let b = c.client.create_circle(&c.admin, &other, &0);
    c.client.join(&a, &c.alice);
    c.client.join(&b, &c.alice);
    c.client.contribute(&a, &c.alice, &100);
    c.client.contribute(&b, &c.alice, &50);
    assert_eq!(TokenClient::new(&c.env, &c.token).balance(&c.contract), 100);
    assert_eq!(TokenClient::new(&c.env, &other).balance(&c.contract), 50);
}

#[test]
fn payout_limit_boundaries() {
    let c = setup();
    let id = funded_circle(&c, 100);
    assert!(c.client.try_request_payout(&id, &c.alice, &c.bob, &100).is_ok());
    assert_eq!(c.client.try_request_payout(&id, &c.alice, &c.bob, &101), Err(Ok(Error::ExceedsLimit)));
    assert_eq!(c.client.try_request_payout(&id, &c.alice, &c.bob, &0), Err(Ok(Error::InvalidAmount)));
}

#[test]
fn reserved_funds_cannot_be_overcommitted() {
    let c = setup();
    let id = funded_circle(&c, 0);
    c.client.request_payout(&id, &c.alice, &c.bob, &500);
    assert_eq!(c.client.try_request_payout(&id, &c.alice, &c.bob, &1), Err(Ok(Error::InsufficientBalance)));
}

#[test]
fn lowered_limit_applies_to_pending_requests_and_only_admin_sets_it() {
    let c = setup();
    let id = funded_circle(&c, 0);
    let rid = c.client.request_payout(&id, &c.alice, &c.bob, &200);
    c.client.approve(&id, &rid, &c.admin);
    assert_eq!(c.client.try_set_max_payout(&id, &c.alice, &50), Err(Ok(Error::Unauthorized)));
    c.client.set_max_payout(&id, &c.admin, &50);
    assert_eq!(c.client.try_execute(&id, &rid), Err(Ok(Error::ExceedsLimit)));
}

#[test]
fn governance_threshold_is_enforced_and_snapshotted() {
    let c = setup();
    let id = funded_circle(&c, 0);
    let early = c.client.request_payout(&id, &c.alice, &c.bob, &10);
    c.client.set_governance(&id, &c.admin, &2, &vec![&c.env, c.admin.clone(), c.bob.clone()]);

    // The pre-existing request keeps its threshold of 1.
    c.client.approve(&id, &early, &c.admin);
    assert!(c.client.try_execute(&id, &early).is_ok());

    // A new request needs two distinct approvals.
    let late = c.client.request_payout(&id, &c.alice, &c.bob, &10);
    c.client.approve(&id, &late, &c.admin);
    assert_eq!(c.client.try_approve(&id, &late, &c.admin), Err(Ok(Error::AlreadyApproved)));
    assert_eq!(c.client.try_execute(&id, &late), Err(Ok(Error::NotApproved)));
    assert_eq!(c.client.try_approve(&id, &late, &c.alice), Err(Ok(Error::NotApprover)));
    c.client.approve(&id, &late, &c.bob);
    c.client.execute(&id, &late);
    assert_eq!(c.client.try_execute(&id, &late), Err(Ok(Error::AlreadyExecuted)));
}

#[test]
fn invalid_or_unauthorized_governance_changes_are_rejected() {
    let c = setup();
    let id = funded_circle(&c, 0);
    let one = vec![&c.env, c.admin.clone()];
    assert_eq!(c.client.try_set_governance(&id, &c.admin, &0, &one), Err(Ok(Error::InvalidGovernance)));
    assert_eq!(c.client.try_set_governance(&id, &c.admin, &2, &one), Err(Ok(Error::InvalidGovernance)));
    assert_eq!(c.client.try_set_governance(&id, &c.alice, &1, &one), Err(Ok(Error::Unauthorized)));
}

#[test]
fn exit_rules() {
    let c = setup();
    let id = funded_circle(&c, 0);
    c.client.set_governance(&id, &c.admin, &1, &vec![&c.env, c.admin.clone(), c.bob.clone()]);
    assert_eq!(c.client.try_leave(&id, &c.admin), Err(Ok(Error::ExitRestricted)));
    assert_eq!(c.client.try_leave(&id, &c.bob), Err(Ok(Error::ExitRestricted)));

    let rid = c.client.request_payout(&id, &c.alice, &c.alice, &10);
    assert_eq!(c.client.try_leave(&id, &c.alice), Err(Ok(Error::PendingObligations)));
    c.client.approve(&id, &rid, &c.admin);
    c.client.execute(&id, &rid);

    c.client.leave(&id, &c.alice);
    assert!(!c.client.is_member(&id, &c.alice));
    assert_eq!(c.client.contribution_of(&id, &c.alice), 300);
    assert_eq!(c.client.try_contribute(&id, &c.alice, &1), Err(Ok(Error::NotMember)));
    assert_eq!(c.client.try_leave(&id, &c.alice), Err(Ok(Error::NotMember)));
}

#[test]
fn failed_operations_leave_state_unchanged() {
    let c = setup();
    let id = funded_circle(&c, 0);
    let tok = TokenClient::new(&c.env, &c.token);
    let stranger = Address::generate(&c.env);
    assert_eq!(c.client.try_contribute(&id, &stranger, &10), Err(Ok(Error::NotMember)));
    assert_eq!(c.client.try_contribute(&id, &c.alice, &-5), Err(Ok(Error::InvalidAmount)));
    assert_eq!(c.client.try_request_payout(&id, &stranger, &c.bob, &10), Err(Ok(Error::NotMember)));
    assert_eq!(c.client.try_get_circle(&99), Err(Ok(Error::NotFound)));
    assert_eq!(tok.balance(&c.contract), 500);
    assert_eq!(c.client.get_circle(&id).balance, 500);
}

#[test]
fn withdrawal_request_registration_and_validation() {
    let c = setup();
    let id = funded_circle(&c, 0);
    let stranger = Address::generate(&c.env);

    // Non-member cannot submit withdrawal request
    assert_eq!(
        c.client.try_request_withdrawal(&id, &stranger, &c.bob, &50),
        Err(Ok(Error::NotMember))
    );

    // Amount <= 0 fails
    assert_eq!(
        c.client.try_request_withdrawal(&id, &c.alice, &c.bob, &0),
        Err(Ok(Error::InvalidAmount))
    );
    assert_eq!(
        c.client.try_request_withdrawal(&id, &c.alice, &c.bob, &-10),
        Err(Ok(Error::InvalidAmount))
    );

    // Destination cannot be the contract address itself
    assert_eq!(
        c.client.try_request_withdrawal(&id, &c.alice, &c.contract, &50),
        Err(Ok(Error::InvalidDestination))
    );

    // Exceeding available funds fails
    assert_eq!(
        c.client.try_request_withdrawal(&id, &c.alice, &c.bob, &501),
        Err(Ok(Error::InsufficientBalance))
    );

    // Successful request creates unique identifiers and stores full withdrawal request information
    let rid0 = c.client.request_withdrawal(&id, &c.alice, &c.bob, &150);
    assert_eq!(rid0, 0);

    let req0 = c.client.get_withdrawal_request(&id, &rid0);
    assert_eq!(req0.requester, c.alice);
    assert_eq!(req0.to, c.bob);
    assert_eq!(req0.amount, 150);
    assert_eq!(req0.asset, c.token);
    assert_eq!(req0.status, RequestStatus::Pending);
    assert_eq!(req0.threshold, 1);
    assert!(!req0.executed);

    let rid1 = c.client.request_withdrawal(&id, &c.bob, &c.alice, &50);
    assert_eq!(rid1, 1);
    assert_ne!(rid0, rid1);

    // Available funds were properly reduced by reserved amount
    assert_eq!(c.client.get_available_balance(&id), 300);
}

#[test]
fn multi_member_payout_approval_and_progress_tracking() {
    let c = setup();
    let id = funded_circle(&c, 0);

    // Admin adds bob as an approver and updates threshold to 2
    c.client.add_approver(&id, &c.admin, &c.bob);
    c.client.set_approval_threshold(&id, &c.admin, &2);

    let rid = c.client.request_withdrawal(&id, &c.alice, &c.bob, &100);

    // Query approval progress initially
    let mut progress = c.client.get_approval_progress(&id, &rid);
    assert_eq!(progress.current_approvals, 0);
    assert_eq!(progress.required_threshold, 2);
    assert!(!progress.is_approved);
    assert!(!progress.is_executed);

    // Alice is not an approver
    assert_eq!(c.client.try_approve(&id, &rid, &c.alice), Err(Ok(Error::NotApprover)));

    // Admin approves
    c.client.approve(&id, &rid, &c.admin);

    // Duplicate approval rejected
    assert_eq!(c.client.try_approve(&id, &rid, &c.admin), Err(Ok(Error::AlreadyApproved)));

    // Execution fails before threshold is reached
    assert_eq!(c.client.try_execute(&id, &rid), Err(Ok(Error::NotApproved)));

    // Approval progress updated
    progress = c.client.get_approval_progress(&id, &rid);
    assert_eq!(progress.current_approvals, 1);
    assert!(!progress.is_approved);

    // Bob approves -> reaches threshold of 2
    c.client.approve(&id, &rid, &c.bob);
    progress = c.client.get_approval_progress(&id, &rid);
    assert_eq!(progress.current_approvals, 2);
    assert!(progress.is_approved);

    // Request status updated to Approved
    let req = c.client.get_payout(&id, &rid);
    assert_eq!(req.status, RequestStatus::Approved);

    // Execution succeeds now
    c.client.execute_payout(&id, &rid);
    let executed_req = c.client.get_payout(&id, &rid);
    assert_eq!(executed_req.status, RequestStatus::Executed);
    assert!(executed_req.executed);

    // Repeated execution prevented
    assert_eq!(c.client.try_execute(&id, &rid), Err(Ok(Error::AlreadyExecuted)));
}

#[test]
fn circle_balance_management_and_reconciliation() {
    let c = setup();
    let id = funded_circle(&c, 0);

    // Verify initial balance breakdown
    let initial_bal = c.client.get_circle_balance(&id);
    assert_eq!(initial_bal.asset, c.token);
    assert_eq!(initial_bal.total_balance, 500);
    assert_eq!(initial_bal.reserved_balance, 0);
    assert_eq!(initial_bal.available_balance, 500);
    assert_eq!(initial_bal.total_contributed, 500);
    assert_eq!(initial_bal.total_paid, 0);

    assert_eq!(c.client.get_available_balance(&id), 500);
    assert_eq!(c.client.get_total_contributions(&id), 500);
    assert_eq!(c.client.get_total_payouts(&id), 0);

    // Reconcile initial balance with token contract
    let recon = c.client.reconcile_circle_balance(&id);
    assert_eq!(recon.recorded_balance, 500);
    assert_eq!(recon.on_chain_token_balance, 500);
    assert!(recon.is_reconciled);

    // Reconcile asset-level balance
    let asset_recon = c.client.reconcile_asset_balance(&c.token);
    assert_eq!(asset_recon.recorded_balance, 500);
    assert!(asset_recon.is_reconciled);

    // Request payout reserves funds
    let rid = c.client.request_withdrawal(&id, &c.alice, &c.bob, &200);
    let after_request = c.client.get_circle_balance(&id);
    assert_eq!(after_request.total_balance, 500);
    assert_eq!(after_request.reserved_balance, 200);
    assert_eq!(after_request.available_balance, 300);

    // Failed execution (not approved) does not reduce balances
    assert_eq!(c.client.try_execute(&id, &rid), Err(Ok(Error::NotApproved)));
    let after_failed = c.client.get_circle_balance(&id);
    assert_eq!(after_failed.total_balance, 500);
    assert_eq!(after_failed.reserved_balance, 200);
    assert_eq!(after_failed.available_balance, 300);

    // Approve and execute payout
    c.client.approve(&id, &rid, &c.admin);
    c.client.execute(&id, &rid);

    // Completed payout atomically updates accounting
    let after_exec = c.client.get_circle_balance(&id);
    assert_eq!(after_exec.total_balance, 300);
    assert_eq!(after_exec.reserved_balance, 0);
    assert_eq!(after_exec.available_balance, 300);
    assert_eq!(after_exec.total_contributed, 500);
    assert_eq!(after_exec.total_paid, 200);

    assert_eq!(c.client.get_total_payouts(&id), 200);
    assert_eq!(c.client.get_available_balance(&id), 300);

    // Reconcile balance after payout
    let recon_after = c.client.reconcile_circle_balance(&id);
    assert_eq!(recon_after.recorded_balance, 300);
    assert_eq!(recon_after.on_chain_token_balance, 300);
    assert!(recon_after.is_reconciled);

    // Asset-specific info
    let asset_info = c.client.get_asset_balance_info(&c.token);
    assert_eq!(asset_info.total_balance, 300);
    assert_eq!(asset_info.reserved_balance, 0);
    assert_eq!(asset_info.available_balance, 300);
    assert_eq!(asset_info.total_contributed, 500);
    assert_eq!(asset_info.total_paid, 200);
}
