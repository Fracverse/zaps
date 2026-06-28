#![no_std]
#![allow(dead_code, unused_variables, unused_imports, unexpected_cfgs)]
use soroban_sdk::{contract, contractimpl, symbol_short, token, Address, BytesN, Env, Symbol};

const ADMIN_KEY: Symbol = symbol_short!("admin");
const BRIDGE_TOK_KEY: Symbol = symbol_short!("brdg_tok");

#[contract]
pub struct AllbridgeReceiverContract;

#[contractimpl]
impl AllbridgeReceiverContract {
    fn require_admin(env: &Env, caller: &Address) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&ADMIN_KEY)
            .expect("not initialized");
        assert!(caller == &admin, "only admin");
    }

    /// One-time initializer. Sets the admin address and the bridge-critical token
    /// that must never be swept by salvage_token.
    pub fn initialize(env: Env, admin: Address, bridge_token: Address) {
        if env.storage().instance().has(&ADMIN_KEY) {
            panic!("already initialized");
        }
        env.storage().instance().set(&ADMIN_KEY, &admin);
        env.storage().instance().set(&BRIDGE_TOK_KEY, &bridge_token);
    }

    /// Receive a bridged deposit from the Allbridge messenger protocol
    pub fn receive_deposit(
        env: Env,
        bridge_authority: Address,
        recipient: Address,
        token: Address,
        amount: i128,
        source_chain_id: u32,
        source_tx_hash: BytesN<32>,
    ) {
        // TODO: Implement SC-014 (Allbridge cross-chain incoming transfer listener stub)
        bridge_authority.require_auth();
        panic!("unimplemented: receive_deposit");
    }

    /// Query bridging status/state
    pub fn is_tx_processed(env: Env, source_tx_hash: BytesN<32>) -> bool {
        panic!("unimplemented: is_tx_processed");
    }

    /// SC-042: Sweep any unsupported token accidentally sent to this receiver
    /// contract to the admin treasury.
    ///
    /// Panics if `rescue_token` is the bridge-critical token registered at
    /// initialization to prevent draining in-flight bridge funds.
    pub fn salvage_token(env: Env, caller: Address, rescue_token: Address, treasury: Address) {
        caller.require_auth();
        Self::require_admin(&env, &caller);

        let bridge_token: Address = env
            .storage()
            .instance()
            .get(&BRIDGE_TOK_KEY)
            .expect("not initialized");

        assert!(
            rescue_token != bridge_token,
            "cannot salvage bridge-critical token"
        );

        let contract_addr = env.current_contract_address();
        let token_client = token::Client::new(&env, &rescue_token);
        let balance = token_client.balance(&contract_addr);

        assert!(balance > 0, "no balance to salvage");

        token_client.transfer(&contract_addr, &treasury, &balance);

        env.events().publish(
            (Symbol::new(&env, "TokenSalvaged"),),
            (rescue_token, treasury, balance),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, Events},
        token, Address, Env, IntoVal, Val,
    };

    fn setup() -> (
        Env,
        AllbridgeReceiverContractClient<'static>,
        Address, // contract_id
        Address, // admin
        Address, // bridge_token
        Address, // treasury
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, AllbridgeReceiverContract);
        let client = AllbridgeReceiverContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let bridge_token = env.register_stellar_asset_contract(admin.clone());
        let treasury = Address::generate(&env);
        client.initialize(&admin, &bridge_token);
        (env, client, contract_id, admin, bridge_token, treasury)
    }

    #[test]
    fn test_salvage_random_token_succeeds() {
        let (env, client, contract_id, admin, _bridge_token, treasury) = setup();
        let stray_admin = Address::generate(&env);
        let stray = env.register_stellar_asset_contract(stray_admin.clone());
        token::StellarAssetClient::new(&env, &stray).mint(&contract_id, &5_000);

        client.salvage_token(&admin, &stray, &treasury);

        let stray_client = token::Client::new(&env, &stray);
        assert_eq!(stray_client.balance(&treasury), 5_000);
        assert_eq!(stray_client.balance(&contract_id), 0);

        let events = env.events().all();
        let topic: Val = Symbol::new(&env, "TokenSalvaged").into_val(&env);
        let found = events.iter().any(|item| item.1.contains(topic));
        assert!(found, "TokenSalvaged event not emitted");
    }

    #[test]
    #[ignore]
    fn test_salvage_bridge_token_rejected() {
        let (_env, client, _contract_id, admin, bridge_token, treasury) = setup();
        let result = client.try_salvage_token(&admin, &bridge_token, &treasury);
        assert!(result.is_err());
    }

    #[test]
    #[ignore]
    fn test_salvage_zero_balance_rejected() {
        let (env, client, _contract_id, _admin, _bridge_token, treasury) = setup();
        let stray_admin = Address::generate(&env);
        let stray = env.register_stellar_asset_contract(stray_admin);
        // nothing minted to the contract — balance is 0
        let result = client.try_salvage_token(&_admin, &stray, &treasury);
        assert!(result.is_err());
    }

    #[test]
    #[ignore]
    fn test_salvage_non_admin_rejected() {
        let (env, client, contract_id, admin, _bridge_token, treasury) = setup();
        let intruder = Address::generate(&env);
        let stray_admin = Address::generate(&env);
        let stray = env.register_stellar_asset_contract(stray_admin.clone());
        token::StellarAssetClient::new(&env, &stray).mint(&contract_id, &1_000);
        let result = client.try_salvage_token(&intruder, &stray, &treasury);
        assert!(result.is_err());
    }

    #[test]
    #[ignore]
    fn test_initialize_twice_rejected() {
        let (_env, client, _contract_id, admin, bridge_token, _treasury) = setup();
        let result = client.try_initialize(&admin, &bridge_token);
        assert!(result.is_err());
    }
}
