#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype, panic_with_error, contracterror,
    symbol_short, Address, Env, Symbol, BytesN,
    token::{Client as TokenClient},
};

#[contracttype]
#[derive(Clone)]
pub struct Escrow {
    pub buyer: Address,
    pub seller: Address,
    pub arbitrator: Option<Address>,
    pub token: Address,
    pub amount: i128,
    pub state: EscrowState,
    pub memo: BytesN<32>,
    pub created_at: u64,
    pub timeout_ledger: u32,
    pub dispute_resolver: Option<Address>,
    pub buyer_vote: Option<bool>,
    pub seller_vote: Option<bool>,
}

#[contracttype]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum EscrowState {
    Locked = 1,
    Released = 2,
    Refunded = 3,
    Disputed = 4,
}

#[contracterror]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum EscrowError {
    NotAuthorized = 1,
    AlreadyLocked = 2,
    NotLocked = 3,
    AlreadyFinalized = 4,
    InvalidAmount = 5,
    InvalidState = 6,
    InvalidArbitrator = 7,
    TimeoutNotReached = 8,
    NotDisputed = 9,
    VoteAlreadyCast = 10,
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
        timeout_ledger: u32,
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
            timeout_ledger,
            dispute_resolver: Option::None,
            buyer_vote: Option::None,
            seller_vote: Option::None,
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

    /// Initiate a dispute for an escrow
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `escrow_id` - The escrow ID to dispute
    /// * `caller` - The address initiating the dispute (buyer or seller)
    /// * `resolver` - The arbitrator/resolver for the dispute
    pub fn initiate_dispute(
        env: Env,
        escrow_id: BytesN<32>,
        caller: Address,
        resolver: Address,
    ) {
        caller.require_auth();

        let key = escrow_key(&escrow_id);
        let mut escrow: Escrow = env.storage().persistent().get(&key)
            .unwrap_or_else(|| panic_with_error!(env, EscrowError::NotLocked));

        if escrow.state != EscrowState::Locked {
            panic_with_error!(env, EscrowError::InvalidState);
        }

        if caller != escrow.buyer && caller != escrow.seller {
            panic_with_error!(env, EscrowError::NotAuthorized);
        }

        escrow.state = EscrowState::Disputed;
        escrow.dispute_resolver = Some(resolver.clone());

        env.storage().persistent().set(&key, &escrow);

        env.events().publish(
            (symbol_short!("escrow"), symbol_short!("disputed")),
            (escrow_id, caller, resolver)
        );
    }

    /// Cast a vote to resolve a dispute
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `escrow_id` - The escrow ID
    /// * `caller` - The address voting (must be buyer or seller)
    /// * `resolve_to_seller` - true to release to seller, false to refund buyer
    pub fn vote_resolution(
        env: Env,
        escrow_id: BytesN<32>,
        caller: Address,
        resolve_to_seller: bool,
    ) {
        caller.require_auth();

        let key = escrow_key(&escrow_id);
        let mut escrow: Escrow = env.storage().persistent().get(&key)
            .unwrap_or_else(|| panic_with_error!(env, EscrowError::NotLocked));

        if escrow.state != EscrowState::Disputed {
            panic_with_error!(env, EscrowError::NotDisputed);
        }

        if caller == escrow.buyer {
            if escrow.buyer_vote.is_some() {
                panic_with_error!(env, EscrowError::VoteAlreadyCast);
            }
            escrow.buyer_vote = Some(resolve_to_seller);
        } else if caller == escrow.seller {
            if escrow.seller_vote.is_some() {
                panic_with_error!(env, EscrowError::VoteAlreadyCast);
            }
            escrow.seller_vote = Some(resolve_to_seller);
        } else {
            panic_with_error!(env, EscrowError::NotAuthorized);
        }

        // Check if both have voted
        if let (Some(buyer_vote), Some(seller_vote)) = (escrow.buyer_vote, escrow.seller_vote) {
            let token_client = TokenClient::new(&env, &escrow.token);

            if buyer_vote == seller_vote {
                // Agreement reached
                if resolve_to_seller {
                    token_client.transfer(
                        &env.current_contract_address(),
                        &escrow.seller,
                        &escrow.amount,
                    );
                    escrow.state = EscrowState::Released;
                } else {
                    token_client.transfer(
                        &env.current_contract_address(),
                        &escrow.buyer,
                        &escrow.amount,
                    );
                    escrow.state = EscrowState::Refunded;
                }
            }
        }

        env.storage().persistent().set(&key, &escrow);

        env.events().publish(
            (symbol_short!("escrow"), symbol_short!("vote")),
            (escrow_id, caller, resolve_to_seller)
        );
    }

    /// Get escrow state
    pub fn get_state(env: Env, escrow_id: BytesN<32>) -> EscrowState {
        let key = escrow_key(&escrow_id);
        env.storage().persistent()
            .get::<_, Escrow>(&key)
            .unwrap_or_else(|| panic_with_error!(env, EscrowError::NotLocked))
            .state
    }
}

// Helpers

fn escrow_key(id: &BytesN<32>) -> (Symbol, BytesN<32>) {
    (symbol_short!("escrow"), id.clone())
}

mod test;