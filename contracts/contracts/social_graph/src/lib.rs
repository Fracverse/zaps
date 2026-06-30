#![no_std]
#![allow(unexpected_cfgs)]
use soroban_sdk::{contract, contracterror, contractimpl, contracttype, Address, Env};

#[contracttype]
#[derive(Clone)]
enum DataKey {
    Friendship(Address, Address),
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FriendshipStatus {
    /// A one-sided request from the requester to the recipient, awaiting the
    /// recipient's approval.
    Pending,
    /// Both parties have approved the relationship.
    Active,
    /// A previously established friendship that has since been removed.
    Removed,
}

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    /// A user cannot send a friend request to themselves.
    CannotFriendSelf = 1,
    /// The two parties are already mutual friends.
    AlreadyFriends = 2,
    /// An identical request from this user to this friend is already pending.
    RequestAlreadyPending = 3,
    /// The other party already requested us; that request should be accepted
    /// instead of opening a competing one.
    IncomingRequestExists = 4,
    /// There is no pending request between these parties to act on.
    NoPendingRequest = 5,
}

#[contract]
pub struct SocialGraphContract;

#[contractimpl]
impl SocialGraphContract {
    /// Request a friendship with `friend` on-chain.
    ///
    /// This records a one-sided, pending request from `user` to `friend`. The
    /// relationship only becomes mutual once `friend` calls [`accept_friend`].
    ///
    /// Each request is stored independently under the composite key
    /// `(user, friend)`, avoiding loading or rewriting a per-user vector of
    /// addresses as the user's social graph grows.
    ///
    /// [`accept_friend`]: Self::accept_friend
    pub fn request_friend(env: Env, user: Address, friend: Address) -> Result<(), Error> {
        user.require_auth();

        if user == friend {
            return Err(Error::CannotFriendSelf);
        }

        match Self::status(&env, &user, &friend) {
            Some(FriendshipStatus::Active) => return Err(Error::AlreadyFriends),
            Some(FriendshipStatus::Pending) => return Err(Error::RequestAlreadyPending),
            _ => {}
        }

        // If the other party already requested us, accepting their request is
        // the correct action rather than opening a second, opposing request.
        if Self::status(&env, &friend, &user) == Some(FriendshipStatus::Pending) {
            return Err(Error::IncomingRequestExists);
        }

        env.storage().persistent().set(
            &DataKey::Friendship(user, friend),
            &FriendshipStatus::Pending,
        );
        Ok(())
    }

    /// Accept a pending friend request previously sent by `requester`.
    ///
    /// Must be called by `friend`, the recipient of the request. Once accepted,
    /// both directions of the relationship are marked [`Active`], making the
    /// friendship mutual.
    ///
    /// [`Active`]: FriendshipStatus::Active
    pub fn accept_friend(env: Env, friend: Address, requester: Address) -> Result<(), Error> {
        friend.require_auth();

        if Self::status(&env, &requester, &friend) != Some(FriendshipStatus::Pending) {
            return Err(Error::NoPendingRequest);
        }

        env.storage().persistent().set(
            &DataKey::Friendship(requester.clone(), friend.clone()),
            &FriendshipStatus::Active,
        );
        env.storage().persistent().set(
            &DataKey::Friendship(friend, requester),
            &FriendshipStatus::Active,
        );
        Ok(())
    }

    /// Reject a pending friend request previously sent by `requester`.
    ///
    /// Must be called by `friend`, the recipient of the request. The pending
    /// request is cleared, leaving no relationship between the two parties.
    pub fn reject_friend(env: Env, friend: Address, requester: Address) -> Result<(), Error> {
        friend.require_auth();

        if Self::status(&env, &requester, &friend) != Some(FriendshipStatus::Pending) {
            return Err(Error::NoPendingRequest);
        }

        env.storage()
            .persistent()
            .remove(&DataKey::Friendship(requester, friend));
        Ok(())
    }

    /// Remove an established friendship on-chain.
    ///
    /// Either party may unfriend the other; both directions of the relationship
    /// are marked [`Removed`].
    ///
    /// [`Removed`]: FriendshipStatus::Removed
    pub fn remove_friend(env: Env, user: Address, friend: Address) {
        user.require_auth();

        env.storage().persistent().set(
            &DataKey::Friendship(user.clone(), friend.clone()),
            &FriendshipStatus::Removed,
        );
        env.storage().persistent().set(
            &DataKey::Friendship(friend, user),
            &FriendshipStatus::Removed,
        );
    }

