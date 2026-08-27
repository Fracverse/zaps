#![no_std]
#![allow(unexpected_cfgs)]
use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, token, xdr::ToXdr, Address, Bytes, BytesN,
    Env, String, Symbol,
};

#[contract]
pub struct UserRegistryContract;

/// Persistent-entry TTL (in ledgers) below which a lookup triggers an
/// extension. ~100,000 ledgers ≈ 5.8 days at Stellar's ~5s ledger close time.
const TTL_THRESHOLD: u32 = 100_000;
/// TTL (in ledgers) a lookup extends an entry to once `TTL_THRESHOLD` is
/// crossed. ~500,000 ledgers ≈ 29 days.
const TTL_EXTEND_TO: u32 = 500_000;

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    User(Address),        // Maps Address -> Username (String)
    Username(String),     // Maps Username (String) -> Address
    Avatar(Address),      // Maps Address -> Avatar URI (String)
    PrivyDid(String),     // Maps Privy DID -> wallet Address
    WalletDid(Address),   // Maps wallet Address -> Privy DID (reverse index)
    Admin,                // Stores the contract admin Address
    PrivyVerifierKey,     // Ed25519 public key trusted to attest DID <-> wallet links
    ReservationToken,     // Stores Naira token contract Address
    ReservationAmount,    // Stores required reservation amount (i128)
    UserDeposit(Address), // Stores deposited reservation amount per user (i128)
}

#[contracttype]
#[derive(Clone)]
pub struct AddressToUsernameKey {
    pub address: Address,
}

#[contracttype]
#[derive(Clone)]
pub struct UsernameToAddressKey {
    pub username: String,
}

