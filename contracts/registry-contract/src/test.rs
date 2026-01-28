#![cfg(test)]
extern crate std;

use crate::{ContractEntry, Registry, RegistryClient, RegistryError};
use soroban_sdk::{testutils::Address as _, Address, Env, String};

#[test]
fn test_initialize() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, Registry);
    let client = RegistryClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    client.initialize(&admin);

    assert_eq!(client.get_admin(), admin);
}

#[test]
fn test_initialize_twice_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, Registry);
    let client = RegistryClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    client.initialize(&admin);
    let result = client.try_initialize(&admin);

    assert_eq!(result, Err(Ok(RegistryError::AlreadyInitialized)));
}

#[test]
fn test_register_contract() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, Registry);
    let client = RegistryClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let contract_addr = Address::generate(&env);

    client.initialize(&admin);

    let name = String::from_str(&env, "token");
    client.register_contract(&name, &contract_addr);

    let retrieved = client.get_contract(&name);
    assert_eq!(retrieved, Some(contract_addr));
}

#[test]
fn test_register_multiple_contracts() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, Registry);
    let client = RegistryClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let token_addr = Address::generate(&env);
    let vault_addr = Address::generate(&env);
    let oracle_addr = Address::generate(&env);

    client.initialize(&admin);

    let token_name = String::from_str(&env, "token");
    let vault_name = String::from_str(&env, "vault");
    let oracle_name = String::from_str(&env, "oracle");

    client.register_contract(&token_name, &token_addr);
    client.register_contract(&vault_name, &vault_addr);
    client.register_contract(&oracle_name, &oracle_addr);

    assert_eq!(client.get_contract(&token_name), Some(token_addr));
    assert_eq!(client.get_contract(&vault_name), Some(vault_addr));
    assert_eq!(client.get_contract(&oracle_name), Some(oracle_addr));
}

#[test]
fn test_get_contract_not_found() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, Registry);
    let client = RegistryClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    client.initialize(&admin);

    let name = String::from_str(&env, "nonexistent");
    let result = client.get_contract(&name);
    assert_eq!(result, None);
}

#[test]
fn test_list_contracts_empty() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, Registry);
    let client = RegistryClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    client.initialize(&admin);

    let contracts = client.list_contracts();
    assert_eq!(contracts.len(), 0);
}

#[test]
fn test_list_contracts() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, Registry);
    let client = RegistryClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let token_addr = Address::generate(&env);
    let vault_addr = Address::generate(&env);

    client.initialize(&admin);

    let token_name = String::from_str(&env, "token");
    let vault_name = String::from_str(&env, "vault");

    client.register_contract(&token_name, &token_addr);
    client.register_contract(&vault_name, &vault_addr);

    let contracts = client.list_contracts();
    assert_eq!(contracts.len(), 2);

    let token_entry = ContractEntry {
        name: token_name.clone(),
        address: token_addr.clone(),
    };
    let vault_entry = ContractEntry {
        name: vault_name.clone(),
        address: vault_addr.clone(),
    };

    let has_token = contracts.iter().any(|e| e == token_entry);
    let has_vault = contracts.iter().any(|e| e == vault_entry);

    assert!(has_token, "token entry should exist");
    assert!(has_vault, "vault entry should exist");
}

#[test]
fn test_update_existing_contract() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, Registry);
    let client = RegistryClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let old_addr = Address::generate(&env);
    let new_addr = Address::generate(&env);

    client.initialize(&admin);

    let name = String::from_str(&env, "token");
    client.register_contract(&name, &old_addr);
    assert_eq!(client.get_contract(&name), Some(old_addr));

    client.register_contract(&name, &new_addr);
    assert_eq!(client.get_contract(&name), Some(new_addr.clone()));

    let contracts = client.list_contracts();
    assert_eq!(contracts.len(), 1);
    assert_eq!(contracts.get(0).unwrap().address, new_addr);
}

#[test]
fn test_register_without_initialize() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, Registry);
    let client = RegistryClient::new(&env, &contract_id);
    let contract_addr = Address::generate(&env);
    let name = String::from_str(&env, "token");

    let result = client.try_register_contract(&name, &contract_addr);
    assert_eq!(result, Err(Ok(RegistryError::NotInitialized)));
}

#[test]
fn test_list_contracts_without_initialize() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, Registry);
    let client = RegistryClient::new(&env, &contract_id);

    let result = client.try_list_contracts();
    assert_eq!(result, Err(Ok(RegistryError::NotInitialized)));
}

#[test]
fn test_get_admin_without_initialize() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, Registry);
    let client = RegistryClient::new(&env, &contract_id);

    let result = client.try_get_admin();
    assert_eq!(result, Err(Ok(RegistryError::NotInitialized)));
}

#[test]
fn test_admin_auth_required() {
    let env = Env::default();
    let contract_id = env.register_contract(None, Registry);
    let client = RegistryClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let non_admin = Address::generate(&env);
    let contract_addr = Address::generate(&env);

    client.initialize(&admin);

    env.mock_all_auths();

    let name = String::from_str(&env, "token");
    client.register_contract(&name, &contract_addr);

    let auths = env.auths();
    assert!(!auths.is_empty(), "should have auth entries");

    let (auth_addr, _) = &auths[0];
    assert_eq!(*auth_addr, admin);
    assert_ne!(*auth_addr, non_admin);
}
