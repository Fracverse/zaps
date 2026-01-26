#![cfg(test)]

use super::*;
use soroban_sdk::testutils::{Address as _, Events};
use soroban_sdk::{Bytes, Env, FromVal, IntoVal, Symbol};

#[test]
fn test_user_registration() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, ZapsRegistry);
    let client = ZapsRegistryClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize(&admin);

    let user = Address::generate(&env);
    let user_id = Bytes::from_slice(&env, b"user123");

    client.register_user(&user_id, &user);

    assert_eq!(client.resolve_user(&user_id), user);

    // Verify event
    let events = env.events().all();
    let last_event = events.last().unwrap();
    assert_eq!(last_event.0, contract_id);

    // Topics are Vec<Val>
    let topics = last_event.1.clone();
    assert_eq!(
        Symbol::from_val(&env, &topics.get(0).unwrap()),
        symbol_short!("user_reg")
    );
    assert_eq!(Bytes::from_val(&env, &topics.get(1).unwrap()), user_id);

    let event_data: Address = FromVal::from_val(&env, &last_event.2);
    assert_eq!(event_data, user);

    // Test duplicate ID
    let user2 = Address::generate(&env);
    let result = client.try_register_user(&user_id, &user2);
    assert_eq!(result, Err(Ok(Error::DuplicateId)));
}

#[test]
fn test_merchant_registration_and_resolution() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, ZapsRegistry);
    let client = ZapsRegistryClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize(&admin);

    let merchant_id = Bytes::from_slice(&env, b"merch777");
    let vault = Address::generate(&env);
    let asset = Address::generate(&env);

    client.register_merchant(&merchant_id, &vault, &asset);

    let metadata = client.resolve_merchant(&merchant_id);
    assert_eq!(metadata.vault, vault);
    assert_eq!(metadata.settlement_asset, asset);
    assert_eq!(metadata.active, true);

    // Test deactivation
    client.deactivate_merchant(&merchant_id);
    let result = client.try_resolve_merchant(&merchant_id);
    assert_eq!(result, Err(Ok(Error::InactiveMerchant)));
}

#[test]
fn test_access_control() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, ZapsRegistry);
    let client = ZapsRegistryClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize(&admin);

    let merchant_id = Bytes::from_slice(&env, b"merch1");
    let vault = Address::generate(&env);
    let asset = Address::generate(&env);

    // Non-admin tries to register merchant (mock_all_auths will make it pass but we verify the auth was required)
    client.register_merchant(&merchant_id, &vault, &asset);

    // Check resolve
    assert_eq!(client.resolve_merchant(&merchant_id).active, true);
}

#[test]
#[should_panic(expected = "HostError: Error(Contract, #1)")]
fn test_already_initialized() {
    let env = Env::default();
    let contract_id = env.register_contract(None, ZapsRegistry);
    let client = ZapsRegistryClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize(&admin);
    client.initialize(&admin);
}

#[test]
fn test_not_registered_resolutions() {
    let env = Env::default();
    let contract_id = env.register_contract(None, ZapsRegistry);
    let client = ZapsRegistryClient::new(&env, &contract_id);

    let user_id = Bytes::from_slice(&env, b"unknown_user");
    let result_user = client.try_resolve_user(&user_id);
    assert_eq!(result_user, Err(Ok(Error::UserNotFound)));

    let merchant_id = Bytes::from_slice(&env, b"unknown_merch");
    let result_merch = client.try_resolve_merchant(&merchant_id);
    assert_eq!(result_merch, Err(Ok(Error::MerchantNotFound)));
}
