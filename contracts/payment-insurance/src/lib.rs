#![no_std]

//! # Payment Insurance Contract
//!
//! Provides insurance policies where holders pay premiums and can claim coverage.
//! Underwriters provide collateral to the reserve to back claims.

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype,
    symbol_short, token::Client as TokenClient,
    Address, Env, Map, Symbol,
};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const BPS_DIVISOR: i128 = 10_000;

// ---------------------------------------------------------------------------
// Storage keys
// ---------------------------------------------------------------------------

const KEY_ADMIN: Symbol = symbol_short!("admin");
const KEY_TOKEN: Symbol = symbol_short!("token");
const KEY_RESERVE: Symbol = symbol_short!("reserve");
const KEY_UNDERWRITERS: Symbol = symbol_short!("underwr");

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum PolicyStatus {
    Active,
    ClaimPending,
    ClaimApproved,
    ClaimDenied,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct Policy {
    pub holder: Address,
    pub coverage_amount: i128,
    pub premium_paid: i128,
    pub active: bool,
    pub claim_status: PolicyStatus,
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
    ZeroAmount = 4,
    PolicyNotFound = 5,
    AlreadyProcessed = 6,
    InsufficientReserve = 7,
    NotUnderwriter = 8,
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct PaymentInsurance;

#[contractimpl]
impl PaymentInsurance {
    // -----------------------------------------------------------------------
    // Initialisation
    // -----------------------------------------------------------------------

    pub fn initialize(env: Env, admin: Address, token: Address) -> Result<(), Error> {
        if env.storage().instance().has(&KEY_ADMIN) {
            return Err(Error::AlreadyInitialized);
        }

        admin.require_auth();

        env.storage().instance().set(&KEY_ADMIN, &admin);
        env.storage().instance().set(&KEY_TOKEN, &token);
        env.storage().instance().set(&KEY_RESERVE, &0i128);
        env.storage().instance().set(&KEY_UNDERWRITERS, &Map::<Address, i128>::new(&env));

        env.events().publish(
            (symbol_short!("pay_ins"), symbol_short!("init")),
            admin,
        );

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Underwriter management (admin only)
    // -----------------------------------------------------------------------

    pub fn add_underwriter(env: Env, underwriter: Address, stake: i128) -> Result<(), Error> {
        if stake == 0 {
            return Err(Error::ZeroAmount);
        }

        let admin: Address = env.storage().instance().get(&KEY_ADMIN).unwrap();
        admin.require_auth();
        
        underwriter.require_auth();

        let token: Address = env.storage().instance().get(&KEY_TOKEN).unwrap();
        TokenClient::new(&env, &token).transfer(
            &underwriter,
            &env.current_contract_address(),
            &stake,
        );

        let mut underwriters: Map<Address, i128> =
            env.storage().instance().get(&KEY_UNDERWRITERS).unwrap();

        let current_stake = underwriters.get(underwriter.clone()).unwrap_or(0);
        underwriters.set(underwriter.clone(), current_stake + stake);
        env.storage().instance().set(&KEY_UNDERWRITERS, &underwriters);

        let mut reserve: i128 = env.storage().instance().get(&KEY_RESERVE).unwrap();
        reserve = reserve + stake;
        env.storage().instance().set(&KEY_RESERVE, &reserve);

        env.events().publish(
            (symbol_short!("pay_ins"), symbol_short!("und_added")),
            (underwriter, stake),
        );

        Ok(())
    }

    pub fn remove_underwriter(env: Env, underwriter: Address) -> Result<i128, Error> {
        let admin: Address = env.storage().instance().get(&KEY_ADMIN).unwrap();
        admin.require_auth();

        let mut underwriters: Map<Address, i128> =
            env.storage().instance().get(&KEY_UNDERWRITERS).unwrap();

        let stake = underwriters.get(underwriter.clone()).ok_or(Error::NotUnderwriter)?;
        underwriters.remove(underwriter.clone());
        env.storage().instance().set(&KEY_UNDERWRITERS, &underwriters);

        let mut reserve: i128 = env.storage().instance().get(&KEY_RESERVE).unwrap();
        reserve = reserve - stake;
        env.storage().instance().set(&KEY_RESERVE, &reserve);

        env.events().publish(
            (symbol_short!("pay_ins"), symbol_short!("und_rem")),
            (underwriter, stake),
        );

        Ok(stake)
    }

    // -----------------------------------------------------------------------
    // Premium calculation
    // -----------------------------------------------------------------------

    pub fn calculate_premium(_env: Env, coverage_amount: i128, risk_bps: i128) -> i128 {
        coverage_amount * risk_bps / BPS_DIVISOR
    }

    // -----------------------------------------------------------------------
    // Policy management
    // -----------------------------------------------------------------------

    pub fn buy_policy(
        env: Env,
        holder: Address,
        coverage_amount: i128,
        risk_bps: i128,
    ) -> Result<u32, Error> {
        if coverage_amount == 0 {
            return Err(Error::ZeroAmount);
        }

        holder.require_auth();

        // Policy counter in instance storage
        let mut counter: u32 = env.storage().instance().get(&symbol_short!("p_ctr")).unwrap_or(0);
        counter = counter + 1;
        env.storage().instance().set(&symbol_short!("p_ctr"), &counter);

        let premium = coverage_amount * risk_bps / BPS_DIVISOR;
        let token: Address = env.storage().instance().get(&KEY_TOKEN).unwrap();
        TokenClient::new(&env, &token).transfer(
            &holder,
            &env.current_contract_address(),
            &premium,
        );

        let mut reserve: i128 = env.storage().instance().get(&KEY_RESERVE).unwrap();
        reserve = reserve + premium;
        env.storage().instance().set(&KEY_RESERVE, &reserve);

        let policy = Policy {
            holder,
            coverage_amount,
            premium_paid: premium,
            active: true,
            claim_status: PolicyStatus::Active,
        };

        let policy_key: u32 = counter;
        env.storage().instance().set(&policy_key, &policy);

        env.events().publish(
            (symbol_short!("pay_ins"), symbol_short!("p_bought")),
            (counter, premium),
        );

        Ok(counter)
    }

    pub fn submit_claim(env: Env, policy_id: u32) -> Result<(), Error> {
        let policy: Policy = env.storage().instance().get(&policy_id)
            .ok_or(Error::PolicyNotFound)?;

        if !policy.active {
            return Err(Error::AlreadyProcessed);
        }

        let mut policy = policy;
        policy.holder.require_auth();
        policy.claim_status = PolicyStatus::ClaimPending;
        env.storage().instance().set(&policy_id, &policy);

        env.events().publish(
            (symbol_short!("pay_ins"), symbol_short!("c_sub")),
            policy_id,
        );

        Ok(())
    }

    pub fn process_claim(env: Env, policy_id: u32, approve: bool) -> Result<(), Error> {
        let admin: Address = env.storage().instance().get(&KEY_ADMIN).unwrap();
        admin.require_auth();

        let policy: Policy = env.storage().instance().get(&policy_id)
            .ok_or(Error::PolicyNotFound)?;

        if !policy.active {
            return Err(Error::AlreadyProcessed);
        }

        if !matches!(policy.claim_status, PolicyStatus::ClaimPending) {
            return Err(Error::AlreadyProcessed);
        }

        let mut policy = policy;
        let reserve: i128 = env.storage().instance().get(&KEY_RESERVE).unwrap();
        let token: Address = env.storage().instance().get(&KEY_TOKEN).unwrap();

        if approve {
            if reserve < policy.coverage_amount {
                return Err(Error::InsufficientReserve);
            }

            TokenClient::new(&env, &token).transfer(
                &env.current_contract_address(),
                &policy.holder,
                &policy.coverage_amount,
            );

            let new_reserve = reserve - policy.coverage_amount;
            env.storage().instance().set(&KEY_RESERVE, &new_reserve);
            policy.active = false;
            policy.claim_status = PolicyStatus::ClaimApproved;

            env.events().publish(
                (symbol_short!("pay_ins"), symbol_short!("c_appr")),
                policy_id,
            );
        } else {
            policy.claim_status = PolicyStatus::ClaimDenied;

            env.events().publish(
                (symbol_short!("pay_ins"), symbol_short!("c_den")),
                policy_id,
            );
        }

        env.storage().instance().set(&policy_id, &policy);
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Views
    // -----------------------------------------------------------------------

    pub fn get_policy(env: Env, policy_id: u32) -> Result<Policy, Error> {
        env.storage().instance().get(&policy_id).ok_or(Error::PolicyNotFound)
    }

    pub fn get_reserve(env: Env) -> i128 {
        env.storage().instance().get(&KEY_RESERVE).unwrap_or(0)
    }

    pub fn get_underwriters(env: Env) -> Map<Address, i128> {
        env.storage().instance().get(&KEY_UNDERWRITERS).unwrap_or_else(|| Map::<Address, i128>::new(&env))
    }
}

mod test;