//! Unit tests for circle initialization (Issue 73), membership (Issue 74),
//! role management (Issue 75) and contributions (Issue 79).

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Events, Ledger},
    vec, Address, Env, IntoVal, String, Symbol, TryIntoVal, Val, Vec,
};

const CONTRIBUTION: i128 = 1_000;
const MAX_WITHDRAWAL: i128 = 5_000;
const MAX_MEMBERS: u32 = 5;
const LONG_NAME: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"; // 65

struct Ctx {
    env: Env,
    client: CirclesContractClient<'static>,
    owner: Address,
    admin: Address,
    asset: Address,
}

fn settings(env: &Env, max_members: u32) -> CircleSettings {
    CircleSettings {
        name: String::from_str(env, "Family Circle"),
        max_members,
        contribution_amount: CONTRIBUTION,
        max_withdrawal: MAX_WITHDRAWAL,
    }
}

fn uninitialized() -> (Env, CirclesContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, CirclesContract);
    let client = CirclesContractClient::new(&env, &contract_id);
    (env, client)
}

fn setup_with(max_members: u32) -> Ctx {
    let (env, client) = uninitialized();
    let owner = Address::generate(&env);
    let admin = Address::generate(&env);
    let asset = Address::generate(&env);
    client.initialize(
        &owner,
        &admin,
        &settings(&env, max_members),
        &vec![&env, asset.clone()],
    );
    Ctx {
        env,
        client,
        owner,
        admin,
        asset,
    }
}

fn setup() -> Ctx {
    setup_with(MAX_MEMBERS)
}

/// Add a fresh regular member through the owner.
fn new_member(ctx: &Ctx) -> Address {
    let member = Address::generate(&ctx.env);
    ctx.client.add_member(&ctx.owner, &member);
    member
}

fn assert_err<T: core::fmt::Debug, E: core::fmt::Debug>(
    res: Result<T, Result<Error, E>>,
    expected: Error,
) {
    match res {
        Err(Ok(err)) => assert_eq!(err, expected),
        other => panic!("expected {:?}, got {:?}", expected, other),
    }
}

/// Data payload of the most recent event in the last invocation whose topics
/// contain `name`.
fn find_event(env: &Env, name: &str) -> Option<Val> {
    let topic: Val = Symbol::new(env, name).into_val(env);
    let mut found = None;
    for item in env.events().all().iter() {
        if item.1.contains(topic) {
            found = Some(item.2);
        }
    }
    found
}

fn advance_time(env: &Env, secs: u64) {
    env.ledger().with_mut(|li| {
        li.timestamp = li.timestamp.saturating_add(secs);
    });
}

// ─── Issue 73: initialization ───────────────────────────────────────────────

#[test]
fn initialize_sets_owner_admin_settings_and_assets() {
    let ctx = setup();
    let c = &ctx.client;

    assert!(c.is_initialized());
    assert_eq!(c.get_owner(), ctx.owner);
    assert_eq!(c.get_settings(), settings(&ctx.env, MAX_MEMBERS));
    assert_eq!(c.get_supported_assets(), vec![&ctx.env, ctx.asset.clone()]);
    assert!(c.is_supported_asset(&ctx.asset));
    assert!(!c.is_supported_asset(&Address::generate(&ctx.env)));
    assert_eq!(c.get_role(&ctx.owner), Some(Role::Owner));
    assert_eq!(c.get_role(&ctx.admin), Some(Role::Admin));
    assert_eq!(c.member_count(), 2);
    assert_eq!(c.created_at(), ctx.env.ledger().timestamp());
}

#[test]
fn initialize_emits_circle_initialized_event() {
    let (env, client) = uninitialized();
    let owner = Address::generate(&env);
    let admin = Address::generate(&env);
    let asset = Address::generate(&env);
    client.initialize(
        &owner,
        &admin,
        &settings(&env, MAX_MEMBERS),
        &vec![&env, asset],
    );

    let data = find_event(&env, "CircleInitialized").expect("CircleInitialized not emitted");
    let (ev_admin, ev_max, ev_assets): (Address, u32, u32) = data.try_into_val(&env).unwrap();
    assert_eq!(ev_admin, admin);
    assert_eq!(ev_max, MAX_MEMBERS);
    assert_eq!(ev_assets, 1);
    assert!(find_event(&env, "RoleChanged").is_some());
}

#[test]
fn initialize_with_creator_as_admin_has_single_member() {
    let (env, client) = uninitialized();
    let owner = Address::generate(&env);
    client.initialize(
        &owner,
        &owner,
        &settings(&env, MAX_MEMBERS),
        &vec![&env, Address::generate(&env)],
    );

    assert_eq!(client.member_count(), 1);
    assert_eq!(client.get_role(&owner), Some(Role::Owner));
}

#[test]
fn reinitialization_is_rejected() {
    let ctx = setup();
    let other = Address::generate(&ctx.env);
    let res = ctx.client.try_initialize(
        &other,
        &other,
        &settings(&ctx.env, 10),
        &vec![&ctx.env, ctx.asset.clone()],
    );
    assert_err(res, Error::AlreadyInitialized);
    assert_eq!(ctx.client.get_owner(), ctx.owner);
}

#[test]
fn queries_before_initialization_fail() {
    let (env, client) = uninitialized();
    assert!(!client.is_initialized());
    assert_err(client.try_get_owner(), Error::NotInitialized);
    assert_err(client.try_get_settings(), Error::NotInitialized);
    assert_err(client.try_get_supported_assets(), Error::NotInitialized);
    assert_err(client.try_created_at(), Error::NotInitialized);
    assert_eq!(client.member_count(), 0);
    assert!(!client.is_supported_asset(&Address::generate(&env)));
    assert_err(
        client.try_verify_member(&Address::generate(&env)),
        Error::NotInitialized,
    );
}

