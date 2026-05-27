#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::Address as _,
    Address, Env, Symbol,
};

fn setup() -> (Env, LoyaltyPointsClient<'static>, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let points_name = Symbol::new(&env, "ZAPS_POINTS");

    let contract_id = env.register_contract(None, LoyaltyPoints);
    let client = LoyaltyPointsClient::new(&env, &contract_id);
    client.initialize(&admin, &points_name, &100_000);

    let client: LoyaltyPointsClient<'static> = unsafe { core::mem::transmute(client) };

    (env, client, admin, user)
}

#[test]
fn test_initialize() {
    let (_, _, _, _) = setup();
}

#[test]
fn test_award_and_balance() {
    let (env, client, admin, user) = setup();
    client.award_points(&user, &1000);
    assert_eq!(client.get_balance(&user), 1000);
}

#[test]
fn test_redeem_points() {
    let (env, client, admin, user) = setup();
    client.award_points(&user, &1000);

    let opt_name = Symbol::new(&env, "DISCOUNT_10");
    let opt_id = client.add_redemption_option(&opt_name, &500);

    client.redeem_points(&user, &opt_id);

    assert_eq!(client.get_balance(&user), 500);
}

#[test]
fn test_insufficient_points() {
    let (env, client, admin, user) = setup();
    client.award_points(&user, &100);

    let opt_name = Symbol::new(&env, "DISCOUNT_50");
    let opt_id = client.add_redemption_option(&opt_name, &500);

    let result = client.try_redeem_points(&user, &opt_id);
    assert!(result.is_err());
}

#[test]
fn test_add_redemption_option() {
    let (env, client, _, _) = setup();
    let opt_name = Symbol::new(&env, "DISCOUNT_20");
    let opt_id = client.add_redemption_option(&opt_name, &200);

    let option = client.get_redemption_option(&opt_id);
    assert_eq!(option.name, opt_name);
    assert_eq!(option.cost, 200);
    assert!(option.active);
}

#[test]
fn test_transfer_admin() {
    let (_, client, _, _) = setup();
    let new_admin = Address::generate(&client.env);
    client.transfer_admin(&new_admin);
}
