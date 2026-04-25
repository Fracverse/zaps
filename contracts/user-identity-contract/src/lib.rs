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

/// User data structure containing address, username, role, and profile
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct User {
    pub address: Address,
    pub username: String,
    pub role: String,
    pub profile_uri: String,
    pub reputation_score: u32,
}

/// Storage keys for the contract
#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    User(Address),
    UsernameToAddress(String),
}

/// User Identity Contract
/// Maps wallet addresses to roles for identity management
#[contract]
pub struct UserIdentityContract;

#[contractimpl]
impl UserIdentityContract {
    /// Register a new user with an address, username, and role
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `address` - The wallet address to register
    /// * `username` - Unique username for the user
    /// * `role` - The role to assign to the user
    /// * `profile_uri` - URI to the user's profile metadata
    ///
    /// # Errors
    /// * `Error::AlreadyRegistered` - If the address is already registered
    ///
    /// # Authentication
    /// Requires the address to authenticate (sign) the transaction
    pub fn register(
        env: Env,
        address: Address,
        username: String,
        role: String,
        profile_uri: String,
    ) -> Result<(), Error> {
        // Require authentication from the address being registered
        address.require_auth();

        let key = DataKey::User(address.clone());
        let username_key = DataKey::UsernameToAddress(username.clone());

        // Check if address is already registered
        if env.storage().persistent().has(&key) {
            return Err(Error::AlreadyRegistered);
        }

        // Check if username is already taken
        if env.storage().persistent().has(&username_key) {
            return Err(Error::AlreadyRegistered);
        }

        // Create and store the user
        let user = User {
            address: address.clone(),
            username: username.clone(),
            role,
            profile_uri,
            reputation_score: 0,
        };

        // Store in persistent storage with TTL
        env.storage().persistent().set(&key, &user);
        env.storage().persistent().set(&username_key, &address);

        // Extend TTL for the stored data (30 days worth of ledgers, ~5 second ledgers)
        env.storage().persistent().extend_ttl(&key, 518400, 518400);
        env.storage().persistent().extend_ttl(&username_key, 518400, 518400);

        // Emit event for user registration
        env.events().publish(("register", "user"), (&address, &username));

        Ok(())
    }

    /// Update user's profile metadata
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `address` - The wallet address to update
    /// * `profile_uri` - New URI to the user's profile metadata
    ///
    /// # Errors
    /// * `Error::UserNotFound` - If the address is not registered
    ///
    /// # Authentication
    /// Requires the address to authenticate
    pub fn update_profile(env: Env, address: Address, profile_uri: String) -> Result<(), Error> {
        address.require_auth();

        let key = DataKey::User(address.clone());
        let mut user: User = env.storage().persistent().get(&key)
            .ok_or(Error::UserNotFound)?;

        user.profile_uri = profile_uri;
        env.storage().persistent().set(&key, &user);

        env.events().publish(("update", "profile"), &address);

        Ok(())
    }

    /// Update user's reputation score
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `address` - The wallet address to update
    /// * `score` - New reputation score
    ///
    /// # Errors
    /// * `Error::UserNotFound` - If the address is not registered
    ///
    /// # Authentication
    /// Requires the address to authenticate
    pub fn update_reputation(env: Env, address: Address, score: u32) -> Result<(), Error> {
        address.require_auth();

        let key = DataKey::User(address.clone());
        let mut user: User = env.storage().persistent().get(&key)
            .ok_or(Error::UserNotFound)?;

        user.reputation_score = score;
        env.storage().persistent().set(&key, &user);

        env.events().publish(("update", "reputation"), (&address, &score));

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

    /// Get address by username
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `username` - The username to look up
    ///
    /// # Returns
    /// Returns the Address if found
    ///
    /// # Errors
    /// * `Error::UserNotFound` - If the username is not registered
    pub fn get_address_by_username(env: Env, username: String) -> Result<Address, Error> {
        let key = DataKey::UsernameToAddress(username);
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

    /// Check if a username is taken
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `username` - The username to check
    ///
    /// # Returns
    /// Returns true if the username is taken, false otherwise
    pub fn is_username_taken(env: Env, username: String) -> bool {
        let key = DataKey::UsernameToAddress(username);
        env.storage().persistent().has(&key)
    }
}

mod test;
