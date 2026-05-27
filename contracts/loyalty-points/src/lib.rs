#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, symbol_short,
    Address, Env, Map, Symbol, Vec,
};

const KEY_ADMIN: Symbol = symbol_short!("admin");
const KEY_VERSION: Symbol = symbol_short!("version");
const KEY_POINT_NAME: Symbol = symbol_short!("pt_name");
const KEY_EXPIRY_LEDGERS: Symbol = symbol_short!("exp_ldgrs");

#[contracttype]
#[derive(Clone)]
pub struct PointBalance {
    pub balance: i128,
    pub expires_at_ledger: u32,
}

#[contracttype]
#[derive(Clone)]
pub struct RedemptionOption {
    pub name: Symbol,
    pub cost: i128,
    pub active: bool,
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Balance(Address),
    Redemption(u32),
    NextRedemptionId,
    RedemptionHistory(Address, u32),
    RedemptionCount(Address),
}

#[contracterror]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum LoyaltyError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    InsufficientPoints = 4,
    ExpiredPoints = 5,
    InvalidAmount = 6,
    RedemptionNotFound = 7,
    RedemptionInactive = 8,
    InvalidExpiry = 9,
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
        .unwrap_or_else(|| panic_with_error!(env, LoyaltyError::NotInitialized));
    admin.require_auth();
    admin
}

fn load_balance(env: &Env, account: &Address) -> PointBalance {
    let key = DataKey::Balance(account.clone());
    env.storage()
        .persistent()
        .get(&key)
        .unwrap_or(PointBalance { balance: 0, expires_at_ledger: 0 })
}

fn save_balance(env: &Env, account: &Address, bal: &PointBalance) {
    let key = DataKey::Balance(account.clone());
    env.storage().persistent().set(&key, bal);
    bump_persistent(env, &key);
}

#[contract]
pub struct LoyaltyPoints;

#[contractimpl]
impl LoyaltyPoints {
    pub fn initialize(
        env: Env,
        admin: Address,
        point_name: Symbol,
        expiry_ledgers: u32,
    ) {
        if env.storage().instance().has(&KEY_ADMIN) {
            panic_with_error!(env, LoyaltyError::AlreadyInitialized);
        }
        admin.require_auth();

        if expiry_ledgers == 0 {
            panic_with_error!(env, LoyaltyError::InvalidExpiry);
        }

        env.storage().instance().set(&KEY_ADMIN, &admin);
        env.storage().instance().set(&KEY_POINT_NAME, &point_name);
        env.storage().instance().set(&KEY_EXPIRY_LEDGERS, &expiry_ledgers);
        env.storage().instance().set(&KEY_VERSION, &1u32);
        bump_instance(&env);

        env.events().publish(
            (symbol_short!("loyalty"), symbol_short!("init")),
            (admin, point_name, expiry_ledgers),
        );
    }

    pub fn award_points(env: Env, account: Address, amount: i128) {
        require_admin(&env);

        if amount <= 0 {
            panic_with_error!(env, LoyaltyError::InvalidAmount);
        }

        let expiry: u32 = env.storage().instance().get(&KEY_EXPIRY_LEDGERS).unwrap();
        let expires_at = env.ledger().sequence() + expiry;

        let mut bal = load_balance(&env, &account);

        if bal.expires_at_ledger < env.ledger().sequence() && bal.balance > 0 {
            bal.balance = 0;
        }

        bal.balance = bal.balance.checked_add(amount).expect("Balance overflow");
        bal.expires_at_ledger = expires_at.max(bal.expires_at_ledger);
        save_balance(&env, &account, &bal);

        env.events().publish(
            (symbol_short!("loyalty"), symbol_short!("awarded")),
            (account, amount, bal.balance),
        );
    }

    pub fn redeem_points(env: Env, account: Address, redemption_id: u32) {
        account.require_auth();

        let option: RedemptionOption = env
            .storage()
            .persistent()
            .get(&DataKey::Redemption(redemption_id))
            .unwrap_or_else(|| panic_with_error!(env, LoyaltyError::RedemptionNotFound));

        if !option.active {
            panic_with_error!(env, LoyaltyError::RedemptionInactive);
        }

        let mut bal = load_balance(&env, &account);

        if bal.expires_at_ledger < env.ledger().sequence() {
            panic_with_error!(env, LoyaltyError::ExpiredPoints);
        }

        if bal.balance < option.cost {
            panic_with_error!(env, LoyaltyError::InsufficientPoints);
        }

        bal.balance -= option.cost;
        save_balance(&env, &account, &bal);

        let redemption_count: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::RedemptionCount(account.clone()))
            .unwrap_or(0);
        let hist_key = DataKey::RedemptionHistory(account.clone(), redemption_count);
        env.storage().persistent().set(&hist_key, &redemption_id);
        bump_persistent(&env, &hist_key);
        env.storage()
            .persistent()
            .set(&DataKey::RedemptionCount(account.clone()), &(redemption_count + 1));

        env.events().publish(
            (symbol_short!("loyalty"), symbol_short!("redeemed")),
            (account, redemption_id, option.cost, bal.balance),
        );
    }

    pub fn add_redemption_option(env: Env, name: Symbol, cost: i128) -> u32 {
        require_admin(&env);

        if cost <= 0 {
            panic_with_error!(env, LoyaltyError::InvalidAmount);
        }

        let next_id: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::NextRedemptionId)
            .unwrap_or(0);
        let option_id = next_id + 1;

        let option = RedemptionOption { name, cost, active: true };
        env.storage()
            .persistent()
            .set(&DataKey::Redemption(option_id), &option);
        env.storage()
            .persistent()
            .set(&DataKey::NextRedemptionId, &option_id);

        env.events().publish(
            (symbol_short!("loyalty"), symbol_short!("opt_add")),
            (option_id, option.name, option.cost),
        );

        option_id
    }

    pub fn toggle_redemption_option(env: Env, redemption_id: u32, active: bool) {
        require_admin(&env);

        let mut option: RedemptionOption = env
            .storage()
            .persistent()
            .get(&DataKey::Redemption(redemption_id))
            .unwrap_or_else(|| panic_with_error!(env, LoyaltyError::RedemptionNotFound));

        option.active = active;
        env.storage()
            .persistent()
            .set(&DataKey::Redemption(redemption_id), &option);

        env.events().publish(
            (symbol_short!("loyalty"), symbol_short!("opt_tog")),
            (redemption_id, active),
        );
    }

    pub fn get_balance(env: Env, account: Address) -> i128 {
        let bal = load_balance(&env, &account);
        if bal.expires_at_ledger < env.ledger().sequence() {
            0
        } else {
            bal.balance
        }
    }

    pub fn get_redemption_option(env: Env, redemption_id: u32) -> RedemptionOption {
        env.storage()
            .persistent()
            .get(&DataKey::Redemption(redemption_id))
            .unwrap_or_else(|| panic_with_error!(env, LoyaltyError::RedemptionNotFound))
    }

    pub fn get_redemption_history(env: Env, account: Address, index: u32) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::RedemptionHistory(account, index))
            .unwrap_or_else(|| panic_with_error!(env, LoyaltyError::RedemptionNotFound))
    }

    pub fn get_redemption_count(env: Env, account: Address) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::RedemptionCount(account))
            .unwrap_or(0)
    }

    pub fn transfer_admin(env: Env, new_admin: Address) {
        require_admin(&env);
        env.storage().instance().set(&KEY_ADMIN, &new_admin);
        env.events().publish(
            (symbol_short!("loyalty"), symbol_short!("adm_xfer")),
            new_admin,
        );
    }
}

mod test;
