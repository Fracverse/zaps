#![no_std]
#![allow(dead_code, unused_variables, unused_imports, unexpected_cfgs)]
use soroban_sdk::{contract, contractimpl, Address, Env, String, Map};

#[contract]
pub struct UserRegistryContract;

// Storage keys for persistent storage
const ADDRESS_TO_USERNAME: &str = "address_to_username";
const USERNAME_TO_ADDRESS: &str = "username_to_address";

#[contractimpl]
impl UserRegistryContract {
    /// Register a username mapping to the sender's address
    pub fn register_user(env: Env, user: Address, username: String) {
        // TODO: Implement SC-002 (Validate username rules: length 3-15, alphanumeric, lowercase)
        user.require_auth();

        // Convert storage keys to Soroban String
        let address_key = String::from_str(&env, ADDRESS_TO_USERNAME);
        let username_key = String::from_str(&env, USERNAME_TO_ADDRESS);

        // Get storage instances
        let address_to_username: Map<Address, String> = env.storage().persistent().get(&address_key).unwrap_or(Map::new(&env));
        let username_to_address: Map<String, Address> = env.storage().persistent().get(&username_key).unwrap_or(Map::new(&env));

        // Check if username is already taken (uniqueness validation)
        if username_to_address.contains_key(username.clone()) {
            panic!("username already taken");
        }

        // Store the mappings
        let mut address_to_username = address_to_username;
        let mut username_to_address = username_to_address;
        
        address_to_username.set(user.clone(), username.clone());
        username_to_address.set(username, user);

        // Persist to storage
        env.storage().persistent().set(&address_key, &address_to_username);
        env.storage().persistent().set(&username_key, &username_to_address);
    }

    /// Retrieve the Address associated with a username
    pub fn get_address(env: Env, username: String) -> Address {
        let username_key = String::from_str(&env, USERNAME_TO_ADDRESS);
        let username_to_address: Map<String, Address> = env.storage().persistent().get(&username_key)
            .unwrap_or(Map::new(&env));
        
        username_to_address.get(username)
            .unwrap_or_else(|| panic!("username not found"))
    }

    /// Retrieve the username associated with an Address
    pub fn get_username(env: Env, user: Address) -> String {
        let address_key = String::from_str(&env, ADDRESS_TO_USERNAME);
        let address_to_username: Map<Address, String> = env.storage().persistent().get(&address_key)
            .unwrap_or(Map::new(&env));
        
        address_to_username.get(user)
            .unwrap_or_else(|| panic!("address not registered"))
    }

    /// Update user profile metadata (e.g. avatar URI)
    pub fn update_profile(env: Env, user: Address, avatar_uri: String) {
        // TODO: Implement SC-003 (Update profile avatar URI)
        user.require_auth();
        panic!("unimplemented: update_profile");
    }
}
