#![no_std]

//! # Cross-Chain Bridge Contract
//!
//! Facilitates asset transfers between Stellar and EVM-compatible chains
//! (Ethereum, Polygon).
//!
//! ## Architecture
//!
//! The bridge uses a **lock-and-mint / burn-and-release** model operated by a
//! set of whitelisted relayers.  All security-sensitive state changes require
//! relayer authorisation; users only need to sign their own lock transactions.
//!
//! ### Inbound flow  (EVM → Stellar)
//! 1. User locks tokens on the EVM side (handled off-chain / by EVM contract).
//! 2. A relayer calls `claim_inbound` with the EVM tx hash, source chain,
//!    amount, recipient, and a unique nonce.
//! 3. The contract validates the claim (chain allowed, relayer whitelisted,
//!    nonce not replayed, amount in range) and transfers tokens to the
//!    recipient from its own balance.
//!
//! ### Outbound flow  (Stellar → EVM)
//! 1. User calls `lock_outbound`, specifying destination chain, EVM address,
//!    and amount.  Tokens are transferred from the user to this contract.
//! 2. A relayer calls `confirm_outbound` once the EVM-side mint is confirmed.
//! 3. If the relayer never confirms within `OUTBOUND_TIMEOUT_LEDGERS`, the
//!    user may call `refund_outbound` to recover their tokens.
//!
//! ## Security measures
//! - Reentrancy guard (instance-storage lock).
//! - Nonce replay protection for inbound claims.
//! - Chain allowlist (only `ethereum` and `polygon` are supported).
//! - Relayer whitelist — only admin-approved addresses may submit claims or
//!   confirmations.
//! - Per-transfer min/max amount limits.
//! - Emergency pause (circuit breaker) — admin can halt all operations.
//! - Timeout-based refunds for stuck outbound transfers.
//! - Checks-Effects-Interactions ordering throughout.

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype,
    symbol_short, token::Client as TokenClient,
    Address, Bytes, Env, Symbol, Vec,
};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Ledgers before an unconfirmed outbound transfer can be refunded
/// (~7 days at 5 s/ledger = 120 960 ledgers).
pub const OUTBOUND_TIMEOUT_LEDGERS: u32 = 120_960;

/// Supported source/destination chain identifiers.
pub const CHAIN_ETHEREUM: &str = "ethereum";
pub const CHAIN_POLYGON: &str = "polygon";

// ---------------------------------------------------------------------------
// Storage keys
// ---------------------------------------------------------------------------

const KEY_ADMIN: Symbol = symbol_short!("admin");
const KEY_TOKEN: Symbol = symbol_short!("token");
const KEY_PAUSED: Symbol = symbol_short!("paused");
const KEY_TIMEOUT: Symbol = symbol_short!("timeout");
const KEY_LOCKED: Symbol = symbol_short!("locked"); // reentrancy guard
const KEY_MIN_AMT: Symbol = symbol_short!("min_amt");
const KEY_MAX_AMT: Symbol = symbol_short!("max_amt");
const KEY_NEXT_ID: Symbol = symbol_short!("next_id");
const KEY_TOT_IN: Symbol = symbol_short!("tot_in");
const KEY_TOT_OUT: Symbol = symbol_short!("tot_out");

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Supported external chains.
#[contracttype]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Chain {
    Ethereum = 1,
    Polygon = 2,
}

/// Status of an outbound transfer.
#[contracttype]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum OutboundStatus {
    Pending = 1,
    Confirmed = 2,
    Refunded = 3,
}

/// An outbound (Stellar → EVM) transfer record.
#[contracttype]
#[derive(Clone)]
pub struct OutboundTransfer {
    pub id: u64,
    pub sender: Address,
    pub dest_chain: Chain,
    /// EVM destination address encoded as raw bytes (20 bytes for ETH/Polygon).
    pub dest_address: Bytes,
    pub amount: i128,
    pub created_ledger: u32,
    pub timeout_ledger: u32,
    pub status: OutboundStatus,
}

