#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype, panic_with_error, contracterror,
    symbol_short, Address, Env, Symbol, BytesN,
    token::{Client as TokenClient},
};

#[contracttype]
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

#[contractimpl]
impl EscrowContract {

    pub fn lock_funds(
        env: Env,
        escrow_id: BytesN<32>,
        buyer: Address,
        seller: Address,
        token: Address,
        amount: i128,
        _timeout_ledger: u32,           // prefixed with _ since not used yet
        memo: BytesN<32>,
    ) {
        buyer.require_auth();

        if amount <= 0 {
            panic_with_error!(env, EscrowError::InvalidAmount);
        }

        let key = escrow_key(&escrow_id);

        if env.storage().persistent().has(&key) {
            panic_with_error!(env, EscrowError::AlreadyLocked);
        }

        let token_client = TokenClient::new(&env, &token);
        token_client.transfer(&buyer, &env.current_contract_address(), &amount);

        let escrow = Escrow {
            buyer: buyer.clone(),
            seller: seller.clone(),
            arbitrator: Option::None,
            token,
            amount,
            state: EscrowState::Locked,
            memo,
            created_at: env.ledger().timestamp(),
        };

        env.storage().persistent().set(&key, &escrow);

        env.events().publish(
            (symbol_short!("escrow"), symbol_short!("locked")),
            (escrow_id, buyer, seller, amount)
        );
    }

    pub fn release_funds(
        env: Env,
        escrow_id: BytesN<32>,
        caller: Address,
    ) {
        caller.require_auth();

        let key = escrow_key(&escrow_id);
        let mut escrow: Escrow = env.storage().persistent().get(&key)
            .unwrap_or_else(|| panic_with_error!(env, EscrowError::NotLocked));

        if escrow.state != EscrowState::Locked {
            panic_with_error!(env, EscrowError::InvalidState);
        }

        if caller != escrow.seller {
            if let Some(arb) = &escrow.arbitrator {
                if caller != *arb {
                    panic_with_error!(env, EscrowError::NotAuthorized);
                }
            } else {
                panic_with_error!(env, EscrowError::NotAuthorized);
            }
        }

        let token_client = TokenClient::new(&env, &escrow.token);
        token_client.transfer(
            &env.current_contract_address(),
            &escrow.seller,
            &escrow.amount,
        );

        escrow.state = EscrowState::Released;
        env.storage().persistent().set(&key, &escrow);

        env.events().publish(
            (symbol_short!("escrow"), symbol_short!("released")),
            (escrow_id, caller, escrow.seller, escrow.amount)
        );
    }

    pub fn refund_funds(
        env: Env,
        escrow_id: BytesN<32>,
        caller: Address,
    ) {
        caller.require_auth();

        let key = escrow_key(&escrow_id);
        let mut escrow: Escrow = env.storage().persistent().get(&key)
            .unwrap_or_else(|| panic_with_error!(env, EscrowError::NotLocked));

        if escrow.state != EscrowState::Locked {
            panic_with_error!(env, EscrowError::InvalidState);
        }

        let is_timeout = env.ledger().timestamp() >= escrow.created_at + 7 * 24 * 60 * 60;
        let is_authorized = 
            caller == escrow.buyer ||
            escrow.arbitrator.as_ref().map_or(false, |a| *a == caller);

        if !is_authorized && !is_timeout {
            panic_with_error!(env, EscrowError::NotAuthorized);
        }

        let token_client = TokenClient::new(&env, &escrow.token);
        token_client.transfer(
            &env.current_contract_address(),
            &escrow.buyer,
            &escrow.amount,
        );

        escrow.state = EscrowState::Refunded;
        env.storage().persistent().set(&key, &escrow);

        env.events().publish(
            (symbol_short!("escrow"), symbol_short!("refunded")),
            (escrow_id, caller, escrow.buyer, escrow.amount)
        );
    }

    pub fn get_escrow(env: Env, escrow_id: BytesN<32>) -> Escrow {
        let key = escrow_key(&escrow_id);
        env.storage().persistent()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(env, EscrowError::NotLocked))
    }

    pub fn is_locked(env: Env, escrow_id: BytesN<32>) -> bool {
        let key = escrow_key(&escrow_id);
        match env.storage().persistent().get::<_, Escrow>(&key) {
            Some(escrow) => escrow.state == EscrowState::Locked,
            None => false,
        }
    }
}

// Helpers

fn escrow_key(id: &BytesN<32>) -> (Symbol, BytesN<32>) {
    (symbol_short!("escrow"), id.clone())
}