    /// Check if two addresses are mutual friends on-chain.
    ///
    /// Returns `true` only when both directions of the relationship are
    /// [`Active`], i.e. both parties approved it.
    ///
    /// [`Active`]: FriendshipStatus::Active
    pub fn is_friend(env: Env, user: Address, friend: Address) -> bool {
        Self::status(&env, &user, &friend) == Some(FriendshipStatus::Active)
            && Self::status(&env, &friend, &user) == Some(FriendshipStatus::Active)
    }

    fn status(env: &Env, user: &Address, friend: &Address) -> Option<FriendshipStatus> {
        env.storage()
            .persistent()
            .get(&DataKey::Friendship(user.clone(), friend.clone()))
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
    fn request_alone_does_not_create_friendship() {
        let (_env, client, user, friend, _other) = setup();

        client.request_friend(&user, &friend);

        // A one-sided request must not be treated as a friendship in either
        // direction until the recipient accepts.
        assert!(!client.is_friend(&user, &friend));
        assert!(!client.is_friend(&friend, &user));
    }

    #[test]
    fn accept_makes_friendship_mutual() {
        let (_env, client, user, friend, other) = setup();

        client.request_friend(&user, &friend);
        client.accept_friend(&friend, &user);

        assert!(client.is_friend(&user, &friend));
        assert!(client.is_friend(&friend, &user));
        // Unrelated parties remain unaffected.
        assert!(!client.is_friend(&user, &other));
    }

    #[test]
    fn reject_clears_pending_request() {
        let (_env, client, user, friend, _other) = setup();

        client.request_friend(&user, &friend);
        client.reject_friend(&friend, &user);

        assert!(!client.is_friend(&user, &friend));
        assert!(!client.is_friend(&friend, &user));

        // After a rejection the request slot is free, so a fresh request and
        // accept can still establish the friendship.
        client.request_friend(&user, &friend);
        client.accept_friend(&friend, &user);
        assert!(client.is_friend(&user, &friend));
    }

    #[test]
    fn remove_friend_unfriends_both_directions() {
        let (_env, client, user, friend, _other) = setup();

        client.request_friend(&user, &friend);
        client.accept_friend(&friend, &user);
        assert!(client.is_friend(&user, &friend));

        client.remove_friend(&user, &friend);
        assert!(!client.is_friend(&user, &friend));
        assert!(!client.is_friend(&friend, &user));
    }

    #[test]
    fn cannot_accept_without_request() {
        let (_env, client, user, friend, _other) = setup();

        assert_eq!(
            client.try_accept_friend(&friend, &user),
            Err(Ok(Error::NoPendingRequest))
        );
    }

    #[test]
    fn cannot_reject_without_request() {
        let (_env, client, user, friend, _other) = setup();

        assert_eq!(
            client.try_reject_friend(&friend, &user),
            Err(Ok(Error::NoPendingRequest))
        );
    }

    #[test]
    fn cannot_request_twice() {
        let (_env, client, user, friend, _other) = setup();

        client.request_friend(&user, &friend);
        assert_eq!(
            client.try_request_friend(&user, &friend),
            Err(Ok(Error::RequestAlreadyPending))
        );
    }

    #[test]
    fn cannot_open_opposing_request() {
        let (_env, client, user, friend, _other) = setup();

        client.request_friend(&user, &friend);
        // friend should accept, not open their own competing request.
        assert_eq!(
            client.try_request_friend(&friend, &user),
            Err(Ok(Error::IncomingRequestExists))
        );
    }

    #[test]
    fn cannot_request_existing_friend() {
        let (_env, client, user, friend, _other) = setup();

        client.request_friend(&user, &friend);
        client.accept_friend(&friend, &user);
        assert_eq!(
            client.try_request_friend(&user, &friend),
            Err(Ok(Error::AlreadyFriends))
        );
    }

    #[test]
    fn cannot_friend_yourself() {
        let (_env, client, user, _friend, _other) = setup();

        assert_eq!(
            client.try_request_friend(&user, &user),
            Err(Ok(Error::CannotFriendSelf))
        );
    }
}
