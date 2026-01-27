#![cfg(test)]

use super::*;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{vec, Env, IntoVal};

#[test]
fn test_initialize() {
    let env = Env::default();
    let contract_id = env.register_contract(None, ReputationScoreContract);
    let client = ReputationScoreContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize(&admin);

    // Should panic if initialized again
    let result = client.try_initialize(&admin);
    assert!(result.is_err());
}

#[test]
fn test_increase_score() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, ReputationScoreContract);
    let client = ReputationScoreContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin);

    client.increase_score(&user, &10);
    assert_eq!(client.get_score(&user), 10);

    client.increase_score(&user, &5);
    assert_eq!(client.get_score(&user), 15);
}

#[test]
fn test_decrease_score() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, ReputationScoreContract);
    let client = ReputationScoreContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin);

    client.increase_score(&user, &20);
    assert_eq!(client.get_score(&user), 20);

    client.decrease_score(&user, &5);
    assert_eq!(client.get_score(&user), 15);

    // Test underflow prevention
    client.decrease_score(&user, &20);
    assert_eq!(client.get_score(&user), 0);
}

#[test]
#[should_panic(expected = "HostError: Error(Auth, InvalidAction)")]
fn test_auth_enforcement_increase() {
    let env = Env::default();
    // No mock_all_auths() here to test actual auth

    let contract_id = env.register_contract(None, ReputationScoreContract);
    let client = ReputationScoreContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let attacker = Address::generate(&env);

    client.initialize(&admin);

    // This should fail because attacker is trying to call it
    // In a real test we'd need to mock the auth for the admin,
    // but here we just want to see it fail when no auth is provided or wrong one is used.
    // client.increase_score(&user, &10);

    // Setting up the specific auth for admin
    env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &attacker,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &contract_id,
            fn_name: "increase_score",
            args: vec![&env, user.into_val(&env), 10u32.into_val(&env)],
            sub_invokes: &[],
        },
    }]);

    client.increase_score(&user, &10);
}

#[test]
fn test_get_score_uninitialized_user() {
    let env = Env::default();
    let contract_id = env.register_contract(None, ReputationScoreContract);
    let client = ReputationScoreContractClient::new(&env, &contract_id);

    let user = Address::generate(&env);
    assert_eq!(client.get_score(&user), 0);
}

#[test]
fn test_multiple_users() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, ReputationScoreContract);
    let client = ReputationScoreContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);

    client.initialize(&admin);

    client.increase_score(&user1, &10);
    client.increase_score(&user2, &20);

    assert_eq!(client.get_score(&user1), 10);
    assert_eq!(client.get_score(&user2), 20);

    client.decrease_score(&user1, &5);
    assert_eq!(client.get_score(&user1), 5);
    assert_eq!(client.get_score(&user2), 20);
}

#[test]
#[should_panic(expected = "Score overflow")]
fn test_score_overflow() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, ReputationScoreContract);
    let client = ReputationScoreContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.initialize(&admin);

    client.increase_score(&user, &u32::MAX);
    client.increase_score(&user, &1); // Should panic
}

#[test]
#[should_panic(expected = "HostError: Error(Auth, InvalidAction)")]
fn test_auth_enforcement_decrease() {
    let env = Env::default();

    let contract_id = env.register_contract(None, ReputationScoreContract);
    let client = ReputationScoreContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let attacker = Address::generate(&env);

    client.initialize(&admin);

    env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &attacker,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &contract_id,
            fn_name: "decrease_score",
            args: vec![&env, user.into_val(&env), 10u32.into_val(&env)],
            sub_invokes: &[],
        },
    }]);

    client.decrease_score(&user, &10);
}
