#![cfg(test)]
extern crate std;

use crate::{MigrationStep, UpgradeError, UpgradeableProxy, UpgradeableProxyClient};
use soroban_sdk::{
    symbol_short, testutils::Address as _, vec, Address, Env, IntoVal, TryFromVal, Val, Vec,
};

mod logic_v1 {
    use soroban_sdk::{contract, contractimpl, Env};

    #[contract]
    pub struct Contract;

    #[contractimpl]
    impl Contract {
        pub fn ping(_env: Env, value: u32) -> u32 {
            value + 1
        }
    }
}

mod logic_v2 {
    use soroban_sdk::{contract, contractimpl, Env};

    #[contract]
    pub struct Contract;

    #[contractimpl]
    impl Contract {
        pub fn ping(_env: Env, value: u32) -> u32 {
            value + 2
        }
    }
}

mod additive_migrator {
    use soroban_sdk::{contract, contractimpl, Env, Symbol};

    #[contract]
    pub struct Contract;

    #[contractimpl]
    impl Contract {
        pub fn migrate(
            _env: Env,
            _key: Symbol,
            current_value: i128,
            data: i128,
            _from_version: u32,
            _to_version: u32,
        ) -> i128 {
            current_value + data
        }
    }
}

#[test]
fn test_forward_uses_current_implementation() {
    let env = Env::default();
    env.mock_all_auths();

    let v1 = env.register_contract(None, logic_v1::Contract);
    let proxy_id = env.register_contract(None, UpgradeableProxy);
    let proxy = UpgradeableProxyClient::new(&env, &proxy_id);

    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    proxy.initialize(&admin, &v1, &1);

    let args: Vec<Val> = vec![&env, 10u32.into_val(&env)];
    let result = proxy.forward(&user, &symbol_short!("ping"), &args);
    let decoded = u32::try_from_val(&env, &result).unwrap();
    assert_eq!(decoded, 11);
}

#[test]
fn test_upgrade_with_migration_tracks_version_and_preserves_state() {
    let env = Env::default();
    env.mock_all_auths();

    let v1 = env.register_contract(None, logic_v1::Contract);
    let v2 = env.register_contract(None, logic_v2::Contract);
    let migrator = env.register_contract(None, additive_migrator::Contract);
    let proxy_id = env.register_contract(None, UpgradeableProxy);
    let proxy = UpgradeableProxyClient::new(&env, &proxy_id);

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let counter = symbol_short!("counter");

    proxy.initialize(&admin, &v1, &1);
    proxy.set_state_value(&counter, &100);

    let steps = vec![
        &env,
        MigrationStep {
            key: counter.clone(),
            script: migrator,
            data: 25,
        },
    ];

    proxy.stage_upgrade(&v2, &2, &steps);
    proxy.execute_upgrade();

    assert_eq!(proxy.current_version(), 2);
    assert_eq!(proxy.current_implementation(), v2.clone());
    assert_eq!(proxy.implementation_for_version(&1), Some(v1));
    assert_eq!(proxy.implementation_for_version(&2), Some(v2.clone()));
    assert_eq!(proxy.get_state_value(&counter), 125);

    let args: Vec<Val> = vec![&env, 10u32.into_val(&env)];
    let result = proxy.forward(&user, &symbol_short!("ping"), &args);
    let decoded = u32::try_from_val(&env, &result).unwrap();
    assert_eq!(decoded, 12);
}

#[test]
fn test_only_admin_is_authorized_for_upgrade_controls() {
    let env = Env::default();
    env.mock_all_auths();

    let v1 = env.register_contract(None, logic_v1::Contract);
    let v2 = env.register_contract(None, logic_v2::Contract);
    let proxy_id = env.register_contract(None, UpgradeableProxy);
    let proxy = UpgradeableProxyClient::new(&env, &proxy_id);

    let admin = Address::generate(&env);
    let non_admin = Address::generate(&env);
    let steps: Vec<MigrationStep> = Vec::new(&env);

    proxy.initialize(&admin, &v1, &1);
    proxy.stage_upgrade(&v2, &2, &steps);

    let auths = env.auths();
    assert!(!auths.is_empty(), "expected auth entries");
    let (auth_addr, _) = &auths[0];
    assert_eq!(*auth_addr, admin);
    assert_ne!(*auth_addr, non_admin);
}

