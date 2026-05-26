#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Events},
    token::StellarAssetClient,
    vec, Address, Env, Symbol, TryFromVal,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_token(env: &Env, admin: &Address) -> Address {
    env.register_stellar_asset_contract_v2(admin.clone()).address()
}

fn mint(env: &Env, token: &Address, to: &Address, amount: i128) {
    StellarAssetClient::new(env, token).mint(to, &amount);
}

fn token_balance(env: &Env, token: &Address, who: &Address) -> i128 {
    soroban_sdk::token::Client::new(env, token).balance(who)
}

/// Build a Recipient with zero total_received (initial state).
fn recip(address: Address, share_bps: u32) -> Recipient {
    Recipient { address, share_bps, total_received: 0 }
}

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

struct Setup {
    env: Env,
    client: FeeDistributionClient<'static>,
    admin: Address,
    token: Address,
    /// Three recipients: r0 = 50 %, r1 = 30 %, r2 = 20 %
    r: [Address; 3],
}

impl Setup {
    fn new() -> Self {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let token = make_token(&env, &admin);

        let r0 = Address::generate(&env);
        let r1 = Address::generate(&env);
        let r2 = Address::generate(&env);

        let recipients = vec![
            &env,
            recip(r0.clone(), 5_000), // 50 %
            recip(r1.clone(), 3_000), // 30 %
            recip(r2.clone(), 2_000), // 20 %
        ];

        let contract_id = env.register_contract(None, FeeDistribution);
        let client = FeeDistributionClient::new(&env, &contract_id);
        client.initialize(&admin, &token, &recipients);

        let client: FeeDistributionClient<'static> = unsafe { core::mem::transmute(client) };

        Setup { env, client, admin, token, r: [r0, r1, r2] }
    }

    /// Deposit `amount` from a freshly minted depositor.
    fn deposit(&self, amount: i128) -> Address {
        let depositor = Address::generate(&self.env);
        mint(&self.env, &self.token, &depositor, amount);
        self.client.deposit(&depositor, &amount);
        depositor
    }
}

fn has_event(env: &Env, t0: &str, t1: &str) -> bool {
    let events = env.events().all();
    events.iter().any(|(_, topics, _)| {
        if topics.len() != 2 { return false; }
        let a = <Symbol as TryFromVal<Env, _>>::try_from_val(env, &topics.get(0).unwrap());
        let b = <Symbol as TryFromVal<Env, _>>::try_from_val(env, &topics.get(1).unwrap());
        matches!((a, b), (Ok(x), Ok(y))
            if x == Symbol::new(env, t0) && y == Symbol::new(env, t1))
    })
}

// ---------------------------------------------------------------------------
// Initialisation
// ---------------------------------------------------------------------------

#[test]
fn test_initialize_stores_config() {
    let s = Setup::new();
    assert_eq!(s.client.get_admin(), s.admin);
    assert_eq!(s.client.get_token(), s.token);
    assert_eq!(s.client.get_recipients().len(), 3);
    assert_eq!(s.client.get_pending(), 0);
}

#[test]
fn test_initialize_twice_fails() {
    let s = Setup::new();
    let result = s.client.try_initialize(
        &s.admin,
        &s.token,
        &vec![&s.env, recip(Address::generate(&s.env), 10_000)],
    );
    assert_eq!(result, Err(Ok(Error::AlreadyInitialized)));
}

#[test]
fn test_initialize_empty_recipients_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let token = make_token(&env, &admin);
    let contract_id = env.register_contract(None, FeeDistribution);
    let client = FeeDistributionClient::new(&env, &contract_id);

    let result = client.try_initialize(&admin, &token, &vec![&env]);
    assert_eq!(result, Err(Ok(Error::EmptyRecipients)));
}

#[test]
fn test_initialize_shares_not_summing_to_10000_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let token = make_token(&env, &admin);
    let contract_id = env.register_contract(None, FeeDistribution);
    let client = FeeDistributionClient::new(&env, &contract_id);

    let bad = vec![
        &env,
        recip(Address::generate(&env), 5_000),
        recip(Address::generate(&env), 4_000), // sums to 9 000, not 10 000
    ];
    let result = client.try_initialize(&admin, &token, &bad);
    assert_eq!(result, Err(Ok(Error::InvalidShares)));
}

