#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, symbol_short, vec,
    Address, Bytes, Env, Symbol, Vec,
};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const KEY_ADMIN: Symbol = symbol_short!("admin");
const INSTANCE_TTL_EXTEND: u32 = 6_307_200;
const INSTANCE_TTL_THRESHOLD: u32 = 100_000;
/// Max tags per payment.
const MAX_TAGS_PER_PAYMENT: u32 = 20;
/// Max tag name length in bytes.
const MAX_TAG_LEN: u32 = 64;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[contracterror]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum TagError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    TagAlreadyExists = 4,
    TagNotFound = 5,
    TooManyTags = 6,
    InvalidTagName = 7,
    MerchantNotFound = 8,
}

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// A tag attached to a payment.
#[contracttype]
#[derive(Clone)]
pub struct Tag {
    pub name: Bytes,
    pub created_by: Address,
    pub created_at: u32,
}

/// Merchant record: tracks which address owns a merchant ID.
#[contracttype]
#[derive(Clone)]
pub struct Merchant {
    pub owner: Address,
    pub active: bool,
}

// ---------------------------------------------------------------------------
// Storage key helpers
// ---------------------------------------------------------------------------

/// Key for tags on a specific payment: ("ptags", payment_id)
fn payment_tags_key(payment_id: &Bytes) -> (Symbol, Bytes) {
    (symbol_short!("ptags"), payment_id.clone())
}

/// Key for merchant record: ("merch", merchant_id)
fn merchant_key(merchant_id: &Bytes) -> (Symbol, Bytes) {
    (symbol_short!("merch"), merchant_id.clone())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn require_admin(env: &Env) {
    let admin: Address = env.storage().instance().get(&KEY_ADMIN).unwrap();
    admin.require_auth();
}

fn is_initialized(env: &Env) -> bool {
    env.storage().instance().has(&KEY_ADMIN)
}

fn bump(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(INSTANCE_TTL_THRESHOLD, INSTANCE_TTL_EXTEND);
}

fn require_initialized(env: &Env) {
    if !is_initialized(env) {
        panic_with_error!(env, TagError::NotInitialized);
    }
}

fn require_merchant_owner(env: &Env, merchant_id: &Bytes, caller: &Address) {
    let key = merchant_key(merchant_id);
    let merchant: Merchant = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or_else(|| panic_with_error!(env, TagError::MerchantNotFound));
    if merchant.owner != *caller {
        panic_with_error!(env, TagError::Unauthorized);
    }
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct PaymentTagging;

#[contractimpl]
impl PaymentTagging {
    pub fn initialize(env: Env, admin: Address) {
        if is_initialized(&env) {
            panic_with_error!(&env, TagError::AlreadyInitialized);
        }
        admin.require_auth();
        env.storage().instance().set(&KEY_ADMIN, &admin);
        bump(&env);
    }

    // ------------------------------------------------------------------
    // Merchant management (admin only)
    // ------------------------------------------------------------------

    pub fn register_merchant(env: Env, merchant_id: Bytes, owner: Address) {
        require_initialized(&env);
        require_admin(&env);
        let key = merchant_key(&merchant_id);
        let merchant = Merchant {
            owner,
            active: true,
        };
        env.storage().persistent().set(&key, &merchant);
        env.storage()
            .persistent()
            .extend_ttl(&key, INSTANCE_TTL_THRESHOLD, INSTANCE_TTL_EXTEND);
        bump(&env);
    }

    pub fn deactivate_merchant(env: Env, merchant_id: Bytes) {
        require_initialized(&env);
        require_admin(&env);
        let key = merchant_key(&merchant_id);
        let mut merchant: Merchant = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(&env, TagError::MerchantNotFound));
        merchant.active = false;
        env.storage().persistent().set(&key, &merchant);
        bump(&env);
    }

    // ------------------------------------------------------------------
    // Tag management
    // ------------------------------------------------------------------

    /// Add a tag to a payment. Caller must be the merchant owner or admin.
    pub fn add_tag(
        env: Env,
        caller: Address,
        payment_id: Bytes,
        merchant_id: Bytes,
        tag_name: Bytes,
    ) {
        require_initialized(&env);
        caller.require_auth();

        // Validate tag name length
        if tag_name.len() == 0 || tag_name.len() > MAX_TAG_LEN {
            panic_with_error!(&env, TagError::InvalidTagName);
        }

        // Caller must be merchant owner
        require_merchant_owner(&env, &merchant_id, &caller);

        let key = payment_tags_key(&payment_id);
        let mut tags: Vec<Tag> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or(vec![&env]);

        if tags.len() >= MAX_TAGS_PER_PAYMENT {
            panic_with_error!(&env, TagError::TooManyTags);
        }

        // Duplicate check
        for t in tags.iter() {
            if t.name == tag_name {
                panic_with_error!(&env, TagError::TagAlreadyExists);
            }
        }

        tags.push_back(Tag {
            name: tag_name,
            created_by: caller,
            created_at: env.ledger().sequence(),
        });

        env.storage().persistent().set(&key, &tags);
        env.storage()
            .persistent()
            .extend_ttl(&key, INSTANCE_TTL_THRESHOLD, INSTANCE_TTL_EXTEND);
        bump(&env);
    }

    /// Remove a tag from a payment. Caller must be the merchant owner or admin.
    pub fn remove_tag(
        env: Env,
        caller: Address,
        payment_id: Bytes,
        merchant_id: Bytes,
        tag_name: Bytes,
    ) {
        require_initialized(&env);
        caller.require_auth();
        require_merchant_owner(&env, &merchant_id, &caller);

        let key = payment_tags_key(&payment_id);
        let tags: Vec<Tag> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or(vec![&env]);

        let mut new_tags: Vec<Tag> = vec![&env];
        let mut found = false;
        for t in tags.iter() {
            if t.name == tag_name {
                found = true;
            } else {
                new_tags.push_back(t);
            }
        }
        if !found {
            panic_with_error!(&env, TagError::TagNotFound);
        }
        env.storage().persistent().set(&key, &new_tags);
        bump(&env);
    }

    // ------------------------------------------------------------------
    // Queries
    // ------------------------------------------------------------------

    /// Get all tags for a payment.
    pub fn get_tags(env: Env, payment_id: Bytes) -> Vec<Tag> {
        require_initialized(&env);
        let key = payment_tags_key(&payment_id);
        env.storage()
            .persistent()
            .get(&key)
            .unwrap_or(vec![&env])
    }

    /// Check whether a payment has a specific tag.
    pub fn has_tag(env: Env, payment_id: Bytes, tag_name: Bytes) -> bool {
        require_initialized(&env);
        let key = payment_tags_key(&payment_id);
        let tags: Vec<Tag> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or(vec![&env]);
        for t in tags.iter() {
            if t.name == tag_name {
                return true;
            }
        }
        false
    }

    /// Get merchant info.
    pub fn get_merchant(env: Env, merchant_id: Bytes) -> Merchant {
        require_initialized(&env);
        let key = merchant_key(&merchant_id);
        env.storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(&env, TagError::MerchantNotFound))
    }
}
