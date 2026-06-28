#![no_std]
#![allow(unexpected_cfgs)]
use soroban_sdk::{contract, contractimpl, contracttype, Address, Env};

#[contracttype]
#[derive(Clone)]
enum DataKey {
    Friendship(Address, Address),
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FriendshipStatus {
    Active,
    Removed,
}

#[contract]
pub struct SocialGraphContract;

#[contractimpl]
impl SocialGraphContract {
    /// Add a directed friend relationship on-chain.
    ///
    /// Each friendship is stored independently under the composite key
    /// `(user, friend)`, avoiding loading or rewriting a per-user vector of
    /// addresses as the user's social graph grows.
    pub fn add_friend(env: Env, user: Address, friend: Address) {
        user.require_auth();
        assert!(user != friend, "cannot friend yourself");

        let key = DataKey::Friendship(user, friend);
        env.storage()
            .persistent()
            .set(&key, &FriendshipStatus::Active);
    }

    /// Remove a directed friend relationship on-chain.
    pub fn remove_friend(env: Env, user: Address, friend: Address) {
        user.require_auth();

        let key = DataKey::Friendship(user, friend);
        env.storage()
            .persistent()
            .set(&key, &FriendshipStatus::Removed);
    }

    /// Check if two addresses are friends on-chain.
    pub fn is_friend(env: Env, user: Address, friend: Address) -> bool {
        let key = DataKey::Friendship(user, friend);
        let status: Option<FriendshipStatus> = env.storage().persistent().get(&key);

        status == Some(FriendshipStatus::Active)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;

    fn setup() -> (
        Env,
        SocialGraphContractClient<'static>,
        Address,
        Address,
        Address,
    ) {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, SocialGraphContract);
        let client = SocialGraphContractClient::new(&env, &contract_id);
        let user = Address::generate(&env);
        let friend = Address::generate(&env);
        let other = Address::generate(&env);

        (env, client, user, friend, other)
    }

    #[test]
    fn add_friend_stores_independent_composite_friendship() {
        let (_env, client, user, friend, other) = setup();

        assert!(!client.is_friend(&user, &friend));
        assert!(!client.is_friend(&user, &other));

        client.add_friend(&user, &friend);

        assert!(client.is_friend(&user, &friend));
        assert!(!client.is_friend(&user, &other));
        assert!(!client.is_friend(&friend, &user));
    }

    #[test]
    fn remove_friend_marks_composite_friendship_removed() {
        let (_env, client, user, friend, _other) = setup();

        client.add_friend(&user, &friend);
        assert!(client.is_friend(&user, &friend));

        client.remove_friend(&user, &friend);
        assert!(!client.is_friend(&user, &friend));
    }

}
