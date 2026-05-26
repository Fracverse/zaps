#![no_std]

//! # Fee Distribution Contract
//!
//! Collects fees from authorised depositors and distributes them among a
//! configurable set of recipients according to basis-point (bps) weights.
//!
//! ## Key design decisions
//!
//! * **Basis points** – each recipient's share is expressed in bps
//!   (1 bps = 0.01 %).  All shares must sum to exactly 10 000 (= 100 %).
//! * **Accumulation** – fees are deposited into the contract's token balance
//!   and tracked in a `pending` counter.  Nothing moves until distribution
//!   is triggered.
//! * **Distribution** – any caller may trigger distribution (permissionless
//!   pull), but the admin also has an explicit `distribute` entry-point.
//!   Rounding remainders (from integer division) are credited to the *first*
//!   recipient so no dust is ever lost.
//! * **Recipient management** – only the admin can add/remove recipients or
//!   change shares.  The full list is replaced atomically to keep the
//!   invariant that shares always sum to 10 000.
//! * **Reentrancy guard** – a simple instance-storage lock prevents
//!   re-entrant calls during distribution.

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype,
    symbol_short, token::Client as TokenClient,
    Address, Env, Symbol, Vec,
};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Total basis points representing 100 %.
const BPS_TOTAL: u32 = 10_000;

// ---------------------------------------------------------------------------
// Storage keys
// ---------------------------------------------------------------------------

const KEY_ADMIN: Symbol = symbol_short!("admin");
const KEY_TOKEN: Symbol = symbol_short!("token");
const KEY_RECIPS: Symbol = symbol_short!("recips");
const KEY_PENDING: Symbol = symbol_short!("pending");
const KEY_TOTAL_IN: Symbol = symbol_short!("total_in");
const KEY_TOTAL_OUT: Symbol = symbol_short!("total_out");
const KEY_LOCKED: Symbol = symbol_short!("locked");

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// A single fee recipient with a basis-point share.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct Recipient {
    pub address: Address,
    /// Share in basis points (0–10 000).  All recipients' shares must sum to
    /// exactly 10 000.
    pub share_bps: u32,
    /// Cumulative amount distributed to this recipient over the contract's
    /// lifetime.
    pub total_received: i128,
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
    InvalidShares = 4,      // shares don't sum to BPS_TOTAL
    EmptyRecipients = 5,
    ZeroAmount = 6,
    NothingToDistribute = 7,
    Reentrant = 8,
    InvalidShareValue = 9,  // individual share is 0 or > BPS_TOTAL
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct FeeDistribution;

#[contractimpl]
impl FeeDistribution {

    // -----------------------------------------------------------------------
    // Initialisation
    // -----------------------------------------------------------------------