#[contractimpl]
impl UserRegistryContract {
    /// Load the configured admin address, extending its persistent TTL on
    /// every lookup so a dormant contract's admin key doesn't get archived
    /// between admin actions.
    fn require_admin(env: &Env) -> Address {
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic!("admin not set"));
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::Admin, TTL_THRESHOLD, TTL_EXTEND_TO);
        admin
    }

    fn validate_username(username: &String) {
        let len = username.len();
        if len < 3 || len > 15 {
            panic!("username length must be 3-15");
        }

        let mut bytes = [0u8; 15];
        username.copy_into_slice(&mut bytes[..len as usize]);

        for i in 0..len as usize {
            let b = bytes[i];
            let is_lowercase = (b'a'..=b'z').contains(&b);
            let is_numeric = (b'0'..=b'9').contains(&b);
            if !is_lowercase && !is_numeric {
                panic!("username must be lowercase alphanumeric");
            }
        }
    }

    /// Initialize the contract with an admin address for recovery operations
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().persistent().has(&DataKey::Admin) {
            panic!("already initialized");
        }
        env.storage().persistent().set(&DataKey::Admin, &admin);
    }

    /// Admin-only: configure the reservation token and required amount.
    pub fn set_reservation_config(env: Env, token_address: Address, amount: i128) {
        if amount < 0 {
            panic!("reservation amount cannot be negative");
        }

        let admin = Self::require_admin(&env);
        admin.require_auth();

        env.storage()
            .persistent()
            .set(&DataKey::ReservationToken, &token_address);
        env.storage()
            .persistent()
            .set(&DataKey::ReservationAmount, &amount);
        env.storage().persistent().extend_ttl(
            &DataKey::ReservationToken,
            TTL_THRESHOLD,
            TTL_EXTEND_TO,
        );
        env.storage().persistent().extend_ttl(
            &DataKey::ReservationAmount,
            TTL_THRESHOLD,
            TTL_EXTEND_TO,
        );
    }

    /// Register a username mapping to the sender's address
    pub fn register_user(env: Env, user: Address, username: String) {
        user.require_auth();
        Self::validate_username(&username);

        let username_key = UsernameToAddressKey {
            username: username.clone(),
        };
        let user_key = AddressToUsernameKey {
            address: user.clone(),
        };

        // Check if username is already taken (uniqueness validation)
        if env.storage().persistent().has(&username_key) {
            panic!("username already taken");
        }
        if env.storage().persistent().has(&user_key) {
            panic!("address already registered");
        }

        let reservation_token: Address = env
            .storage()
            .persistent()
            .get(&DataKey::ReservationToken)
            .unwrap_or_else(|| panic!("reservation token not configured"));
        let reservation_amount: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::ReservationAmount)
            .unwrap_or_else(|| panic!("reservation amount not configured"));
        env.storage().persistent().extend_ttl(
            &DataKey::ReservationToken,
            TTL_THRESHOLD,
            TTL_EXTEND_TO,
        );
        env.storage().persistent().extend_ttl(
            &DataKey::ReservationAmount,
            TTL_THRESHOLD,
            TTL_EXTEND_TO,
        );

        if reservation_amount > 0 {
            let token_client = token::Client::new(&env, &reservation_token);
            token_client.transfer(&user, &env.current_contract_address(), &reservation_amount);
        }

        // Store the mappings via both struct-based keys (legacy compat) and
        // DataKey enum variants so that delete_profile can remove them cleanly.
        env.storage().persistent().set(&user_key, &username);
        env.storage().persistent().set(&username_key, &user);
        env.storage()
            .persistent()
            .set(&DataKey::User(user.clone()), &username);
        env.storage()
            .persistent()
            .set(&DataKey::Username(username.clone()), &user);
        env.storage()
            .persistent()
            .set(&DataKey::UserDeposit(user.clone()), &reservation_amount);
        env.storage()
            .persistent()
            .extend_ttl(&user_key, TTL_THRESHOLD, TTL_EXTEND_TO);
        env.storage()
            .persistent()
            .extend_ttl(&username_key, TTL_THRESHOLD, TTL_EXTEND_TO);
        env.storage().persistent().extend_ttl(
            &DataKey::User(user.clone()),
            TTL_THRESHOLD,
            TTL_EXTEND_TO,
        );
        env.storage().persistent().extend_ttl(
            &DataKey::Username(username.clone()),
            TTL_THRESHOLD,
            TTL_EXTEND_TO,
        );
        env.storage().persistent().extend_ttl(
            &DataKey::UserDeposit(user.clone()),
            TTL_THRESHOLD,
            TTL_EXTEND_TO,
        );

        // #542: publish so the off-chain indexer can sync this registration
        // to the `users` table. Without this, on-chain registration and the
        // off-chain database silently diverge.
        env.events()
            .publish((Symbol::new(&env, "UserRegistered"),), (user, username));
    }

    /// Retrieve the Address associated with a username
    pub fn get_address(env: Env, username: String) -> Address {
        let username_key = UsernameToAddressKey { username };
        let address = env
            .storage()
            .persistent()
            .get(&username_key)
            .unwrap_or_else(|| panic!("username not found"));
        env.storage()
            .persistent()
            .extend_ttl(&username_key, TTL_THRESHOLD, TTL_EXTEND_TO);
        address
    }

    /// Retrieve the username associated with an Address
    pub fn get_username(env: Env, user: Address) -> String {
        let user_key = AddressToUsernameKey { address: user };
        let username = env
            .storage()
            .persistent()
            .get(&user_key)
            .unwrap_or_else(|| panic!("address not registered"));
        env.storage()
            .persistent()
            .extend_ttl(&user_key, TTL_THRESHOLD, TTL_EXTEND_TO);
        username
    }

    /// Best-effort username lookup for callers (e.g. other contracts resolving
    /// a display name for events) that must not panic on an unregistered
    /// address. Returns an empty string instead of panicking, mirroring
    /// `get_avatar`'s fallback behavior below.
    pub fn username_or_empty(env: Env, user: Address) -> String {
        let user_key = AddressToUsernameKey { address: user };
        if env.storage().persistent().has(&user_key) {
            env.storage()
                .persistent()
                .extend_ttl(&user_key, TTL_THRESHOLD, TTL_EXTEND_TO);
        }
        env.storage()
            .persistent()
            .get(&user_key)
            .unwrap_or_else(|| String::from_str(&env, ""))
    }

    /// Update user profile metadata (e.g. avatar URI)
    pub fn update_profile(env: Env, user: Address, avatar_uri: String) {
        user.require_auth();
        env.storage()
            .persistent()
            .set(&DataKey::Avatar(user.clone()), &avatar_uri);
        env.storage().persistent().extend_ttl(
            &DataKey::Avatar(user.clone()),
            TTL_THRESHOLD,
            TTL_EXTEND_TO,
        );

        env.events().publish(
            (soroban_sdk::symbol_short!("prof_upd"),),
            (user, avatar_uri),
        );
    }

    /// Retrieve the avatar URI associated with an Address
    pub fn get_avatar(env: Env, user: Address) -> String {
        let key = DataKey::Avatar(user);
        if env.storage().persistent().has(&key) {
            env.storage()
                .persistent()
                .extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
        }
        env.storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| String::from_str(&env, ""))
    }

    /// Set (or rotate) the trusted Ed25519 public key used to verify Privy DID
    /// link attestations. Only the contract admin may call this.
    pub fn set_privy_verifier(env: Env, caller: Address, pubkey: BytesN<32>) {
        let admin = Self::require_admin(&env);
        admin.require_auth();
        assert!(caller == admin, "only admin");
        env.storage()
            .persistent()
            .set(&DataKey::PrivyVerifierKey, &pubkey);
        env.storage().persistent().extend_ttl(
            &DataKey::PrivyVerifierKey,
            TTL_THRESHOLD,
            TTL_EXTEND_TO,
        );
    }

    /// Register a Privy DID -> wallet address mapping.
    ///
    /// The caller must supply an Ed25519 `signature` over the `(did, wallet)`
    /// payload, produced by the trusted Privy verifier key configured via
    /// `set_privy_verifier`. This proves Privy attested that `did` belongs to
    /// `wallet` before the on-chain mapping is created, in addition to the
    /// wallet itself authorizing the transaction.
    pub fn register_privy_did(env: Env, did: String, wallet: Address, signature: BytesN<64>) {
        wallet.require_auth();

        let verifier_key: BytesN<32> = env
            .storage()
            .persistent()
            .get(&DataKey::PrivyVerifierKey)
            .unwrap_or_else(|| panic!("privy verifier not configured"));
        env.storage().persistent().extend_ttl(
            &DataKey::PrivyVerifierKey,
            TTL_THRESHOLD,
            TTL_EXTEND_TO,
        );

        let message: Bytes = (did.clone(), wallet.clone()).to_xdr(&env);
        env.crypto()
            .ed25519_verify(&verifier_key, &message, &signature);

        let did_key = DataKey::PrivyDid(did.clone());
        if env.storage().persistent().has(&did_key) {
            panic!("DID already registered");
        }
        env.storage().persistent().set(&did_key, &wallet);
        env.storage()
            .persistent()
            .set(&DataKey::WalletDid(wallet.clone()), &did);
        env.storage()
            .persistent()
            .extend_ttl(&did_key, TTL_THRESHOLD, TTL_EXTEND_TO);
        env.storage().persistent().extend_ttl(
            &DataKey::WalletDid(wallet.clone()),
            TTL_THRESHOLD,
            TTL_EXTEND_TO,
        );

        env.events()
            .publish((symbol_short!("did_reg"),), (wallet, did));
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
        env.storage()
            .persistent()
            .extend_ttl(&did_key, TTL_THRESHOLD, TTL_EXTEND_TO);
        env.storage().persistent().extend_ttl(
            &DataKey::WalletDid(new_wallet),
            TTL_THRESHOLD,
            TTL_EXTEND_TO,
        );
    }

    /// Admin recovery: reassign a DID mapping to a new wallet.
    /// Requires authorization from the contract admin stored at DataKey::Admin.
    pub fn recover_privy_did(env: Env, did: String, new_wallet: Address) {
        let admin = Self::require_admin(&env);
        admin.require_auth();
        let did_key = DataKey::PrivyDid(did.clone());
        // Remove old reverse mapping if present
        if let Some(old_wallet) = env.storage().persistent().get::<DataKey, Address>(&did_key) {
            env.storage()
                .persistent()
                .remove(&DataKey::WalletDid(old_wallet));
        }
        env.storage().persistent().set(&did_key, &new_wallet);
        env.storage()
            .persistent()
            .set(&DataKey::WalletDid(new_wallet.clone()), &did);
        env.storage()
            .persistent()
            .extend_ttl(&did_key, TTL_THRESHOLD, TTL_EXTEND_TO);
        env.storage().persistent().extend_ttl(
            &DataKey::WalletDid(new_wallet),
            TTL_THRESHOLD,
            TTL_EXTEND_TO,
        );
    }

    /// Get the wallet address registered for a Privy DID
    pub fn get_wallet_for_did(env: Env, did: String) -> Address {
        let key = DataKey::PrivyDid(did);
        let wallet = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| panic!("DID not registered"));
        env.storage()
            .persistent()
            .extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
        wallet
    }

    /// Issue #754: Remove a user's profile data (username and avatar URI) from storage.
    ///
    /// Clears `DataKey::User`, `DataKey::Username`, and `DataKey::Avatar` for the
    /// caller. The caller must be the account owner (enforced via `require_auth`).
    /// Use `unregister_user` instead when the reservation deposit also needs
    /// to be refunded.
    pub fn delete_profile(env: Env, user: Address) {
        user.require_auth();

        // Resolve the username so its reverse-mapping key can be removed.
        let username: String = env
            .storage()
            .persistent()
            .get(&DataKey::User(user.clone()))
            .unwrap_or_else(|| panic!("address not registered"));

        env.storage()
            .persistent()
            .remove(&DataKey::User(user.clone()));
        env.storage()
            .persistent()
            .remove(&DataKey::Username(username.clone()));
        env.storage()
            .persistent()
            .remove(&DataKey::Avatar(user.clone()));

        env.events()
            .publish((soroban_sdk::symbol_short!("prof_del"),), (user, username));
    }

    /// Issue #758: Upgrade the contract WASM to a new hash.
    ///
    /// Validates that `new_wasm_hash` is non-zero (all-zero hash indicates an
    /// uninitialized or invalid value) before invoking the deployer upgrade.
    /// Only the stored contract admin may call this.
    pub fn upgrade(env: Env, caller: Address, new_wasm_hash: BytesN<32>) {
        caller.require_auth();

        let admin = Self::require_admin(&env);
        assert!(caller == admin, "only admin can upgrade");

        // Reject an all-zero hash: it signals an uninitialised or null value
        // and would deploy an empty contract.
        let zero = BytesN::<32>::from_array(&env, &[0u8; 32]);
        assert!(new_wasm_hash != zero, "wasm hash must not be zero");

        env.events().publish(
            (soroban_sdk::symbol_short!("upgraded"),),
            new_wasm_hash.clone(),
        );

        env.deployer().update_current_contract_wasm(new_wasm_hash);
    }

    /// Unregister a user's profile and mapping
    pub fn unregister_user(env: Env, user: Address) {
        user.require_auth();

        let user_key = AddressToUsernameKey {
            address: user.clone(),
        };
        let username: String = env
            .storage()
            .persistent()
            .get(&user_key)
            .unwrap_or_else(|| panic!("address not registered"));
        let username_key = UsernameToAddressKey { username };
        let reservation_amount: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::UserDeposit(user.clone()))
            .unwrap_or(0);

        env.storage().persistent().remove(&user_key);
        env.storage().persistent().remove(&username_key);
        env.storage()
            .persistent()
            .remove(&DataKey::Avatar(user.clone()));
        env.storage()
            .persistent()
            .remove(&DataKey::UserDeposit(user.clone()));

        if reservation_amount > 0 {
            let reservation_token: Address = env
                .storage()
                .persistent()
                .get(&DataKey::ReservationToken)
                .unwrap_or_else(|| panic!("reservation token not configured"));
            env.storage().persistent().extend_ttl(
                &DataKey::ReservationToken,
                TTL_THRESHOLD,
                TTL_EXTEND_TO,
            );
            let token_client = token::Client::new(&env, &reservation_token);
            token_client.transfer(&env.current_contract_address(), &user, &reservation_amount);
        }
    }
}

