#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype, panic_with_error, contracterror,
    symbol_short, Address, Env, Symbol, BytesN,
    token::{Client as TokenClient},
};

// ─── Reentrancy Guard ────────────────────────────────────────────────────────
//
// A contract-wide mutex stored in instance storage.  Instance storage is the
// cheapest persistent store and is always loaded with the contract, so reads
// and writes add minimal overhead (no extra ledger-entry fetch).
//
// Usage pattern (mirrors OpenZeppelin's ReentrancyGuard):
//   1. Call `reentrancy_guard_enter(&env)` at the top of every state-changing
//      function.  It panics with `Reentrant` if the lock is already held.
//   2. Perform all work (including external token calls).
//   3. Call `reentrancy_guard_exit(&env)` before returning.
//
// Because Soroban executes contracts atomically within a single transaction,
// the lock is automatically cleared at the end of each top-level invocation.
// The explicit exit call is still required so that the storage slot is reset
// for any subsequent calls within the same transaction (e.g. batched ops).

fn reentrancy_guard_enter(env: &Env) {
    let key = symbol_short!("re_lock");
    if env.storage().instance().get::<Symbol, bool>(&key).unwrap_or(false) {
        panic_with_error!(env, EscrowError::Reentrant);
    }
    env.storage().instance().set(&key, &true);
}

fn reentrancy_guard_exit(env: &Env) {
    let key = symbol_short!("re_lock");
    env.storage().instance().set(&key, &false);
}

// ─── Data Types ──────────────────────────────────────────────────────────────

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
    /// Reentrancy detected – a state-changing call is already in progress.
    Reentrant = 11,
}

// ─── Contract ────────────────────────────────────────────────────────────────

#[contract]
pub struct EscrowContract;

#[contractimpl]
impl EscrowContract {

    /// Lock funds into escrow.
    ///
    /// The buyer transfers `amount` tokens to this contract.  The escrow is
    /// identified by the caller-supplied `escrow_id`; duplicate IDs are
    /// rejected.
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
        // ── Reentrancy guard ──────────────────────────────────────────────
        reentrancy_guard_enter(&env);

        buyer.require_auth();

        if amount <= 0 {
            reentrancy_guard_exit(&env);
            panic_with_error!(env, EscrowError::InvalidAmount);
        }

        let key = escrow_key(&escrow_id);

        if env.storage().persistent().has(&key) {
            reentrancy_guard_exit(&env);
            panic_with_error!(env, EscrowError::AlreadyLocked);
        }

        // ── Checks-Effects-Interactions ───────────────────────────────────
        // Write state BEFORE the external token call so that any reentrant
        // invocation of this contract sees the escrow as already existing.
        let escrow = Escrow {
            buyer: buyer.clone(),
            seller: seller.clone(),
            arbitrator: Option::None,
            token: token.clone(),
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

        // External call last.
        let token_client = TokenClient::new(&env, &token);
        token_client.transfer(&buyer, &env.current_contract_address(), &amount);

        env.events().publish(
            (symbol_short!("escrow"), symbol_short!("locked")),
            (escrow_id, buyer, seller, amount)
        );

        reentrancy_guard_exit(&env);
    }

    /// Release escrowed funds to the seller.
    ///
    /// Only the seller or a designated arbitrator may call this.
    pub fn release_funds(
        env: Env,
        escrow_id: BytesN<32>,
        caller: Address,
    ) {
        // ── Reentrancy guard ──────────────────────────────────────────────
        reentrancy_guard_enter(&env);

        caller.require_auth();

        let key = escrow_key(&escrow_id);
        let mut escrow: Escrow = env.storage().persistent().get(&key)
            .unwrap_or_else(|| panic_with_error!(env, EscrowError::NotLocked));

        if escrow.state != EscrowState::Locked {
            reentrancy_guard_exit(&env);
            panic_with_error!(env, EscrowError::InvalidState);
        }

        if caller != escrow.seller {
            if let Some(arb) = &escrow.arbitrator {
                if caller != *arb {
                    reentrancy_guard_exit(&env);
                    panic_with_error!(env, EscrowError::NotAuthorized);
                }
            } else {
                reentrancy_guard_exit(&env);
                panic_with_error!(env, EscrowError::NotAuthorized);
            }
        }

        // ── Checks-Effects-Interactions ───────────────────────────────────
        // Persist the new state BEFORE the external token transfer.
        escrow.state = EscrowState::Released;
        env.storage().persistent().set(&key, &escrow);

        let token_client = TokenClient::new(&env, &escrow.token);
        token_client.transfer(
            &env.current_contract_address(),
            &escrow.seller,
            &escrow.amount,
        );

        env.events().publish(
            (symbol_short!("escrow"), symbol_short!("released")),
            (escrow_id, caller, escrow.seller, escrow.amount)
        );

        reentrancy_guard_exit(&env);
    }