#[test]
fn initialize_rejects_invalid_settings() {
    let (env, client) = uninitialized();
    let owner = Address::generate(&env);
    let assets = vec![&env, Address::generate(&env)];

    let mut s = settings(&env, MAX_MEMBERS);
    s.name = String::from_str(&env, "");
    assert_err(
        client.try_initialize(&owner, &owner, &s, &assets),
        Error::InvalidName,
    );

    let mut s = settings(&env, MAX_MEMBERS);
    s.name = String::from_str(&env, LONG_NAME);
    assert_err(
        client.try_initialize(&owner, &owner, &s, &assets),
        Error::InvalidName,
    );

    for bad in [0, 1, MAX_MEMBERS_CAP + 1] {
        assert_err(
            client.try_initialize(&owner, &owner, &settings(&env, bad), &assets),
            Error::InvalidMaxMembers,
        );
    }

    let mut s = settings(&env, MAX_MEMBERS);
    s.contribution_amount = 0;
    assert_err(
        client.try_initialize(&owner, &owner, &s, &assets),
        Error::InvalidAmount,
    );

    let mut s = settings(&env, MAX_MEMBERS);
    s.max_withdrawal = -1;
    assert_err(
        client.try_initialize(&owner, &owner, &s, &assets),
        Error::InvalidAmount,
    );

    assert!(!client.is_initialized());
}

#[test]
fn initialize_rejects_invalid_assets() {
    let (env, client) = uninitialized();
    let owner = Address::generate(&env);
    let s = settings(&env, MAX_MEMBERS);

    assert_err(
        client.try_initialize(&owner, &owner, &s, &Vec::new(&env)),
        Error::NoSupportedAssets,
    );

    let mut too_many = Vec::new(&env);
    for _ in 0..(MAX_ASSETS + 1) {
        too_many.push_back(Address::generate(&env));
    }
    assert_err(
        client.try_initialize(&owner, &owner, &s, &too_many),
        Error::TooManyAssets,
    );

    let asset = Address::generate(&env);
    assert_err(
        client.try_initialize(&owner, &owner, &s, &vec![&env, asset.clone(), asset]),
        Error::DuplicateAsset,
    );

    assert!(!client.is_initialized());
}

#[test]
fn initialize_requires_creator_auth() {
    let env = Env::default();
    let contract_id = env.register_contract(None, CirclesContract);
    let client = CirclesContractClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    let res = client.try_initialize(
        &owner,
        &owner,
        &settings(&env, MAX_MEMBERS),
        &vec![&env, Address::generate(&env)],
    );
    assert!(res.is_err());
    assert!(!client.is_initialized());
}

// ─── Issue 74: membership ───────────────────────────────────────────────────

#[test]
fn owner_and_admin_can_add_members() {
    let ctx = setup();
    let a = Address::generate(&ctx.env);
    let b = Address::generate(&ctx.env);

    ctx.client.add_member(&ctx.owner, &a);
    ctx.client.add_member(&ctx.admin, &b);

    assert!(ctx.client.is_member(&a));
    assert!(ctx.client.is_member(&b));
    assert_eq!(ctx.client.get_role(&b), Some(Role::Member));
    assert_eq!(ctx.client.member_count(), 4);
}

#[test]
fn add_member_emits_event() {
    let ctx = setup();
    let member = Address::generate(&ctx.env);
    ctx.client.add_member(&ctx.admin, &member);

    let data = find_event(&ctx.env, "MemberAdded").expect("MemberAdded not emitted");
    let (caller, count): (Address, u32) = data.try_into_val(&ctx.env).unwrap();
    assert_eq!(caller, ctx.admin);
    assert_eq!(count, 3);
}

#[test]
fn duplicate_membership_is_rejected() {
    let ctx = setup();
    let member = new_member(&ctx);

    assert_err(
        ctx.client.try_add_member(&ctx.owner, &member),
        Error::AlreadyMember,
    );
    assert_err(
        ctx.client.try_add_member(&ctx.owner, &ctx.admin),
        Error::AlreadyMember,
    );
    assert_eq!(ctx.client.member_count(), 3);
}

#[test]
fn membership_limit_is_enforced() {
    let ctx = setup_with(3);
    new_member(&ctx);
    assert_eq!(ctx.client.member_count(), 3);

    assert_err(
        ctx.client
            .try_add_member(&ctx.owner, &Address::generate(&ctx.env)),
        Error::MembershipLimitReached,
    );
}

#[test]
fn members_and_outsiders_cannot_add_members() {
    let ctx = setup();
    let member = new_member(&ctx);
    let outsider = Address::generate(&ctx.env);

    assert_err(
        ctx.client
            .try_add_member(&member, &Address::generate(&ctx.env)),
        Error::Unauthorized,
    );
    assert_err(
        ctx.client
            .try_add_member(&outsider, &Address::generate(&ctx.env)),
        Error::Unauthorized,
    );
}

#[test]
fn membership_changes_require_auth() {
    let ctx = setup();
    ctx.env.mock_auths(&[]);

    let res = ctx
        .client
        .try_add_member(&ctx.owner, &Address::generate(&ctx.env));
    assert!(res.is_err());
    assert_eq!(ctx.client.member_count(), 2);
}

