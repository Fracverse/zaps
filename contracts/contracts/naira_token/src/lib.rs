#![no_std]
#![allow(unexpected_cfgs)]
use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, Address, Env, String, Symbol,
};

const ADMIN_KEY: Symbol = symbol_short!("admin");
const NAME_KEY: Symbol = symbol_short!("name");
const SYMBOL_KEY: Symbol = symbol_short!("symbol");
const DECIMALS_KEY: Symbol = symbol_short!("decimals");
const PAUSED_KEY: Symbol = symbol_short!("paused");

#[contracttype]
enum DataKey {
    Balance(Address),
    Allowance(Address, Address),
}

#[contract]
pub struct NairaTokenContract;

#[contractimpl]
impl NairaTokenContract {
    fn require_admin(env: &Env) -> Address {
        let admin: Address = env
            .storage()
            .instance()
            .get(&ADMIN_KEY)
            .expect("not initialized");
        admin.require_auth();
        admin
    }

    fn is_paused(env: &Env) -> bool {
        env.storage().instance().get(&PAUSED_KEY).unwrap_or(false)
    }

    fn require_not_paused(env: &Env) {
        assert!(!Self::is_paused(env), "contract is paused");
    }

    pub fn set_paused(env: Env, paused: bool) {
        Self::require_admin(&env);
        env.storage().instance().set(&PAUSED_KEY, &paused);
    }

    pub fn pause(env: Env) {
        Self::set_paused(env, true);
    }

    pub fn unpause(env: Env) {
        Self::set_paused(env, false);
    }

    pub fn initialize(env: Env, admin: Address, name: String, symbol: String, decimals: u32) {
        if env.storage().instance().has(&ADMIN_KEY) {
            panic!("already initialized");
        }
        env.storage().instance().set(&ADMIN_KEY, &admin);
        env.storage().instance().set(&NAME_KEY, &name);
        env.storage().instance().set(&SYMBOL_KEY, &symbol);
        env.storage().instance().set(&DECIMALS_KEY, &decimals);
    }

