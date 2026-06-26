#![no_std]
#![allow(unexpected_cfgs)]
use soroban_sdk::{contract, contractimpl, contracttype, symbol_short, Address, Env, String, Symbol};

const ADMIN_KEY: Symbol = symbol_short!("admin");

#[contracttype]
enum DataKey {
    Balance(Address),
    Allowance(Address, Address),
}

fn require_admin(env: &Env) {
    let admin: Address = env
        .storage()
        .instance()
        .get(&ADMIN_KEY)
        .expect("not initialized");
    admin.require_auth();
}

#[contract]
pub struct NairaTokenContract;

#[contractimpl]
impl NairaTokenContract {
    pub fn initialize(env: Env, admin: Address, _name: String, _symbol: String) {
        if env.storage().instance().has(&ADMIN_KEY) {
            panic!("already initialized");
        }
        env.storage().instance().set(&ADMIN_KEY, &admin);
    }

    /// Mint new Naira stablecoins to `to`. Only the stored anchor/admin may call this.
    pub fn mint(env: Env, to: Address, amount: i128) {
        require_admin(&env);
        assert!(amount > 0, "amount must be positive");
        let bal: i128 = env.storage().persistent().get(&DataKey::Balance(to.clone())).unwrap_or(0);
        env.storage().persistent().set(&DataKey::Balance(to), &(bal + amount));
    }

    /// Burn Naira stablecoins from `from`. Only the stored anchor/admin may call this.
    pub fn burn(env: Env, from: Address, amount: i128) {
        require_admin(&env);
        assert!(amount > 0, "amount must be positive");
        let bal: i128 = env.storage().persistent().get(&DataKey::Balance(from.clone())).unwrap_or(0);
        assert!(bal >= amount, "insufficient balance");
        env.storage().persistent().set(&DataKey::Balance(from), &(bal - amount));
    }

    pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
        from.require_auth();
        assert!(amount > 0, "amount must be positive");
        let from_bal: i128 = env.storage().persistent().get(&DataKey::Balance(from.clone())).unwrap_or(0);
        assert!(from_bal >= amount, "insufficient balance");
        env.storage().persistent().set(&DataKey::Balance(from), &(from_bal - amount));
        let to_bal: i128 = env.storage().persistent().get(&DataKey::Balance(to.clone())).unwrap_or(0);
        env.storage().persistent().set(&DataKey::Balance(to), &(to_bal + amount));
    }

    /// Transfer tokens on behalf of `from` using a pre-approved allowance
    pub fn transfer_from(env: Env, spender: Address, from: Address, to: Address, amount: i128) {
        spender.require_auth();
        assert!(amount > 0, "amount must be positive");
        let allowance_key = DataKey::Allowance(from.clone(), spender.clone());
        let allowance: i128 = env.storage().persistent().get(&allowance_key).unwrap_or(0);
        assert!(allowance >= amount, "allowance exceeded");
        let from_bal: i128 = env.storage().persistent().get(&DataKey::Balance(from.clone())).unwrap_or(0);
        assert!(from_bal >= amount, "insufficient balance");
        env.storage().persistent().set(&allowance_key, &(allowance - amount));
        env.storage().persistent().set(&DataKey::Balance(from), &(from_bal - amount));
        let to_bal: i128 = env.storage().persistent().get(&DataKey::Balance(to.clone())).unwrap_or(0);
        env.storage().persistent().set(&DataKey::Balance(to), &(to_bal + amount));
    }

    /// Approve `spender` to transfer up to `amount` tokens from the caller
    pub fn approve(env: Env, from: Address, spender: Address, amount: i128) {
        from.require_auth();
        assert!(amount >= 0, "allowance cannot be negative");
        env.storage().persistent().set(&DataKey::Allowance(from, spender), &amount);
    }

    pub fn balance(env: Env, id: Address) -> i128 {
        env.storage().persistent().get(&DataKey::Balance(id)).unwrap_or(0)
    }

    /// Query the allowance granted by `from` to `spender`
    pub fn allowance(env: Env, from: Address, spender: Address) -> i128 {
        env.storage().persistent().get(&DataKey::Allowance(from, spender)).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, MockAuth, MockAuthInvoke},
        IntoVal,
    };

    fn setup() -> (Env, NairaTokenContractClient<'static>, Address, Address) {
        let env = Env::default();
        let contract_id = env.register_contract(None, NairaTokenContract);
        let client = NairaTokenContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let holder = Address::generate(&env);

        client.initialize(
            &admin,
            &String::from_str(&env, "Zaps Naira"),
            &String::from_str(&env, "NGNZ"),
        );

        (env, client, admin, holder)
    }

    #[test]
    fn admin_can_mint_and_burn() {
        let (env, client, admin, holder) = setup();

        client
            .mock_auths(&[MockAuth {
                address: &admin,
                invoke: &MockAuthInvoke {
                    contract: &client.address,
                    fn_name: "mint",
                    args: (&holder, 1_000_i128).into_val(&env),
                    sub_invokes: &[],
                },
            }])
            .mint(&holder, &1_000);
        assert_eq!(client.balance(&holder), 1_000);

        client
            .mock_auths(&[MockAuth {
                address: &admin,
                invoke: &MockAuthInvoke {
                    contract: &client.address,
                    fn_name: "burn",
                    args: (&holder, 400_i128).into_val(&env),
                    sub_invokes: &[],
                },
            }])
            .burn(&holder, &400);
        assert_eq!(client.balance(&holder), 600);
    }
}
