#![no_std]
#![allow(unexpected_cfgs)]
use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, Address, Env, String, Symbol,
};

const ADMIN_KEY: Symbol = symbol_short!("admin");

#[contracttype]
enum DataKey {
    Balance(Address),
    Allowance(Address, Address),
}

#[contracttype]
struct AllowanceData {
    amount: i128,
    expiration_ledger: u32,
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

    pub fn initialize(env: Env, admin: Address, _name: String, _symbol: String) {
        if env.storage().instance().has(&ADMIN_KEY) {
            panic!("already initialized");
        }
        env.storage().instance().set(&ADMIN_KEY, &admin);
    }

    pub fn mint(env: Env, to: Address, amount: i128) {
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
        spender.require_auth();
        assert!(amount > 0, "amount must be positive");
        let allowance_key = DataKey::Allowance(from.clone(), spender.clone());
        let allowance_data: AllowanceData = env
            .storage()
            .persistent()
            .get(&allowance_key)
            .unwrap_or(AllowanceData {
                amount: 0,
                expiration_ledger: 0,
            });
        assert!(
            env.ledger().sequence() < allowance_data.expiration_ledger,
            "allowance has expired"
        );
        assert!(allowance_data.amount >= amount, "allowance exceeded");
        let from_bal: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::Balance(from.clone()))
            .unwrap_or(0);
        assert!(from_bal >= amount, "insufficient balance");
        env.storage()
            .persistent()
            .set(
                &allowance_key,
                &AllowanceData {
                    amount: allowance_data.amount - amount,
                    expiration_ledger: allowance_data.expiration_ledger,
                },
            );
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

    /// Approve `spender` to transfer up to `amount` tokens from the caller until `expiration_ledger`
    pub fn approve(
        env: Env,
        from: Address,
        spender: Address,
        amount: i128,
        expiration_ledger: u32,
    ) {
        from.require_auth();
        assert!(amount >= 0, "allowance cannot be negative");
        assert!(
            env.ledger().sequence() < expiration_ledger,
            "expiration ledger must be in the future"
        );
        env.storage()
            .persistent()
            .set(
                &DataKey::Allowance(from, spender),
                &AllowanceData {
                    amount,
                    expiration_ledger,
                },
            );
    }

    pub fn balance(env: Env, id: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::Balance(id))
            .unwrap_or(0)
    }

    /// Query the allowance granted by `from` to `spender`
    pub fn allowance(env: Env, from: Address, spender: Address) -> i128 {
        let allowance_data: AllowanceData = env
            .storage()
            .persistent()
            .get(&DataKey::Allowance(from, spender))
            .unwrap_or(AllowanceData {
                amount: 0,
                expiration_ledger: 0,
            });
        if env.ledger().sequence() < allowance_data.expiration_ledger {
            allowance_data.amount
        } else {
            0
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::{testutils::{Address as _, Ledger}, Address, Env, String};

    fn setup() -> (Env, NairaTokenContractClient<'static>, Address, Address, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, NairaTokenContract);
        let client = NairaTokenContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let owner = Address::generate(&env);
        let spender = Address::generate(&env);
        client.initialize(
            &admin,
            &String::from_str(&env, "Naira"),
            &String::from_str(&env, "NAR"),
        );
        client.mint(&owner, &100);
        (env, client, admin, owner, spender)
    }

    #[test]
    fn test_approve_sets_allowance_with_expiration() {
        let (env, client, _admin, owner, spender) = setup();
        client.approve(&owner, &spender, &50, &10);
        assert_eq!(client.allowance(&owner, &spender), 50);
        assert!(env.ledger().sequence() < 10);
    }

    #[test]
    fn test_allowance_expires_as_expected() {
        let (env, client, _admin, owner, spender) = setup();
        client.approve(&owner, &spender, &50, &1);
        assert_eq!(client.allowance(&owner, &spender), 50);
        env.ledger().with_mut(|li| li.sequence_number = 1);
        assert_eq!(client.allowance(&owner, &spender), 0);
    }
}