    pub fn mint(env: Env, to: Address, amount: i128) {
        Self::require_not_paused(&env);
        Self::require_admin(&env);
        assert!(amount > 0, "amount must be positive");
        let bal: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::Balance(to.clone()))
            .unwrap_or(0);
        env.storage()
            .persistent()
            .set(&DataKey::Balance(to), &(bal + amount));
    }

    pub fn burn(env: Env, from: Address, amount: i128) {
        Self::require_not_paused(&env);
        Self::require_admin(&env);
        assert!(amount > 0, "amount must be positive");
        let bal: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::Balance(from.clone()))
            .unwrap_or(0);
        assert!(bal >= amount, "insufficient balance");
        env.storage()
            .persistent()
            .set(&DataKey::Balance(from), &(bal - amount));
    }

    pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
        Self::require_not_paused(&env);
        from.require_auth();
        assert!(amount > 0, "amount must be positive");
        let from_bal: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::Balance(from.clone()))
            .unwrap_or(0);
        assert!(from_bal >= amount, "insufficient balance");
        env.storage()
            .persistent()
            .set(&DataKey::Balance(from), &(from_bal - amount));
        let to_bal: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::Balance(to.clone()))
            .unwrap_or(0);
        env.storage()
            .persistent()
            .set(&DataKey::Balance(to), &(to_bal + amount));
    }

    /// Transfer tokens on behalf of `from` using a pre-approved allowance
    pub fn transfer_from(env: Env, spender: Address, from: Address, to: Address, amount: i128) {
        Self::require_not_paused(&env);
        spender.require_auth();
        assert!(amount > 0, "amount must be positive");
        let allowance_key = DataKey::Allowance(from.clone(), spender.clone());
        let allowance: i128 = env.storage().persistent().get(&allowance_key).unwrap_or(0);
        assert!(allowance >= amount, "allowance exceeded");
        let from_bal: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::Balance(from.clone()))
            .unwrap_or(0);
        assert!(from_bal >= amount, "insufficient balance");
        env.storage()
            .persistent()
            .set(&allowance_key, &(allowance - amount));
        env.storage()
            .persistent()
            .set(&DataKey::Balance(from), &(from_bal - amount));
        let to_bal: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::Balance(to.clone()))
            .unwrap_or(0);
        env.storage()
            .persistent()
            .set(&DataKey::Balance(to), &(to_bal + amount));
    }

    /// Approve `spender` to transfer up to `amount` tokens from the caller
    pub fn approve(env: Env, from: Address, spender: Address, amount: i128) {
        from.require_auth();
        assert!(amount >= 0, "allowance cannot be negative");
        env.storage()
            .persistent()
            .set(&DataKey::Allowance(from, spender), &amount);
    }

    pub fn balance(env: Env, id: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::Balance(id))
            .unwrap_or(0)
    }

    /// Query the allowance granted by `from` to `spender`
    pub fn allowance(env: Env, from: Address, spender: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::Allowance(from, spender))
            .unwrap_or(0)
    }

    pub fn name(env: Env) -> String {
        env.storage()
            .instance()
            .get(&NAME_KEY)
            .expect("not initialized")
    }

    pub fn symbol(env: Env) -> String {
        env.storage()
            .instance()
            .get(&SYMBOL_KEY)
            .expect("not initialized")
    }

    pub fn decimals(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DECIMALS_KEY)
            .expect("not initialized")
    }

    /// SC-050: Transfer admin authority to a new address.
    /// Requires the current admin's signature.
    pub fn transfer_admin(env: Env, new_admin: Address) {
        Self::require_admin(&env);
        env.storage().instance().set(&ADMIN_KEY, &new_admin);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Env};

    fn setup() -> (Env, NairaTokenContractClient<'static>, Address, Address) {
        let env = Env::default();
        let contract_id = env.register_contract(None, NairaTokenContract);
        let client = NairaTokenContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let user = Address::generate(&env);
        let name = String::from_str(&env, "Naira Token");
        let symbol = String::from_str(&env, "NGN");
        let decimals = 7u32;
        client.initialize(&admin, &name, &symbol, &decimals);
        (env, client, admin, user)
    }

    #[test]
    fn metadata_returns_initialized_values() {
        let env = Env::default();
        let contract_id = env.register_contract(None, NairaTokenContract);
        let client = NairaTokenContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let name = String::from_str(&env, "Naira Token");
        let symbol = String::from_str(&env, "NGN");
        let decimals = 7u32;

        client.initialize(&admin, &name, &symbol, &decimals);

        assert_eq!(client.name(), name);
        assert_eq!(client.symbol(), symbol);
        assert_eq!(client.decimals(), decimals);
    }

    #[test]
    fn admin_can_pause_and_unpause() {
        let (_env, client, admin, _user) = setup();
        client.pause(&admin);
        client.unpause(&admin);
    }

    #[test]
    fn transfer_fails_when_paused() {
        let (env, client, admin, user) = setup();
        client.mint(&admin, &user, &1000);
        client.pause(&admin);
        let result = client.try_transfer(&user, &user, &1000);
        assert!(result.is_err());
    }

    #[test]
    fn mint_fails_when_paused() {
        let (_env, client, admin, user) = setup();
        client.pause(&admin);
        let result = client.try_mint(&admin, &user, &1000);
        assert!(result.is_err());
    }

    #[test]
    fn burn_fails_when_paused() {
        let (_env, client, admin, user) = setup();
        client.mint(&admin, &user, &1000);
        client.pause(&admin);
        let result = client.try_burn(&admin, &user, &500);
        assert!(result.is_err());
    }

    #[test]
    fn transfer_from_fails_when_paused() {
        let (env, client, admin, user) = setup();
        let spender = Address::generate(&env);
        client.mint(&admin, &user, &1000);
        client.approve(&user, &spender, &1000);
        client.pause(&admin);
        let result = client.try_transfer_from(&spender, &user, &user, &500);
        assert!(result.is_err());
    }

    #[test]
    fn non_admin_cannot_pause() {
        let (_env, client, _admin, user) = setup();
        let result = client.try_pause(&user);
        assert!(result.is_err());
    }

    #[test]
    fn operations_succeed_after_unpause() {
        let (env, client, admin, user) = setup();
        client.mint(&admin, &user, &1000);
        client.pause(&admin);
        client.unpause(&admin);
        client.transfer(&user, &user, &500);
        assert_eq!(client.balance(&user), 1000);
    }
}
