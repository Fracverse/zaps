#![no_std]
#![allow(unexpected_cfgs)]
use soroban_sdk::{contract, contractimpl, contracttype, symbol_short, Address, Env, Symbol, Vec};

const FRIENDS_KEY: fn(Address) -> DataKey = DataKey::Friends;

#[contracttype]
enum DataKey {
    Friends(Address),
}

#[contract]
pub struct SocialGraphContract;

#[contractimpl]
impl SocialGraphContract {
    /// Add a friend relationship on-chain (stored as persistent vec per user)
    pub fn add_friend(env: Env, user: Address, friend: Address) {
        user.require_auth();
        assert!(user != friend, "cannot friend yourself");
        let key = DataKey::Friends(user.clone());
        let mut friends: Vec<Address> = env.storage().persistent().get(&key).unwrap_or(Vec::new(&env));
        if !friends.contains(&friend) {
            friends.push_back(friend);
            env.storage().persistent().set(&key, &friends);
        }
    }

    /// Remove a friend relationship on-chain
    pub fn remove_friend(env: Env, user: Address, friend: Address) {
        user.require_auth();
        let key = DataKey::Friends(user.clone());
        let friends: Vec<Address> = env.storage().persistent().get(&key).unwrap_or(Vec::new(&env));
        let mut updated: Vec<Address> = Vec::new(&env);
        for f in friends.iter() {
            if f != friend {
                updated.push_back(f);
            }
        }
        env.storage().persistent().set(&key, &updated);
    }

    /// Check if two addresses are friends on-chain
    pub fn is_friend(env: Env, user: Address, friend: Address) -> bool {
        let key = DataKey::Friends(user);
        let friends: Vec<Address> = env.storage().persistent().get(&key).unwrap_or(Vec::new(&env));
        friends.contains(&friend)
    }

    /// Get the friends list for a user
    pub fn get_friends(env: Env, user: Address) -> Vec<Address> {
        let key = DataKey::Friends(user);
        env.storage().persistent().get(&key).unwrap_or(Vec::new(&env))
    }
}