    /// Initialise the contract.
    ///
    /// * `admin`      – address that controls recipient management and can
    ///                  trigger manual distribution
    /// * `token`      – the SAC / token contract whose balance this contract
    ///                  accumulates and distributes
    /// * `recipients` – initial recipient list; shares must sum to 10 000
    pub fn initialize(
        env: Env,
        admin: Address,
        token: Address,
        recipients: Vec<Recipient>,
    ) -> Result<(), Error> {
        if env.storage().instance().has(&KEY_ADMIN) {
            return Err(Error::AlreadyInitialized);
        }

        admin.require_auth();

        Self::validate_recipients(&env, &recipients)?;

        env.storage().instance().set(&KEY_ADMIN, &admin);
        env.storage().instance().set(&KEY_TOKEN, &token);
        env.storage().instance().set(&KEY_RECIPS, &recipients);
        env.storage().instance().set(&KEY_PENDING, &0i128);
        env.storage().instance().set(&KEY_TOTAL_IN, &0i128);
        env.storage().instance().set(&KEY_TOTAL_OUT, &0i128);
        env.storage().instance().set(&KEY_LOCKED, &false);

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Fee deposit
    // -----------------------------------------------------------------------

    /// Deposit `amount` tokens into the pending pool.
    ///
    /// The caller must have approved this contract to transfer `amount` from
    /// their balance (standard SAC allowance flow).  Any address may deposit;
    /// access control is enforced by the token contract itself.
    pub fn deposit(env: Env, from: Address, amount: i128) -> Result<(), Error> {
        from.require_auth();

        if amount <= 0 {
            return Err(Error::ZeroAmount);
        }

        Self::require_initialized(&env)?;

        let token: Address = env.storage().instance().get(&KEY_TOKEN).unwrap();
        TokenClient::new(&env, &token)
            .transfer(&from, &env.current_contract_address(), &amount);

        let pending: i128 = env.storage().instance().get(&KEY_PENDING).unwrap_or(0);
        let total_in: i128 = env.storage().instance().get(&KEY_TOTAL_IN).unwrap_or(0);

        env.storage().instance().set(&KEY_PENDING, &(pending + amount));
        env.storage().instance().set(&KEY_TOTAL_IN, &(total_in + amount));

        env.events().publish(
            (symbol_short!("fee_dist"), symbol_short!("deposited")),
            (from, amount, pending + amount),
        );

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Distribution
    // -----------------------------------------------------------------------

    /// Distribute all pending fees to recipients according to their shares.
    ///
    /// Permissionless — anyone can call this.  The admin also has a dedicated
    /// `admin_distribute` entry-point that is identical but requires admin
    /// auth, making it easy to trigger from governance tooling.
    ///
    /// Returns the total amount distributed.
    pub fn distribute(env: Env) -> Result<i128, Error> {
        Self::require_initialized(&env)?;
        Self::do_distribute(&env)
    }

    /// Admin-gated distribution trigger (same logic as `distribute`).
    pub fn admin_distribute(env: Env) -> Result<i128, Error> {
        Self::require_initialized(&env)?;
        let admin: Address = env.storage().instance().get(&KEY_ADMIN).unwrap();
        admin.require_auth();
        Self::do_distribute(&env)
    }

    // -----------------------------------------------------------------------
    // Recipient management (admin only)
    // -----------------------------------------------------------------------

    /// Replace the entire recipient list atomically.
    ///
    /// The new list must be non-empty and shares must sum to exactly 10 000.
    /// Any pending fees are distributed with the *old* list before the update
    /// takes effect, so no funds are mis-attributed.
    pub fn set_recipients(
        env: Env,
        recipients: Vec<Recipient>,
    ) -> Result<(), Error> {
        Self::require_initialized(&env)?;
        let admin: Address = env.storage().instance().get(&KEY_ADMIN).unwrap();
        admin.require_auth();

        Self::validate_recipients(&env, &recipients)?;

        // Flush pending fees under the current allocation before switching.
        let pending: i128 = env.storage().instance().get(&KEY_PENDING).unwrap_or(0);
        if pending > 0 {
            Self::do_distribute(&env)?;
        }

        env.storage().instance().set(&KEY_RECIPS, &recipients);

        env.events().publish(
            (symbol_short!("fee_dist"), symbol_short!("recips_up")),
            recipients.len() as u32,
        );

        Ok(())
    }

    /// Transfer admin rights to a new address.
    pub fn transfer_admin(env: Env, new_admin: Address) -> Result<(), Error> {
        Self::require_initialized(&env)?;
        let admin: Address = env.storage().instance().get(&KEY_ADMIN).unwrap();
        admin.require_auth();
        env.storage().instance().set(&KEY_ADMIN, &new_admin);

        env.events().publish(
            (symbol_short!("fee_dist"), symbol_short!("adm_xfer")),
            new_admin,
        );

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Views
    // -----------------------------------------------------------------------

    pub fn get_admin(env: Env) -> Result<Address, Error> {
        Self::require_initialized(&env)?;
        Ok(env.storage().instance().get(&KEY_ADMIN).unwrap())
    }

    pub fn get_token(env: Env) -> Result<Address, Error> {
        Self::require_initialized(&env)?;
        Ok(env.storage().instance().get(&KEY_TOKEN).unwrap())
    }

    pub fn get_recipients(env: Env) -> Result<Vec<Recipient>, Error> {
        Self::require_initialized(&env)?;
        Ok(env.storage().instance().get(&KEY_RECIPS).unwrap())
    }

    pub fn get_pending(env: Env) -> Result<i128, Error> {
        Self::require_initialized(&env)?;
        Ok(env.storage().instance().get(&KEY_PENDING).unwrap_or(0))
    }

    pub fn get_total_in(env: Env) -> Result<i128, Error> {
        Self::require_initialized(&env)?;
        Ok(env.storage().instance().get(&KEY_TOTAL_IN).unwrap_or(0))
    }

    pub fn get_total_out(env: Env) -> Result<i128, Error> {
        Self::require_initialized(&env)?;
        Ok(env.storage().instance().get(&KEY_TOTAL_OUT).unwrap_or(0))
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    fn require_initialized(env: &Env) -> Result<(), Error> {
        if !env.storage().instance().has(&KEY_ADMIN) {
            return Err(Error::NotInitialized);
        }
        Ok(())
    }

    fn validate_recipients(env: &Env, recipients: &Vec<Recipient>) -> Result<(), Error> {
        if recipients.is_empty() {
            return Err(Error::EmptyRecipients);
        }

        let mut total: u32 = 0;
        for r in recipients.iter() {
            if r.share_bps == 0 || r.share_bps > BPS_TOTAL {
                return Err(Error::InvalidShareValue);
            }
            total = total.checked_add(r.share_bps).unwrap_or(BPS_TOTAL + 1);
        }

        if total != BPS_TOTAL {
            return Err(Error::InvalidShares);
        }

        let _ = env; // env available for future use (e.g. logging)
        Ok(())
    }

    /// Core distribution logic.
    ///
    /// Algorithm:
    /// 1. Read `pending` — bail early if zero.
    /// 2. For each recipient compute `floor(pending * share_bps / 10_000)`.
    /// 3. Sum the computed amounts; the difference from `pending` is the
    ///    rounding remainder, which is added to the first recipient's payout.
    /// 4. Transfer tokens and update per-recipient `total_received`.
    /// 5. Reset `pending` to zero, bump `total_out`.
    fn do_distribute(env: &Env) -> Result<i128, Error> {
        // Reentrancy guard.
        if env.storage().instance().get(&KEY_LOCKED).unwrap_or(false) {
            return Err(Error::Reentrant);
        }
        env.storage().instance().set(&KEY_LOCKED, &true);

        let pending: i128 = env.storage().instance().get(&KEY_PENDING).unwrap_or(0);
        if pending == 0 {
            env.storage().instance().set(&KEY_LOCKED, &false);
            return Err(Error::NothingToDistribute);
        }

        let token: Address = env.storage().instance().get(&KEY_TOKEN).unwrap();
        let token_client = TokenClient::new(env, &token);
        let contract_addr = env.current_contract_address();

        let recipients: Vec<Recipient> = env.storage().instance().get(&KEY_RECIPS).unwrap();

        // --- Compute per-recipient amounts -----------------------------------
        // Use a fixed-size stack array approach: collect amounts first, then
        // transfer, to keep the CEI pattern (state before external calls).

        let n = recipients.len();
        let mut amounts: Vec<i128> = soroban_sdk::vec![env];
        let mut distributed: i128 = 0;

        for r in recipients.iter() {
            let amt = pending * (r.share_bps as i128) / (BPS_TOTAL as i128);
            amounts.push_back(amt);
            distributed += amt;
        }

        // Remainder goes to the first recipient (index 0).
        let remainder = pending - distributed;

        // --- State update (Effects) before token transfers (Interactions) ---
        // Reset pending and bump total_out.
        env.storage().instance().set(&KEY_PENDING, &0i128);
        let total_out: i128 = env.storage().instance().get(&KEY_TOTAL_OUT).unwrap_or(0);
        env.storage().instance().set(&KEY_TOTAL_OUT, &(total_out + pending));

        // Update per-recipient total_received in storage.
        let mut updated: Vec<Recipient> = soroban_sdk::vec![env];
        for i in 0..n {
            let mut r = recipients.get(i as u32).unwrap();
            let extra = if i == 0 { remainder } else { 0 };
            r.total_received += amounts.get(i as u32).unwrap() + extra;
            updated.push_back(r);
        }
        env.storage().instance().set(&KEY_RECIPS, &updated);

        // --- Token transfers (Interactions) ----------------------------------
        for i in 0..n {
            let r = updated.get(i as u32).unwrap();
            let base_amt = amounts.get(i as u32).unwrap();
            let extra = if i == 0 { remainder } else { 0 };
            let payout = base_amt + extra;
            if payout > 0 {
                token_client.transfer(&contract_addr, &r.address, &payout);
            }
        }

        env.storage().instance().set(&KEY_LOCKED, &false);

        env.events().publish(
            (symbol_short!("fee_dist"), symbol_short!("distrib")),
            (pending, n as u32),
        );

        Ok(pending)
    }
}

mod test;
