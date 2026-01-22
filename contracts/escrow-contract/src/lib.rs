#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype, panic_with_error, contracterror,
    symbol_short, Address, Env, Map, Symbol, BytesN,
    token::{Client as TokenClient},
};

const ESCROW_PREFIX: Symbol = symbol_short!("escrow");

#[contracterror]
#[derive(Copy, Clone, PartialEq, Eq)]
pub enum EscrowState {
    Locked = 1,
    Released = 2,
    Refunded = 3,
    Disputed = 4,   // optional – can be used later
}

#[contracttype]
#[derive(Clone)]
pub struct Escrow {
    pub buyer: Address,
    pub seller: Address,
    pub arbitrator: Option<Address>,   // optional trusted third party
    pub token: Address,
    pub amount: i128,
    pub state: EscrowState,
    pub memo: BytesN<32>,              // optional short identifier / order id
    pub created_at: u64,
}

#[contracterror]
#[derive(Copy, Clone, PartialEq, Eq)]
pub enum EscrowError {
    NotAuthorized = 1,
    AlreadyLocked = 2,
    NotLocked = 3,
    AlreadyFinalized = 4,
    InvalidAmount = 5,
    InvalidState = 6,
    InvalidArbitrator = 7,
    TimeoutNotReached = 8,
}

#[contract]
pub struct EscrowContract;

// #[contractimpl]
// impl EscrowContract {

    // Buyer locks funds into escrow
    // Anyone can call, but must be the buyer and must authorize
    // pub fn lock_funds(
    //     env: Env,
    //     escrow_id: BytesN<32>,
    //     buyer: Address,
    //     seller: Address,
    //     token: Address,
    //     amount: i128,
    //     timeout_ledger: u32,           // optional: after how many ledgers buyer can refund
    //     memo: BytesN<32>,
    // ) {
    //     buyer.require_auth();

    //     if amount <= 0 {
    //         panic_with_error!(env, EscrowError::InvalidAmount);
    //     }

    //     let key = escrow_key(&escrow_id);
    //     if env.storage().persistent().has(&key) {
    //         panic_with_error!(env, EscrowError::AlreadyLocked);
    //     }

    //     // Transfer tokens from buyer → contract
    //     let token_client = TokenClient::new(&env, &token);
    //     token_client.transfer(&buyer, &env.current_contract_address(), &amount);

    //     let escrow = Escrow {
    //         buyer: buyer.clone(),
    //         seller: seller.clone(),
    //         arbitrator: Option::None,           // can be set later or passed in extension
    //         token,
    //         amount,
    //         state: EscrowState::Locked,
    //         memo,
    //         created_at: env.ledger().timestamp(),
    //     };

    //     env.storage().persistent().set(&key, &escrow);

    //     // Optional: emit event
    //     env.events().publish(
    //         (symbol_short!("escrow"), symbol_short!("locked")),
    //         (escrow_id, buyer, seller, amount)
    //     );
    // }

    // Seller (or arbitrator) releases funds to seller
    // pub fn release_funds(
    //     env: Env,
    //     escrow_id: BytesN<32>,
    //     caller: Address,
    // ) {
    //     caller.require_auth();

    //     let key = escrow_key(&escrow_id);
    //     let mut escrow: Escrow = env.storage().persistent().get(&key)
    //         .unwrap_or_else(|| panic_with_error!(env, EscrowError::NotLocked));

    //     if escrow.state != EscrowState::Locked {
    //         panic_with_error!(env, EscrowError::InvalidState);
    //     }

    //     // Only seller or arbitrator can release
    //     if caller != escrow.seller {
    //         if let Some(arb) = &escrow.arbitrator {
    //             if caller != *arb {
    //                 panic_with_error!(env, EscrowError::NotAuthorized);
    //             }
    //         } else {
    //             panic_with_error!(env, EscrowError::NotAuthorized);
    //         }
    //     }

    //     let token_client = TokenClient::new(&env, &escrow.token);
    //     token_client.transfer(
    //         &env.current_contract_address(),
    //         &escrow.seller,
    //         &escrow.amount,
    //     );

    //     escrow.state = EscrowState::Released;
    //     env.storage().persistent().set(&key, &escrow);

    //     env.events().publish(
    //         (symbol_short!("escrow"), symbol_short!("released")),
    //         (escrow_id, caller, escrow.seller, escrow.amount)
    //     );
    // }

    // Buyer can refund if timeout passed (or arbitrator decides)
    // pub fn refund_funds(
    //     env: Env,
    //     escrow_id: BytesN<32>,
    //     caller: Address,
    // ) {
    //     caller.require_auth();

    //     let key = escrow_key(&escrow_id);
    //     let mut escrow: Escrow = env.storage().persistent().get(&key)
    //         .unwrap_or_else(|| panic_with_error!(env, EscrowError::NotLocked));

    //     if escrow.state != EscrowState::Locked {
    //         panic_with_error!(env, EscrowError::InvalidState);
    //     }

    //     let is_timeout = env.ledger().timestamp() >= escrow.created_at + 7 * 24 * 60 * 60; // example: 7 days
    //     let is_authorized = 
    //         caller == escrow.buyer ||
    //         escrow.arbitrator.as_ref().map_or(false, |a| *a == caller);

    //     if !is_authorized && !is_timeout {
    //         panic_with_error!(env, EscrowError::NotAuthorized);
    //     }

    //     let token_client = TokenClient::new(&env, &escrow.token);
    //     token_client.transfer(
    //         &env.current_contract_address(),
    //         &escrow.buyer,
    //         &escrow.amount,
    //     );

    //     escrow.state = EscrowState::Refunded;
    //     env.storage().persistent().set(&key, &escrow);

    //     env.events().publish(
    //         (symbol_short!("escrow"), symbol_short!("refunded")),
    //         (escrow_id, caller, escrow.buyer, escrow.amount)
    //     );
    // }

    // ────────────────────────────────────────────────
    // View functions
    // ────────────────────────────────────────────────

    // pub fn get_escrow(env: Env, escrow_id: BytesN<32>) -> Escrow {
    //     let key = escrow_key(&escrow_id);
    //     env.storage().persistent()
    //         .get(&key)
    //         .unwrap_or_else(|| panic_with_error!(env, EscrowError::NotLocked))
    // }

    // pub fn is_locked(env: Env, escrow_id: BytesN<32>) -> bool {
    //     if !env.storage().persistent().has(&escrow_key(&escrow_id)) {
    //         return false;
    //     }
    //     let escrow: Escrow = env.storage().persistent().get(&escrow_key(&escrow_id));
    //     escrow.state == EscrowState::Locked
    // }
// }

// Helpers
// fn escrow_key(id: &BytesN<32>) -> Symbol {
//     Symbol::new(&Env::default(), &format!("{}{}", ESCROW_PREFIX, id.to_array().iter().map(|b| format!("{:02x}", b)).collect::<String>()))
//     // Alternative (simpler but longer keys):
//     // Symbol::new(&env, "escrow_").concat(&id.to_bytes())
// }