#[test]
fn test_initialize_zero_share_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let token = make_token(&env, &admin);
    let contract_id = env.register_contract(None, FeeDistribution);
    let client = FeeDistributionClient::new(&env, &contract_id);

    let bad = vec![
        &env,
        recip(Address::generate(&env), 0),      // invalid
        recip(Address::generate(&env), 10_000),
    ];
    let result = client.try_initialize(&admin, &token, &bad);
    assert_eq!(result, Err(Ok(Error::InvalidShareValue)));
}

#[test]
fn test_initialize_single_recipient_100_pct() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let token = make_token(&env, &admin);
    let r = Address::generate(&env);
    let contract_id = env.register_contract(None, FeeDistribution);
    let client = FeeDistributionClient::new(&env, &contract_id);

    client.initialize(&admin, &token, &vec![&env, recip(r, 10_000)]);
    assert_eq!(client.get_recipients().len(), 1);
}

// ---------------------------------------------------------------------------
// Deposit
// ---------------------------------------------------------------------------

#[test]
fn test_deposit_increases_pending() {
    let s = Setup::new();
    s.deposit(1_000);
    assert_eq!(s.client.get_pending(), 1_000);
    s.deposit(500);
    assert_eq!(s.client.get_pending(), 1_500);
}

#[test]
fn test_deposit_increases_total_in() {
    let s = Setup::new();
    s.deposit(1_000);
    s.deposit(2_000);
    assert_eq!(s.client.get_total_in(), 3_000);
}

#[test]
fn test_deposit_zero_fails() {
    let s = Setup::new();
    let depositor = Address::generate(&s.env);
    let result = s.client.try_deposit(&depositor, &0);
    assert_eq!(result, Err(Ok(Error::ZeroAmount)));
}

#[test]
fn test_deposit_negative_fails() {
    let s = Setup::new();
    let depositor = Address::generate(&s.env);
    let result = s.client.try_deposit(&depositor, &-1);
    assert_eq!(result, Err(Ok(Error::ZeroAmount)));
}

#[test]
fn test_deposit_emits_event() {
    let s = Setup::new();
    s.deposit(500);
    assert!(has_event(&s.env, "fee_dist", "deposited"));
}

// ---------------------------------------------------------------------------
// Distribution — correct amounts
// ---------------------------------------------------------------------------

#[test]
fn test_distribute_splits_50_30_20() {
    let s = Setup::new();
    s.deposit(10_000);

    s.client.distribute();

    assert_eq!(token_balance(&s.env, &s.token, &s.r[0]), 5_000); // 50 %
    assert_eq!(token_balance(&s.env, &s.token, &s.r[1]), 3_000); // 30 %
    assert_eq!(token_balance(&s.env, &s.token, &s.r[2]), 2_000); // 20 %
}

#[test]
fn test_distribute_resets_pending_to_zero() {
    let s = Setup::new();
    s.deposit(10_000);
    s.client.distribute();
    assert_eq!(s.client.get_pending(), 0);
}

#[test]
fn test_distribute_increases_total_out() {
    let s = Setup::new();
    s.deposit(10_000);
    s.client.distribute();
    assert_eq!(s.client.get_total_out(), 10_000);
}

#[test]
fn test_distribute_updates_recipient_total_received() {
    let s = Setup::new();
    s.deposit(10_000);
    s.client.distribute();

    let recips = s.client.get_recipients();
    assert_eq!(recips.get(0).unwrap().total_received, 5_000);
    assert_eq!(recips.get(1).unwrap().total_received, 3_000);
    assert_eq!(recips.get(2).unwrap().total_received, 2_000);
}

#[test]
fn test_distribute_accumulates_total_received_across_rounds() {
    let s = Setup::new();
    s.deposit(10_000);
    s.client.distribute();
    s.deposit(10_000);
    s.client.distribute();

    let recips = s.client.get_recipients();
    assert_eq!(recips.get(0).unwrap().total_received, 10_000);
    assert_eq!(recips.get(1).unwrap().total_received, 6_000);
    assert_eq!(recips.get(2).unwrap().total_received, 4_000);
}

#[test]
fn test_distribute_nothing_to_distribute_fails() {
    let s = Setup::new();
    let result = s.client.try_distribute();
    assert_eq!(result, Err(Ok(Error::NothingToDistribute)));
}

#[test]
fn test_distribute_emits_event() {
    let s = Setup::new();
    s.deposit(1_000);
    s.client.distribute();
    assert!(has_event(&s.env, "fee_dist", "distrib"));
}

// ---------------------------------------------------------------------------
// Rounding — remainder goes to first recipient
// ---------------------------------------------------------------------------

