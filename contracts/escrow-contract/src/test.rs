#![cfg(test)]

use super::*;
use soroban_sdk::{testutils::Address as _, Address, Env, String};

#[test]
fn test_register_user_success() {
    let env = Env::default();
    let contract_id = env.register_contract(None, UserIdentityContract);
    let client = UserIdentityContractClient::new(&env, &contract_id);

    // Mock the test environment
    env.mock_all_auths();

    let user_addr = Address::generate(&env);
    let role = String::from_str(&env, "admin");

    // Register user
    client.register(&user_addr, &role);

    // Verify user is registered
    assert!(client.is_registered(&user_addr));

    // Verify user data
    let user = client.get_user(&user_addr);
    assert_eq!(user.address, user_addr);
    assert_eq!(user.role, role);
}

#[test]
fn test_register_duplicate_user() {
    let env = Env::default();
    let contract_id = env.register_contract(None, UserIdentityContract);
    let client = UserIdentityContractClient::new(&env, &contract_id);

    env.mock_all_auths();

    let user_addr = Address::generate(&env);
    let role = String::from_str(&env, "user");

    // Register user first time
    client.register(&user_addr, &role);

    // Try to register the same user again
    let result = client.try_register(&user_addr, &role);
    assert_eq!(result, Err(Ok(Error::AlreadyRegistered)));
}

#[test]
fn test_get_user_not_found() {
    let env = Env::default();
    let contract_id = env.register_contract(None, UserIdentityContract);
    let client = UserIdentityContractClient::new(&env, &contract_id);

    let user_addr = Address::generate(&env);

    // Try to get user that doesn't exist
    let result = client.try_get_user(&user_addr);
    assert_eq!(result, Err(Ok(Error::UserNotFound)));
}

#[test]
fn test_is_registered_false() {
    let env = Env::default();
    let contract_id = env.register_contract(None, UserIdentityContract);
    let client = UserIdentityContractClient::new(&env, &contract_id);

    let user_addr = Address::generate(&env);

    // Check if unregistered user is registered
    assert!(!client.is_registered(&user_addr));
}

#[test]
fn test_multiple_users_with_different_roles() {
    let env = Env::default();
    let contract_id = env.register_contract(None, UserIdentityContract);
    let client = UserIdentityContractClient::new(&env, &contract_id);

    env.mock_all_auths();

    // Register multiple users with different roles
    let admin_addr = Address::generate(&env);
    let admin_role = String::from_str(&env, "admin");

    let moderator_addr = Address::generate(&env);
    let moderator_role = String::from_str(&env, "moderator");

    let user_addr = Address::generate(&env);
    let user_role = String::from_str(&env, "user");

    // Register all users
    client.register(&admin_addr, &admin_role);
    client.register(&moderator_addr, &moderator_role);
    client.register(&user_addr, &user_role);

    // Verify all are registered
    assert!(client.is_registered(&admin_addr));
    assert!(client.is_registered(&moderator_addr));
    assert!(client.is_registered(&user_addr));

    // Verify correct roles
    let admin = client.get_user(&admin_addr);
    assert_eq!(admin.role, admin_role);

    let moderator = client.get_user(&moderator_addr);
    assert_eq!(moderator.role, moderator_role);

    let user = client.get_user(&user_addr);
    assert_eq!(user.role, user_role);
}

#[test]
fn test_register_requires_auth() {
    let env = Env::default();
    let contract_id = env.register_contract(None, UserIdentityContract);
    let client = UserIdentityContractClient::new(&env, &contract_id);

    let user_addr = Address::generate(&env);
    let role = String::from_str(&env, "user");

    // Mock authentication
    env.mock_all_auths();

    client.register(&user_addr, &role);

    // Verify auth was required by checking that auth was recorded
    let auths = env.auths();
    assert!(!auths.is_empty());
    assert_eq!(auths.len(), 1);
    assert_eq!(auths[0].0, user_addr);
}

#[test]
fn test_register_with_empty_role() {
    let env = Env::default();
    let contract_id = env.register_contract(None, UserIdentityContract);
    let client = UserIdentityContractClient::new(&env, &contract_id);

    env.mock_all_auths();

    let user_addr = Address::generate(&env);
    let empty_role = String::from_str(&env, "");

    // Register user with empty role (should succeed as validation is up to the caller)
    client.register(&user_addr, &empty_role);

    // Verify user is registered with empty role
    let user = client.get_user(&user_addr);
    assert_eq!(user.role, empty_role);
}

#[test]
fn test_register_with_long_role_name() {
    let env = Env::default();
    let contract_id = env.register_contract(None, UserIdentityContract);
    let client = UserIdentityContractClient::new(&env, &contract_id);

    env.mock_all_auths();

    let user_addr = Address::generate(&env);
    let long_role = String::from_str(&env, "super_administrator_with_full_permissions");

    // Register user with long role name
    client.register(&user_addr, &long_role);

    // Verify user data
    let user = client.get_user(&user_addr);
    assert_eq!(user.role, long_role);
}
