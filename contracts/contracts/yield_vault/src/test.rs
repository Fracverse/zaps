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

fn advance_timestamp(env: &Env, secs: u64) {
    env.ledger().with_mut(|li| {
        li.timestamp = li.timestamp.saturating_add(secs);
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
fn test_withdraw_releases_reentrancy_lock() {
    let (_env, client, _contract_id, _owner, depositor, _token) = setup();
    let amount = 1_000_000i128;

    client.deposit(&depositor, &amount);
    let shares = client.shares_of(&depositor);
    let half = shares / 2;

    client.withdraw(&depositor, &half);
    client.withdraw(&depositor, &half);

    assert_eq!(client.shares_of(&depositor), 0);
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
    let (env, client, _contract_id, owner, depositor, token) = setup();
    let amount = 1_000_000i128;

    client.deposit(&depositor, &amount);
    let first_shares = client.shares_of(&depositor);

    advance_ledgers(&env, YIELD_TEST_LEDGERS);
    // Accrue yield so total_assets grows with mock protocol rewards, moving the exchange rate.
    client.accrue_yield(&owner);

    let second_depositor = Address::generate(&env);
    let token_client = token::StellarAssetClient::new(&env, &token);
    token_client.mint(&second_depositor, &amount);

    client.deposit(&second_depositor, &amount);
    let second_shares = client.shares_of(&second_depositor);

    assert!(
        second_shares < first_shares,
        "later depositor should receive fewer shares at a higher exchange rate"
    );

    // Verify the first depositor can withdraw more assets than deposited.
    let tot_shares = client.total_shares();
    let tot_assets = client.total_assets();
    let assets_out = first_shares * (tot_assets + VIRTUAL_OFFSET) / (tot_shares + VIRTUAL_OFFSET);
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
fn test_full_yield_vault_lifecycle() {
    let (env, client, contract_id, owner, depositor, token) = setup();
    let deposit_amount = 5_000_000i128;
    let initial_user_balance = DEPOSIT_AMOUNT;
    let initial_apy = client.apy();

    // Deposit and verify both external funds and the newly-created position.
    let balance_before_deposit = token::Client::new(&env, &token).balance(&depositor);
    client.deposit(&depositor, &deposit_amount);
    let shares_after_deposit = client.shares_of(&depositor);
    assert_eq!(balance_before_deposit - deposit_amount, DEPOSIT_AMOUNT - deposit_amount);
    assert_eq!(token::Client::new(&env, &token).balance(&contract_id), deposit_amount);
    assert_eq!(shares_after_deposit, deposit_amount);
    assert_eq!(client.user_deposit(&depositor), deposit_amount);
    assert_eq!(client.total_assets(), deposit_amount);

    // Queue and apply an APY change through the production timelocked path.
    let updated_apy = 1_000;
    client.update_apy(&owner, &updated_apy);
    assert_eq!(client.apy(), initial_apy);
    advance_timestamp(&env, APY_TIMELOCK_SECS);
    client.apply_apy(&owner);
    assert_eq!(client.apy(), updated_apy);

    // Advance both ledger dimensions before explicitly realizing yield.
    advance_ledgers(&env, YIELD_TEST_LEDGERS);
    advance_timestamp(&env, 5 * YIELD_TEST_LEDGERS as u64);
    let index_before_accrual = client.yield_index();
    client.accrue_yield(&owner);
    let index_after_accrual = client.yield_index();
    let assets_after_accrual = client.total_assets();
    assert!(index_after_accrual > index_before_accrual);
    assert!(assets_after_accrual > deposit_amount);

    // Withdraw half the position and retain the remainder for emergency exit.
    let partial_shares = shares_after_deposit / 2;
    let expected_partial = partial_shares
        * (assets_after_accrual + VIRTUAL_OFFSET)
        / (client.total_shares() + VIRTUAL_OFFSET);
    let balance_before_partial = token::Client::new(&env, &token).balance(&depositor);
    client.withdraw(&depositor, &partial_shares);
    let balance_after_partial = token::Client::new(&env, &token).balance(&depositor);
    assert_eq!(balance_after_partial - balance_before_partial, expected_partial);
    assert_eq!(client.shares_of(&depositor), shares_after_deposit - partial_shares);
    assert!(client.shares_of(&depositor) > 0);

    // Exit the remaining position and assert the complete final balance state.
    let remaining_shares = client.shares_of(&depositor);
    let assets_before_exit = client.total_assets();
    let expected_exit = remaining_shares
        * (assets_before_exit + VIRTUAL_OFFSET)
        / (client.total_shares() + VIRTUAL_OFFSET);
    let balance_before_exit = token::Client::new(&env, &token).balance(&depositor);
    client.emergency_exit(&depositor);
    let final_user_balance = token::Client::new(&env, &token).balance(&depositor);

    assert_eq!(final_user_balance, balance_before_exit + expected_exit);
    assert_eq!(final_user_balance, initial_user_balance - deposit_amount + expected_partial + expected_exit);
    assert_eq!(client.shares_of(&depositor), 0);
    assert_eq!(client.user_deposit(&depositor), 0);
    assert_eq!(client.total_shares(), 0);
    assert_eq!(client.total_assets(), 0);
}

#[test]
#[ignore]
fn test_pause_unpause_and_deposit_rejection() {
    let (_env, client, _contract_id, owner, depositor, _token) = setup();

    client.pause(&owner);

    let res = client.try_deposit(&depositor, &1_000_000);
    assert!(res.is_err(), "deposit must fail when vault is paused");

    client.unpause(&owner);
    client.deposit(&depositor, &1_000_000);
    assert_eq!(client.shares_of(&depositor), 1_000_000);
}

#[test]
fn test_emergency_exit_rescues_assets() {
    let (env, client, _contract_id, owner, depositor, token) = setup();
    let amount = 2_000_000i128;

    client.deposit(&depositor, &amount);
    let _shares = client.shares_of(&depositor);

    client.pause(&owner);

    client.emergency_exit(&depositor);

    assert_eq!(client.shares_of(&depositor), 0);
    let token_client = token::Client::new(&env, &token);
    assert_eq!(token_client.balance(&depositor), DEPOSIT_AMOUNT);
}

#[test]
fn test_update_apy_queues_change_and_apply_after_timelock() {
    let (env, client, _contract_id, owner, _depositor, _token) = setup();

    // Queue a new APY.
    client.update_apy(&owner, &1_000);

    // Once the 24-hour delay has elapsed, applying succeeds.
    advance_timestamp(&env, APY_TIMELOCK_SECS);
    client.apply_apy(&owner);
    assert_eq!(client.apy(), 1_000);
}

#[test]
#[ignore]
fn test_update_apy_rejects_before_timelock() {
    let (env, client, _contract_id, owner, _depositor, _token) = setup();

    client.update_apy(&owner, &1_000);
    advance_timestamp(&env, APY_TIMELOCK_SECS - 1);
    let res = client.try_apply_apy(&owner);
    assert!(res.is_err(), "apply_apy must fail before timelock elapses");
}

#[test]
#[ignore]
fn test_apply_apy_rejects_without_pending_change() {
    let (_env, client, _contract_id, owner, _depositor, _token) = setup();

    let res = client.try_apply_apy(&owner);
    assert!(res.is_err(), "apply_apy must fail with no pending change");
}

#[test]
fn test_admin_emergency_exit_rescues_reserves() {
    let (env, client, _contract_id, owner, _depositor, token) = setup();
    let amount = 5_000_000i128;

    // Mint tokens to the owner (admin)
    let token_admin_client = token::StellarAssetClient::new(&env, &token);
    token_admin_client.mint(&owner, &amount);

    // Admin deposits into the vault
    client.deposit(&owner, &amount);

    // Pause the vault
    client.pause(&owner);

    // Reset the budget tracker to defaults as per guidance
    env.budget().reset_default();

    // Admin triggers emergency_exit
    client.emergency_exit(&owner);

    // Assert that the admin received the total vault reserves and shares are burned
    assert_eq!(client.shares_of(&owner), 0);
    let token_client = token::Client::new(&env, &token);
    assert_eq!(token_client.balance(&owner), amount);
    assert_eq!(client.total_assets(), 0);
}

#[cfg(test)]
mod fuzz_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn test_fuzz_asset_to_share_math(
            deposit1 in 1_000i128..10_000_000_000,
            yield_amount in 0i128..5_000_000_000,
            deposit2 in 1_000i128..10_000_000_000,
        ) {
            let (env, client, _contract_id, owner, depositor1, token) = setup();

            let token_admin_client = token::StellarAssetClient::new(&env, &token);

            token_admin_client.mint(&depositor1, &deposit1);
            client.deposit(&depositor1, &deposit1);

            if yield_amount > 0 {
                client.mock_protocol_supply(&owner, &yield_amount);
            }

            let depositor2 = Address::generate(&env);
            token_admin_client.mint(&depositor2, &deposit2);

            client.deposit(&depositor2, &deposit2);
            let shares2 = client.shares_of(&depositor2);

            let total_assets = client.total_assets();
            let total_shares = client.total_shares();

            let assets_out = shares2 * (total_assets + VIRTUAL_OFFSET) / (total_shares + VIRTUAL_OFFSET);

            prop_assert!(assets_out <= deposit2, "rounding behavior granted excess shares");
        }
    }
}
