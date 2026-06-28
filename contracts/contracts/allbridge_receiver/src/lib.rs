#![no_std]
#![allow(dead_code, unused_variables, unused_imports, unexpected_cfgs)]
use soroban_sdk::{contract, contractimpl, symbol_short, Address, BytesN, Env, Symbol};

const ADMIN_KEY: Symbol = symbol_short!("admin");
const BRIDGE_AUTH_KEY: Symbol = symbol_short!("bridge");

#[contract]
pub struct AllbridgeReceiverContract;

#[contractimpl]
impl AllbridgeReceiverContract {
    fn require_admin(env: &Env) -> Address {
        let admin: Address = env
            .storage()
            .instance()
            .get(&ADMIN_KEY)
            .expect("not initialized");
        admin.require_auth();
        admin
    }

    pub fn initialize(env: Env, admin: Address, bridge_authority: Address) {
        if env.storage().instance().has(&ADMIN_KEY) {
            panic!("already initialized");
        }

        env.storage().instance().set(&ADMIN_KEY, &admin);
        env.storage()
            .instance()
            .set(&BRIDGE_AUTH_KEY, &bridge_authority);
    }

    pub fn bridge_authority(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&BRIDGE_AUTH_KEY)
            .expect("not initialized")
    }

    pub fn set_bridge_authority(env: Env, new_bridge_authority: Address) {
        Self::require_admin(&env);
        env.storage()
            .instance()
            .set(&BRIDGE_AUTH_KEY, &new_bridge_authority);
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
        let trusted_bridge_authority: Address = env
            .storage()
            .instance()
            .get(&BRIDGE_AUTH_KEY)
            .expect("not initialized");
        assert!(
            bridge_authority == trusted_bridge_authority,
            "unauthorized bridge authority"
        );
        bridge_authority.require_auth();
        panic!("unimplemented: receive_deposit");
    }

    /// Query bridging status/state
    pub fn is_tx_processed(env: Env, source_tx_hash: BytesN<32>) -> bool {
        panic!("unimplemented: is_tx_processed");
    }
}
