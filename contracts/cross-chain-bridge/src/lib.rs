#![no_std]

//! # Cross-Chain Bridge Contract
//!
//! Enables secure transfer of assets across multiple blockchain networks.
//! Supports locking assets on source chain and minting wrapped tokens on destination chain.
//!
//! ## Design
//! * Assets are locked in the contract on the source chain
//! * Validators sign off on cross-chain transfers
//! * Wrapped tokens are minted on destination chains
//! * Supports multiple chains and asset types
//! * Admin controls validator set and chain configuration

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype,
    symbol_short, token::Client as TokenClient,
    Address, Env, Symbol, Vec, Map,
};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const MIN_VALIDATORS: u32 = 1;
const MAX_VALIDATORS: u32 = 100;

// ---------------------------------------------------------------------------
// Storage keys
// ---------------------------------------------------------------------------

const KEY_ADMIN: Symbol = symbol_short!("admin");
const KEY_VALIDATORS: Symbol = symbol_short!("validators");
const KEY_CHAINS: Symbol = symbol_short!("chains");
const KEY_LOCKED_ASSETS: Symbol = symbol_short!("locked");
const KEY_BRIDGE_NONCE: Symbol = symbol_short!("nonce");
const KEY_PROCESSED_TXS: Symbol = symbol_short!("processed");

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ChainConfig {
    pub chain_id: u32,
    pub name: Symbol,
    pub enabled: bool,
    pub min_confirmations: u32,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct BridgeTransfer {
    pub id: u64,
    pub source_chain: u32,
    pub dest_chain: u32,
    pub token: Address,
    pub amount: i128,
    pub sender: Address,
    pub recipient: Address,
    pub status: Symbol, // "pending", "confirmed", "completed", "failed"
    pub validator_signatures: u32,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct LockedAsset {
    pub token: Address,
    pub amount: i128,
    pub chain_id: u32,
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    InvalidChainId = 4,
    ChainDisabled = 5,
    InvalidValidator = 6,
    InsufficientValidators = 7,
    TransferNotFound = 8,
    TransferAlreadyProcessed = 9,
    InvalidAmount = 10,
    InsufficientLocked = 11,
    DuplicateValidator = 12,
    TooManyValidators = 13,
    InvalidSignature = 14,
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct CrossChainBridge;

#[contractimpl]
impl CrossChainBridge {

    // -----------------------------------------------------------------------
    // Initialization
    // -----------------------------------------------------------------------

    pub fn initialize(
        env: Env,
        admin: Address,
        validators: Vec<Address>,
        chains: Vec<ChainConfig>,
    ) -> Result<(), Error> {
        if env.storage().instance().has(&KEY_ADMIN) {
            return Err(Error::AlreadyInitialized);
        }
        admin.require_auth();

        if validators.is_empty() || validators.len() > MAX_VALIDATORS as usize {
            return Err(Error::InsufficientValidators);
        }

        // Check for duplicates
        for i in 0..validators.len() {
            for j in (i + 1)..validators.len() {
                if validators.get(i as u32).unwrap() == validators.get(j as u32).unwrap() {
                    return Err(Error::DuplicateValidator);
                }
            }
        }

        env.storage().instance().set(&KEY_ADMIN, &admin);
        env.storage().instance().set(&KEY_VALIDATORS, &validators);
        env.storage().instance().set(&KEY_CHAINS, &chains);
        env.storage().instance().set(&KEY_BRIDGE_NONCE, &0u64);

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Bridge Operations
    // -----------------------------------------------------------------------

    /// Initiate a cross-chain transfer by locking assets
    pub fn lock_and_bridge(
        env: Env,
        token: Address,
        amount: i128,
        dest_chain: u32,
        recipient: Address,
    ) -> Result<u64, Error> {
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        Self::require_initialized(&env)?;

        let sender = env.invoker();
        let contract_addr = env.current_contract_address();

        // Verify destination chain exists and is enabled
        let chains: Vec<ChainConfig> = env.storage().instance().get(&KEY_CHAINS).unwrap();
        let mut dest_valid = false;
        for chain in chains.iter() {
            if chain.chain_id == dest_chain && chain.enabled {
                dest_valid = true;
                break;
            }
        }
        if !dest_valid {
            return Err(Error::ChainDisabled);
        }

        // Lock the tokens
        let token_client = TokenClient::new(&env, &token);
        token_client.transfer(&sender, &contract_addr, &amount);

        // Generate transfer ID
        let nonce: u64 = env.storage().instance().get(&KEY_BRIDGE_NONCE).unwrap_or(0);
        let transfer_id = nonce + 1;
        env.storage().instance().set(&KEY_BRIDGE_NONCE, &transfer_id);

        // Create transfer record
        let transfer = BridgeTransfer {
            id: transfer_id,
            source_chain: 1, // Assume current chain is 1
            dest_chain,
            token: token.clone(),
            amount,
            sender: sender.clone(),
            recipient,
            status: symbol_short!("pending"),
            validator_signatures: 0,
        };

        // Store transfer
        let mut transfers: Map<u64, BridgeTransfer> = env.storage()
            .instance()
            .get(&KEY_LOCKED_ASSETS)
            .unwrap_or_else(|| Map::new(&env));
        transfers.set(transfer_id, transfer);
        env.storage().instance().set(&KEY_LOCKED_ASSETS, &transfers);

        env.events().publish(
            (symbol_short!("bridge"), symbol_short!("locked")),
            (transfer_id, amount, dest_chain),
        );

        Ok(transfer_id)
    }

    /// Validators confirm a cross-chain transfer
    pub fn confirm_transfer(
        env: Env,
        transfer_id: u64,
    ) -> Result<(), Error> {
        Self::require_initialized(&env)?;

        let validator = env.invoker();
        let validators: Vec<Address> = env.storage().instance().get(&KEY_VALIDATORS).unwrap();

        // Check if caller is a validator
        let mut is_validator = false;
        for v in validators.iter() {
            if v == validator {
                is_validator = true;
                break;
            }
        }
        if !is_validator {
            return Err(Error::InvalidValidator);
        }

        // Get transfer
        let mut transfers: Map<u64, BridgeTransfer> = env.storage()
            .instance()
            .get(&KEY_LOCKED_ASSETS)
            .unwrap_or_else(|| Map::new(&env));

        let mut transfer = transfers.get(transfer_id).ok_or(Error::TransferNotFound)?;

        // Increment validator signatures
        transfer.validator_signatures += 1;

        // Check if we have enough confirmations (simple majority)
        let required_sigs = (validators.len() as u32 + 1) / 2;
        if transfer.validator_signatures >= required_sigs {
            transfer.status = symbol_short!("confirmed");
        }

        transfers.set(transfer_id, transfer);
        env.storage().instance().set(&KEY_LOCKED_ASSETS, &transfers);

        env.events().publish(
            (symbol_short!("bridge"), symbol_short!("confirmed")),
            (transfer_id, transfer.validator_signatures),
        );

        Ok(())
    }

    /// Complete a confirmed transfer (mint wrapped tokens on destination)
    pub fn complete_transfer(
        env: Env,
        transfer_id: u64,
    ) -> Result<(), Error> {
        Self::require_initialized(&env)?;

        let mut transfers: Map<u64, BridgeTransfer> = env.storage()
            .instance()
            .get(&KEY_LOCKED_ASSETS)
            .unwrap_or_else(|| Map::new(&env));

        let mut transfer = transfers.get(transfer_id).ok_or(Error::TransferNotFound)?;

        if transfer.status != symbol_short!("confirmed") {
            return Err(Error::TransferAlreadyProcessed);
        }

        transfer.status = symbol_short!("completed");
        transfers.set(transfer_id, transfer);
        env.storage().instance().set(&KEY_LOCKED_ASSETS, &transfers);

        env.events().publish(
            (symbol_short!("bridge"), symbol_short!("completed")),
            (transfer_id, transfer.recipient),
        );

        Ok(())
    }

    /// Unlock and return assets if transfer fails
    pub fn unlock_assets(
        env: Env,
        transfer_id: u64,
    ) -> Result<(), Error> {
        Self::require_initialized(&env)?;
        let admin: Address = env.storage().instance().get(&KEY_ADMIN).unwrap();
        admin.require_auth();

        let mut transfers: Map<u64, BridgeTransfer> = env.storage()
            .instance()
            .get(&KEY_LOCKED_ASSETS)
            .unwrap_or_else(|| Map::new(&env));

        let mut transfer = transfers.get(transfer_id).ok_or(Error::TransferNotFound)?;

        let token_client = TokenClient::new(&env, &transfer.token);
        let contract_addr = env.current_contract_address();

        // Return tokens to sender
        token_client.transfer(&contract_addr, &transfer.sender, &transfer.amount);

        transfer.status = symbol_short!("failed");
        transfers.set(transfer_id, transfer);
        env.storage().instance().set(&KEY_LOCKED_ASSETS, &transfers);

        env.events().publish(
            (symbol_short!("bridge"), symbol_short!("unlocked")),
            (transfer_id, transfer.amount),
        );

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Admin Functions
    // -----------------------------------------------------------------------

    pub fn add_validator(env: Env, validator: Address) -> Result<(), Error> {
        Self::require_initialized(&env)?;
        let admin: Address = env.storage().instance().get(&KEY_ADMIN).unwrap();
        admin.require_auth();

        let mut validators: Vec<Address> = env.storage().instance().get(&KEY_VALIDATORS).unwrap();

        // Check for duplicates
        for v in validators.iter() {
            if v == validator {
                return Err(Error::DuplicateValidator);
            }
        }

        if validators.len() >= MAX_VALIDATORS as usize {
            return Err(Error::TooManyValidators);
        }

        validators.push_back(validator);
        env.storage().instance().set(&KEY_VALIDATORS, &validators);

        Ok(())
    }

    pub fn remove_validator(env: Env, validator: Address) -> Result<(), Error> {
        Self::require_initialized(&env)?;
        let admin: Address = env.storage().instance().get(&KEY_ADMIN).unwrap();
        admin.require_auth();

        let mut validators: Vec<Address> = env.storage().instance().get(&KEY_VALIDATORS).unwrap();

        if validators.len() <= MIN_VALIDATORS as usize {
            return Err(Error::InsufficientValidators);
        }

        let mut found = false;
        for i in 0..validators.len() {
            if validators.get(i as u32).unwrap() == validator {
                validators.remove(i as u32);
                found = true;
                break;
            }
        }

        if !found {
            return Err(Error::InvalidValidator);
        }

        env.storage().instance().set(&KEY_VALIDATORS, &validators);
        Ok(())
    }

    pub fn add_chain(env: Env, chain: ChainConfig) -> Result<(), Error> {
        Self::require_initialized(&env)?;
        let admin: Address = env.storage().instance().get(&KEY_ADMIN).unwrap();
        admin.require_auth();

        let mut chains: Vec<ChainConfig> = env.storage().instance().get(&KEY_CHAINS).unwrap();
        chains.push_back(chain);
        env.storage().instance().set(&KEY_CHAINS, &chains);

        Ok(())
    }

    pub fn disable_chain(env: Env, chain_id: u32) -> Result<(), Error> {
        Self::require_initialized(&env)?;
        let admin: Address = env.storage().instance().get(&KEY_ADMIN).unwrap();
        admin.require_auth();

        let mut chains: Vec<ChainConfig> = env.storage().instance().get(&KEY_CHAINS).unwrap();

        for i in 0..chains.len() {
            let mut chain = chains.get(i as u32).unwrap();
            if chain.chain_id == chain_id {
                chain.enabled = false;
                chains.set(i as u32, chain);
                break;
            }
        }

        env.storage().instance().set(&KEY_CHAINS, &chains);
        Ok(())
    }

    pub fn transfer_admin(env: Env, new_admin: Address) -> Result<(), Error> {
        Self::require_initialized(&env)?;
        let admin: Address = env.storage().instance().get(&KEY_ADMIN).unwrap();
        admin.require_auth();
        env.storage().instance().set(&KEY_ADMIN, &new_admin);
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Views
    // -----------------------------------------------------------------------

    pub fn get_transfer(env: Env, transfer_id: u64) -> Result<BridgeTransfer, Error> {
        Self::require_initialized(&env)?;

        let transfers: Map<u64, BridgeTransfer> = env.storage()
            .instance()
            .get(&KEY_LOCKED_ASSETS)
            .unwrap_or_else(|| Map::new(&env));

        transfers.get(transfer_id).ok_or(Error::TransferNotFound)
    }

    pub fn get_validators(env: Env) -> Result<Vec<Address>, Error> {
        Self::require_initialized(&env)?;
        Ok(env.storage().instance().get(&KEY_VALIDATORS).unwrap())
    }

    pub fn get_chains(env: Env) -> Result<Vec<ChainConfig>, Error> {
        Self::require_initialized(&env)?;
        Ok(env.storage().instance().get(&KEY_CHAINS).unwrap())
    }

    pub fn get_admin(env: Env) -> Result<Address, Error> {
        Self::require_initialized(&env)?;
        Ok(env.storage().instance().get(&KEY_ADMIN).unwrap())
    }

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn require_initialized(env: &Env) -> Result<(), Error> {
        if !env.storage().instance().has(&KEY_ADMIN) {
            return Err(Error::NotInitialized);
        }
        Ok(())
    }
}
