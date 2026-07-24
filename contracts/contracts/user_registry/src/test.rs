#![cfg(test)]

use super::*;
use soroban_sdk::{testutils::Address as _, Env, String};

fn setup() -> (Env, UserRegistryContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, UserRegistryContract);
    let client = UserRegistryContractClient::new(&env, &contract_id);
    (env, client)
}

/// Verify that registering the same DID twice panics with a duplicate error.
#[test]
#[should_panic(expected = "DID already registered")]
fn test_register_privy_did_duplicate_fails() {
    let (env, client) = setup();
    let wallet = Address::generate(&env);
    let did = String::from_str(&env, "did:privy:abc123");
    client.register_privy_did(&did, &wallet);
    // Second registration with same DID must panic
    client.register_privy_did(&did, &wallet);
}

/// Verify that a successful DID registration stores the correct wallet mapping.
#[test]
fn test_register_privy_did_success() {
    let (env, client) = setup();
    let wallet = Address::generate(&env);
    let did = String::from_str(&env, "did:privy:user1");
    client.register_privy_did(&did, &wallet);
    assert_eq!(client.get_wallet_for_did(&did), wallet);
}

/// Verify that updating a DID mapping with the correct old wallet succeeds.
#[test]
fn test_update_privy_did_success() {
    let (env, client) = setup();
    let old_wallet = Address::generate(&env);
    let new_wallet = Address::generate(&env);
    let did = String::from_str(&env, "did:privy:user2");
    client.register_privy_did(&did, &old_wallet);
    client.update_privy_did(&did, &old_wallet, &new_wallet);
    assert_eq!(client.get_wallet_for_did(&did), new_wallet);
}

/// Verify that updating a DID mapping with the wrong old wallet panics.
#[test]
#[should_panic(expected = "unauthorized: old wallet does not match registered wallet")]
fn test_update_privy_did_wrong_wallet_fails() {
    let (env, client) = setup();
    let correct_wallet = Address::generate(&env);
    let wrong_wallet = Address::generate(&env);
    let new_wallet = Address::generate(&env);
    let did = String::from_str(&env, "did:privy:user3");
    client.register_privy_did(&did, &correct_wallet);
    client.update_privy_did(&did, &wrong_wallet, &new_wallet);
}

/// Verify that admin recovery reassigns a DID to a new wallet.
#[test]
fn test_recover_privy_did_as_admin() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let old_wallet = Address::generate(&env);
    let new_wallet = Address::generate(&env);
    let did = String::from_str(&env, "did:privy:user4");
    client.initialize(&admin);
    client.register_privy_did(&did, &old_wallet);
    client.recover_privy_did(&did, &new_wallet);
    assert_eq!(client.get_wallet_for_did(&did), new_wallet);
}

/// Verify that querying an unregistered DID panics.
#[test]
#[should_panic(expected = "DID not registered")]
fn test_get_wallet_for_unregistered_did_fails() {
    let (env, client) = setup();
    let did = String::from_str(&env, "did:privy:ghost");
    client.get_wallet_for_did(&did);
}