/// Storage key variants for per-transfer and per-nonce data.
#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Relayer(Address),       // relayer whitelist entry
    UsedNonce(Bytes),       // inbound replay-protection nonce
    Outbound(u64),          // outbound transfer record
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[contracterror]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum BridgeError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    ContractPaused = 4,
    Reentrant = 5,
    UnsupportedChain = 6,
    AmountTooLow = 7,
    AmountTooHigh = 8,
    NonceAlreadyUsed = 9,
    TransferNotFound = 10,
    TransferNotPending = 11,
    TimeoutNotReached = 12,
    TimeoutAlreadyExpired = 13,
    InvalidDestAddress = 14,
    RelayerAlreadyAdded = 15,
    RelayerNotFound = 16,
    InvalidAmountLimits = 17,
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn require_not_paused(env: &Env) {
    if env.storage().instance().get::<Symbol, bool>(&KEY_PAUSED).unwrap_or(false) {
        soroban_sdk::panic_with_error!(env, BridgeError::ContractPaused);
    }
}

fn require_admin(env: &Env) -> Address {
    let admin: Address = env
        .storage()
        .instance()
        .get(&KEY_ADMIN)
        .unwrap_or_else(|| soroban_sdk::panic_with_error!(env, BridgeError::NotInitialized));
    admin.require_auth();
    admin
}

fn require_relayer(env: &Env, relayer: &Address) {
    relayer.require_auth();
    if !env
        .storage()
        .persistent()
        .get::<DataKey, bool>(&DataKey::Relayer(relayer.clone()))
        .unwrap_or(false)
    {
        soroban_sdk::panic_with_error!(env, BridgeError::Unauthorized);
    }
}

fn require_initialized(env: &Env) {
    if !env.storage().instance().has(&KEY_ADMIN) {
        soroban_sdk::panic_with_error!(env, BridgeError::NotInitialized);
    }
}

fn parse_chain(env: &Env, chain: &Symbol) -> Chain {
    if *chain == Symbol::new(env, CHAIN_ETHEREUM) {
        Chain::Ethereum
    } else if *chain == Symbol::new(env, CHAIN_POLYGON) {
        Chain::Polygon
    } else {
        soroban_sdk::panic_with_error!(env, BridgeError::UnsupportedChain);
    }
}

fn validate_amount(env: &Env, amount: i128) {
    let min: i128 = env.storage().instance().get(&KEY_MIN_AMT).unwrap_or(1);
    let max: i128 = env.storage().instance().get(&KEY_MAX_AMT).unwrap_or(i128::MAX);
    if amount < min {
        soroban_sdk::panic_with_error!(env, BridgeError::AmountTooLow);
    }
    if amount > max {
        soroban_sdk::panic_with_error!(env, BridgeError::AmountTooHigh);
    }
}

/// EVM addresses are exactly 20 bytes.
fn validate_dest_address(env: &Env, addr: &Bytes) {
    if addr.len() != 20 {
        soroban_sdk::panic_with_error!(env, BridgeError::InvalidDestAddress);
    }
}

fn reentrancy_enter(env: &Env) {
    if env.storage().instance().get::<Symbol, bool>(&KEY_LOCKED).unwrap_or(false) {
        soroban_sdk::panic_with_error!(env, BridgeError::Reentrant);
    }
    env.storage().instance().set(&KEY_LOCKED, &true);
}

fn reentrancy_exit(env: &Env) {
    env.storage().instance().set(&KEY_LOCKED, &false);
}

