#![no_std]
#![allow(dead_code, unused_variables, unused_imports, unexpected_cfgs)]
use soroban_sdk::{contract, contractimpl, Address, BytesN, Env};

#[contract]
pub struct AllbridgeReceiverContract;

#[contractimpl]
impl AllbridgeReceiverContract {
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
}