#[test]
fn test_rounding_remainder_goes_to_first_recipient() {
    // 3 recipients: 50 %, 30 %, 20 %.  Deposit 1 (indivisible).
    // floor(1 * 5000 / 10000) = 0, floor(1 * 3000 / 10000) = 0,
    // floor(1 * 2000 / 10000) = 0.  distributed = 0, remainder = 1.
    // r0 gets 0 + 1 = 1, r1 gets 0, r2 gets 0.
    let s = Setup::new();
    s.deposit(1);
    s.client.distribute();

    assert_eq!(token_balance(&s.env, &s.token, &s.r[0]), 1);
    assert_eq!(token_balance(&s.env, &s.token, &s.r[1]), 0);
    assert_eq!(token_balance(&s.env, &s.token, &s.r[2]), 0);
}

#[test]
fn test_rounding_no_dust_lost() {
    // Deposit an amount that doesn't divide evenly.
    // 10_001 with 50/30/20 split:
    //   r0 = floor(10001 * 5000 / 10000) = 5000
    //   r1 = floor(10001 * 3000 / 10000) = 3000
    //   r2 = floor(10001 * 2000 / 10000) = 2000
    //   distributed = 10000, remainder = 1 → r0 gets +1 = 5001
    let s = Setup::new();
    s.deposit(10_001);
    s.client.distribute();

    let r0 = token_balance(&s.env, &s.token, &s.r[0]);
    let r1 = token_balance(&s.env, &s.token, &s.r[1]);
    let r2 = token_balance(&s.env, &s.token, &s.r[2]);

    assert_eq!(r0 + r1 + r2, 10_001, "no dust must be lost");
    assert_eq!(r0, 5_001); // remainder lands on first recipient
    assert_eq!(r1, 3_000);
    assert_eq!(r2, 2_000);
}

#[test]
fn test_rounding_large_remainder() {
    // 2 recipients: 33.33 % (3333 bps) and 66.67 % (6667 bps).
    // Deposit 10: floor(10*3333/10000)=3, floor(10*6667/10000)=6 → sum=9, rem=1.
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let token = make_token(&env, &admin);
    let r0 = Address::generate(&env);
    let r1 = Address::generate(&env);

    let contract_id = env.register_contract(None, FeeDistribution);
    let client = FeeDistributionClient::new(&env, &contract_id);
    client.initialize(
        &admin,
        &token,
        &vec![&env, recip(r0.clone(), 3_333), recip(r1.clone(), 6_667)],
    );

    let depositor = Address::generate(&env);
    mint(&env, &token, &depositor, 10);
    client.deposit(&depositor, &10);
    client.distribute();

    let b0 = token_balance(&env, &token, &r0);
    let b1 = token_balance(&env, &token, &r1);
    assert_eq!(b0 + b1, 10, "no dust lost");
    assert_eq!(b0, 4); // 3 base + 1 remainder
    assert_eq!(b1, 6);
}

// ---------------------------------------------------------------------------
// admin_distribute
// ---------------------------------------------------------------------------

#[test]
fn test_admin_distribute_works() {
    let s = Setup::new();
    s.deposit(1_000);
    let total = s.client.admin_distribute();
    assert_eq!(total, 1_000);
    assert_eq!(s.client.get_pending(), 0);
}

#[test]
fn test_admin_distribute_nothing_fails() {
    let s = Setup::new();
    let result = s.client.try_admin_distribute();
    assert_eq!(result, Err(Ok(Error::NothingToDistribute)));
}