#[test]
fn admin_removes_member_and_event_is_emitted() {
    let ctx = setup();
    let member = new_member(&ctx);

    ctx.client.remove_member(&ctx.admin, &member);
    assert!(!ctx.client.is_member(&member));
    assert_eq!(ctx.client.get_role(&member), None);
    assert_eq!(ctx.client.member_count(), 2);

    let data = find_event(&ctx.env, "MemberRemoved").expect("MemberRemoved not emitted");
    let (caller, count): (Address, u32) = data.try_into_val(&ctx.env).unwrap();
    assert_eq!(caller, ctx.admin);
    assert_eq!(count, 2);

    // A removed member frees a slot and may be re-added.
    ctx.client.add_member(&ctx.owner, &member);
    assert!(ctx.client.is_member(&member));
}

#[test]
fn remove_member_governance_rules() {
    let ctx = setup();
    let second_admin = new_member(&ctx);
    ctx.client
        .set_role(&ctx.owner, &second_admin, &Role::Admin);
    let member = new_member(&ctx);

    assert_err(
        ctx.client.try_remove_member(&ctx.admin, &ctx.owner),
        Error::CannotRemoveOwner,
    );
    assert_err(
        ctx.client.try_remove_member(&ctx.admin, &second_admin),
        Error::Unauthorized,
    );
    assert_err(
        ctx.client.try_remove_member(&member, &ctx.admin),
        Error::Unauthorized,
    );
    assert_err(
        ctx.client
            .try_remove_member(&ctx.owner, &Address::generate(&ctx.env)),
        Error::NotMember,
    );

    ctx.client.remove_member(&ctx.owner, &second_admin);
    assert!(!ctx.client.is_member(&second_admin));
}

#[test]
fn members_can_leave_but_owner_cannot() {
    let ctx = setup();
    let member = new_member(&ctx);

    ctx.client.leave_circle(&member);
    assert!(!ctx.client.is_member(&member));
    assert_eq!(ctx.client.member_count(), 2);

    assert_err(
        ctx.client.try_leave_circle(&ctx.owner),
        Error::CannotRemoveOwner,
    );
    assert_err(ctx.client.try_leave_circle(&member), Error::NotMember);
}

#[test]
fn verify_member_for_financial_actions() {
    let ctx = setup();
    let member = new_member(&ctx);

    ctx.client.verify_member(&member);
    ctx.client.verify_member(&ctx.owner);
    assert_err(
        ctx.client.try_verify_member(&Address::generate(&ctx.env)),
        Error::NotMember,
    );

    ctx.client.remove_member(&ctx.owner, &member);
    assert_err(ctx.client.try_verify_member(&member), Error::NotMember);
    assert_err(
        ctx.client
            .try_authorize_withdrawal(&ctx.admin, &member, &ctx.asset, &100),
        Error::NotMember,
    );
}

// ─── Issue 75: roles & permissions ──────────────────────────────────────────

#[test]
fn permission_matrix() {
    let ctx = setup();
    let member = new_member(&ctx);
    let outsider = Address::generate(&ctx.env);
    let c = &ctx.client;

    for p in [
        Permission::Participate,
        Permission::ManageMembers,
        Permission::AuthorizeWithdrawal,
        Permission::ModifyFinancialRules,
        Permission::ManageRoles,
    ] {
        assert!(c.has_permission(&ctx.owner, &p));
        assert!(!c.has_permission(&outsider, &p));
    }

    assert!(c.has_permission(&ctx.admin, &Permission::Participate));
    assert!(c.has_permission(&ctx.admin, &Permission::ManageMembers));
    assert!(c.has_permission(&ctx.admin, &Permission::AuthorizeWithdrawal));
    assert!(!c.has_permission(&ctx.admin, &Permission::ModifyFinancialRules));
    assert!(!c.has_permission(&ctx.admin, &Permission::ManageRoles));

    assert!(c.has_permission(&member, &Permission::Participate));
    assert!(!c.has_permission(&member, &Permission::ManageMembers));
    assert!(!c.has_permission(&member, &Permission::AuthorizeWithdrawal));
    assert!(!c.has_permission(&member, &Permission::ModifyFinancialRules));
    assert!(!c.has_permission(&member, &Permission::ManageRoles));
}

#[test]
fn owner_grants_and_revokes_admin() {
    let ctx = setup();
    let member = new_member(&ctx);

    ctx.client.set_role(&ctx.owner, &member, &Role::Admin);
    assert_eq!(ctx.client.get_role(&member), Some(Role::Admin));

    let data = find_event(&ctx.env, "RoleChanged").expect("RoleChanged not emitted");
    let (caller, old, new): (Address, Option<Role>, Option<Role>) =
        data.try_into_val(&ctx.env).unwrap();
    assert_eq!(caller, ctx.owner);
    assert_eq!(old, Some(Role::Member));
    assert_eq!(new, Some(Role::Admin));

    // The promoted admin can now manage members.
    ctx.client
        .add_member(&member, &Address::generate(&ctx.env));

    ctx.client.set_role(&ctx.owner, &member, &Role::Member);
    assert_eq!(ctx.client.get_role(&member), Some(Role::Member));
    assert_err(
        ctx.client
            .try_add_member(&member, &Address::generate(&ctx.env)),
        Error::Unauthorized,
    );
}

#[test]
fn set_role_rejected_cases() {
    let ctx = setup();
    let member = new_member(&ctx);

    assert_err(
        ctx.client.try_set_role(&ctx.admin, &member, &Role::Admin),
        Error::Unauthorized,
    );
    assert_err(
        ctx.client.try_set_role(&member, &member, &Role::Admin),
        Error::Unauthorized,
    );
    assert_err(
        ctx.client.try_set_role(&ctx.owner, &member, &Role::Owner),
        Error::InvalidRole,
    );
    assert_err(
        ctx.client
            .try_set_role(&ctx.owner, &ctx.owner, &Role::Member),
        Error::CannotRemoveOwner,
    );
    assert_err(
        ctx.client
            .try_set_role(&ctx.owner, &Address::generate(&ctx.env), &Role::Admin),
        Error::NotMember,
    );
    assert_err(
        ctx.client.try_set_role(&ctx.owner, &member, &Role::Member),
        Error::InvalidRole,
    );
}

