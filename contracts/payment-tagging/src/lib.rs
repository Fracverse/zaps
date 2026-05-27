#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, symbol_short,
    Address, Bytes, Env, Map, Symbol, Vec,
};

const KEY_ADMIN: Symbol = symbol_short!("admin");
const KEY_VERSION: Symbol = symbol_short!("version");

#[contracttype]
#[derive(Clone)]
pub struct Tag {
    pub name: Bytes,
    pub merchant: Address,
    pub active: bool,
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Tag(Bytes),
    TagList(Address),
    TagListCount(Address),
    PaymentTag(u64, Address),
    PaymentTagCount(u64),
    MerchantTagIndex(Address, Bytes),
}

#[contracterror]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum TaggingError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    TagNotFound = 4,
    TagAlreadyExists = 5,
    TagInactive = 6,
    InvalidTagName = 7,
    PaymentNotFound = 8,
    TagNotOnPayment = 9,
    AlreadyTagged = 10,
}

fn bump_instance(env: &Env) {
    let ttl = 6_307_200u32;
    let threshold = 100_000u32;
    env.storage().instance().extend_ttl(threshold, ttl);
}

fn bump_persistent<K>(env: &Env, key: &K)
where
    K: soroban_sdk::IntoVal<Env, soroban_sdk::Val>,
{
    env.storage()
        .persistent()
        .extend_ttl(key, 50_000u32, 3_153_600u32);
}

fn require_admin(env: &Env) -> Address {
    let admin: Address = env
        .storage()
        .instance()
        .get(&KEY_ADMIN)
        .unwrap_or_else(|| panic_with_error!(env, TaggingError::NotInitialized));
    admin.require_auth();
    admin
}

fn validate_tag_name(name: &Bytes) -> bool {
    !name.is_empty() && name.len() <= 64
}

#[contract]
pub struct PaymentTagging;