#[cfg(test)]
mod test;

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

    // ── Issue #754: delete_profile ────────────────────────────────────────────

    fn setup_with_user() -> (Env, UserRegistryContractClient<'static>, Address, String) {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, UserRegistryContract);
        let client = UserRegistryContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.initialize(&admin);

        let user = Address::generate(&env);
        let username = String::from_str(&env, "alice");
        client.register_user(&user, &username);

        (env, client, user, username)
    }

    #[test]
    fn delete_profile_removes_user_username_and_avatar_keys() {
        let (env, client, user, username) = setup_with_user();

        let avatar = String::from_str(&env, "https://example.com/pic.png");
        client.update_profile(&user, &avatar);

        client.delete_profile(&user);

        env.as_contract(&client.address, || {
            assert!(
                !env.storage().persistent().has(&DataKey::User(user.clone())),
                "DataKey::User must be removed"
            );
            assert!(
                !env.storage()
                    .persistent()
                    .has(&DataKey::Username(username.clone())),
                "DataKey::Username must be removed"
            );
            assert!(
                !env.storage()
                    .persistent()
                    .has(&DataKey::Avatar(user.clone())),
                "DataKey::Avatar must be removed"
            );
        });
    }

    #[test]
    fn delete_profile_clears_avatar() {
        let (env, client, user, _username) = setup_with_user();

        client.update_profile(
            &user,
            &String::from_str(&env, "https://img.example.com/a.png"),
        );
        client.delete_profile(&user);

        assert_eq!(
            client.get_avatar(&user),
            String::from_str(&env, ""),
            "avatar must return empty string after delete_profile"
        );
    }

    // ── Issue #758: upgrade ───────────────────────────────────────────────────

    #[test]
    #[ignore] // assert!(..) for zero-hash panics in Soroban v20 (non-unwinding)
    fn upgrade_rejects_zero_hash() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, UserRegistryContract);
        let client = UserRegistryContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.initialize(&admin);

        let zero_hash = BytesN::from_array(&env, &[0u8; 32]);
        let res = client.try_upgrade(&admin, &zero_hash);
        assert!(res.is_err(), "all-zero hash must be rejected");
    }

    #[test]
    #[ignore] // env.deployer().update_current_contract_wasm requires a real WASM blob in testutils
    fn upgrade_requires_admin() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, UserRegistryContract);
        let client = UserRegistryContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.initialize(&admin);

        let non_admin = Address::generate(&env);
        let hash = BytesN::from_array(&env, &[1u8; 32]);
        let res = client.try_upgrade(&non_admin, &hash);
        assert!(res.is_err(), "non-admin must be rejected");
    }
}