fn next_outbound_id(env: &Env) -> u64 {
    let id: u64 = env.storage().instance().get(&KEY_NEXT_ID).unwrap_or(0);
    env.storage().instance().set(&KEY_NEXT_ID, &(id + 1));
    id
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct CrossChainBridge;

#[contractimpl]
impl CrossChainBridge {

    // -----------------------------------------------------------------------
    // Initialisation
    // -----------------------------------------------------------------------

    /// Initialise the bridge.
    ///
    /// * `token`      – the Stellar token this bridge locks/releases
    /// * `min_amount` – minimum transfer amount (inclusive)
    /// * `max_amount` – maximum transfer amount (inclusive)
    pub fn initialize(
        env: Env,
        admin: Address,
        token: Address,
        min_amount: i128,
        max_amount: i128,
        timeout_ledgers: u32,
    ) {
        if env.storage().instance().has(&KEY_ADMIN) {
            soroban_sdk::panic_with_error!(env, BridgeError::AlreadyInitialized);
        }
        if min_amount <= 0 || max_amount < min_amount {
            soroban_sdk::panic_with_error!(env, BridgeError::InvalidAmountLimits);
        }

        admin.require_auth();

        env.storage().instance().set(&KEY_ADMIN, &admin);
        env.storage().instance().set(&KEY_TOKEN, &token);
        env.storage().instance().set(&KEY_MIN_AMT, &min_amount);
        env.storage().instance().set(&KEY_MAX_AMT, &max_amount);
        env.storage().instance().set(&KEY_PAUSED, &false);
        env.storage().instance().set(&KEY_LOCKED, &false);
        env.storage().instance().set(&KEY_NEXT_ID, &0u64);
        env.storage().instance().set(&KEY_TOT_IN, &0i128);
        env.storage().instance().set(&KEY_TOT_OUT, &0i128);
        env.storage().instance().set(&KEY_TIMEOUT, &timeout_ledgers);
        // Extend instance TTL to survive long timeout periods in tests and production.
        env.storage().instance().extend_ttl(200_000, 6_307_200);
    }

    // -----------------------------------------------------------------------
    // Relayer management (admin only)
    // -----------------------------------------------------------------------

    pub fn add_relayer(env: Env, relayer: Address) {
        require_admin(&env);
        let key = DataKey::Relayer(relayer.clone());
        if env.storage().persistent().get::<DataKey, bool>(&key).unwrap_or(false) {
            soroban_sdk::panic_with_error!(env, BridgeError::RelayerAlreadyAdded);
        }
        env.storage().persistent().set(&key, &true);
        env.events().publish(
            (symbol_short!("bridge"), symbol_short!("rly_add")),
            relayer,
        );
    }

    pub fn remove_relayer(env: Env, relayer: Address) {
        require_admin(&env);
        let key = DataKey::Relayer(relayer.clone());
        if !env.storage().persistent().get::<DataKey, bool>(&key).unwrap_or(false) {
            soroban_sdk::panic_with_error!(env, BridgeError::RelayerNotFound);
        }
        env.storage().persistent().remove(&key);
        env.events().publish(
            (symbol_short!("bridge"), symbol_short!("rly_rm")),
            relayer,
        );
    }

    // -----------------------------------------------------------------------
    // Circuit breaker (admin only)
    // -----------------------------------------------------------------------

    pub fn pause(env: Env) {
        require_admin(&env);
        env.storage().instance().set(&KEY_PAUSED, &true);
        env.events().publish(
            (symbol_short!("bridge"), symbol_short!("paused")),
            (),
        );
    }

    pub fn unpause(env: Env) {
        require_admin(&env);
        env.storage().instance().set(&KEY_PAUSED, &false);
        env.events().publish(
            (symbol_short!("bridge"), symbol_short!("unpaused")),
            (),
        );
    }

    // -----------------------------------------------------------------------
    // Amount limit management (admin only)
    // -----------------------------------------------------------------------

    pub fn set_limits(env: Env, min_amount: i128, max_amount: i128) {
        require_admin(&env);
        if min_amount <= 0 || max_amount < min_amount {
            soroban_sdk::panic_with_error!(env, BridgeError::InvalidAmountLimits);
        }
        env.storage().instance().set(&KEY_MIN_AMT, &min_amount);
        env.storage().instance().set(&KEY_MAX_AMT, &max_amount);
        env.events().publish(
            (symbol_short!("bridge"), symbol_short!("limits")),
            (min_amount, max_amount),
        );
    }

    // -----------------------------------------------------------------------
    // Inbound: EVM → Stellar
    // -----------------------------------------------------------------------

    /// Process an inbound transfer from an EVM chain.
    ///
    /// Called by a whitelisted relayer after observing a lock event on the
    /// EVM side.
    ///
    /// * `nonce`      – globally unique identifier for this EVM-side event
    ///                  (e.g. keccak256(tx_hash ++ log_index)), 32 bytes
    /// * `src_chain`  – `"ethereum"` or `"polygon"`
    /// * `evm_tx`     – EVM transaction hash (32 bytes), stored for audit
    /// * `recipient`  – Stellar address to receive tokens
    /// * `amount`     – token amount in base units
    pub fn claim_inbound(
        env: Env,
        relayer: Address,
        nonce: Bytes,
        src_chain: Symbol,
        evm_tx: Bytes,
        recipient: Address,
        amount: i128,
    ) {
        require_not_paused(&env);
        require_initialized(&env);
        require_relayer(&env, &relayer);

        // Validate chain.
        parse_chain(&env, &src_chain);

        // Validate amount.
        validate_amount(&env, amount);

        // Replay protection — nonce must be fresh.
        let nonce_key = DataKey::UsedNonce(nonce.clone());
        if env.storage().persistent().has(&nonce_key) {
            soroban_sdk::panic_with_error!(env, BridgeError::NonceAlreadyUsed);
        }

        // --- Effects (state before token transfer) --------------------------
        reentrancy_enter(&env);

        env.storage().persistent().set(&nonce_key, &true);

        let tot_in: i128 = env.storage().instance().get(&KEY_TOT_IN).unwrap_or(0);
        env.storage().instance().set(&KEY_TOT_IN, &(tot_in + amount));

        // --- Interaction (token transfer) -----------------------------------
        let token: Address = env.storage().instance().get(&KEY_TOKEN).unwrap();
        TokenClient::new(&env, &token)
            .transfer(&env.current_contract_address(), &recipient, &amount);

        reentrancy_exit(&env);

        env.events().publish(
            (symbol_short!("bridge"), symbol_short!("inbound")),
            (nonce, src_chain, evm_tx, recipient, amount),
        );
    }

    // -----------------------------------------------------------------------
    // Outbound: Stellar → EVM
    // -----------------------------------------------------------------------

    /// Lock tokens for an outbound transfer to an EVM chain.
    ///
    /// * `dest_chain`    – `"ethereum"` or `"polygon"`
    /// * `dest_address`  – EVM destination address, exactly 20 bytes
    /// * `amount`        – token amount in base units
    ///
    /// Returns the transfer ID.
    pub fn lock_outbound(
        env: Env,
        sender: Address,
        dest_chain: Symbol,
        dest_address: Bytes,
        amount: i128,
    ) -> u64 {
        require_not_paused(&env);
        require_initialized(&env);

        sender.require_auth();

        let chain = parse_chain(&env, &dest_chain);
        validate_amount(&env, amount);
        validate_dest_address(&env, &dest_address);

        reentrancy_enter(&env);

        let id = next_outbound_id(&env);
        let created = env.ledger().sequence();
        let timeout_ledgers: u32 = env.storage().instance().get(&KEY_TIMEOUT)
            .unwrap_or(OUTBOUND_TIMEOUT_LEDGERS);
        let timeout = created + timeout_ledgers;

        let transfer = OutboundTransfer {
            id,
            sender: sender.clone(),
            dest_chain: chain,
            dest_address: dest_address.clone(),
            amount,
            created_ledger: created,
            timeout_ledger: timeout,
            status: OutboundStatus::Pending,
        };

        // Effects: persist record before token transfer.
        env.storage().persistent().set(&DataKey::Outbound(id), &transfer);
        // Extend TTL well beyond the timeout so the entry survives until refund.
        env.storage().persistent().extend_ttl(
            &DataKey::Outbound(id),
            timeout_ledgers + 1,
            timeout_ledgers * 2 + 1,
        );

        let tot_out: i128 = env.storage().instance().get(&KEY_TOT_OUT).unwrap_or(0);
        env.storage().instance().set(&KEY_TOT_OUT, &(tot_out + amount));

        // Interaction: pull tokens from sender.
        let token: Address = env.storage().instance().get(&KEY_TOKEN).unwrap();
        TokenClient::new(&env, &token)
            .transfer(&sender, &env.current_contract_address(), &amount);

        reentrancy_exit(&env);

        env.events().publish(
            (symbol_short!("bridge"), symbol_short!("locked")),
            (id, sender, dest_chain, dest_address, amount, timeout),
        );

        id
    }

    /// Confirm that an outbound transfer was successfully processed on the
    /// EVM side.  Called by a whitelisted relayer.
    ///
    /// * `evm_tx` – EVM transaction hash of the mint, stored for audit
    pub fn confirm_outbound(env: Env, relayer: Address, transfer_id: u64, evm_tx: Bytes) {
        require_not_paused(&env);
        require_initialized(&env);
        require_relayer(&env, &relayer);
        env.storage().instance().extend_ttl(200_000, 6_307_200);

        let key = DataKey::Outbound(transfer_id);
        let mut transfer: OutboundTransfer = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| soroban_sdk::panic_with_error!(env, BridgeError::TransferNotFound));

        if transfer.status != OutboundStatus::Pending {
            soroban_sdk::panic_with_error!(env, BridgeError::TransferNotPending);
        }

        // Relayer must confirm before timeout.
        if env.ledger().sequence() > transfer.timeout_ledger {
            soroban_sdk::panic_with_error!(env, BridgeError::TimeoutAlreadyExpired);
        }

        transfer.status = OutboundStatus::Confirmed;
        env.storage().persistent().set(&key, &transfer);
        let t: u32 = env.storage().instance().get(&KEY_TIMEOUT).unwrap_or(OUTBOUND_TIMEOUT_LEDGERS);
        env.storage().persistent().extend_ttl(&key, t + 1, t * 2 + 1);

        env.events().publish(
            (symbol_short!("bridge"), symbol_short!("confirmed")),
            (transfer_id, evm_tx, transfer.amount),
        );
    }

    /// Refund a timed-out outbound transfer back to the original sender.
    ///
    /// Anyone may call this once the timeout has elapsed, but only the
    /// original sender receives the refund.
    pub fn refund_outbound(env: Env, transfer_id: u64) {
        require_initialized(&env);
        env.storage().instance().extend_ttl(200_000, 6_307_200);

        let key = DataKey::Outbound(transfer_id);
        let mut transfer: OutboundTransfer = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| soroban_sdk::panic_with_error!(env, BridgeError::TransferNotFound));

        if transfer.status != OutboundStatus::Pending {
            soroban_sdk::panic_with_error!(env, BridgeError::TransferNotPending);
        }

        if env.ledger().sequence() <= transfer.timeout_ledger {
            soroban_sdk::panic_with_error!(env, BridgeError::TimeoutNotReached);
        }

        reentrancy_enter(&env);

        // Effects before interaction.
        transfer.status = OutboundStatus::Refunded;
        env.storage().persistent().set(&key, &transfer);
        let t: u32 = env.storage().instance().get(&KEY_TIMEOUT).unwrap_or(OUTBOUND_TIMEOUT_LEDGERS);
        env.storage().persistent().extend_ttl(&key, t + 1, t * 2 + 1);

        // Interaction: return tokens to original sender.
        let token: Address = env.storage().instance().get(&KEY_TOKEN).unwrap();
        TokenClient::new(&env, &token)
            .transfer(&env.current_contract_address(), &transfer.sender, &transfer.amount);

        reentrancy_exit(&env);

        env.events().publish(
            (symbol_short!("bridge"), symbol_short!("refunded")),
            (transfer_id, transfer.sender, transfer.amount),
        );
    }

    // -----------------------------------------------------------------------
    // Admin: transfer admin
    // -----------------------------------------------------------------------

    pub fn transfer_admin(env: Env, new_admin: Address) {
        require_admin(&env);
        env.storage().instance().set(&KEY_ADMIN, &new_admin);
        env.events().publish(
            (symbol_short!("bridge"), symbol_short!("adm_xfer")),
            new_admin,
        );
    }

    // -----------------------------------------------------------------------
    // Views
    // -----------------------------------------------------------------------

    pub fn get_outbound(env: Env, transfer_id: u64) -> OutboundTransfer {
        env.storage()
            .persistent()
            .get(&DataKey::Outbound(transfer_id))
            .unwrap_or_else(|| soroban_sdk::panic_with_error!(env, BridgeError::TransferNotFound))
    }

    pub fn is_nonce_used(env: Env, nonce: Bytes) -> bool {
        env.storage().persistent().has(&DataKey::UsedNonce(nonce))
    }

    pub fn is_relayer(env: Env, relayer: Address) -> bool {
        env.storage()
            .persistent()
            .get::<DataKey, bool>(&DataKey::Relayer(relayer))
            .unwrap_or(false)
    }

    pub fn is_paused(env: Env) -> bool {
        env.storage().instance().get(&KEY_PAUSED).unwrap_or(false)
    }

    pub fn get_limits(env: Env) -> (i128, i128) {
        let min: i128 = env.storage().instance().get(&KEY_MIN_AMT).unwrap_or(0);
        let max: i128 = env.storage().instance().get(&KEY_MAX_AMT).unwrap_or(0);
        (min, max)
    }

    pub fn get_total_inbound(env: Env) -> i128 {
        env.storage().instance().get(&KEY_TOT_IN).unwrap_or(0)
    }

    pub fn get_total_outbound(env: Env) -> i128 {
        env.storage().instance().get(&KEY_TOT_OUT).unwrap_or(0)
    }

    pub fn get_admin(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&KEY_ADMIN)
            .unwrap_or_else(|| soroban_sdk::panic_with_error!(env, BridgeError::NotInitialized))
    }

    pub fn get_token(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&KEY_TOKEN)
            .unwrap_or_else(|| soroban_sdk::panic_with_error!(env, BridgeError::NotInitialized))
    }
}

mod test;
