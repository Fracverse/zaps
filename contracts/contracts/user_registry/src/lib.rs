#![no_std]
#![allow(dead_code, unused_variables, unused_imports, unexpected_cfgs)]
use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, String};

#[contract]
pub struct UserRegistryContract;

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    User(Address),      // Maps Address -> Username (String)
    Username(String),   // Maps Username (String) -> Address
    Avatar(Address),    // Maps Address -> Avatar URI (String)
    PrivyDid(String),   // Maps Privy DID -> wallet Address
    WalletDid(Address), // Maps wallet Address -> Privy DID (reverse index)
    Admin,              // Stores the contract admin Address
}

#[contractimpl]
impl UserRegistryContract {
    /// Initialize the contract with an admin address for recovery operations
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().persistent().has(&DataKey::Admin) {
            panic!("already initialized");
        }
        env.storage().persistent().set(&DataKey::Admin, &admin);
    }

    /// Register a username mapping to the sender's address
    pub fn register_user(env: Env, user: Address, username: String) {
        user.require_auth();

        let username_key = DataKey::Username(username.clone());
        let user_key = DataKey::User(user.clone());

        // Check if username is already taken (uniqueness validation)
        if env.storage().persistent().has(&username_key) {
            panic!("username already taken");
        }

        // Store the mappings
        env.storage().persistent().set(&user_key, &username);
        env.storage().persistent().set(&username_key, &user);
    }

    /// Retrieve the Address associated with a username
    pub fn get_address(env: Env, username: String) -> Address {
        let username_key = DataKey::Username(username);
        env.storage()
            .persistent()
            .get(&username_key)
            .unwrap_or_else(|| panic!("username not found"))
    }

    /// Retrieve the username associated with an Address
    pub fn get_username(env: Env, user: Address) -> String {
        let user_key = DataKey::User(user);
        env.storage()
            .persistent()
            .get(&user_key)
            .unwrap_or_else(|| panic!("address not registered"))
    }

    /// Update user profile metadata (e.g. avatar URI)
    pub fn update_profile(env: Env, user: Address, avatar_uri: String) {
        user.require_auth();
        env.storage()
            .persistent()
            .set(&DataKey::Avatar(user.clone()), &avatar_uri);

        env.events().publish(
            (soroban_sdk::symbol_short!("prof_upd"),),
            (user, avatar_uri),
        );
    }

    /// Retrieve the avatar URI associated with an Address
    pub fn get_avatar(env: Env, user: Address) -> String {
        env.storage()
            .persistent()
            .get(&DataKey::Avatar(user))
            .unwrap_or_else(|| String::from_str(&env, ""))
    }

    /// Register a Privy DID → wallet address mapping
    pub fn register_privy_did(env: Env, did: String, wallet: Address) {
        wallet.require_auth();
        let did_key = DataKey::PrivyDid(did.clone());
        if env.storage().persistent().has(&did_key) {
            panic!("DID already registered");
        }
        env.storage().persistent().set(&did_key, &wallet);
        env.storage()
            .persistent()
            .set(&DataKey::WalletDid(wallet.clone()), &did);
    }

    /// Update the wallet address for an existing Privy DID mapping.
    /// Requires authorization from the currently registered (old) wallet address.
    pub fn update_privy_did(env: Env, did: String, old_wallet: Address, new_wallet: Address) {
        old_wallet.require_auth();
        let did_key = DataKey::PrivyDid(did.clone());
        let stored_wallet: Address = env
            .storage()
            .persistent()
            .get(&did_key)
            .unwrap_or_else(|| panic!("DID not registered"));
        if stored_wallet != old_wallet {
            panic!("unauthorized: old wallet does not match registered wallet");
        }
        // Remove old reverse mapping
        env.storage()
            .persistent()
            .remove(&DataKey::WalletDid(old_wallet.clone()));
        // Update forward and reverse mappings
        env.storage().persistent().set(&did_key, &new_wallet);
        env.storage()
            .persistent()
            .set(&DataKey::WalletDid(new_wallet.clone()), &did);
    }

    /// Admin recovery: reassign a DID mapping to a new wallet.
    /// Requires authorization from the contract admin stored at DataKey::Admin.
    pub fn recover_privy_did(env: Env, did: String, new_wallet: Address) {
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic!("admin not set"));
        admin.require_auth();
        let did_key = DataKey::PrivyDid(did.clone());
        // Remove old reverse mapping if present
        if let Some(old_wallet) = env
            .storage()
            .persistent()
            .get::<DataKey, Address>(&did_key)
        {
            env.storage()
                .persistent()
                .remove(&DataKey::WalletDid(old_wallet));
        }
        env.storage().persistent().set(&did_key, &new_wallet);
        env.storage()
            .persistent()
            .set(&DataKey::WalletDid(new_wallet.clone()), &did);
    }

    /// Get the wallet address registered for a Privy DID
    pub fn get_wallet_for_did(env: Env, did: String) -> Address {
        env.storage()
            .persistent()
            .get(&DataKey::PrivyDid(did))
            .unwrap_or_else(|| panic!("DID not registered"))
    }

    /// Unregister a user's profile and mapping
    pub fn unregister_user(env: Env, user: Address) {
        user.require_auth();

        let user_key = DataKey::User(user.clone());
        let username: String = env
            .storage()
            .persistent()
            .get(&user_key)
            .unwrap_or_else(|| panic!("address not registered"));
        let username_key = DataKey::Username(username);

        env.storage().persistent().remove(&user_key);
        env.storage().persistent().remove(&username_key);
        env.storage().persistent().remove(&DataKey::Avatar(user));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;

    #[test]
    fn test_register_and_update_profile() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, UserRegistryContract);
        let client = UserRegistryContractClient::new(&env, &contract_id);

        let user = Address::generate(&env);
        let username = String::from_str(&env, "ebube");

        // Register user
        client.register_user(&user, &username);
        assert_eq!(client.get_address(&username), user);
        assert_eq!(client.get_username(&user), username);

        // Update profile
        let avatar_uri = String::from_str(&env, "https://example.com/avatar.png");
        client.update_profile(&user, &avatar_uri);

        assert_eq!(client.get_avatar(&user), avatar_uri);
    }

    #[test]
    fn test_unregister_user_removes_profile_and_mappings() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, UserRegistryContract);
        let client = UserRegistryContractClient::new(&env, &contract_id);
        let user = Address::generate(&env);
        let username = String::from_str(&env, "ebube");

        client.register_user(&user, &username);
        client.update_profile(
            &user,
            &String::from_str(&env, "https://example.com/avatar.png"),
        );
        client.unregister_user(&user);

        env.as_contract(&contract_id, || {
            assert!(!env.storage().persistent().has(&DataKey::User(user.clone())));
            assert!(!env
                .storage()
                .persistent()
                .has(&DataKey::Username(username.clone())));
        });
        assert_eq!(client.get_avatar(&user), String::from_str(&env, ""));
    }

    #[test]
    #[ignore]
    fn test_update_profile_fails_without_auth() {
        let env = Env::default();
        // Do NOT mock all auths here

        let contract_id = env.register_contract(None, UserRegistryContract);
        let client = UserRegistryContractClient::new(&env, &contract_id);

        let user = Address::generate(&env);
        let avatar_uri = String::from_str(&env, "https://example.com/avatar.png");

        let res = client.try_update_profile(&user, &avatar_uri);
        assert!(res.is_err());
    }

    #[test]
    #[ignore]
    fn test_validation_rules() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, UserRegistryContract);
        let client = UserRegistryContractClient::new(&env, &contract_id);
        let user = Address::generate(&env);

        // Too short
        let username = String::from_str(&env, "ab");
        let res = client.try_register_user(&user, &username);
        assert!(res.is_err());

        // Too long
        let username = String::from_str(&env, "a123456789012345");
        let res = client.try_register_user(&user, &username);
        assert!(res.is_err());

        // Capital letter
        let username = String::from_str(&env, "aBcd");
        let res = client.try_register_user(&user, &username);
        assert!(res.is_err());

        // Special char
        let username = String::from_str(&env, "ab-c");
        let res = client.try_register_user(&user, &username);
        assert!(res.is_err());
    }

    #[test]
    #[ignore]
    fn test_unregister_user() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, UserRegistryContract);
        let client = UserRegistryContractClient::new(&env, &contract_id);
        let user = Address::generate(&env);
        let username = String::from_str(&env, "ebube");

        client.register_user(&user, &username);
        assert_eq!(client.get_address(&username), user);

        client.unregister_user(&user);
        let res = client.try_get_address(&username);
        assert!(res.is_err());
    }
}