#[test]
fn set_role_requires_auth() {
    let ctx = setup();
    let member = new_member(&ctx);
    ctx.env.mock_auths(&[]);

    assert!(ctx
        .client
        .try_set_role(&ctx.owner, &member, &Role::Admin)
        .is_err());
    assert_eq!(ctx.client.get_role(&member), Some(Role::Member));
}

#[test]
fn only_owner_modifies_financial_rules() {
    let ctx = setup();
    let mut s = settings(&ctx.env, 8);
    s.contribution_amount = 2_000;

    assert_err(
        ctx.client.try_update_settings(&ctx.admin, &s),
        Error::Unauthorized,
    );
    ctx.client.update_settings(&ctx.owner, &s);
    assert_eq!(ctx.client.get_settings(), s);
    assert!(find_event(&ctx.env, "SettingsUpdated").is_some());

    let mut bad = s.clone();
    bad.max_withdrawal = 0;
    assert_err(
        ctx.client.try_update_settings(&ctx.owner, &bad),
        Error::InvalidAmount,
    );
}

#[test]
fn settings_cannot_shrink_below_member_count() {
    let ctx = setup();
    new_member(&ctx);
    new_member(&ctx);
    assert_eq!(ctx.client.member_count(), 4);

    assert_err(
        ctx.client
            .try_update_settings(&ctx.owner, &settings(&ctx.env, 3)),
        Error::InvalidMaxMembers,
    );
    ctx.client
        .update_settings(&ctx.owner, &settings(&ctx.env, 4));
}

#[test]
fn only_owner_sets_supported_assets() {
    let ctx = setup();
    let other = Address::generate(&ctx.env);
    let assets = vec![&ctx.env, ctx.asset.clone(), other.clone()];

    assert_err(
        ctx.client.try_set_supported_assets(&ctx.admin, &assets),
        Error::Unauthorized,
    );
    assert_err(
        ctx.client.try_set_supported_assets(
            &ctx.owner,
            &vec![&ctx.env, other.clone(), other.clone()],
        ),
        Error::DuplicateAsset,
    );

    ctx.client.set_supported_assets(&ctx.owner, &assets);
    assert!(ctx.client.is_supported_asset(&other));
    assert!(find_event(&ctx.env, "AssetsUpdated").is_some());
}

#[test]
fn admin_authorizes_withdrawal() {
    let ctx = setup();
    let member = new_member(&ctx);

    let id = ctx
        .client
        .authorize_withdrawal(&ctx.admin, &member, &ctx.asset, &MAX_WITHDRAWAL);
    assert_eq!(id, 1);
    let data = find_event(&ctx.env, "WithdrawalAuthorized").expect("event not emitted");
    let (ev_id, ev_asset, ev_amount, ev_caller): (u64, Address, i128, Address) =
        data.try_into_val(&ctx.env).unwrap();
    assert_eq!(
        (ev_id, ev_asset, ev_amount, ev_caller),
        (1, ctx.asset.clone(), MAX_WITHDRAWAL, ctx.admin.clone())
    );

    let record = ctx.client.get_withdrawal(&id).unwrap();
    assert_eq!(record.member, member);
    assert_eq!(record.amount, MAX_WITHDRAWAL);
    assert_eq!(record.approved_by, ctx.admin);

    let id2 = ctx
        .client
        .authorize_withdrawal(&ctx.owner, &member, &ctx.asset, &1);
    assert_eq!(id2, 2);
    assert_eq!(ctx.client.get_withdrawal(&3), None);
}

#[test]
fn withdrawal_rejected_cases() {
    let ctx = setup();
    let member = new_member(&ctx);

    assert_err(
        ctx.client
            .try_authorize_withdrawal(&member, &member, &ctx.asset, &100),
        Error::Unauthorized,
    );
    assert_err(
        ctx.client.try_authorize_withdrawal(
            &ctx.admin,
            &Address::generate(&ctx.env),
            &ctx.asset,
            &100,
        ),
        Error::NotMember,
    );
    assert_err(
        ctx.client.try_authorize_withdrawal(
            &ctx.admin,
            &member,
            &Address::generate(&ctx.env),
            &100,
        ),
        Error::UnsupportedAsset,
    );
    assert_err(
        ctx.client
            .try_authorize_withdrawal(&ctx.admin, &member, &ctx.asset, &0),
        Error::InvalidAmount,
    );
    assert_err(
        ctx.client.try_authorize_withdrawal(
            &ctx.admin,
            &member,
            &ctx.asset,
            &(MAX_WITHDRAWAL + 1),
        ),
        Error::WithdrawalLimitExceeded,
    );
}

// ─── Issue 75: recovery paths ───────────────────────────────────────────────

