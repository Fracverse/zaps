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
const KEY_TOTAL_STAKES: Symbol = symbol_short!("t_stk");
const KEY_RISK_PROFILES: Symbol = symbol_short!("r_prof");
const KEY_POLICY_CTR: Symbol = symbol_short!("p_ctr");
const KEY_RISK_CTR: Symbol = symbol_short!("r_ctr");

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
    pub risk_profile_id: u32,
    pub evidence_hash: Symbol,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct RiskProfile {
    pub id: u32,
    pub name: Symbol,
    pub risk_bps: i128,
    pub active: bool,
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
    RiskProfileNotFound = 9,
    RiskProfileInactive = 10,
    InvalidBps = 11,
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
        env.storage().instance().set(&KEY_TOTAL_STAKES, &0i128);
        env.storage().instance().set(&KEY_UNDERWRITERS, &Map::<Address, i128>::new(&env));
        env.storage().instance().set(&KEY_RISK_PROFILES, &Map::<u32, RiskProfile>::new(&env));
        env.storage().instance().set(&KEY_POLICY_CTR, &0u32);
        env.storage().instance().set(&KEY_RISK_CTR, &0u32);

        env.events().publish(
            (symbol_short!("pay_ins"), symbol_short!("init")),
            admin,
        );

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Underwriter management
    // -----------------------------------------------------------------------

    pub fn add_underwriter(env: Env, underwriter: Address, stake: i128) -> Result<(), Error> {
        if stake <= 0 {
            return Err(Error::ZeroAmount);
        }

        if !env.storage().instance().has(&KEY_ADMIN) {
            return Err(Error::NotInitialized);
        }

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

        let mut total_stakes: i128 = env.storage().instance().get(&KEY_TOTAL_STAKES).unwrap_or(0);
        total_stakes += stake;
        env.storage().instance().set(&KEY_TOTAL_STAKES, &total_stakes);

        let mut reserve: i128 = env.storage().instance().get(&KEY_RESERVE).unwrap();
        reserve += stake;
        env.storage().instance().set(&KEY_RESERVE, &reserve);

        env.events().publish(
            (symbol_short!("pay_ins"), symbol_short!("und_added")),
            (underwriter, stake),
        );

        Ok(())
    }

    pub fn remove_underwriter(env: Env, underwriter: Address) -> Result<i128, Error> {
        if !env.storage().instance().has(&KEY_ADMIN) {
            return Err(Error::NotInitialized);
        }

        underwriter.require_auth();

        let mut underwriters: Map<Address, i128> =
            env.storage().instance().get(&KEY_UNDERWRITERS).unwrap();

        let stake = underwriters.get(underwriter.clone()).ok_or(Error::NotUnderwriter)?;
        underwriters.remove(underwriter.clone());
        env.storage().instance().set(&KEY_UNDERWRITERS, &underwriters);

        let total_stakes: i128 = env.storage().instance().get(&KEY_TOTAL_STAKES).unwrap_or(0);
        let reserve: i128 = env.storage().instance().get(&KEY_RESERVE).unwrap();

        let withdrawable = if total_stakes == 0 {
            0
        } else if reserve >= total_stakes {
            stake
        } else {
            (stake * reserve) / total_stakes
        };

        let new_total_stakes = total_stakes - stake;
        env.storage().instance().set(&KEY_TOTAL_STAKES, &new_total_stakes);

        let new_reserve = reserve - withdrawable;
        env.storage().instance().set(&KEY_RESERVE, &new_reserve);

        if withdrawable > 0 {
            let token: Address = env.storage().instance().get(&KEY_TOKEN).unwrap();
            TokenClient::new(&env, &token).transfer(
                &env.current_contract_address(),
                &underwriter,
                &withdrawable,
            );
        }

        env.events().publish(
            (symbol_short!("pay_ins"), symbol_short!("und_rem")),
            (underwriter, withdrawable),
        );

        Ok(withdrawable)
    }

    // -----------------------------------------------------------------------
    // Risk Profile Registry
    // -----------------------------------------------------------------------

    pub fn create_risk_profile(
        env: Env,
        admin_or_underwriter: Address,
        name: Symbol,
        risk_bps: i128,
    ) -> Result<u32, Error> {
        if !env.storage().instance().has(&KEY_ADMIN) {
            return Err(Error::NotInitialized);
        }

        admin_or_underwriter.require_auth();

        let admin: Address = env.storage().instance().get(&KEY_ADMIN).unwrap();
        let mut authorized = admin == admin_or_underwriter;

        if !authorized {
            let underwriters: Map<Address, i128> = env.storage().instance().get(&KEY_UNDERWRITERS).unwrap();
            if underwriters.contains_key(admin_or_underwriter.clone()) {
                authorized = true;
            }
        }

        if !authorized {
            return Err(Error::Unauthorized);
        }

        if risk_bps < 0 || risk_bps > BPS_DIVISOR {
            return Err(Error::InvalidBps);
        }

        let mut counter: u32 = env.storage().instance().get(&KEY_RISK_CTR).unwrap_or(0);
        counter += 1;
        env.storage().instance().set(&KEY_RISK_CTR, &counter);

        let profile = RiskProfile {
            id: counter,
            name,
            risk_bps,
            active: true,
        };

        let mut profiles: Map<u32, RiskProfile> =
            env.storage().instance().get(&KEY_RISK_PROFILES).unwrap_or_else(|| Map::new(&env));
        profiles.set(counter, profile);
        env.storage().instance().set(&KEY_RISK_PROFILES, &profiles);

        env.events().publish(
            (symbol_short!("pay_ins"), symbol_short!("rp_cre")),
            (counter, risk_bps),
        );

        Ok(counter)
    }

    pub fn toggle_risk_profile(
        env: Env,
        admin_or_underwriter: Address,
        id: u32,
        active: bool,
    ) -> Result<(), Error> {
        if !env.storage().instance().has(&KEY_ADMIN) {
            return Err(Error::NotInitialized);
        }

        admin_or_underwriter.require_auth();

        let admin: Address = env.storage().instance().get(&KEY_ADMIN).unwrap();
        let mut authorized = admin == admin_or_underwriter;

        if !authorized {
            let underwriters: Map<Address, i128> = env.storage().instance().get(&KEY_UNDERWRITERS).unwrap();
            if underwriters.contains_key(admin_or_underwriter.clone()) {
                authorized = true;
            }
        }

        if !authorized {
            return Err(Error::Unauthorized);
        }

        let mut profiles: Map<u32, RiskProfile> =
            env.storage().instance().get(&KEY_RISK_PROFILES).ok_or(Error::RiskProfileNotFound)?;

        let mut profile = profiles.get(id).ok_or(Error::RiskProfileNotFound)?;
        profile.active = active;
        profiles.set(id, profile);
        env.storage().instance().set(&KEY_RISK_PROFILES, &profiles);

        env.events().publish(
            (symbol_short!("pay_ins"), symbol_short!("rp_tog")),
            (id, active),
        );

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Premium calculation
    // -----------------------------------------------------------------------

    pub fn calculate_premium(env: Env, coverage_amount: i128, risk_profile_id: u32) -> Result<i128, Error> {
        let profiles: Map<u32, RiskProfile> =
            env.storage().instance().get(&KEY_RISK_PROFILES).unwrap_or_else(|| Map::new(&env));
        let profile = profiles.get(risk_profile_id).ok_or(Error::RiskProfileNotFound)?;
        if !profile.active {
            return Err(Error::RiskProfileInactive);
        }
        Ok(coverage_amount * profile.risk_bps / BPS_DIVISOR)
    }

    // -----------------------------------------------------------------------
    // Policy management
    // -----------------------------------------------------------------------

    pub fn buy_policy(
        env: Env,
        holder: Address,
        coverage_amount: i128,
        risk_profile_id: u32,
    ) -> Result<u32, Error> {
        if coverage_amount <= 0 {
            return Err(Error::ZeroAmount);
        }

        holder.require_auth();

        let premium = Self::calculate_premium(env.clone(), coverage_amount, risk_profile_id)?;
        let token: Address = env.storage().instance().get(&KEY_TOKEN).unwrap();
        
        // Transfer premium from holder to contract
        TokenClient::new(&env, &token).transfer(
            &holder,
            &env.current_contract_address(),
            &premium,
        );

        // Distribute premium: 90% to reserve, 10% to admin as protocol fee
        let admin: Address = env.storage().instance().get(&KEY_ADMIN).unwrap();
        let fee = (premium * 10) / 100;
        let reserve_share = premium - fee;

        if fee > 0 {
            TokenClient::new(&env, &token).transfer(
                &env.current_contract_address(),
                &admin,
                &fee,
            );
        }

        let mut reserve: i128 = env.storage().instance().get(&KEY_RESERVE).unwrap();
        reserve += reserve_share;
        env.storage().instance().set(&KEY_RESERVE, &reserve);

        // Policy counter
        let mut counter: u32 = env.storage().instance().get(&KEY_POLICY_CTR).unwrap_or(0);
        counter += 1;
        env.storage().instance().set(&KEY_POLICY_CTR, &counter);

        let policy = Policy {
            holder,
            coverage_amount,
            premium_paid: premium,
            active: true,
            claim_status: PolicyStatus::Active,
            risk_profile_id,
            evidence_hash: Symbol::new(&env, ""),
        };

        env.storage().instance().set(&counter, &policy);

        env.events().publish(
            (symbol_short!("pay_ins"), symbol_short!("p_bought")),
            (counter, premium),
        );

        Ok(counter)
    }

    pub fn submit_claim(env: Env, policy_id: u32, evidence_hash: Symbol) -> Result<(), Error> {
        let mut policy: Policy = env.storage().instance().get(&policy_id)
            .ok_or(Error::PolicyNotFound)?;

        if !policy.active {
            return Err(Error::AlreadyProcessed);
        }

        if !matches!(policy.claim_status, PolicyStatus::Active) {
            return Err(Error::AlreadyProcessed);
        }

        policy.holder.require_auth();
        policy.claim_status = PolicyStatus::ClaimPending;
        policy.evidence_hash = evidence_hash;
        
        env.storage().instance().set(&policy_id, &policy);

        env.events().publish(
            (symbol_short!("pay_ins"), symbol_short!("c_sub")),
            policy_id,
        );

        Ok(())
    }

    pub fn process_claim(env: Env, policy_id: u32, approve: bool, payout_amount: i128) -> Result<(), Error> {
        let admin: Address = env.storage().instance().get(&KEY_ADMIN).unwrap();
        admin.require_auth();

        let mut policy: Policy = env.storage().instance().get(&policy_id)
            .ok_or(Error::PolicyNotFound)?;

        if !policy.active {
            return Err(Error::AlreadyProcessed);
        }

        if !matches!(policy.claim_status, PolicyStatus::ClaimPending) {
            return Err(Error::AlreadyProcessed);
        }

        if payout_amount <= 0 || payout_amount > policy.coverage_amount {
            return Err(Error::ZeroAmount);
        }

        let mut reserve: i128 = env.storage().instance().get(&KEY_RESERVE).unwrap();
        let token: Address = env.storage().instance().get(&KEY_TOKEN).unwrap();

        if approve {
            if reserve < payout_amount {
                return Err(Error::InsufficientReserve);
            }

            TokenClient::new(&env, &token).transfer(
                &env.current_contract_address(),
                &policy.holder,
                &payout_amount,
            );

            reserve -= payout_amount;
            env.storage().instance().set(&KEY_RESERVE, &reserve);
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

    pub fn get_total_stakes(env: Env) -> i128 {
        env.storage().instance().get(&KEY_TOTAL_STAKES).unwrap_or(0)
    }

    pub fn get_risk_profile(env: Env, id: u32) -> Result<RiskProfile, Error> {
        let profiles: Map<u32, RiskProfile> =
            env.storage().instance().get(&KEY_RISK_PROFILES).unwrap_or_else(|| Map::new(&env));
        profiles.get(id).ok_or(Error::RiskProfileNotFound)
    }
}

mod test;