    /// Refund escrowed funds to the buyer.
    ///
    /// The buyer may refund at any time.  Anyone may trigger a refund once the
    /// 7-day timeout has elapsed.  An arbitrator (if set) may also refund.
    pub fn refund_funds(
        env: Env,
        escrow_id: BytesN<32>,
        caller: Address,
    ) {
        // ── Reentrancy guard ──────────────────────────────────────────────
        reentrancy_guard_enter(&env);

        caller.require_auth();

        let key = escrow_key(&escrow_id);
        let mut escrow: Escrow = env.storage().persistent().get(&key)
            .unwrap_or_else(|| panic_with_error!(env, EscrowError::NotLocked));

        if escrow.state != EscrowState::Locked {
            reentrancy_guard_exit(&env);
            panic_with_error!(env, EscrowError::InvalidState);
        }

        let is_timeout = env.ledger().timestamp() >= escrow.created_at + 7 * 24 * 60 * 60;
        let is_authorized =
            caller == escrow.buyer ||
            escrow.arbitrator.as_ref().map_or(false, |a| *a == caller);

        if !is_authorized && !is_timeout {
            reentrancy_guard_exit(&env);
            panic_with_error!(env, EscrowError::NotAuthorized);
        }

        // ── Checks-Effects-Interactions ───────────────────────────────────
        // Persist the new state BEFORE the external token transfer.
        escrow.state = EscrowState::Refunded;
        env.storage().persistent().set(&key, &escrow);

        let token_client = TokenClient::new(&env, &escrow.token);
        token_client.transfer(
            &env.current_contract_address(),
            &escrow.buyer,
            &escrow.amount,
        );

        env.events().publish(
            (symbol_short!("escrow"), symbol_short!("refunded")),
            (escrow_id, caller, escrow.buyer, escrow.amount)
        );

        reentrancy_guard_exit(&env);
    }

    /// Initiate a dispute for an escrow.
    ///
    /// Either the buyer or seller may open a dispute while the escrow is in
    /// the `Locked` state.  A `resolver` address is recorded for off-chain
    /// reference; on-chain resolution is handled via `vote_resolution`.
    pub fn initiate_dispute(
        env: Env,
        escrow_id: BytesN<32>,
        caller: Address,
        resolver: Address,
    ) {
        // ── Reentrancy guard ──────────────────────────────────────────────
        reentrancy_guard_enter(&env);

        caller.require_auth();

        let key = escrow_key(&escrow_id);
        let mut escrow: Escrow = env.storage().persistent().get(&key)
            .unwrap_or_else(|| panic_with_error!(env, EscrowError::NotLocked));

        if escrow.state != EscrowState::Locked {
            reentrancy_guard_exit(&env);
            panic_with_error!(env, EscrowError::InvalidState);
        }

        if caller != escrow.buyer && caller != escrow.seller {
            reentrancy_guard_exit(&env);
            panic_with_error!(env, EscrowError::NotAuthorized);
        }

        escrow.state = EscrowState::Disputed;
        escrow.dispute_resolver = Some(resolver.clone());
        env.storage().persistent().set(&key, &escrow);

        env.events().publish(
            (symbol_short!("escrow"), symbol_short!("disputed")),
            (escrow_id, caller, resolver)
        );

        reentrancy_guard_exit(&env);
    }

    /// Cast a vote to resolve a dispute.
    ///
    /// Both buyer and seller must vote.  When their votes agree the funds are
    /// transferred automatically.  If they disagree an off-chain arbitrator
    /// must intervene (future work).
    pub fn vote_resolution(
        env: Env,
        escrow_id: BytesN<32>,
        caller: Address,
        resolve_to_seller: bool,
    ) {
        // ── Reentrancy guard ──────────────────────────────────────────────
        reentrancy_guard_enter(&env);

        caller.require_auth();

        let key = escrow_key(&escrow_id);
        let mut escrow: Escrow = env.storage().persistent().get(&key)
            .unwrap_or_else(|| panic_with_error!(env, EscrowError::NotLocked));

        if escrow.state != EscrowState::Disputed {
            reentrancy_guard_exit(&env);
            panic_with_error!(env, EscrowError::NotDisputed);
        }

        if caller == escrow.buyer {
            if escrow.buyer_vote.is_some() {
                reentrancy_guard_exit(&env);
                panic_with_error!(env, EscrowError::VoteAlreadyCast);
            }
            escrow.buyer_vote = Some(resolve_to_seller);
        } else if caller == escrow.seller {
            if escrow.seller_vote.is_some() {
                reentrancy_guard_exit(&env);
                panic_with_error!(env, EscrowError::VoteAlreadyCast);
            }
            escrow.seller_vote = Some(resolve_to_seller);
        } else {
            reentrancy_guard_exit(&env);
            panic_with_error!(env, EscrowError::NotAuthorized);
        }

        // ── Checks-Effects-Interactions ───────────────────────────────────
        // Determine final state before any external call.
        if let (Some(buyer_vote), Some(seller_vote)) = (escrow.buyer_vote, escrow.seller_vote) {
            if buyer_vote == seller_vote {
                // Agreement reached – update state first, then transfer.
                if resolve_to_seller {
                    escrow.state = EscrowState::Released;
                } else {
                    escrow.state = EscrowState::Refunded;
                }
            }
        }

        // Persist updated escrow (including any state change) before the
        // external token call.
        env.storage().persistent().set(&key, &escrow);

        // External token transfer only after state is committed.
        if escrow.state == EscrowState::Released {
            let token_client = TokenClient::new(&env, &escrow.token);
            token_client.transfer(
                &env.current_contract_address(),
                &escrow.seller,
                &escrow.amount,
            );
        } else if escrow.state == EscrowState::Refunded {
            let token_client = TokenClient::new(&env, &escrow.token);
            token_client.transfer(
                &env.current_contract_address(),
                &escrow.buyer,
                &escrow.amount,
            );
        }

        env.events().publish(
            (symbol_short!("escrow"), symbol_short!("vote")),
            (escrow_id, caller, resolve_to_seller)
        );

        reentrancy_guard_exit(&env);
    }

    // ── Read-only helpers ─────────────────────────────────────────────────────

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

    /// Get escrow state.
    pub fn get_state(env: Env, escrow_id: BytesN<32>) -> EscrowState {
        let key = escrow_key(&escrow_id);
        env.storage().persistent()
            .get::<_, Escrow>(&key)
            .unwrap_or_else(|| panic_with_error!(env, EscrowError::NotLocked))
            .state
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn escrow_key(id: &BytesN<32>) -> (Symbol, BytesN<32>) {
    (symbol_short!("escrow"), id.clone())
}

mod test;