#[test]
fn owner_transfers_ownership_two_step() {
    let ctx = setup();
    let member = new_member(&ctx);

    ctx.client.transfer_ownership(&ctx.owner, &member);
    assert_eq!(ctx.client.get_pending_owner(), Some(member.clone()));
    // Ownership has not moved until the new owner accepts.
    assert_eq!(ctx.client.get_owner(), ctx.owner);

    assert_err(
        ctx.client.try_accept_ownership(&ctx.admin),
        Error::NoPendingTransfer,
    );

    ctx.client.accept_ownership(&member);
    assert_eq!(ctx.client.get_owner(), member);
    assert_eq!(ctx.client.get_role(&member), Some(Role::Owner));
    assert_eq!(ctx.client.get_role(&ctx.owner), Some(Role::Admin));
    assert_eq!(ctx.client.get_pending_owner(), None);
    assert_eq!(ctx.client.member_count(), 3);

    let data = find_event(&ctx.env, "OwnershipTransferred").expect("event not emitted");
    let old: Address = data.try_into_val(&ctx.env).unwrap();
    assert_eq!(old, ctx.owner);

    // The former owner lost owner-only permissions.
    assert!(!ctx
        .client
        .has_permission(&ctx.owner, &Permission::ManageRoles));
}

#[test]
fn ownership_transfer_rejected_cases() {
    let ctx = setup();
    let member = new_member(&ctx);

    assert_err(
        ctx.client.try_transfer_ownership(&ctx.admin, &member),
        Error::Unauthorized,
    );
    assert_err(
        ctx.client.try_transfer_ownership(&ctx.owner, &ctx.owner),
        Error::InvalidTarget,
    );
    assert_err(
        ctx.client
            .try_transfer_ownership(&ctx.owner, &Address::generate(&ctx.env)),
        Error::NotMember,
    );
    assert_err(
        ctx.client.try_cancel_ownership_transfer(&ctx.owner),
        Error::NoPendingTransfer,
    );

    ctx.client.transfer_ownership(&ctx.owner, &member);
    assert_err(
        ctx.client.try_cancel_ownership_transfer(&member),
        Error::Unauthorized,
    );
    ctx.client.cancel_ownership_transfer(&ctx.owner);
    assert_err(
        ctx.client.try_accept_ownership(&member),
        Error::NoPendingTransfer,
    );

    // Removing the nominee also clears the pending transfer.
    ctx.client.transfer_ownership(&ctx.owner, &member);
    ctx.client.remove_member(&ctx.owner, &member);
    assert_eq!(ctx.client.get_pending_owner(), None);
}

#[test]
fn admin_recovers_ownership_after_timelock() {
    let ctx = setup();
    let start = ctx.env.ledger().timestamp();

    let unlocks_at = ctx.client.initiate_recovery(&ctx.admin);
    assert_eq!(unlocks_at, start + RECOVERY_DELAY_SECS);
    assert_eq!(
        ctx.client.get_recovery(),
        Some(RecoveryRequest {
            candidate: ctx.admin.clone(),
            unlocks_at,
        })
    );

    assert_err(
        ctx.client.try_execute_recovery(&ctx.admin),
        Error::RecoveryTimelockActive,
    );
    advance_time(&ctx.env, RECOVERY_DELAY_SECS - 1);
    assert_err(
        ctx.client.try_execute_recovery(&ctx.admin),
        Error::RecoveryTimelockActive,
    );

    advance_time(&ctx.env, 1);
    ctx.client.execute_recovery(&ctx.admin);
    assert_eq!(ctx.client.get_owner(), ctx.admin);
    assert_eq!(ctx.client.get_role(&ctx.admin), Some(Role::Owner));
    assert_eq!(ctx.client.get_role(&ctx.owner), Some(Role::Member));
    assert_eq!(ctx.client.get_recovery(), None);

    let data = find_event(&ctx.env, "OwnershipRecovered").expect("event not emitted");
    let old: Address = data.try_into_val(&ctx.env).unwrap();
    assert_eq!(old, ctx.owner);
}

#[test]
fn owner_can_cancel_recovery() {
    let ctx = setup();
    let member = new_member(&ctx);

    ctx.client.initiate_recovery(&ctx.admin);
    assert_err(ctx.client.try_cancel_recovery(&member), Error::Unauthorized);
    ctx.client.cancel_recovery(&ctx.owner);
    assert_eq!(ctx.client.get_recovery(), None);
    assert!(find_event(&ctx.env, "RecoveryCancelled").is_some());

    advance_time(&ctx.env, RECOVERY_DELAY_SECS);
    assert_err(
        ctx.client.try_execute_recovery(&ctx.admin),
        Error::NoPendingRecovery,
    );
    assert_eq!(ctx.client.get_owner(), ctx.owner);
}

#[test]
fn recovery_rejected_cases() {
    let ctx = setup();
    let member = new_member(&ctx);

    assert_err(
        ctx.client.try_initiate_recovery(&member),
        Error::Unauthorized,
    );
    assert_err(
        ctx.client.try_initiate_recovery(&ctx.owner),
        Error::Unauthorized,
    );
    assert_err(
        ctx.client.try_cancel_recovery(&ctx.owner),
        Error::NoPendingRecovery,
    );

    ctx.client.set_role(&ctx.owner, &member, &Role::Admin);
    ctx.client.initiate_recovery(&ctx.admin);
    assert_err(
        ctx.client.try_initiate_recovery(&member),
        Error::RecoveryAlreadyPending,
    );

    advance_time(&ctx.env, RECOVERY_DELAY_SECS);
    // Only the candidate may execute.
    assert_err(
        ctx.client.try_execute_recovery(&member),
        Error::Unauthorized,
    );

    // A candidate demoted by the owner can no longer execute.
    ctx.client.set_role(&ctx.owner, &ctx.admin, &Role::Member);
    assert_err(
        ctx.client.try_execute_recovery(&ctx.admin),
        Error::Unauthorized,
    );
    assert_eq!(ctx.client.get_owner(), ctx.owner);
}

