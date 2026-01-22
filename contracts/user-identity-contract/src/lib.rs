#![no_std]

use soroban_sdk::{contract, contracterror, contractimpl, contracttype, Address, Env, String};

/// Error codes for the User Identity Contract
#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    /// User is already registered
    AlreadyRegistered = 1,
    /// User not found
    UserNotFound = 2,
}

/// User data structure containing address and role
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct User {
    pub address: Address,
    pub role: String,
}

/// Storage keys for the contract
#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    User(Address),
}

/// User Identity Contract
/// Maps wallet addresses to roles for identity management
#[contract]
pub struct UserIdentityContract;

#[contractimpl]
impl UserIdentityContract {
    /// Register a new user with an address and role
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `address` - The wallet address to register
    /// * `role` - The role to assign to the user
    ///
    /// # Errors
    /// * `Error::AlreadyRegistered` - If the address is already registered
    ///
    /// # Authentication
    /// Requires the address to authenticate (sign) the transaction
    pub fn register(env: Env, address: Address, role: String) -> Result<(), Error> {
        // Require authentication from the address being registered
        address.require_auth();

        let key = DataKey::User(address.clone());

        // Check if user is already registered
        if env.storage().persistent().has(&key) {
            return Err(Error::AlreadyRegistered);
        }

        // Create and store the user
        let user = User {
            address: address.clone(),
            role,
        };

        // Store in persistent storage with TTL
        env.storage().persistent().set(&key, &user);

        // Extend TTL for the stored data (30 days worth of ledgers, ~5 second ledgers)
        env.storage().persistent().extend_ttl(&key, 518400, 518400);

        // Emit event for user registration
        env.events().publish(("register", "user"), address);

        Ok(())
    }

    /// Get user information for a given address
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `address` - The wallet address to query
    ///
    /// # Returns
    /// Returns the User struct if found
    ///
    /// # Errors
    /// * `Error::UserNotFound` - If the address is not registered
    pub fn get_user(env: Env, address: Address) -> Result<User, Error> {
        let key = DataKey::User(address);

        env.storage()
            .persistent()
            .get(&key)
            .ok_or(Error::UserNotFound)
    }

    /// Check if an address is registered
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `address` - The wallet address to check
    ///
    /// # Returns
    /// Returns true if the address is registered, false otherwise
    pub fn is_registered(env: Env, address: Address) -> bool {
        let key = DataKey::User(address);
        env.storage().persistent().has(&key)
    }
}

mod test;
