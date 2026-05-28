#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::Address as _,
    token::StellarAssetClient,
    vec, Address, Env,
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

fn balance(env: &Env, token: &Address, who: &Address) -> i128 {
    soroban_sdk::token::Client::new(env, token).balance(who)
}

fn pct(recipient: Address, bps: u32) -> Split {
    Split { recipient, kind: SplitKind::Percentage(bps), total_received: 0 }
}

fn fixed(recipient: Address, amount: i128) -> Split {
    Split { recipient, kind: SplitKind::Fixed(amount), total_received: 0 }
}

struct Setup {
    env: Env,
    client: PaymentSplitterClient<'static>,
    admin: Address,
    token: Address,
    r: [Address; 3],
}

impl Setup {
    /// Three percentage recipients: 50 %, 30 %, 20 %.
    fn new_pct() -> Self {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let token = make_token(&env, &admin);
        let r0 = Address::generate(&env);
        let r1 = Address::generate(&env);
        let r2 = Address::generate(&env);

        let splits = vec![
            &env,
            pct(r0.clone(), 5_000),
            pct(r1.clone(), 3_000),
            pct(r2.clone(), 2_000),
        ];

        let id = env.register_contract(None, PaymentSplitter);
        let client = PaymentSplitterClient::new(&env, &id);
        client.initialize(&admin, &token, &splits);

        let client: PaymentSplitterClient<'static> = unsafe { core::mem::transmute(client) };
        Setup { env, client, admin, token, r: [r0, r1, r2] }
    }

    fn do_split(&self, amount: i128) -> Address {
        let sender = Address::generate(&self.env);
        mint(&self.env, &self.token, &sender, amount);
        self.client.split(&sender, &amount);
        sender
    }
}

// ---------------------------------------------------------------------------
// Initialisation
// ---------------------------------------------------------------------------

#[test]
fn test_initialize_ok() {
    let s = Setup::new_pct();
    assert_eq!(s.client.get_admin(), s.admin);
    assert_eq!(s.client.get_splits().len(), 3);
    assert_eq!(s.client.get_total_in(), 0);
}

#[test]
fn test_initialize_twice_fails() {
    let s = Setup::new_pct();
    let result = s.client.try_initialize(
        &s.admin,
        &s.token,
        &vec![&s.env, pct(Address::generate(&s.env), 10_000)],
    );
    assert_eq!(result, Err(Ok(Error::AlreadyInitialized)));
}

#[test]
fn test_initialize_empty_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let token = make_token(&env, &admin);
    let id = env.register_contract(None, PaymentSplitter);
    let client = PaymentSplitterClient::new(&env, &id);
    assert_eq!(
        client.try_initialize(&admin, &token, &vec![&env]),
        Err(Ok(Error::EmptyRecipients))
    );
}

#[test]
fn test_initialize_bad_bps_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let token = make_token(&env, &admin);
    let id = env.register_contract(None, PaymentSplitter);
    let client = PaymentSplitterClient::new(&env, &id);
    let bad = vec![
        &env,
        pct(Address::generate(&env), 5_000),
        pct(Address::generate(&env), 3_000), // sums to 8 000
    ];
    assert_eq!(
        client.try_initialize(&admin, &token, &bad),
        Err(Ok(Error::InvalidShares))
    );
}

// ---------------------------------------------------------------------------
// Percentage splits
// ---------------------------------------------------------------------------

#[test]
fn test_split_50_30_20() {
    let s = Setup::new_pct();
    s.do_split(10_000);
    assert_eq!(balance(&s.env, &s.token, &s.r[0]), 5_000);
    assert_eq!(balance(&s.env, &s.token, &s.r[1]), 3_000);
    assert_eq!(balance(&s.env, &s.token, &s.r[2]), 2_000);
}

#[test]
fn test_split_updates_total_in() {
    let s = Setup::new_pct();
    s.do_split(1_000);
    s.do_split(2_000);
    assert_eq!(s.client.get_total_in(), 3_000);
}

#[test]
fn test_split_zero_fails() {
    let s = Setup::new_pct();
    let sender = Address::generate(&s.env);
    assert_eq!(s.client.try_split(&sender, &0), Err(Ok(Error::ZeroAmount)));
}

#[test]
fn test_split_remainder_goes_to_first_pct_recipient() {
    // 1 token, 50/30/20 → all floor to 0, remainder 1 → r0 gets 1.
    let s = Setup::new_pct();
    s.do_split(1);
    assert_eq!(balance(&s.env, &s.token, &s.r[0]), 1);
    assert_eq!(balance(&s.env, &s.token, &s.r[1]), 0);
    assert_eq!(balance(&s.env, &s.token, &s.r[2]), 0);
}