#[test]
fn removing_recovery_candidate_clears_request() {
    let ctx = setup();
    ctx.client.initiate_recovery(&ctx.admin);
    ctx.client.remove_member(&ctx.owner, &ctx.admin);
    assert_eq!(ctx.client.get_recovery(), None);
}

// ─── Issue 79: contributions ────────────────────────────────────────────────

const FUNDS: i128 = 10_000;

mod skimming_token {
    use soroban_sdk::{contract, contractimpl, Address, Env};

    /// Token that reports success but delivers one unit less than requested.
    #[contract]
    pub struct SkimmingToken;

    #[contractimpl]
    impl SkimmingToken {
        pub fn balance(env: Env, id: Address) -> i128 {
            env.storage().persistent().get(&id).unwrap_or(0)
        }

        pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
            from.require_auth();
            let balance = Self::balance(env.clone(), to.clone()) + amount - 1;
            env.storage().persistent().set(&to, &balance);
        }
    }
}

/// Circle supporting one real token, with a member holding FUNDS of it.
fn setup_contributions() -> (Ctx, Address) {
    let mut ctx = setup();
    let member = new_member(&ctx);
    ctx.asset = ctx
        .env
        .register_stellar_asset_contract(Address::generate(&ctx.env));
    ctx.client
        .set_supported_assets(&ctx.owner, &vec![&ctx.env, ctx.asset.clone()]);
    mint(&ctx, &member, FUNDS);
    (ctx, member)
}

fn mint(ctx: &Ctx, to: &Address, amount: i128) {
    token::StellarAssetClient::new(&ctx.env, &ctx.asset).mint(to, &amount);
}

fn balance(ctx: &Ctx, account: &Address) -> i128 {
    token::Client::new(&ctx.env, &ctx.asset).balance(account)
}

fn set_time(env: &Env, timestamp: u64) {
    env.ledger().with_mut(|li| {
        li.timestamp = timestamp;
    });
}

fn bounds(min_amount: i128, max_amount: i128) -> ContributionRules {
    ContributionRules {
        min_amount,
        max_amount,
        starts_at: 0,
        ends_at: 0,
        round_secs: 0,
        max_per_round: 0,
    }
}

/// Assert that nothing was recorded, accounted for or announced.
fn assert_no_contributions(ctx: &Ctx, member: &Address, asset: &Address) {
    assert_eq!(ctx.client.contribution_count(), 0);
    assert_eq!(ctx.client.get_contribution(&1), None);
    assert_eq!(ctx.client.member_contributed(member, asset), 0);
    assert_eq!(ctx.client.total_contributed(asset), 0);
    assert!(find_event(&ctx.env, "ContributionReceived").is_none());
}

#[test]
fn member_contribution_moves_funds_and_updates_accounting() {
    let (ctx, member) = setup_contributions();
    set_time(&ctx.env, 1_234);

    let id = ctx.client.contribute(&member, &ctx.asset, &CONTRIBUTION);
    assert_eq!(id, 1);
    assert_eq!(balance(&ctx, &member), FUNDS - CONTRIBUTION);
    assert_eq!(balance(&ctx, &ctx.client.address), CONTRIBUTION);
    assert_eq!(
        ctx.client.get_contribution(&id),
        Some(ContributionRecord {
            id,
            contributor: member.clone(),
            asset: ctx.asset.clone(),
            amount: CONTRIBUTION,
            round_start: 0,
            timestamp: 1_234,
            ledger: ctx.env.ledger().sequence(),
        })
    );

    // Admins and the owner are members too; totals stay separate per member.
    mint(&ctx, &ctx.admin, FUNDS);
    let id = ctx.client.contribute(&ctx.admin, &ctx.asset, &CONTRIBUTION);
    assert_eq!(id, 2);
    ctx.client.contribute(&member, &ctx.asset, &CONTRIBUTION);

    assert_eq!(ctx.client.contribution_count(), 3);
    assert_eq!(
        ctx.client.member_contributed(&member, &ctx.asset),
        2 * CONTRIBUTION
    );
    assert_eq!(
        ctx.client.member_contributed(&ctx.admin, &ctx.asset),
        CONTRIBUTION
    );
    assert_eq!(ctx.client.total_contributed(&ctx.asset), 3 * CONTRIBUTION);
    assert_eq!(balance(&ctx, &ctx.client.address), 3 * CONTRIBUTION);
}

#[test]
fn contributions_are_accounted_per_asset() {
    let (mut ctx, member) = setup_contributions();
    let first = ctx.asset.clone();
    let second = ctx
        .env
        .register_stellar_asset_contract(Address::generate(&ctx.env));
    ctx.client
        .set_supported_assets(&ctx.owner, &vec![&ctx.env, first.clone(), second.clone()]);
    ctx.asset = second.clone();
    mint(&ctx, &member, FUNDS);

    ctx.client.contribute(&member, &first, &CONTRIBUTION);
    assert_eq!(ctx.client.member_contributed(&member, &first), CONTRIBUTION);
    assert_eq!(ctx.client.member_contributed(&member, &second), 0);
    assert_eq!(ctx.client.total_contributed(&second), 0);

    ctx.client.contribute(&member, &second, &CONTRIBUTION);
    assert_eq!(ctx.client.total_contributed(&first), CONTRIBUTION);
    assert_eq!(ctx.client.total_contributed(&second), CONTRIBUTION);
}

