use super::*;
use soroban_sdk::testutils::{Address as _, Events, Ledger as _};
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
    client.initialize(&admin);
    client.add_supported_asset(&admin, &token);
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
    c.client.add_supported_asset(&c.admin, &other);
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
fn supported_assets_are_authorized_queryable_and_used_for_financial_operations() {
    let c = setup();
    let other = asset(&c.env, &[&c.alice]);

    assert!(c.client.is_supported_asset(&c.token));
    assert!(!c.client.is_supported_asset(&other));
    let configured_assets = c.client.get_supported_assets();
    assert_eq!(configured_assets.len(), 1);
    assert!(configured_assets.contains(&c.token));
    assert_eq!(c.client.try_initialize(&c.alice), Err(Ok(Error::AlreadyInitialized)));
    assert_eq!(
        c.client.try_create_circle(&c.admin, &other, &0),
        Err(Ok(Error::UnsupportedAsset))
    );
    assert_eq!(
        c.client.try_add_supported_asset(&c.alice, &other),
        Err(Ok(Error::Unauthorized))
    );

    c.client.add_supported_asset(&c.admin, &other);
    c.client.add_supported_asset(&c.admin, &other);
    assert!(c.client.is_supported_asset(&other));
    assert_eq!(c.client.get_supported_assets().len(), 2);

    let id = c.client.create_circle(&c.admin, &other, &0);
    c.client.join(&id, &c.alice);
    let contribution_id = c.client.contribute_with_record(&id, &c.alice, &50);
    assert_eq!(c.client.get_contribution_record(&contribution_id).asset, other);

    let payout_id = c.client.request_payout(&id, &c.alice, &c.bob, &50);
    assert_eq!(c.client.get_payout(&id, &payout_id).asset, other);
    assert_eq!(
        c.client.try_remove_supported_asset(&c.admin, &other),
        Err(Ok(Error::AssetInUse))
    );
    c.client.approve(&id, &payout_id, &c.admin);
    c.client.execute(&id, &payout_id);
    assert_eq!(TokenClient::new(&c.env, &other).balance(&c.bob), 1_050);

    c.client.remove_supported_asset(&c.admin, &other);
    assert!(!c.client.is_supported_asset(&other));
    assert_eq!(c.client.get_supported_assets().len(), 1);
    assert_eq!(
        c.client.try_contribute(&id, &c.alice, &1),
        Err(Ok(Error::UnsupportedAsset))
    );
    assert_eq!(
        c.client.try_request_payout(&id, &c.alice, &c.bob, &1),
        Err(Ok(Error::UnsupportedAsset))
    );
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
fn contribution_rules_are_authorized_validated_and_enforced_on_every_path() {
    let c = setup();
    let id = c.client.create_circle(&c.admin, &c.token, &0);
    c.client.join(&id, &c.alice);

    let defaults = c.client.get_contribution_rules(&id);
    assert_eq!(
        defaults,
        ContributionRules {
            fixed_amount: 0,
            min_amount: 0,
            max_amount: 0,
        }
    );
    assert_eq!(
        c.client.try_set_contribution_rules(&id, &c.alice, &25, &10, &100),
        Err(Ok(Error::Unauthorized))
    );
    assert_eq!(
        c.client.try_set_contribution_rules(&id, &c.admin, &0, &10, &9),
        Err(Ok(Error::InvalidContributionRules))
    );
    assert_eq!(
        c.client.try_set_contribution_rules(&id, &c.admin, &5, &10, &100),
        Err(Ok(Error::InvalidContributionRules))
    );
    assert_eq!(
        c.client.try_set_contribution_rules(&id, &c.admin, &0, &-1, &100),
        Err(Ok(Error::InvalidContributionRules))
    );

    c.client.set_contribution_rules(&id, &c.admin, &0, &10, &100);
    assert_eq!(
        c.client.get_contribution_rules(&id),
        ContributionRules {
            fixed_amount: 0,
            min_amount: 10,
            max_amount: 100,
        }
    );
    let goal_id = c.client.create_goal(&id, &c.admin, &1_000, &0);

    assert_eq!(
        c.client.try_contribute(&id, &c.alice, &9),
        Err(Ok(Error::ContributionRuleViolation))
    );
    assert_eq!(
        c.client.try_contribute_with_record(&id, &c.alice, &101),
        Err(Ok(Error::ContributionRuleViolation))
    );
    assert_eq!(
        c.client.try_contribute_to_goal(&id, &goal_id, &c.alice, &9),
        Err(Ok(Error::ContributionRuleViolation))
    );
    assert_eq!(TokenClient::new(&c.env, &c.token).balance(&c.alice), 1_000);
    assert_eq!(c.client.get_total_records_count(), 0);

    c.client.contribute(&id, &c.alice, &10);
    c.client.contribute_with_record(&id, &c.alice, &100);
    c.client.contribute_to_goal(&id, &goal_id, &c.alice, &50);

    c.client.set_contribution_rules(&id, &c.admin, &25, &10, &100);
    assert_eq!(
        c.client.try_contribute(&id, &c.alice, &26),
        Err(Ok(Error::ContributionRuleViolation))
    );
    assert_eq!(
        c.client.try_contribute_with_record(&id, &c.alice, &24),
        Err(Ok(Error::ContributionRuleViolation))
    );
    assert_eq!(
        c.client.try_contribute_to_goal(&id, &goal_id, &c.alice, &50),
        Err(Ok(Error::ContributionRuleViolation))
    );

    c.client.contribute(&id, &c.alice, &25);
    c.client.contribute_with_record(&id, &c.alice, &25);
    c.client.contribute_to_goal(&id, &goal_id, &c.alice, &25);
    assert_eq!(c.client.get_circle_balance(&id).total_balance, 235);
    assert_eq!(TokenClient::new(&c.env, &c.token).balance(&c.alice), 765);
}

#[test]
fn circle_configuration_is_initialized_authorized_and_preserves_financial_state() {
    let c = setup();
    let id = funded_circle(&c, 0);

    let initial = c.client.get_circle_configuration(&id);
    assert_eq!(initial.asset, c.token);
    assert!(initial.asset_supported);
    assert_eq!(initial.contribution_rules.min_amount, 0);
    assert_eq!(initial.max_members, 0);
    assert_eq!(initial.member_count, 3);
    assert_eq!(initial.max_payout, 0);
    assert_eq!(initial.withdrawal_threshold, 1);
    assert!(initial.withdrawal_approvers.contains(&c.admin));

    assert_eq!(
        c.client.try_set_member_limit(&id, &c.alice, &3),
        Err(Ok(Error::Unauthorized))
    );
    c.client.set_contribution_rules(&id, &c.admin, &0, &10, &100);
    c.client.set_max_payout(&id, &c.admin, &80);
    c.client.set_governance(
        &id,
        &c.admin,
        &2,
        &vec![&c.env, c.admin.clone(), c.bob.clone()],
    );
    c.client.set_member_limit(&id, &c.admin, &3);

    let (emitter, topics, data) = c.env.events().all().last().unwrap();
    assert_eq!(emitter, c.contract);
    let event_name: soroban_sdk::Symbol = topics.get(0).unwrap().into_val(&c.env);
    let member_limit: u32 = data.into_val(&c.env);
    assert_eq!((event_name, member_limit), (symbol_short!("mem_cap"), 3));

    assert_eq!(
        c.client.try_set_member_limit(&id, &c.admin, &2),
        Err(Ok(Error::InvalidConfiguration))
    );
    let stranger = Address::generate(&c.env);
    assert_eq!(
        c.client.try_join(&id, &stranger),
        Err(Ok(Error::MembershipLimitReached))
    );
    c.client.join(&id, &c.alice);
    assert_eq!(c.client.get_circle_configuration(&id).member_count, 3);

    c.client.set_member_limit(&id, &c.admin, &4);
    c.client.join(&id, &stranger);
    c.client.leave(&id, &c.alice);
    let updated = c.client.get_circle_configuration(&id);
    assert_eq!(updated.max_members, 4);
    assert_eq!(updated.member_count, 3);
    assert_eq!(updated.contribution_rules.min_amount, 10);
    assert_eq!(updated.max_payout, 80);
    assert_eq!(updated.withdrawal_threshold, 2);
    assert_eq!(updated.withdrawal_approvers.len(), 2);
    assert_eq!(c.client.get_circle_balance(&id).total_balance, 500);
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

#[test]
fn test_contribution_accounting_per_member_and_asset() {
    let c = setup();
    let id = c.client.create_circle(&c.admin, &c.token, &0);
    c.client.join(&id, &c.alice);
    c.client.join(&id, &c.bob);

    // Initial state
    assert_eq!(c.client.contribution_of(&id, &c.alice), 0);
    assert_eq!(c.client.get_member_contribution_count(&id, &c.alice), 0);
    assert_eq!(c.client.get_member_asset_contribution(&c.alice, &c.token), 0);

    // Alice contributes twice: 150 + 150 = 300 (integer precision preserved, no double count)
    c.client.contribute(&id, &c.alice, &150);
    c.client.contribute(&id, &c.alice, &150);

    assert_eq!(c.client.contribution_of(&id, &c.alice), 300);
    assert_eq!(c.client.get_member_contribution_count(&id, &c.alice), 2);
    assert_eq!(c.client.get_member_asset_contribution(&c.alice, &c.token), 300);

    // Asset balances are tracked
    assert_eq!(c.client.get_asset_balance(&c.token), 300);
    assert_eq!(c.client.get_total_contributions(&id), 300);

    // Multiple assets tracked independently
    let other_token = asset(&c.env, &[&c.alice]);
    c.client.add_supported_asset(&c.admin, &other_token);
    let other_circle = c.client.create_circle(&c.admin, &other_token, &0);
    c.client.join(&other_circle, &c.alice);
    c.client.contribute(&other_circle, &c.alice, &250);

    assert_eq!(c.client.get_member_asset_contribution(&c.alice, &c.token), 300);
    assert_eq!(c.client.get_member_asset_contribution(&c.alice, &other_token), 250);
    assert_eq!(c.client.get_asset_balance(&c.token), 300);
    assert_eq!(c.client.get_asset_balance(&other_token), 250);

    // Large integer precision test
    let large_asset = asset(&c.env, &[]);
    StellarAssetClient::new(&c.env, &large_asset).mint(&c.bob, &10_000_000_000_000_000i128);
    c.client.add_supported_asset(&c.admin, &large_asset);
    let large_circle = c.client.create_circle(&c.admin, &large_asset, &0);
    c.client.join(&large_circle, &c.bob);
    c.client.contribute(&large_circle, &c.bob, &10_000_000_000_000_000i128);
    assert_eq!(c.client.contribution_of(&large_circle, &c.bob), 10_000_000_000_000_000i128);
    assert_eq!(c.client.get_asset_balance(&large_asset), 10_000_000_000_000_000i128);

    // Failed transactions do not alter balances
    let stranger = Address::generate(&c.env);
    assert_eq!(c.client.try_contribute(&id, &stranger, &50), Err(Ok(Error::NotMember)));
    assert_eq!(c.client.try_contribute(&id, &c.alice, &-10), Err(Ok(Error::InvalidAmount)));
    assert_eq!(c.client.try_contribute(&id, &c.alice, &0), Err(Ok(Error::InvalidAmount)));

    // Verify balances remain unchanged after failures
    assert_eq!(c.client.contribution_of(&id, &c.alice), 300);
    assert_eq!(c.client.get_member_contribution_count(&id, &c.alice), 2);
    assert_eq!(c.client.get_asset_balance(&c.token), 300);
}

#[test]
fn test_goal_contribution_tracking() {
    let c = setup();
    let id = c.client.create_circle(&c.admin, &c.token, &0);
    c.client.join(&id, &c.alice);
    c.client.join(&id, &c.bob);

    // Create goal with target 500
    let goal_id = c.client.create_goal(&id, &c.admin, &500, &0);
    assert_eq!(goal_id, 0);
    assert_eq!(c.client.get_goal_count(&id), 1);

    let initial_goal = c.client.get_goal(&id, &goal_id);
    assert_eq!(initial_goal.target_amount, 500);
    assert_eq!(initial_goal.current_amount, 0);
    assert_eq!(initial_goal.token, c.token);
    assert!(!initial_goal.completed);

    let initial_prog = c.client.get_goal_progress(&id, &goal_id);
    assert_eq!(initial_prog.target_amount, 500);
    assert_eq!(initial_prog.current_amount, 0);
    assert_eq!(initial_prog.remaining_amount, 500);
    assert_eq!(initial_prog.progress_bps, 0);
    assert!(!initial_prog.is_completed);

    // Alice contributes 200 towards goal atomically
    let rec_id = c.client.contribute_to_goal(&id, &goal_id, &c.alice, &200);
    assert_eq!(c.client.get_member_goal_contribution(&id, &goal_id, &c.alice), 200);
    assert_eq!(c.client.contribution_of(&id, &c.alice), 200);
    assert_eq!(c.client.get_circle_balance(&id).total_balance, 200);

    let prog_after_alice = c.client.get_goal_progress(&id, &goal_id);
    assert_eq!(prog_after_alice.current_amount, 200);
    assert_eq!(prog_after_alice.remaining_amount, 300);
    assert_eq!(prog_after_alice.progress_bps, 4_000); // 40.00%
    assert!(!prog_after_alice.is_completed);

    // Verify record associates with goal
    let rec = c.client.get_contribution_record(&rec_id);
    assert_eq!(rec.goal_id, Some(goal_id));
    assert_eq!(rec.amount, 200);

    // Bob contributes 300 to reach and complete the goal
    c.client.contribute_to_goal(&id, &goal_id, &c.bob, &300);
    let final_prog = c.client.get_goal_progress(&id, &goal_id);
    assert_eq!(final_prog.current_amount, 500);
    assert_eq!(final_prog.remaining_amount, 0);
    assert_eq!(final_prog.progress_bps, 10_000); // 100.00%
    assert!(final_prog.is_completed);

    let completed_goal = c.client.get_goal(&id, &goal_id);
    assert!(completed_goal.completed);

    // Rejection of further contributions to completed goal
    assert_eq!(
        c.client.try_contribute_to_goal(&id, &goal_id, &c.alice, &50),
        Err(Ok(Error::GoalAlreadyCompleted))
    );

    // Unrelated goal checks
    assert_eq!(
        c.client.try_contribute_to_goal(&id, &999, &c.alice, &50),
        Err(Ok(Error::GoalNotFound))
    );

    // Goal creation validation: non-member cannot create, amount must be positive
    let stranger = Address::generate(&c.env);
    assert_eq!(c.client.try_create_goal(&id, &stranger, &100, &0), Err(Ok(Error::NotMember)));
    assert_eq!(c.client.try_create_goal(&id, &c.alice, &0, &0), Err(Ok(Error::InvalidAmount)));
    assert_eq!(c.client.try_create_goal(&id, &c.alice, &-10, &0), Err(Ok(Error::InvalidAmount)));
}

#[test]
fn test_contribution_deadline_validation() {
    let c = setup();
    let id = c.client.create_circle(&c.admin, &c.token, &0);
    c.client.join(&id, &c.alice);

    // Unset deadline (0) allows contributions at any time
    assert_eq!(c.client.get_circle_deadline(&id), 0);
    assert!(c.client.is_circle_active(&id));
    assert!(c.client.try_contribute(&id, &c.alice, &50).is_ok());

    // Admin sets deadline to timestamp 1_000_000
    c.client.set_circle_deadline(&id, &c.admin, &1_000_000);
    assert_eq!(c.client.get_circle_deadline(&id), 1_000_000);

    // Unauthorized non-admin cannot change deadline
    assert_eq!(
        c.client.try_set_circle_deadline(&id, &c.alice, &2_000_000),
        Err(Ok(Error::Unauthorized))
    );

    // Boundary 1: Within deadline (timestamp < deadline)
    c.env.ledger().set_timestamp(999_999);
    assert!(c.client.is_circle_active(&id));
    assert!(c.client.try_contribute(&id, &c.alice, &50).is_ok());

    // Boundary 2: Exact deadline boundary (timestamp == deadline)
    c.env.ledger().set_timestamp(1_000_000);
    assert!(c.client.is_circle_active(&id));
    assert!(c.client.try_contribute(&id, &c.alice, &50).is_ok());

    // Boundary 3: Expired period (timestamp == deadline + 1)
    c.env.ledger().set_timestamp(1_000_001);
    assert!(!c.client.is_circle_active(&id));
    assert_eq!(
        c.client.try_contribute(&id, &c.alice, &50),
        Err(Ok(Error::DeadlineExpired))
    );
    assert_eq!(
        c.client.try_contribute_with_record(&id, &c.alice, &50),
        Err(Ok(Error::DeadlineExpired))
    );

    // Goal-specific deadline validation
    // Circle deadline reset to future
    c.client.set_circle_deadline(&id, &c.admin, &5_000_000);
    // Create goal with deadline at 2_000_000
    let goal_id = c.client.create_goal(&id, &c.admin, &300, &2_000_000);

    // Contributing to goal before deadline succeeds
    c.env.ledger().set_timestamp(1_999_999);
    assert!(c.client.try_contribute_to_goal(&id, &goal_id, &c.alice, &100).is_ok());

    // Contributing to goal at exact boundary succeeds
    c.env.ledger().set_timestamp(2_000_000);
    assert!(c.client.try_contribute_to_goal(&id, &goal_id, &c.alice, &50).is_ok());

    // Contributing to goal past deadline fails
    c.env.ledger().set_timestamp(2_000_001);
    assert_eq!(
        c.client.try_contribute_to_goal(&id, &goal_id, &c.alice, &50),
        Err(Ok(Error::DeadlineExpired))
    );

    // Cannot create a goal with an expired deadline
    assert_eq!(
        c.client.try_create_goal(&id, &c.admin, &300, &1_000_000),
        Err(Ok(Error::InvalidDeadline))
    );
}

#[test]
fn test_contribution_transaction_records() {
    let c = setup();
    let id = c.client.create_circle(&c.admin, &c.token, &0);
    c.client.join(&id, &c.alice);
    c.client.join(&id, &c.bob);

    assert_eq!(c.client.get_total_records_count(), 0);
    assert_eq!(c.client.get_circle_contribution_count(&id), 0);

    // Alice contributes with unique record identifier
    let rec0_id = c.client.contribute_with_record(&id, &c.alice, &150);
    assert_eq!(rec0_id, 0);

    let rec0 = c.client.get_contribution_record(&rec0_id);
    assert_eq!(rec0.id, 0);
    assert_eq!(rec0.circle_id, id);
    assert_eq!(rec0.contributor, c.alice);
    assert_eq!(rec0.amount, 150);
    assert_eq!(rec0.asset, c.token);
    assert_eq!(rec0.goal_id, None);

    // Bob contributes with unique monotonic identifier
    let rec1_id = c.client.contribute_with_record(&id, &c.bob, &250);
    assert_eq!(rec1_id, 1);
    assert_ne!(rec0_id, rec1_id);

    let rec1 = c.client.get_contribution_record(&rec1_id);
    assert_eq!(rec1.id, 1);
    assert_eq!(rec1.contributor, c.bob);
    assert_eq!(rec1.amount, 250);

    // Goal contribution generates record with goal association
    let goal_id = c.client.create_goal(&id, &c.admin, &500, &0);
    let rec2_id = c.client.contribute_to_goal(&id, &goal_id, &c.alice, &100);
    assert_eq!(rec2_id, 2);

    let rec2 = c.client.get_contribution_record(&rec2_id);
    assert_eq!(rec2.id, 2);
    assert_eq!(rec2.goal_id, Some(goal_id));
    assert_eq!(rec2.amount, 100);

    // Query circle contribution count and global count
    assert_eq!(c.client.get_circle_contribution_count(&id), 3);
    assert_eq!(c.client.get_total_records_count(), 3);

    // Paginated history retrieval
    let all_records = c.client.get_circle_contributions(&id, &0, &10);
    assert_eq!(all_records.len(), 3);
    assert_eq!(all_records.get(0).unwrap().id, 0);
    assert_eq!(all_records.get(1).unwrap().id, 1);
    assert_eq!(all_records.get(2).unwrap().id, 2);

    // Sub-slice pagination
    let paged = c.client.get_circle_contributions(&id, &1, &2);
    assert_eq!(paged.len(), 2);
    assert_eq!(paged.get(0).unwrap().id, 1);
    assert_eq!(paged.get(1).unwrap().id, 2);

    // Query non-existent record
    assert_eq!(c.client.try_get_contribution_record(&999), Err(Ok(Error::NotFound)));

    // Atomicity: failed contribution creates no record
    let stranger = Address::generate(&c.env);
    assert_eq!(c.client.try_contribute_with_record(&id, &stranger, &50), Err(Ok(Error::NotMember)));
    assert_eq!(c.client.get_circle_contribution_count(&id), 3);
    assert_eq!(c.client.get_total_records_count(), 3);
}
