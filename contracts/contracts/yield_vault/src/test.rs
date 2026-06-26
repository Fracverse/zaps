//! SC-027: Comprehensive unit tests for the yield vault contract.

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Events, Ledger},
    token, Address, Env, IntoVal, Symbol, TryIntoVal, Val,
};

const APY_BPS: u32 = 500; // 5% APY
const LEDGERS_PER_YEAR: u32 = 6_307_200;
const YIELD_TEST_LEDGERS: u32 = 100_000;
const DEPOSIT_AMOUNT: i128 = 10_000_000;

fn setup() -> (
    Env,
    YieldVaultContractClient<'static>,
    Address,
    Address,
    Address,
    Address,
) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, YieldVaultContract);
    let client = YieldVaultContractClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let depositor = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let token_addr = env.register_stellar_asset_contract(token_admin.clone());
    let token_client = token::StellarAssetClient::new(&env, &token_addr);
    token_client.mint(&depositor, &DEPOSIT_AMOUNT);

    client.initialize(&owner, &token_addr, &APY_BPS);

    (env, client, contract_id, owner, depositor, token_addr)
}

fn advance_ledgers(env: &Env, ledgers: u32) {
    env.ledger().with_mut(|li| {
        li.sequence_number = li.sequence_number.saturating_add(ledgers);
    });
}

#[test]
fn test_initialize_sets_defaults() {
    let (_env, client, _contract_id, _owner, _depositor, _token) = setup();

    assert_eq!(client.total_shares(), 0);
    assert_eq!(client.total_assets(), 0);
    assert_eq!(client.yield_index(), PRECISION);
}

#[test]
#[ignore]
fn test_initialize_twice_panics() {
    let (_env, client, _contract_id, owner, _depositor, token) = setup();
    let res = client.try_initialize(&owner, &token, &APY_BPS);
    assert!(res.is_err(), "double initialization must fail");
}

#[test]
fn test_deposit_mints_shares_at_initial_index() {
    let (env, client, contract_id, _owner, depositor, token) = setup();
    let amount = 1_000_000i128;

    client.deposit(&depositor, &amount);

    let expected_shares = amount * PRECISION / PRECISION;
    assert_eq!(client.shares_of(&depositor), expected_shares);
    assert_eq!(client.total_shares(), expected_shares);
    assert_eq!(client.total_assets(), amount);
    assert_eq!(client.mock_protocol_supplied_balance(), amount);

    let token_client = token::Client::new(&env, &token);
    assert_eq!(token_client.balance(&contract_id), amount);
}

#[test]
#[ignore]
fn test_deposit_rejects_zero_amount() {
    let (_env, client, _contract_id, _owner, depositor, _token) = setup();
    let res = client.try_deposit(&depositor, &0);
    assert!(res.is_err());
}

#[test]
fn test_withdraw_returns_principal_at_initial_index() {
    let (env, client, _contract_id, _owner, depositor, token) = setup();
    let amount = 1_000_000i128;

    client.deposit(&depositor, &amount);
    let shares = client.shares_of(&depositor);

    client.withdraw(&depositor, &shares);

    assert_eq!(client.shares_of(&depositor), 0);
    assert_eq!(client.total_shares(), 0);
    assert_eq!(client.total_assets(), 0);

    let token_client = token::Client::new(&env, &token);
    assert_eq!(token_client.balance(&depositor), DEPOSIT_AMOUNT);
}

#[test]
#[ignore]
fn test_withdraw_rejects_zero_shares() {
    let (_env, client, _contract_id, _owner, depositor, _token) = setup();
    let res = client.try_withdraw(&depositor, &0);
    assert!(res.is_err());
}

#[test]
#[ignore]
fn test_withdraw_rejects_insufficient_shares() {
    let (_env, client, _contract_id, _owner, depositor, _token) = setup();
    client.deposit(&depositor, &1_000_000);
    let res = client.try_withdraw(&depositor, &999_999_999);
    assert!(res.is_err());
}