#[test]
fn test_rollback_restores_previous_version() {
    let env = Env::default();
    env.mock_all_auths();

    let v1 = env.register_contract(None, logic_v1::Contract);
    let v2 = env.register_contract(None, logic_v2::Contract);
    let proxy_id = env.register_contract(None, UpgradeableProxy);
    let proxy = UpgradeableProxyClient::new(&env, &proxy_id);

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let steps: Vec<MigrationStep> = Vec::new(&env);

    proxy.initialize(&admin, &v1, &1);
    proxy.stage_upgrade(&v2, &2, &steps);
    proxy.execute_upgrade();

    assert_eq!(proxy.current_version(), 2);
    proxy.rollback();
    assert_eq!(proxy.current_version(), 1);
    assert_eq!(proxy.current_implementation(), v1);

    let args: Vec<Val> = vec![&env, 10u32.into_val(&env)];
    let result = proxy.forward(&user, &symbol_short!("ping"), &args);
    let decoded = u32::try_from_val(&env, &result).unwrap();
    assert_eq!(decoded, 11);

    let history = proxy.get_upgrade_history();
    assert_eq!(history.len(), 2);
    assert!(!history.get(0).unwrap().rollback);
    assert!(history.get(1).unwrap().rollback);
}

#[test]
fn test_emergency_pause_blocks_forwarding() {
    let env = Env::default();
    env.mock_all_auths();

    let v1 = env.register_contract(None, logic_v1::Contract);
    let proxy_id = env.register_contract(None, UpgradeableProxy);
    let proxy = UpgradeableProxyClient::new(&env, &proxy_id);

    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    proxy.initialize(&admin, &v1, &1);
    proxy.pause();

    let args: Vec<Val> = vec![&env, 5u32.into_val(&env)];
    let paused_result = proxy.try_forward(&user, &symbol_short!("ping"), &args);
    assert!(matches!(
        paused_result,
        Err(Ok(UpgradeError::ContractPaused))
    ));

    proxy.unpause();
    let result = proxy.forward(&user, &symbol_short!("ping"), &args);
    let decoded = u32::try_from_val(&env, &result).unwrap();
    assert_eq!(decoded, 6);
}

#[test]
fn test_admin_cannot_use_fallback_calls() {
    let env = Env::default();
    env.mock_all_auths();

    let v1 = env.register_contract(None, logic_v1::Contract);
    let proxy_id = env.register_contract(None, UpgradeableProxy);
    let proxy = UpgradeableProxyClient::new(&env, &proxy_id);

    let admin = Address::generate(&env);
    proxy.initialize(&admin, &v1, &1);

    let args: Vec<Val> = vec![&env, 1u32.into_val(&env)];
    let result = proxy.try_forward(&admin, &symbol_short!("ping"), &args);
    assert!(matches!(
        result,
        Err(Ok(UpgradeError::AdminCannotFallback))
    ));
}

#[test]
fn test_execute_upgrade_requires_pending_upgrade() {
    let env = Env::default();
    env.mock_all_auths();

    let v1 = env.register_contract(None, logic_v1::Contract);
    let proxy_id = env.register_contract(None, UpgradeableProxy);
    let proxy = UpgradeableProxyClient::new(&env, &proxy_id);

    let admin = Address::generate(&env);
    proxy.initialize(&admin, &v1, &1);

    let result = proxy.try_execute_upgrade();
    assert_eq!(result, Err(Ok(UpgradeError::NoPendingUpgrade)));
}
