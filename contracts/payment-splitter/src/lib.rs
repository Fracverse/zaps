#![no_std]

//! # Payment Splitter Contract
//!
//! Automatically splits incoming payments among multiple recipients using
//! either percentage-based (basis-point) or fixed-amount allocations.
//!
//! ## Design
//! * Recipients are stored as a list of `Split` entries.
//! * Each split is either `Percentage` (bps, must sum to 10 000) or
//!   `Fixed` (exact token amount).
//! * Fixed splits are paid first; the remainder is distributed among
//!   percentage recipients.
//! * Any dust from integer division goes to the first percentage recipient.
//! * Only the admin can manage recipients.

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype,
    symbol_short, token::Client as TokenClient,
    Address, Env, Symbol, Vec,
};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const BPS_TOTAL: u32 = 10_000;

// ---------------------------------------------------------------------------
// Storage keys
// ---------------------------------------------------------------------------

const KEY_ADMIN: Symbol = symbol_short!("admin");
const KEY_TOKEN: Symbol = symbol_short!("token");
const KEY_SPLITS: Symbol = symbol_short!("splits");
const KEY_TOTAL_IN: Symbol = symbol_short!("total_in");

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum SplitKind {
    /// Share expressed in basis points (1 bps = 0.01 %).
    Percentage(u32),
    /// Exact token amount taken off the top before percentage splits.
    Fixed(i128),
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct Split {
    pub recipient: Address,
    pub kind: SplitKind,
    /// Cumulative amount received over the contract's lifetime.
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
    InvalidShares = 4,
    EmptyRecipients = 5,
    ZeroAmount = 6,
    InsufficientForFixed = 7,
    InvalidFixedAmount = 8,
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct PaymentSplitter;

#[contractimpl]
impl PaymentSplitter {

    // -----------------------------------------------------------------------
    // Initialisation
    // -----------------------------------------------------------------------

    pub fn initialize(
        env: Env,
        admin: Address,
        token: Address,
        splits: Vec<Split>,
    ) -> Result<(), Error> {
        if env.storage().instance().has(&KEY_ADMIN) {
            return Err(Error::AlreadyInitialized);
        }
        admin.require_auth();
        Self::validate_splits(&splits)?;

        env.storage().instance().set(&KEY_ADMIN, &admin);
        env.storage().instance().set(&KEY_TOKEN, &token);
        env.storage().instance().set(&KEY_SPLITS, &splits);
        env.storage().instance().set(&KEY_TOTAL_IN, &0i128);

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Split a payment
    // -----------------------------------------------------------------------

    /// Transfer `amount` from `from` into the contract and immediately
    /// distribute it to all recipients according to the configured splits.
    pub fn split(env: Env, from: Address, amount: i128) -> Result<(), Error> {
        from.require_auth();
        if amount <= 0 {
            return Err(Error::ZeroAmount);
        }
        Self::require_initialized(&env)?;

        let token: Address = env.storage().instance().get(&KEY_TOKEN).unwrap();
        let token_client = TokenClient::new(&env, &token);
        let contract_addr = env.current_contract_address();

        // Pull funds in.
        token_client.transfer(&from, &contract_addr, &amount);

        // Update total_in.
        let total_in: i128 = env.storage().instance().get(&KEY_TOTAL_IN).unwrap_or(0);
        env.storage().instance().set(&KEY_TOTAL_IN, &(total_in + amount));

        let splits: Vec<Split> = env.storage().instance().get(&KEY_SPLITS).unwrap();

        // --- Phase 1: pay fixed recipients first ----------------------------
        let mut remaining = amount;
        for s in splits.iter() {
            if let SplitKind::Fixed(fixed_amt) = s.kind.clone() {
                if remaining < fixed_amt {
                    return Err(Error::InsufficientForFixed);
                }
                remaining -= fixed_amt;
            }
        }

        // --- Phase 2: compute percentage amounts ----------------------------
        let mut pct_amounts: Vec<i128> = soroban_sdk::vec![&env];
        let mut pct_distributed: i128 = 0;
        for s in splits.iter() {
            if let SplitKind::Percentage(bps) = s.kind.clone() {
                let amt = remaining * (bps as i128) / (BPS_TOTAL as i128);
                pct_amounts.push_back(amt);
                pct_distributed += amt;
            } else {
                pct_amounts.push_back(0i128);
            }
        }
        let remainder = remaining - pct_distributed;

        // --- Phase 3: update state then transfer (CEI) ----------------------
        let mut updated: Vec<Split> = soroban_sdk::vec![&env];
        let mut pct_idx: u32 = 0;
        let mut first_pct: Option<u32> = None;

        for (i, s) in splits.iter().enumerate() {
            let mut s = s.clone();
            let payout = match s.kind.clone() {
                SplitKind::Fixed(amt) => amt,
                SplitKind::Percentage(_) => {
                    let base = pct_amounts.get(i as u32).unwrap_or(0);
                    if first_pct.is_none() {
                        first_pct = Some(pct_idx);
                    }
                    pct_idx += 1;
                    base
                }
            };
            s.total_received += payout;
            updated.push_back(s);
        }

        // Add remainder to first percentage recipient.
        if remainder > 0 {
            if let Some(fp_idx) = first_pct {
                let mut fp = updated.get(fp_idx).unwrap();
                fp.total_received += remainder;
                updated.set(fp_idx, fp);
            }
        }

        env.storage().instance().set(&KEY_SPLITS, &updated);

        // --- Phase 4: token transfers ----------------------------------------
        let mut pct_idx2: u32 = 0;
        let mut first_pct2: Option<u32> = None;
        for (i, s) in updated.iter().enumerate() {
            let payout = match s.kind.clone() {
                SplitKind::Fixed(amt) => amt,
                SplitKind::Percentage(_) => {
                    let base = pct_amounts.get(i as u32).unwrap_or(0);
                    let extra = if first_pct2.is_none() {
                        first_pct2 = Some(pct_idx2);
                        remainder
                    } else {
                        0
                    };
                    pct_idx2 += 1;
                    base + extra
                }
            };
            if payout > 0 {
                token_client.transfer(&contract_addr, &s.recipient, &payout);
            }
        }

        env.events().publish(
            (symbol_short!("splitter"), symbol_short!("split")),
            (from, amount),
        );

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Recipient management (admin only)
    // -----------------------------------------------------------------------

    pub fn set_splits(env: Env, splits: Vec<Split>) -> Result<(), Error> {
        Self::require_initialized(&env)?;
        let admin: Address = env.storage().instance().get(&KEY_ADMIN).unwrap();
        admin.require_auth();
        Self::validate_splits(&splits)?;
        env.storage().instance().set(&KEY_SPLITS, &splits);

        env.events().publish(
            (symbol_short!("splitter"), symbol_short!("splits_up")),
            splits.len() as u32,
        );

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

    pub fn get_admin(env: Env) -> Result<Address, Error> {
        Self::require_initialized(&env)?;
        Ok(env.storage().instance().get(&KEY_ADMIN).unwrap())
    }

    pub fn get_splits(env: Env) -> Result<Vec<Split>, Error> {
        Self::require_initialized(&env)?;
        Ok(env.storage().instance().get(&KEY_SPLITS).unwrap())
    }

    pub fn get_total_in(env: Env) -> Result<i128, Error> {
        Self::require_initialized(&env)?;
        Ok(env.storage().instance().get(&KEY_TOTAL_IN).unwrap_or(0))
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

    fn validate_splits(splits: &Vec<Split>) -> Result<(), Error> {
        if splits.is_empty() {
            return Err(Error::EmptyRecipients);
        }

        let mut bps_total: u32 = 0;
        for s in splits.iter() {
            match s.kind.clone() {
                SplitKind::Percentage(bps) => {
                    if bps == 0 || bps > BPS_TOTAL {
                        return Err(Error::InvalidShares);
                    }
                    bps_total = bps_total.checked_add(bps).unwrap_or(BPS_TOTAL + 1);
                }
                SplitKind::Fixed(amt) => {
                    if amt <= 0 {
                        return Err(Error::InvalidFixedAmount);
                    }
                }
            }
        }

        // If there are any percentage splits they must sum to 10 000.
        if bps_total != 0 && bps_total != BPS_TOTAL {
            return Err(Error::InvalidShares);
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Advanced Payment Splitting Features
// ---------------------------------------------------------------------------

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct SplitSchedule {
    pub recipient: Address,
    pub amount_per_period: i128,
    pub period_ledgers: u32,
    pub periods_remaining: u32,
    pub last_payout_ledger: u32,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ConditionalSplit {
    pub recipient: Address,
    pub condition_type: ConditionType,
    pub threshold: i128,
    pub percentage: u32,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum ConditionType {
    MinimumAmount = 1,
    MaximumAmount = 2,
    TimeWindow = 3,
}

const KEY_SCHEDULES: Symbol = symbol_short!("scheds");
const KEY_CONDITIONAL: Symbol = symbol_short!("cond");
const KEY_LAST_SPLIT: Symbol = symbol_short!("last_sp");

#[contractimpl]
impl PaymentSplitter {
    // -----------------------------------------------------------------------
    // Scheduled Splits (Vesting)
    // -----------------------------------------------------------------------

    /// Add a scheduled split that vests over time.
    pub fn add_scheduled_split(
        env: Env,
        recipient: Address,
        amount_per_period: i128,
        period_ledgers: u32,
        periods: u32,
    ) -> Result<(), Error> {
        Self::require_initialized(&env)?;
        let admin: Address = env.storage().instance().get(&KEY_ADMIN).unwrap();
        admin.require_auth();

        if amount_per_period <= 0 || period_ledgers == 0 || periods == 0 {
            return Err(Error::InvalidFixedAmount);
        }

        let mut schedules: Vec<SplitSchedule> = env.storage().instance()
            .get(&KEY_SCHEDULES)
            .unwrap_or(soroban_sdk::vec![&env]);

        let schedule = SplitSchedule {
            recipient: recipient.clone(),
            amount_per_period,
            period_ledgers,
            periods_remaining: periods,
            last_payout_ledger: env.ledger().sequence(),
        };

        schedules.push_back(schedule);
        env.storage().instance().set(&KEY_SCHEDULES, &schedules);

        env.events().publish(
            (symbol_short!("splitter"), symbol_short!("sch_add")),
            (recipient, amount_per_period, periods),
        );

        Ok(())
    }

    /// Process scheduled splits that are due.
    pub fn process_scheduled_splits(env: Env) -> Result<u32, Error> {
        Self::require_initialized(&env)?;

        let token: Address = env.storage().instance().get(&KEY_TOKEN).unwrap();
        let token_client = TokenClient::new(&env, &token);
        let contract_addr = env.current_contract_address();
        let current_ledger = env.ledger().sequence();

        let mut schedules: Vec<SplitSchedule> = env.storage().instance()
            .get(&KEY_SCHEDULES)
            .unwrap_or(soroban_sdk::vec![&env]);

        let mut processed = 0u32;
        let mut updated: Vec<SplitSchedule> = soroban_sdk::vec![&env];

        for schedule in schedules.iter() {
            let mut s = schedule.clone();
            let ledgers_elapsed = current_ledger.saturating_sub(s.last_payout_ledger);

            if ledgers_elapsed >= s.period_ledgers && s.periods_remaining > 0 {
                token_client.transfer(&contract_addr, &s.recipient, &s.amount_per_period);
                s.last_payout_ledger = current_ledger;
                s.periods_remaining = s.periods_remaining.saturating_sub(1);
                processed += 1;

                env.events().publish(
                    (symbol_short!("splitter"), symbol_short!("sch_pay")),
                    (s.recipient.clone(), s.amount_per_period),
                );
            }

            if s.periods_remaining > 0 {
                updated.push_back(s);
            }
        }

        env.storage().instance().set(&KEY_SCHEDULES, &updated);
        Ok(processed)
    }

    // -----------------------------------------------------------------------
    // Conditional Splits
    // -----------------------------------------------------------------------

    /// Add a conditional split that activates based on payment amount.
    pub fn add_conditional_split(
        env: Env,
        recipient: Address,
        condition_type: ConditionType,
        threshold: i128,
        percentage: u32,
    ) -> Result<(), Error> {
        Self::require_initialized(&env)?;
        let admin: Address = env.storage().instance().get(&KEY_ADMIN).unwrap();
        admin.require_auth();

        if percentage == 0 || percentage > BPS_TOTAL {
            return Err(Error::InvalidShares);
        }

        let mut conditionals: Vec<ConditionalSplit> = env.storage().instance()
            .get(&KEY_CONDITIONAL)
            .unwrap_or(soroban_sdk::vec![&env]);

        let cond_split = ConditionalSplit {
            recipient: recipient.clone(),
            condition_type,
            threshold,
            percentage,
        };

        conditionals.push_back(cond_split);
        env.storage().instance().set(&KEY_CONDITIONAL, &conditionals);

        env.events().publish(
            (symbol_short!("splitter"), symbol_short!("cond_add")),
            (recipient, threshold, percentage),
        );

        Ok(())
    }

    /// Evaluate and apply conditional splits to a payment.
    pub fn split_with_conditions(
        env: Env,
        from: Address,
        amount: i128,
    ) -> Result<(), Error> {
        from.require_auth();
        if amount <= 0 {
            return Err(Error::ZeroAmount);
        }
        Self::require_initialized(&env)?;

        let token: Address = env.storage().instance().get(&KEY_TOKEN).unwrap();
        let token_client = TokenClient::new(&env, &token);
        let contract_addr = env.current_contract_address();

        // Pull funds in.
        token_client.transfer(&from, &contract_addr, &amount);

        // Update total_in.
        let total_in: i128 = env.storage().instance().get(&KEY_TOTAL_IN).unwrap_or(0);
        env.storage().instance().set(&KEY_TOTAL_IN, &(total_in + amount));

        let splits: Vec<Split> = env.storage().instance().get(&KEY_SPLITS).unwrap();
        let conditionals: Vec<ConditionalSplit> = env.storage().instance()
            .get(&KEY_CONDITIONAL)
            .unwrap_or(soroban_sdk::vec![&env]);

        // Evaluate conditions
        let mut conditional_amount = 0i128;
        for cond in conditionals.iter() {
            let matches = match cond.condition_type {
                ConditionType::MinimumAmount => amount >= cond.threshold,
                ConditionType::MaximumAmount => amount <= cond.threshold,
                ConditionType::TimeWindow => true, // Simplified; would need timestamp logic
            };

            if matches {
                let payout = amount * (cond.percentage as i128) / (BPS_TOTAL as i128);
                conditional_amount += payout;
                token_client.transfer(&contract_addr, &cond.recipient, &payout);
            }
        }

        let remaining = amount - conditional_amount;

        // Process regular splits with remaining amount
        let mut remaining_after_fixed = remaining;
        for s in splits.iter() {
            if let SplitKind::Fixed(fixed_amt) = s.kind.clone() {
                if remaining_after_fixed < fixed_amt {
                    return Err(Error::InsufficientForFixed);
                }
                remaining_after_fixed -= fixed_amt;
            }
        }

        // Distribute percentage splits
        let mut pct_amounts: Vec<i128> = soroban_sdk::vec![&env];
        let mut pct_distributed: i128 = 0;
        for s in splits.iter() {
            if let SplitKind::Percentage(bps) = s.kind.clone() {
                let amt = remaining_after_fixed * (bps as i128) / (BPS_TOTAL as i128);
                pct_amounts.push_back(amt);
                pct_distributed += amt;
            } else {
                pct_amounts.push_back(0i128);
            }
        }
        let remainder = remaining_after_fixed - pct_distributed;

        // Update state and transfer
        let mut updated: Vec<Split> = soroban_sdk::vec![&env];
        let mut pct_idx: u32 = 0;
        let mut first_pct: Option<u32> = None;

        for (i, s) in splits.iter().enumerate() {
            let mut s = s.clone();
            let payout = match s.kind.clone() {
                SplitKind::Fixed(amt) => amt,
                SplitKind::Percentage(_) => {
                    let base = pct_amounts.get(i as u32).unwrap_or(0);
                    if first_pct.is_none() {
                        first_pct = Some(pct_idx);
                    }
                    pct_idx += 1;
                    base
                }
            };
            s.total_received += payout;
            updated.push_back(s);
        }

        if remainder > 0 {
            if let Some(fp_idx) = first_pct {
                let mut fp = updated.get(fp_idx).unwrap();
                fp.total_received += remainder;
                updated.set(fp_idx, fp);
            }
        }

        env.storage().instance().set(&KEY_SPLITS, &updated);

        // Transfer to regular split recipients
        let mut pct_idx2: u32 = 0;
        let mut first_pct2: Option<u32> = None;
        for (i, s) in updated.iter().enumerate() {
            let payout = match s.kind.clone() {
                SplitKind::Fixed(amt) => amt,
                SplitKind::Percentage(_) => {
                    let base = pct_amounts.get(i as u32).unwrap_or(0);
                    let extra = if first_pct2.is_none() {
                        first_pct2 = Some(pct_idx2);
                        remainder
                    } else {
                        0
                    };
                    pct_idx2 += 1;
                    base + extra
                }
            };
            if payout > 0 {
                token_client.transfer(&contract_addr, &s.recipient, &payout);
            }
        }

        env.events().publish(
            (symbol_short!("splitter"), symbol_short!("cond_spl")),
            (from, amount, conditional_amount),
        );

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Views for Advanced Features
    // -----------------------------------------------------------------------

    pub fn get_scheduled_splits(env: Env) -> Result<Vec<SplitSchedule>, Error> {
        Self::require_initialized(&env)?;
        Ok(env.storage().instance()
            .get(&KEY_SCHEDULES)
            .unwrap_or(soroban_sdk::vec![&env]))
    }

    pub fn get_conditional_splits(env: Env) -> Result<Vec<ConditionalSplit>, Error> {
        Self::require_initialized(&env)?;
        Ok(env.storage().instance()
            .get(&KEY_CONDITIONAL)
            .unwrap_or(soroban_sdk::vec![&env]))
    }
}