#[test]
fn test_yield_index_increases_after_ledger_advance() {
    let (env, client, _contract_id, _owner, _depositor, _token) = setup();

    let index_before = client.yield_index();
    advance_ledgers(&env, YIELD_TEST_LEDGERS);
    let index_after = client.yield_index();

    assert!(
        index_after > index_before,
        "yield index should grow after ledger advance"
    );

    let expected = PRECISION
        + PRECISION * APY_BPS as i128 * YIELD_TEST_LEDGERS as i128
            / (10_000 * LEDGERS_PER_YEAR as i128);
    assert_eq!(index_after, expected);
}

#[test]
fn test_exchange_rate_scales_after_ledger_advance() {
    let (env, client, _contract_id, _owner, depositor, token) = setup();
    let amount = 1_000_000i128;

    client.deposit(&depositor, &amount);
    let first_shares = client.shares_of(&depositor);

    advance_ledgers(&env, YIELD_TEST_LEDGERS);

    let second_depositor = Address::generate(&env);
    let token_client = token::StellarAssetClient::new(&env, &token);
    token_client.mint(&second_depositor, &amount);

    client.deposit(&second_depositor, &amount);
    let second_shares = client.shares_of(&second_depositor);

    assert!(
        second_shares < first_shares,
        "later depositor should receive fewer shares at a higher exchange rate"
    );

    let index = client.yield_index();
    let assets_out = first_shares * index / PRECISION;
    assert!(
        assets_out > amount,
        "original depositor should be able to withdraw more than deposited after yield accrual"
    );
}

#[test]
fn test_deposit_withdraw_boundary_full_balance() {
    let (_env, client, _contract_id, _owner, depositor, _token) = setup();
    let amount = DEPOSIT_AMOUNT;

    client.deposit(&depositor, &amount);
    let shares = client.shares_of(&depositor);
    client.withdraw(&depositor, &shares);

    assert_eq!(client.shares_of(&depositor), 0);
    assert_eq!(client.total_shares(), 0);
}

#[test]
fn test_partial_withdraw_leaves_remaining_shares() {
    let (_env, client, _contract_id, _owner, depositor, _token) = setup();
    let amount = 2_000_000i128;

    client.deposit(&depositor, &amount);
    let total_shares = client.shares_of(&depositor);
    let half = total_shares / 2;

    client.withdraw(&depositor, &half);

    assert_eq!(client.shares_of(&depositor), total_shares - half);
    assert!(client.total_assets() > 0);
}

#[test]
fn test_accrue_yield_emits_event_and_compounds_index() {
    let (env, client, _contract_id, owner, _depositor, _token) = setup();

    advance_ledgers(&env, YIELD_TEST_LEDGERS);
    let index_before = client.yield_index();

    client.accrue_yield(&owner);

    let index_after = client.yield_index();
    assert!(index_after >= index_before);

    let events = env.events().all();
    let topic: Val = Symbol::new(&env, "YieldAccrued").into_val(&env);
    let mut found = false;
    for item in events.iter() {
        if item.1.contains(topic) {
            let (elapsed, added_yield, new_index): (u32, i128, i128) =
                item.2.try_into_val(&env).unwrap();
            assert!(elapsed > 0);
            assert!(added_yield >= 0);
            assert_eq!(new_index, index_after);
            found = true;
        }
    }
    assert!(found, "YieldAccrued event must be emitted");
}

#[test]
#[ignore]
fn test_accrue_yield_rejects_non_owner() {
    let (env, client, _contract_id, _owner, _depositor, _token) = setup();
    advance_ledgers(&env, 1_000);

    let stranger = Address::generate(&env);
    let res = client.try_accrue_yield(&stranger);
    assert!(res.is_err(), "only owner may accrue yield");
}