#[test]
fn contribution_emits_structured_event() {
    let (ctx, member) = setup_contributions();
    set_time(&ctx.env, 777);
    let id = ctx.client.contribute(&member, &ctx.asset, &CONTRIBUTION);

    let topics: Vec<Val> = (
        Symbol::new(&ctx.env, "ContributionReceived"),
        ctx.client.address.clone(),
        member.clone(),
    )
        .into_val(&ctx.env);
    let mut payload = None;
    for item in ctx.env.events().all().iter() {
        if item.0 == ctx.client.address && item.1 == topics {
            payload = Some(item.2);
        }
    }

    let data = payload.expect("ContributionReceived not emitted");
    let record: ContributionRecord = data.try_into_val(&ctx.env).unwrap();
    assert_eq!(
        record,
        ContributionRecord {
            id,
            contributor: member,
            asset: ctx.asset.clone(),
            amount: CONTRIBUTION,
            round_start: 0,
            timestamp: 777,
            ledger: ctx.env.ledger().sequence(),
        }
    );
}

#[test]
fn contribution_requires_contributor_auth() {
    let (ctx, member) = setup_contributions();

    ctx.client.contribute(&member, &ctx.asset, &CONTRIBUTION);
    let auths = ctx.env.auths();
    assert_eq!(auths.len(), 1);
    assert_eq!(auths[0].0, member);
    // The token transfer is covered by the same authorization tree.
    assert_eq!(auths[0].1.sub_invocations.len(), 1);

    ctx.env.mock_auths(&[]);
    assert!(ctx
        .client
        .try_contribute(&member, &ctx.asset, &CONTRIBUTION)
        .is_err());
    assert_eq!(ctx.client.contribution_count(), 1);
    assert_eq!(ctx.client.total_contributed(&ctx.asset), CONTRIBUTION);
    assert_eq!(balance(&ctx, &member), FUNDS - CONTRIBUTION);
}

#[test]
fn non_members_cannot_contribute() {
    let (ctx, member) = setup_contributions();
    let outsider = Address::generate(&ctx.env);
    mint(&ctx, &outsider, FUNDS);

    assert_err(
        ctx.client
            .try_contribute(&outsider, &ctx.asset, &CONTRIBUTION),
        Error::NotMember,
    );
    assert_no_contributions(&ctx, &outsider, &ctx.asset);
    assert_eq!(balance(&ctx, &outsider), FUNDS);

    // Membership is checked at contribution time.
    ctx.client.remove_member(&ctx.owner, &member);
    assert_err(
        ctx.client
            .try_contribute(&member, &ctx.asset, &CONTRIBUTION),
        Error::NotMember,
    );
    assert_no_contributions(&ctx, &member, &ctx.asset);
    assert_eq!(balance(&ctx, &member), FUNDS);
}

#[test]
fn contribution_before_initialization_fails() {
    let (env, client) = uninitialized();
    assert!(!client.is_active());
    assert_err(client.try_get_contribution_rules(), Error::NotInitialized);
    assert_err(
        client.try_contribute(
            &Address::generate(&env),
            &Address::generate(&env),
            &CONTRIBUTION,
        ),
        Error::NotInitialized,
    );
}

#[test]
fn unsupported_asset_is_rejected() {
    let (mut ctx, member) = setup_contributions();
    ctx.asset = ctx
        .env
        .register_stellar_asset_contract(Address::generate(&ctx.env));
    mint(&ctx, &member, FUNDS);

    assert_err(
        ctx.client
            .try_contribute(&member, &ctx.asset, &CONTRIBUTION),
        Error::UnsupportedAsset,
    );
    assert_no_contributions(&ctx, &member, &ctx.asset);
    assert_eq!(balance(&ctx, &member), FUNDS);
}

#[test]
fn default_rules_require_the_configured_contribution_amount() {
    let (ctx, member) = setup_contributions();
    assert_eq!(
        ctx.client.get_contribution_rules(),
        bounds(CONTRIBUTION, CONTRIBUTION)
    );

    for bad in [0, -1] {
        assert_err(
            ctx.client.try_contribute(&member, &ctx.asset, &bad),
            Error::InvalidAmount,
        );
    }
    assert_err(
        ctx.client
            .try_contribute(&member, &ctx.asset, &(CONTRIBUTION - 1)),
        Error::ContributionBelowMinimum,
    );
    assert_err(
        ctx.client
            .try_contribute(&member, &ctx.asset, &(CONTRIBUTION + 1)),
        Error::ContributionAboveMaximum,
    );
    assert_no_contributions(&ctx, &member, &ctx.asset);

    // The defaults follow the circle settings.
    let mut s = settings(&ctx.env, MAX_MEMBERS);
    s.contribution_amount = CONTRIBUTION + 1;
    ctx.client.update_settings(&ctx.owner, &s);
    ctx.client
        .contribute(&member, &ctx.asset, &(CONTRIBUTION + 1));
}

#[test]
fn contribution_amount_bounds_are_enforced() {
    let (ctx, member) = setup_contributions();
    ctx.client
        .set_contribution_rules(&ctx.owner, &bounds(100, 500));
    assert!(find_event(&ctx.env, "ContributionRulesUpdated").is_some());

    assert_err(
        ctx.client.try_contribute(&member, &ctx.asset, &99),
        Error::ContributionBelowMinimum,
    );
    assert_err(
        ctx.client.try_contribute(&member, &ctx.asset, &501),
        Error::ContributionAboveMaximum,
    );
    assert_no_contributions(&ctx, &member, &ctx.asset);
    assert_eq!(balance(&ctx, &member), FUNDS);

    ctx.client.contribute(&member, &ctx.asset, &100);
    ctx.client.contribute(&member, &ctx.asset, &500);
    assert_eq!(ctx.client.total_contributed(&ctx.asset), 600);
}

