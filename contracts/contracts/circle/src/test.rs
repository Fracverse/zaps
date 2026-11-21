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