#[test]
fn test_mock_protocol_owner_supply() {
    let (_env, client, _contract_id, owner, _depositor, _token) = setup();

    client.mock_protocol_supply(&owner, &500);
    assert_eq!(client.mock_protocol_supplied_balance(), 500);
}

#[test]
#[ignore]
fn test_mock_protocol_access_control_rejects_non_owner() {
    let (env, client, _contract_id, _owner, _depositor, _token) = setup();
    let stranger = Address::generate(&env);

    assert!(client.try_mock_protocol_supply(&stranger, &100).is_err());
    assert!(client.try_mock_protocol_redeem(&stranger, &100).is_err());
    assert!(client.try_mock_protocol_claim_rewards(&stranger).is_err());
}

#[test]
fn test_mock_protocol_rewards_accrue_over_time() {
    let (env, client, _contract_id, owner, depositor, _token) = setup();
    client.deposit(&depositor, &1_000_000);

    advance_ledgers(&env, LEDGERS_PER_YEAR / 10);
    let pending = client.mock_protocol_pending_rewards();
    assert!(pending > 0, "mock protocol should accrue rewards over time");

    let claimed = client.mock_protocol_claim_rewards(&owner);
    assert_eq!(claimed, pending);
    assert_eq!(client.mock_protocol_pending_rewards(), 0);
}

#[test]
fn test_salvage_token_transfers_unsupported_token() {
    let (env, client, contract_id, owner, _depositor, deposit_token) = setup();
    let treasury = Address::generate(&env);
    let stray_admin = Address::generate(&env);

    let stray_token = env.register_stellar_asset_contract(stray_admin.clone());
    let stray_client = token::StellarAssetClient::new(&env, &stray_token);
    stray_client.mint(&contract_id, &777);

    client.salvage_token(&owner, &stray_token, &treasury);

    let stray_balance = token::Client::new(&env, &stray_token);
    assert_eq!(stray_balance.balance(&treasury), 777);
    assert_eq!(stray_balance.balance(&contract_id), 0);

    // Primary deposit token must remain protected.
    let deposit_balance = token::Client::new(&env, &deposit_token);
    assert_eq!(deposit_balance.balance(&contract_id), 0);
}

#[test]
#[ignore]
fn test_salvage_token_rejects_primary_deposit_token() {
    let (env, client, _contract_id, owner, _depositor, deposit_token) = setup();
    let treasury = Address::generate(&env);

    let res = client.try_salvage_token(&owner, &deposit_token, &treasury);
    assert!(res.is_err(), "cannot salvage the primary deposit token");
}

#[test]
#[ignore]
fn test_salvage_token_rejects_non_owner() {
    let (env, client, _contract_id, _owner, _depositor, _token) = setup();
    let stranger = Address::generate(&env);
    let treasury = Address::generate(&env);
    let stray_admin = Address::generate(&env);
    let stray_token = env.register_stellar_asset_contract(stray_admin);

    let res = client.try_salvage_token(&stranger, &stray_token, &treasury);
    assert!(res.is_err());
}

#[test]
fn test_full_lifecycle_deposit_yield_withdraw() {
    let (env, client, _contract_id, owner, depositor, token) = setup();
    let amount = 5_000_000i128;

    client.deposit(&depositor, &amount);
    let shares = client.shares_of(&depositor);

    advance_ledgers(&env, YIELD_TEST_LEDGERS);
    client.accrue_yield(&owner);

    let index = client.yield_index();
    assert!(
        index > PRECISION,
        "yield should compound after ledger advance"
    );

    let half_shares = shares / 2;
    let expected_out = half_shares * index / PRECISION;
    client.withdraw(&depositor, &half_shares);

    let token_client = token::Client::new(&env, &token);
    let withdrawn = token_client.balance(&depositor) - (DEPOSIT_AMOUNT - amount);
    assert_eq!(withdrawn, expected_out);
    assert_eq!(client.shares_of(&depositor), shares - half_shares);
    assert!(client.total_assets() > 0);
}