#[test]
fn contribution_schedule_and_round_limit_are_enforced() {
    let (ctx, member) = setup_contributions();
    let mut rules = bounds(1, FUNDS);
    rules.starts_at = 1_000;
    rules.ends_at = 1_250;
    rules.round_secs = 100;
    rules.max_per_round = 1;
    ctx.client.set_contribution_rules(&ctx.owner, &rules);
    assert_eq!(ctx.client.get_contribution_rules(), rules);

    set_time(&ctx.env, 999);
    assert_err(
        ctx.client.try_contribute(&member, &ctx.asset, &10),
        Error::ContributionWindowClosed,
    );
    assert_no_contributions(&ctx, &member, &ctx.asset);

    set_time(&ctx.env, 1_000);
    ctx.client.contribute(&member, &ctx.asset, &10);

    // Last second of the round, also after leaving and rejoining.
    set_time(&ctx.env, 1_099);
    assert_err(
        ctx.client.try_contribute(&member, &ctx.asset, &10),
        Error::RoundLimitReached,
    );
    ctx.client.leave_circle(&member);
    ctx.client.add_member(&ctx.owner, &member);
    assert_err(
        ctx.client.try_contribute(&member, &ctx.asset, &10),
        Error::RoundLimitReached,
    );

    // The limit is per member.
    mint(&ctx, &ctx.admin, FUNDS);
    ctx.client.contribute(&ctx.admin, &ctx.asset, &10);

    set_time(&ctx.env, 1_100);
    let id = ctx.client.contribute(&member, &ctx.asset, &10);
    let record = ctx.client.get_contribution(&id).unwrap();
    assert_eq!(record.round_start, 1_100);

    set_time(&ctx.env, 1_249);
    let id = ctx.client.contribute(&member, &ctx.asset, &10);
    let record = ctx.client.get_contribution(&id).unwrap();
    assert_eq!(record.round_start, 1_200);

    set_time(&ctx.env, 1_250);
    assert_err(
        ctx.client.try_contribute(&member, &ctx.asset, &10),
        Error::ContributionWindowClosed,
    );
    assert_eq!(ctx.client.member_contributed(&member, &ctx.asset), 30);
    assert_eq!(ctx.client.total_contributed(&ctx.asset), 40);
}

#[test]
fn inactive_circle_rejects_contributions() {
    let (ctx, member) = setup_contributions();
    assert!(ctx.client.is_active());
    assert_err(
        ctx.client.try_set_active(&ctx.admin, &false),
        Error::Unauthorized,
    );

    ctx.client.set_active(&ctx.owner, &false);
    assert!(find_event(&ctx.env, "CircleStatusChanged").is_some());
    assert!(!ctx.client.is_active());
    assert_err(
        ctx.client
            .try_contribute(&member, &ctx.asset, &CONTRIBUTION),
        Error::CircleInactive,
    );
    assert_no_contributions(&ctx, &member, &ctx.asset);
    assert_eq!(balance(&ctx, &member), FUNDS);

    ctx.client.set_active(&ctx.owner, &true);
    ctx.client.contribute(&member, &ctx.asset, &CONTRIBUTION);
}

#[test]
fn contribution_rules_are_owner_only_and_validated() {
    let (ctx, member) = setup_contributions();
    let rules = bounds(1, 2);
    for caller in [&ctx.admin, &member] {
        assert_err(
            ctx.client.try_set_contribution_rules(caller, &rules),
            Error::Unauthorized,
        );
    }

    let mut bad_window = bounds(1, 2);
    bad_window.starts_at = 100;
    bad_window.ends_at = 100;
    for bad in [bounds(0, 10), bounds(-1, 10), bounds(10, 9), bad_window] {
        assert_err(
            ctx.client.try_set_contribution_rules(&ctx.owner, &bad),
            Error::InvalidContributionRules,
        );
    }
    assert_eq!(
        ctx.client.get_contribution_rules(),
        bounds(CONTRIBUTION, CONTRIBUTION)
    );
}

#[test]
fn failed_transfer_leaves_accounting_unchanged() {
    let (ctx, _) = setup_contributions();
    let poor = new_member(&ctx);
    mint(&ctx, &poor, CONTRIBUTION - 1);
    // One contribution per round: a failed attempt must not use up the slot.
    let mut rules = bounds(CONTRIBUTION, CONTRIBUTION);
    rules.max_per_round = 1;
    ctx.client.set_contribution_rules(&ctx.owner, &rules);

    assert_err(
        ctx.client.try_contribute(&poor, &ctx.asset, &CONTRIBUTION),
        Error::TransferFailed,
    );
    assert_no_contributions(&ctx, &poor, &ctx.asset);
    assert_eq!(balance(&ctx, &poor), CONTRIBUTION - 1);
    assert_eq!(balance(&ctx, &ctx.client.address), 0);

    mint(&ctx, &poor, 1);
    let id = ctx.client.contribute(&poor, &ctx.asset, &CONTRIBUTION);
    assert_eq!(id, 1);
    assert_eq!(ctx.client.total_contributed(&ctx.asset), CONTRIBUTION);
}

#[test]
fn partial_transfer_is_reverted() {
    let (ctx, member) = setup_contributions();
    let skimming = ctx
        .env
        .register_contract(None, skimming_token::SkimmingToken);
    ctx.client
        .set_supported_assets(&ctx.owner, &vec![&ctx.env, skimming.clone()]);

    assert_err(
        ctx.client.try_contribute(&member, &skimming, &CONTRIBUTION),
        Error::TransferFailed,
    );
    assert_no_contributions(&ctx, &member, &skimming);
    let vault_balance = token::Client::new(&ctx.env, &skimming).balance(&ctx.client.address);
    assert_eq!(vault_balance, 0);
}