#[test]
fn test_non_admin_cannot_call_admin_distribute() {
    let s = Setup::new();
    s.deposit(1_000);

    // Clear mocked auths so admin auth is not provided.
    s.env.mock_auths(&[]);
    let result = s.client.try_admin_distribute();
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// set_recipients
// ---------------------------------------------------------------------------

#[test]
fn test_set_recipients_replaces_list() {
    let s = Setup::new();
    let new_r0 = Address::generate(&s.env);
    let new_r1 = Address::generate(&s.env);

    let new_list = vec![
        &s.env,
        recip(new_r0.clone(), 6_000),
        recip(new_r1.clone(), 4_000),
    ];
    s.client.set_recipients(&new_list);

    let stored = s.client.get_recipients();
    assert_eq!(stored.len(), 2);
    assert_eq!(stored.get(0).unwrap().share_bps, 6_000);
    assert_eq!(stored.get(1).unwrap().share_bps, 4_000);
}

#[test]
fn test_set_recipients_flushes_pending_first() {
    let s = Setup::new();
    s.deposit(10_000);

    let new_r = Address::generate(&s.env);
    s.client.set_recipients(&vec![&s.env, recip(new_r.clone(), 10_000)]);

    // Pending was flushed to old recipients before the switch.
    assert_eq!(s.client.get_pending(), 0);
    // Old r0 (50 %) received 5 000.
    assert_eq!(token_balance(&s.env, &s.token, &s.r[0]), 5_000);
}

#[test]
fn test_set_recipients_invalid_shares_fails() {
    let s = Setup::new();
    let bad = vec![
        &s.env,
        recip(Address::generate(&s.env), 5_000),
        recip(Address::generate(&s.env), 3_000), // sums to 8 000
    ];
    let result = s.client.try_set_recipients(&bad);
    assert_eq!(result, Err(Ok(Error::InvalidShares)));
}

#[test]
fn test_set_recipients_empty_fails() {
    let s = Setup::new();
    let result = s.client.try_set_recipients(&vec![&s.env]);
    assert_eq!(result, Err(Ok(Error::EmptyRecipients)));
}

#[test]
fn test_non_admin_cannot_set_recipients() {
    let s = Setup::new();
    s.env.mock_auths(&[]);
    let result = s.client.try_set_recipients(&vec![
        &s.env,
        recip(Address::generate(&s.env), 10_000),
    ]);
    assert!(result.is_err());
}

#[test]
fn test_set_recipients_emits_event() {
    let s = Setup::new();
    let new_r = Address::generate(&s.env);
    s.client.set_recipients(&vec![&s.env, recip(new_r, 10_000)]);
    assert!(has_event(&s.env, "fee_dist", "recips_up"));
}

// ---------------------------------------------------------------------------
// transfer_admin
// ---------------------------------------------------------------------------

#[test]
fn test_transfer_admin_changes_admin() {
    let s = Setup::new();
    let new_admin = Address::generate(&s.env);
    s.client.transfer_admin(&new_admin);
    assert_eq!(s.client.get_admin(), new_admin);
}

#[test]
fn test_non_admin_cannot_transfer_admin() {
    let s = Setup::new();
    s.env.mock_auths(&[]);
    let result = s.client.try_transfer_admin(&Address::generate(&s.env));
    assert!(result.is_err());
}

#[test]
fn test_transfer_admin_emits_event() {
    let s = Setup::new();
    let new_admin = Address::generate(&s.env);
    s.client.transfer_admin(&new_admin);
    assert!(has_event(&s.env, "fee_dist", "adm_xfer"));
}

// ---------------------------------------------------------------------------
// Multiple distribution rounds
// ---------------------------------------------------------------------------

#[test]
fn test_multiple_rounds_accumulate_correctly() {
    let s = Setup::new();

    s.deposit(10_000);
    s.client.distribute();

    s.deposit(5_000);
    s.client.distribute();

    assert_eq!(s.client.get_total_in(), 15_000);
    assert_eq!(s.client.get_total_out(), 15_000);
    assert_eq!(s.client.get_pending(), 0);

    // r0 = 50 % of 15 000 = 7 500
    assert_eq!(token_balance(&s.env, &s.token, &s.r[0]), 7_500);
    assert_eq!(token_balance(&s.env, &s.token, &s.r[1]), 4_500);
    assert_eq!(token_balance(&s.env, &s.token, &s.r[2]), 3_000);
}

#[test]
fn test_deposit_then_distribute_then_deposit_then_distribute() {
    let s = Setup::new();

    s.deposit(1_000);
    s.client.distribute();
    assert_eq!(s.client.get_pending(), 0);

    s.deposit(2_000);
    assert_eq!(s.client.get_pending(), 2_000);

    s.client.distribute();
    assert_eq!(s.client.get_pending(), 0);
    assert_eq!(s.client.get_total_out(), 3_000);
}

// ---------------------------------------------------------------------------
// Views before initialisation
// ---------------------------------------------------------------------------

#[test]
fn test_views_before_init_return_not_initialized() {
    let env = Env::default();
    let contract_id = env.register_contract(None, FeeDistribution);
    let client = FeeDistributionClient::new(&env, &contract_id);

    assert_eq!(client.try_get_pending(), Err(Ok(Error::NotInitialized)));
    assert_eq!(client.try_get_total_in(), Err(Ok(Error::NotInitialized)));
    assert_eq!(client.try_get_total_out(), Err(Ok(Error::NotInitialized)));
    assert_eq!(client.try_get_recipients(), Err(Ok(Error::NotInitialized)));
}