#[contractimpl]
impl PaymentTagging {
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&KEY_ADMIN) {
            panic_with_error!(env, TaggingError::AlreadyInitialized);
        }
        admin.require_auth();

        env.storage().instance().set(&KEY_ADMIN, &admin);
        env.storage().instance().set(&KEY_VERSION, &1u32);
        bump_instance(&env);
    }

    pub fn create_tag(env: Env, merchant: Address, name: Bytes) {
        merchant.require_auth();

        if !validate_tag_name(&name) {
            panic_with_error!(env, TaggingError::InvalidTagName);
        }

        let tag_key = DataKey::Tag(name.clone());
        if env.storage().persistent().has(&tag_key) {
            panic_with_error!(env, TaggingError::TagAlreadyExists);
        }

        let tag = Tag {
            name: name.clone(),
            merchant: merchant.clone(),
            active: true,
        };
        env.storage().persistent().set(&tag_key, &tag);
        bump_persistent(&env, &tag_key);

        let count: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::TagListCount(merchant.clone()))
            .unwrap_or(0);
        env.storage()
            .persistent()
            .set(&DataKey::MerchantTagIndex(merchant.clone(), name.clone()), &true);
        env.storage()
            .persistent()
            .set(&DataKey::TagList(merchant.clone()), &name);
        env.storage()
            .persistent()
            .set(&DataKey::TagListCount(merchant.clone()), &(count + 1));
        bump_persistent(&env, &DataKey::TagListCount(merchant.clone()));

        env.events().publish(
            (symbol_short!("tagging"), symbol_short!("created")),
            (merchant, name),
        );
    }

    pub fn toggle_tag(env: Env, merchant: Address, name: Bytes, active: bool) {
        merchant.require_auth();

        let tag_key = DataKey::Tag(name.clone());
        let mut tag: Tag = env.storage()
            .persistent()
            .get(&tag_key)
            .unwrap_or_else(|| panic_with_error!(env, TaggingError::TagNotFound));

        if tag.merchant != merchant {
            panic_with_error!(env, TaggingError::Unauthorized);
        }

        tag.active = active;
        env.storage().persistent().set(&tag_key, &tag);
        bump_persistent(&env, &tag_key);

        env.events().publish(
            (symbol_short!("tagging"), symbol_short!("toggled")),
            (merchant, name, active),
        );
    }

    pub fn tag_payment(env: Env, merchant: Address, payment_id: u64, tag_name: Bytes) {
        merchant.require_auth();

        let tag_key = DataKey::Tag(tag_name.clone());
        let tag: Tag = env.storage()
            .persistent()
            .get(&tag_key)
            .unwrap_or_else(|| panic_with_error!(env, TaggingError::TagNotFound));

        if tag.merchant != merchant {
            panic_with_error!(env, TaggingError::Unauthorized);
        }
        if !tag.active {
            panic_with_error!(env, TaggingError::TagInactive);
        }

        let payment_tag_key = DataKey::PaymentTag(payment_id, merchant.clone());
        let existing_tags: Vec<Bytes> = env.storage()
            .persistent()
            .get(&payment_tag_key)
            .unwrap_or_else(|| Vec::new(&env));

        for t in existing_tags.iter() {
            if t == tag_name {
                panic_with_error!(env, TaggingError::AlreadyTagged);
            }
        }

        let mut updated = existing_tags;
        updated.push_back(tag_name.clone());
        env.storage().persistent().set(&payment_tag_key, &updated);
        bump_persistent(&env, &payment_tag_key);

        let count: u32 = env.storage()
            .persistent()
            .get(&DataKey::PaymentTagCount(payment_id))
            .unwrap_or(0);
        env.storage()
            .persistent()
            .set(&DataKey::PaymentTagCount(payment_id), &(count + 1));

        env.events().publish(
            (symbol_short!("tagging"), symbol_short!("tagged")),
            (merchant, payment_id, tag_name),
        );
    }

    pub fn untag_payment(env: Env, merchant: Address, payment_id: u64, tag_name: Bytes) {
        merchant.require_auth();

        let tag_key = DataKey::Tag(tag_name.clone());
        if !env.storage().persistent().has(&tag_key) {
            panic_with_error!(env, TaggingError::TagNotFound);
        }

        let payment_tag_key = DataKey::PaymentTag(payment_id, merchant.clone());
        let existing_tags: Vec<Bytes> = env.storage()
            .persistent()
            .get(&payment_tag_key)
            .unwrap_or_else(|| panic_with_error!(env, TaggingError::PaymentNotFound));

        let mut found = false;
        let mut updated: Vec<Bytes> = Vec::new(&env);
        for t in existing_tags.iter() {
            if t == tag_name {
                found = true;
            } else {
                updated.push_back(t);
            }
        }

        if !found {
            panic_with_error!(env, TaggingError::TagNotOnPayment);
        }

        env.storage().persistent().set(&payment_tag_key, &updated);
        bump_persistent(&env, &payment_tag_key);

        let count: u32 = env.storage()
            .persistent()
            .get(&DataKey::PaymentTagCount(payment_id))
            .unwrap_or(0);
        if count > 0 {
            env.storage()
                .persistent()
                .set(&DataKey::PaymentTagCount(payment_id), &(count - 1));
        }

        env.events().publish(
            (symbol_short!("tagging"), symbol_short!("untagged")),
            (merchant, payment_id, tag_name),
        );
    }

    pub fn get_payment_tags(env: Env, payment_id: u64, merchant: Address) -> Vec<Bytes> {
        let key = DataKey::PaymentTag(payment_id, merchant);
        env.storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| Vec::new(&env))
    }

    pub fn get_tag(env: Env, name: Bytes) -> Tag {
        env.storage()
            .persistent()
            .get(&DataKey::Tag(name))
            .unwrap_or_else(|| panic_with_error!(env, TaggingError::TagNotFound))
    }

    pub fn get_merchant_tags(env: Env, merchant: Address) -> Vec<Bytes> {
        env.storage()
            .persistent()
            .get(&DataKey::TagList(merchant))
            .unwrap_or_else(|| Vec::new(&env))
    }

    pub fn get_merchant_tag_count(env: Env, merchant: Address) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::TagListCount(merchant))
            .unwrap_or(0)
    }

    pub fn get_payment_tag_count(env: Env, payment_id: u64) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::PaymentTagCount(payment_id))
            .unwrap_or(0)
    }

    pub fn transfer_admin(env: Env, new_admin: Address) {
        require_admin(&env);
        env.storage().instance().set(&KEY_ADMIN, &new_admin);
        env.events().publish(
            (symbol_short!("tagging"), symbol_short!("adm_xfer")),
            new_admin,
        );
    }
}

mod test;
