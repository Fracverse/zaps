#![no_std]
#![allow(dead_code, unused_variables, unused_imports, unexpected_cfgs)]
use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Map, String};

#[contract]
pub struct UserRegistryContract;

// Storage keys for persistent storage
const ADDRESS_TO_USERNAME: &str = "address_to_username";
const USERNAME_TO_ADDRESS: &str = "username_to_address";

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Avatar(Address),
}

#[contractimpl]
impl UserRegistryContract {
    /// Register a username mapping to the sender's address
    pub fn register_user(env: Env, user: Address, username: String) {
        user.require_auth();

        let len = username.len();
        if len < 3 || len > 15 {
            panic!("username must be between 3 and 15 characters");
        }
        
        let mut bytes = [0u8; 15];
        username.copy_into_slice(&mut bytes[..len as usize]);
        for i in 0..len as usize {
            let b = bytes[i];
            let is_lowercase = b >= b'a' && b <= b'z';
            let is_numeric = b >= b'0' && b <= b'9';
            if !is_lowercase && !is_numeric {
                panic!("username must be lowercase alphanumeric");
            }
        }

        // Convert storage keys to Soroban String
        let address_key = String::from_str(&env, ADDRESS_TO_USERNAME);
        let username_key = String::from_str(&env, USERNAME_TO_ADDRESS);

        // Get storage instances
        let address_to_username: Map<Address, String> = env
            .storage()
            .persistent()
            .get(&address_key)
            .unwrap_or(Map::new(&env));
        let username_to_address: Map<String, Address> = env
            .storage()
            .persistent()
            .get(&username_key)
            .unwrap_or(Map::new(&env));

        // Check if username is already taken (uniqueness validation)
        if username_to_address.contains_key(username.clone()) {
            panic!("username already taken");
        }

        // Store the mappings
        let mut address_to_username = address_to_username;
        let mut username_to_address = username_to_address;

        address_to_username.set(user.clone(), username.clone());
        username_to_address.set(username.clone(), user.clone());

        // Persist to storage
        env.storage()
            .persistent()
            .set(&address_key, &address_to_username);
        env.storage()
            .persistent()
            .set(&username_key, &username_to_address);

        env.events().publish(
            (soroban_sdk::symbol_short!("reg_user"),),
            (user, username),
        );
    }

    /// Retrieve the Address associated with a username
    pub fn get_address(env: Env, username: String) -> Address {
        let username_key = String::from_str(&env, USERNAME_TO_ADDRESS);
        let username_to_address: Map<String, Address> = env
            .storage()
            .persistent()
            .get(&username_key)
            .unwrap_or(Map::new(&env));

        username_to_address
            .get(username)
            .unwrap_or_else(|| panic!("username not found"))
    }

    /// Retrieve the username associated with an Address
    pub fn get_username(env: Env, user: Address) -> String {
        let address_key = String::from_str(&env, ADDRESS_TO_USERNAME);
        let address_to_username: Map<Address, String> = env
            .storage()
            .persistent()
            .get(&address_key)
            .unwrap_or(Map::new(&env));

        address_to_username
            .get(user)
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

    /// Unregister a user's profile and mapping
    pub fn unregister_user(env: Env, user: Address) {
        user.require_auth();

        let address_key = String::from_str(&env, ADDRESS_TO_USERNAME);
        let username_key = String::from_str(&env, USERNAME_TO_ADDRESS);

        let mut address_to_username: Map<Address, String> = env
            .storage()
            .persistent()
            .get(&address_key)
            .unwrap_or(Map::new(&env));

        let username = address_to_username
            .get(user.clone())
            .unwrap_or_else(|| panic!("address not registered"));

        let mut username_to_address: Map<String, Address> = env
            .storage()
            .persistent()
            .get(&username_key)
            .unwrap_or(Map::new(&env));

        address_to_username.remove(user.clone());
        username_to_address.remove(username);

        env.storage()
            .persistent()
            .set(&address_key, &address_to_username);
        env.storage()
            .persistent()
            .set(&username_key, &username_to_address);
            
        env.storage()
            .persistent()
            .remove(&DataKey::Avatar(user));
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
