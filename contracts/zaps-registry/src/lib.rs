#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, Address, Bytes, Env,
};

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    DuplicateId = 4,
    MerchantNotFound = 5,
    InactiveMerchant = 6,
    UserNotFound = 7,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MerchantMetadata {
    pub settlement_asset: Address,
    pub vault: Address,
    pub active: bool,
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    User(Bytes),
    Merchant(Bytes),
}

#[contract]
pub struct BLINKSRegistry;

#[contractimpl]
impl BLINKSRegistry {
    /// Initialize the contract with an admin address
    pub fn initialize(env: Env, admin: Address) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        Ok(())
    }

    /// Register a human-readable ID for a user address
    /// Authentication: Required for the user address being registered
    pub fn register_user(env: Env, user_id: Bytes, wallet: Address) -> Result<(), Error> {
        wallet.require_auth();

        let key = DataKey::User(user_id.clone());
        if env.storage().persistent().has(&key) {
            return Err(Error::DuplicateId);
        }

        env.storage().persistent().set(&key, &wallet);

        // Extend TTL to ~30 days
        env.storage().persistent().extend_ttl(&key, 518400, 518400);

        env.events()
            .publish((symbol_short!("user_reg"), user_id), wallet);

        Ok(())
    }

    /// Register a merchant with settlement metadata
    /// Access Control: Admin only
    pub fn register_merchant(
        env: Env,
        merchant_id: Bytes,
        vault: Address,
        asset: Address,
    ) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();

        let key = DataKey::Merchant(merchant_id.clone());
        if env.storage().persistent().has(&key) {
            return Err(Error::DuplicateId);
        }

        let metadata = MerchantMetadata {
            settlement_asset: asset,
            vault,
            active: true,
        };

        env.storage().persistent().set(&key, &metadata);

        // Extend TTL
        env.storage().persistent().extend_ttl(&key, 518400, 518400);

        env.events()
            .publish((symbol_short!("merch_reg"), merchant_id), metadata);

        Ok(())
    }

    /// Resolve a user ID to their wallet address
    pub fn resolve_user(env: Env, user_id: Bytes) -> Result<Address, Error> {
        let key = DataKey::User(user_id);
        env.storage()
            .persistent()
            .get(&key)
            .ok_or(Error::UserNotFound)
    }

    /// Resolve a merchant ID to their metadata
    pub fn resolve_merchant(env: Env, merchant_id: Bytes) -> Result<MerchantMetadata, Error> {
        let key = DataKey::Merchant(merchant_id);
        let metadata: MerchantMetadata = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(Error::MerchantNotFound)?;

        if !metadata.active {
            return Err(Error::InactiveMerchant);
        }

        Ok(metadata)
    }

    /// Deactivate a merchant
    /// Access Control: Admin only
    pub fn deactivate_merchant(env: Env, merchant_id: Bytes) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();

        let key = DataKey::Merchant(merchant_id.clone());
        let mut metadata: MerchantMetadata = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(Error::MerchantNotFound)?;

        metadata.active = false;
        env.storage().persistent().set(&key, &metadata);

        env.events()
            .publish((symbol_short!("merch_dea"), merchant_id), ());

        Ok(())
    }
}

mod test;