#[test]
fn test_split_no_dust_lost() {
    let s = Setup::new_pct();
    s.do_split(10_001);
    let total = balance(&s.env, &s.token, &s.r[0])
        + balance(&s.env, &s.token, &s.r[1])
        + balance(&s.env, &s.token, &s.r[2]);
    assert_eq!(total, 10_001);
}

// ---------------------------------------------------------------------------
// Fixed splits
// ---------------------------------------------------------------------------

#[test]
fn test_fixed_split_only() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let token = make_token(&env, &admin);
    let r0 = Address::generate(&env);
    let r1 = Address::generate(&env);

    let splits = vec![&env, fixed(r0.clone(), 300), fixed(r1.clone(), 700)];
    let id = env.register_contract(None, PaymentSplitter);
    let client = PaymentSplitterClient::new(&env, &id);
    client.initialize(&admin, &token, &splits);

    let sender = Address::generate(&env);
    mint(&env, &token, &sender, 1_000);
    client.split(&sender, &1_000);

    assert_eq!(balance(&env, &token, &r0), 300);
    assert_eq!(balance(&env, &token, &r1), 700);
}

#[test]
fn test_mixed_fixed_and_percentage() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let token = make_token(&env, &admin);
    let r_fixed = Address::generate(&env);
    let r_pct0 = Address::generate(&env);
    let r_pct1 = Address::generate(&env);

    // Fixed 200, then 60/40 on the remaining 800.
    let splits = vec![
        &env,
        fixed(r_fixed.clone(), 200),
        pct(r_pct0.clone(), 6_000),
        pct(r_pct1.clone(), 4_000),
    ];
    let id = env.register_contract(None, PaymentSplitter);
    let client = PaymentSplitterClient::new(&env, &id);
    client.initialize(&admin, &token, &splits);

    let sender = Address::generate(&env);
    mint(&env, &token, &sender, 1_000);
    client.split(&sender, &1_000);

    assert_eq!(balance(&env, &token, &r_fixed), 200);
    assert_eq!(balance(&env, &token, &r_pct0), 480); // 60 % of 800
    assert_eq!(balance(&env, &token, &r_pct1), 320); // 40 % of 800
}

#[test]
fn test_insufficient_for_fixed_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let token = make_token(&env, &admin);
    let r0 = Address::generate(&env);

    let splits = vec![&env, fixed(r0.clone(), 1_000)];
    let id = env.register_contract(None, PaymentSplitter);
    let client = PaymentSplitterClient::new(&env, &id);
    client.initialize(&admin, &token, &splits);

    let sender = Address::generate(&env);
    mint(&env, &token, &sender, 500);
    assert_eq!(
        client.try_split(&sender, &500),
        Err(Ok(Error::InsufficientForFixed))
    );
}

// ---------------------------------------------------------------------------
// set_splits (admin only)
// ---------------------------------------------------------------------------

#[test]
fn test_set_splits_replaces_list() {
    let s = Setup::new_pct();
    let new_r = Address::generate(&s.env);
    s.client.set_splits(&vec![&s.env, pct(new_r.clone(), 10_000)]);
    assert_eq!(s.client.get_splits().len(), 1);
}

#[test]
fn test_non_admin_cannot_set_splits() {
    let s = Setup::new_pct();
    s.env.mock_auths(&[]);
    let result = s.client.try_set_splits(&vec![
        &s.env,
        pct(Address::generate(&s.env), 10_000),
    ]);
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// transfer_admin
// ---------------------------------------------------------------------------

#[test]
fn test_transfer_admin() {
    let s = Setup::new_pct();
    let new_admin = Address::generate(&s.env);
    s.client.transfer_admin(&new_admin);
    assert_eq!(s.client.get_admin(), new_admin);
}

#[test]
fn test_non_admin_cannot_transfer_admin() {
    let s = Setup::new_pct();
    s.env.mock_auths(&[]);
    assert!(s
        .client
        .try_transfer_admin(&Address::generate(&s.env))
        .is_err());
}

// ---------------------------------------------------------------------------
// total_received tracking
// ---------------------------------------------------------------------------

#[test]
fn test_total_received_accumulates() {
    let s = Setup::new_pct();
    s.do_split(10_000);
    s.do_split(10_000);
    let splits = s.client.get_splits();
    assert_eq!(splits.get(0).unwrap().total_received, 10_000); // 50 % × 2
    assert_eq!(splits.get(1).unwrap().total_received, 6_000);
    assert_eq!(splits.get(2).unwrap().total_received, 4_000);
}
