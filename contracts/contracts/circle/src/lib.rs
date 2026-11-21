#![no_std]
#![allow(unexpected_cfgs)]
//! Zaps Circles: pooled contributions with payout limits, multi-member approval
//! governance, asset-specific balance management, and membership exit rules.
use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, token, Address, Env, Vec,
};

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    NotFound = 1,
    Unauthorized = 2,
    NotMember = 3,
    InvalidAmount = 4,
    ExceedsLimit = 5,
    InsufficientBalance = 6,
    InvalidGovernance = 7,
    NotApprover = 8,
    AlreadyApproved = 9,
    NotApproved = 10,
    AlreadyExecuted = 11,
    PendingObligations = 12,
    ExitRestricted = 13,
    InvalidDestination = 14,
    Overflow = 15,
}

#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum RequestStatus {
    Pending = 1,
    Approved = 2,
    Executed = 3,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Circle {
    pub admin: Address,
    pub token: Address,
    pub balance: i128,
    /// Amount reserved by pending payout requests.
    pub reserved: i128,
    /// Maximum amount per payout request. 0 means no limit.
    pub max_payout: i128,
    pub threshold: u32,
    pub approvers: Vec<Address>,
    pub next_request: u32,
    pub total_contributed: i128,
    pub total_paid: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Payout {
    pub requester: Address,
    pub to: Address,
    pub amount: i128,
    pub asset: Address,
    pub status: RequestStatus,
    /// Threshold in force when the request was made; later governance changes do not alter it.
    pub threshold: u32,
    pub approvals: Vec<Address>,
    pub executed: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApprovalProgress {
    pub current_approvals: u32,
    pub required_threshold: u32,
    pub is_approved: bool,
    pub is_executed: bool,
    pub approvers: Vec<Address>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CircleBalance {
    pub asset: Address,
    pub total_balance: i128,
    pub reserved_balance: i128,
    pub available_balance: i128,
    pub total_contributed: i128,
    pub total_paid: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssetBalanceInfo {
    pub asset: Address,
    pub total_balance: i128,
    pub reserved_balance: i128,
    pub available_balance: i128,
    pub total_contributed: i128,
    pub total_paid: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReconciliationResult {
    pub on_chain_token_balance: i128,
    pub recorded_balance: i128,
    pub is_reconciled: bool,
}

#[contracttype]
enum Key {
    Count,
    Circle(u32),
    Member(u32, Address),
    Contribution(u32, Address),
    Pending(u32, Address),
    Payout(u32, u32),
    AssetBalance(Address),
    AssetReserved(Address),
    AssetContributed(Address),
    AssetPaid(Address),
}

#[contract]
pub struct CircleContract;

fn load(env: &Env, id: u32) -> Result<Circle, Error> {
    env.storage().persistent().get(&Key::Circle(id)).ok_or(Error::NotFound)
}

fn save(env: &Env, id: u32, c: &Circle) {
    env.storage().persistent().set(&Key::Circle(id), c);
}

fn is_member(env: &Env, id: u32, who: &Address) -> bool {
    env.storage().persistent().get(&Key::Member(id, who.clone())).unwrap_or(false)
}

fn pending(env: &Env, id: u32, who: &Address) -> u32 {
    env.storage().persistent().get(&Key::Pending(id, who.clone())).unwrap_or(0)
}

fn check_governance(threshold: u32, approvers: &Vec<Address>) -> Result<(), Error> {
    if threshold == 0 || threshold > approvers.len() {
        return Err(Error::InvalidGovernance);
    }
    Ok(())
}

fn get_asset_bal(env: &Env, asset: &Address) -> i128 {
    env.storage().persistent().get(&Key::AssetBalance(asset.clone())).unwrap_or(0)
}

fn set_asset_bal(env: &Env, asset: &Address, val: i128) {
    env.storage().persistent().set(&Key::AssetBalance(asset.clone()), &val);
}

fn get_asset_res(env: &Env, asset: &Address) -> i128 {
    env.storage().persistent().get(&Key::AssetReserved(asset.clone())).unwrap_or(0)
}

fn set_asset_res(env: &Env, asset: &Address, val: i128) {
    env.storage().persistent().set(&Key::AssetReserved(asset.clone()), &val);
}

fn get_asset_contrib(env: &Env, asset: &Address) -> i128 {
    env.storage().persistent().get(&Key::AssetContributed(asset.clone())).unwrap_or(0)
}

fn set_asset_contrib(env: &Env, asset: &Address, val: i128) {
    env.storage().persistent().set(&Key::AssetContributed(asset.clone()), &val);
}

fn get_asset_p(env: &Env, asset: &Address) -> i128 {
    env.storage().persistent().get(&Key::AssetPaid(asset.clone())).unwrap_or(0)
}

fn set_asset_p(env: &Env, asset: &Address, val: i128) {
    env.storage().persistent().set(&Key::AssetPaid(asset.clone()), &val);
}

#[contractimpl]
impl CircleContract {
    /// Creates a circle. The admin becomes the first member and sole approver (threshold 1).
    pub fn create_circle(env: Env, admin: Address, token: Address, max_payout: i128) -> Result<u32, Error> {
        admin.require_auth();
        if max_payout < 0 {
            return Err(Error::InvalidAmount);
        }
        let id: u32 = env.storage().instance().get(&Key::Count).unwrap_or(0);
        let next_count = id.checked_add(1).ok_or(Error::Overflow)?;
        env.storage().instance().set(&Key::Count, &next_count);
        let c = Circle {
            admin: admin.clone(),
            token,
            balance: 0,
            reserved: 0,
            max_payout,
            threshold: 1,
            approvers: Vec::from_array(&env, [admin.clone()]),
            next_request: 0,
            total_contributed: 0,
            total_paid: 0,
        };
        save(&env, id, &c);
        env.storage().persistent().set(&Key::Member(id, admin.clone()), &true);
        env.events().publish((symbol_short!("created"), id), admin);
        Ok(id)
    }

    pub fn join(env: Env, id: u32, member: Address) -> Result<(), Error> {
        member.require_auth();
        load(&env, id)?;
        env.storage().persistent().set(&Key::Member(id, member.clone()), &true);
        env.events().publish((symbol_short!("joined"), id), member);
        Ok(())
    }

    pub fn contribute(env: Env, id: u32, member: Address, amount: i128) -> Result<(), Error> {
        member.require_auth();
        let mut c = load(&env, id)?;
        if !is_member(&env, id, &member) {
            return Err(Error::NotMember);
        }
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        token::Client::new(&env, &c.token).transfer(&member, &env.current_contract_address(), &amount);

        c.balance = c.balance.checked_add(amount).ok_or(Error::Overflow)?;
        c.total_contributed = c.total_contributed.checked_add(amount).ok_or(Error::Overflow)?;
        save(&env, id, &c);

        let key = Key::Contribution(id, member.clone());
        let total: i128 = env.storage().persistent().get(&key).unwrap_or(0);
        let new_total = total.checked_add(amount).ok_or(Error::Overflow)?;
        env.storage().persistent().set(&key, &new_total);

        // Update asset-level tracking
        let ab = get_asset_bal(&env, &c.token).checked_add(amount).ok_or(Error::Overflow)?;
        set_asset_bal(&env, &c.token, ab);
        let ac = get_asset_contrib(&env, &c.token).checked_add(amount).ok_or(Error::Overflow)?;
        set_asset_contrib(&env, &c.token, ac);

        env.events().publish((symbol_short!("contrib"), id, member), amount);
        Ok(())
    }

    /// Admin-only. Sets the per-request payout limit (0 disables the limit).
    pub fn set_max_payout(env: Env, id: u32, admin: Address, max_payout: i128) -> Result<(), Error> {
        admin.require_auth();
        let mut c = load(&env, id)?;
        if admin != c.admin {
            return Err(Error::Unauthorized);
        }
        if max_payout < 0 {
            return Err(Error::InvalidAmount);
        }
        c.max_payout = max_payout;
        save(&env, id, &c);
        env.events().publish((symbol_short!("limit"), id), max_payout);
        Ok(())
    }

    /// Admin-only. Sets full governance: threshold and approvers list. Existing requests keep their snapshot threshold.
    pub fn set_governance(env: Env, id: u32, admin: Address, threshold: u32, approvers: Vec<Address>) -> Result<(), Error> {
        admin.require_auth();
        let mut c = load(&env, id)?;
        if admin != c.admin {
            return Err(Error::Unauthorized);
        }
        check_governance(threshold, &approvers)?;
        c.threshold = threshold;
        c.approvers = approvers;
        save(&env, id, &c);
        env.events().publish((symbol_short!("gov"), id), threshold);
        Ok(())
    }

    /// Admin-only. Configures only the approval threshold.
    pub fn set_approval_threshold(env: Env, id: u32, admin: Address, threshold: u32) -> Result<(), Error> {
        admin.require_auth();
        let mut c = load(&env, id)?;
        if admin != c.admin {
            return Err(Error::Unauthorized);
        }
        check_governance(threshold, &c.approvers)?;
        c.threshold = threshold;
        save(&env, id, &c);
        env.events().publish((symbol_short!("gov_thresh"), id), threshold);
        Ok(())
    }

    /// Admin-only. Adds an approver.
    pub fn add_approver(env: Env, id: u32, admin: Address, approver: Address) -> Result<(), Error> {
        admin.require_auth();
        let mut c = load(&env, id)?;
        if admin != c.admin {
            return Err(Error::Unauthorized);
        }
        if !c.approvers.contains(&approver) {
            c.approvers.push_back(approver.clone());
            save(&env, id, &c);
            env.events().publish((symbol_short!("appr_add"), id), approver);
        }
        Ok(())
    }

    /// Admin-only. Removes an approver if remaining approvers count satisfies threshold.
    pub fn remove_approver(env: Env, id: u32, admin: Address, approver: Address) -> Result<(), Error> {
        admin.require_auth();
        let mut c = load(&env, id)?;
        if admin != c.admin {
            return Err(Error::Unauthorized);
        }
        let mut new_approvers = Vec::new(&env);
        for a in c.approvers.iter() {
            if a != approver {
                new_approvers.push_back(a);
            }
        }
        check_governance(c.threshold, &new_approvers)?;
        c.approvers = new_approvers;
        save(&env, id, &c);
        env.events().publish((symbol_short!("appr_rem"), id), approver);
        Ok(())
    }

    /// Submits a withdrawal / payout request. Only members may request.
    /// Destination is validated, amount is checked against max payout and available balance.
    pub fn request_payout(env: Env, id: u32, member: Address, to: Address, amount: i128) -> Result<u32, Error> {
        member.require_auth();
        let mut c = load(&env, id)?;
        if !is_member(&env, id, &member) {
            return Err(Error::NotMember);
        }
        if to == env.current_contract_address() {
            return Err(Error::InvalidDestination);
        }
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        if c.max_payout > 0 && amount > c.max_payout {
            return Err(Error::ExceedsLimit);
        }
        let available = c.balance.checked_sub(c.reserved).ok_or(Error::InsufficientBalance)?;
        if amount > available {
            return Err(Error::InsufficientBalance);
        }

        let rid = c.next_request;
        c.next_request = c.next_request.checked_add(1).ok_or(Error::Overflow)?;
        c.reserved = c.reserved.checked_add(amount).ok_or(Error::Overflow)?;

        // Update asset-level reserved balance
        let ar = get_asset_res(&env, &c.token).checked_add(amount).ok_or(Error::Overflow)?;
        set_asset_res(&env, &c.token, ar);

        let p = Payout {
            requester: member.clone(),
            to,
            amount,
            asset: c.token.clone(),
            status: RequestStatus::Pending,
            threshold: c.threshold,
            approvals: Vec::new(&env),
            executed: false,
        };
        env.storage().persistent().set(&Key::Payout(id, rid), &p);

        let n = pending(&env, id, &member);
        let new_n = n.checked_add(1).ok_or(Error::Overflow)?;
        env.storage().persistent().set(&Key::Pending(id, member), &new_n);

        save(&env, id, &c);
        env.events().publish((symbol_short!("request"), id, rid), amount);
        Ok(rid)
    }

    /// Alias for `request_payout` with withdrawal nomenclature.
    pub fn request_withdrawal(env: Env, id: u32, member: Address, destination: Address, amount: i128) -> Result<u32, Error> {
        Self::request_payout(env, id, member, destination, amount)
    }

    /// Authorizes a withdrawal/payout request. Only designated approvers may approve.
    pub fn approve(env: Env, id: u32, rid: u32, approver: Address) -> Result<(), Error> {
        approver.require_auth();
        let c = load(&env, id)?;
        if !c.approvers.contains(&approver) {
            return Err(Error::NotApprover);
        }
        let mut p: Payout = env.storage().persistent().get(&Key::Payout(id, rid)).ok_or(Error::NotFound)?;
        if p.executed {
            return Err(Error::AlreadyExecuted);
        }
        if p.approvals.contains(&approver) {
            return Err(Error::AlreadyApproved);
        }
        p.approvals.push_back(approver.clone());
        if p.approvals.len() >= p.threshold {
            p.status = RequestStatus::Approved;
        }
        env.storage().persistent().set(&Key::Payout(id, rid), &p);
        env.events().publish((symbol_short!("approved"), id, rid), approver);
        Ok(())
    }

    /// Executes an approved withdrawal request. Checks authorization threshold,
    /// verifies payout limit and balance, then atomically updates accounting and transfers funds.
    pub fn execute(env: Env, id: u32, rid: u32) -> Result<(), Error> {
        let mut c = load(&env, id)?;
        let mut p: Payout = env.storage().persistent().get(&Key::Payout(id, rid)).ok_or(Error::NotFound)?;
        if p.executed {
            return Err(Error::AlreadyExecuted);
        }
        if p.approvals.len() < p.threshold {
            return Err(Error::NotApproved);
        }
        // Re-check limit and balance so a limit lowered after the request cannot be bypassed.
        if c.max_payout > 0 && p.amount > c.max_payout {
            return Err(Error::ExceedsLimit);
        }
        if c.balance < p.amount {
            return Err(Error::InsufficientBalance);
        }

        p.executed = true;
        p.status = RequestStatus::Executed;

        c.balance = c.balance.checked_sub(p.amount).ok_or(Error::InsufficientBalance)?;
        c.reserved = c.reserved.checked_sub(p.amount).ok_or(Error::InsufficientBalance)?;
        c.total_paid = c.total_paid.checked_add(p.amount).ok_or(Error::Overflow)?;

        // Update asset-level tracking
        let ab = get_asset_bal(&env, &c.token).checked_sub(p.amount).ok_or(Error::InsufficientBalance)?;
        set_asset_bal(&env, &c.token, ab);
        let ar = get_asset_res(&env, &c.token).checked_sub(p.amount).ok_or(Error::InsufficientBalance)?;
        set_asset_res(&env, &c.token, ar);
        let ap = get_asset_p(&env, &c.token).checked_add(p.amount).ok_or(Error::Overflow)?;
        set_asset_p(&env, &c.token, ap);

        env.storage().persistent().set(&Key::Payout(id, rid), &p);

        let n = pending(&env, id, &p.requester);
        if n > 0 {
            env.storage().persistent().set(&Key::Pending(id, p.requester.clone()), &(n - 1));
        }
        save(&env, id, &c);

        token::Client::new(&env, &p.asset).transfer(&env.current_contract_address(), &p.to, &p.amount);
        env.events().publish((symbol_short!("paid"), id, rid), p.amount);
        Ok(())
    }

    /// Alias for `execute`.
    pub fn execute_payout(env: Env, id: u32, rid: u32) -> Result<(), Error> {
        Self::execute(env, id, rid)
    }

    /// Removes a member. Admins and approvers cannot exit, and members with
    /// pending payout requests must wait. Contribution history is kept.
    pub fn leave(env: Env, id: u32, member: Address) -> Result<(), Error> {
        member.require_auth();
        let c = load(&env, id)?;
        if !is_member(&env, id, &member) {
            return Err(Error::NotMember);
        }
        if member == c.admin || c.approvers.contains(&member) {
            return Err(Error::ExitRestricted);
        }
        if pending(&env, id, &member) > 0 {
            return Err(Error::PendingObligations);
        }
        env.storage().persistent().set(&Key::Member(id, member.clone()), &false);
        env.events().publish((symbol_short!("left"), id), member);
        Ok(())
    }

    // ── Queries & Reconciliation ─────────────────────────────────────────

    pub fn get_circle(env: Env, id: u32) -> Result<Circle, Error> {
        load(&env, id)
    }

    pub fn is_member(env: Env, id: u32, who: Address) -> bool {
        is_member(&env, id, &who)
    }

    pub fn contribution_of(env: Env, id: u32, who: Address) -> i128 {
        env.storage().persistent().get(&Key::Contribution(id, who)).unwrap_or(0)
    }

    /// Queries a withdrawal request / payout by circle ID and request ID.
    pub fn get_payout(env: Env, id: u32, rid: u32) -> Result<Payout, Error> {
        load(&env, id)?;
        env.storage().persistent().get(&Key::Payout(id, rid)).ok_or(Error::NotFound)
    }

    /// Alias for `get_payout`.
    pub fn get_withdrawal_request(env: Env, id: u32, rid: u32) -> Result<Payout, Error> {
        Self::get_payout(env, id, rid)
    }

    /// Queries the approval progress of a payout request.
    pub fn get_approval_progress(env: Env, id: u32, rid: u32) -> Result<ApprovalProgress, Error> {
        load(&env, id)?;
        let p: Payout = env.storage().persistent().get(&Key::Payout(id, rid)).ok_or(Error::NotFound)?;
        let count = p.approvals.len();
        Ok(ApprovalProgress {
            current_approvals: count,
            required_threshold: p.threshold,
            is_approved: count >= p.threshold,
            is_executed: p.executed,
            approvers: p.approvals,
        })
    }

    /// Queries circle balance breakdown (total, reserved, available, contributions, payouts).
    pub fn get_circle_balance(env: Env, id: u32) -> Result<CircleBalance, Error> {
        let c = load(&env, id)?;
        Ok(CircleBalance {
            asset: c.token,
            total_balance: c.balance,
            reserved_balance: c.reserved,
            available_balance: c.balance - c.reserved,
            total_contributed: c.total_contributed,
            total_paid: c.total_paid,
        })
    }

    /// Queries available funds for a circle (balance minus reserved).
    pub fn get_available_balance(env: Env, id: u32) -> Result<i128, Error> {
        let c = load(&env, id)?;
        Ok(c.balance - c.reserved)
    }

    /// Queries cumulative contributions made to the circle.
    pub fn get_total_contributions(env: Env, id: u32) -> Result<i128, Error> {
        let c = load(&env, id)?;
        Ok(c.total_contributed)
    }

    /// Queries cumulative payouts executed by the circle.
    pub fn get_total_payouts(env: Env, id: u32) -> Result<i128, Error> {
        let c = load(&env, id)?;
        Ok(c.total_paid)
    }

    /// Queries global balance breakdown across all circles for a specific asset.
    pub fn get_asset_balance_info(env: Env, asset: Address) -> AssetBalanceInfo {
        let total = get_asset_bal(&env, &asset);
        let res = get_asset_res(&env, &asset);
        AssetBalanceInfo {
            asset: asset.clone(),
            total_balance: total,
            reserved_balance: res,
            available_balance: total - res,
            total_contributed: get_asset_contrib(&env, &asset),
            total_paid: get_asset_p(&env, &asset),
        }
    }

    /// Queries total tracked balance for a specific asset across all circles.
    pub fn get_asset_balance(env: Env, asset: Address) -> i128 {
        get_asset_bal(&env, &asset)
    }

    /// Queries available balance for a specific asset across all circles.
    pub fn get_asset_available_balance(env: Env, asset: Address) -> i128 {
        get_asset_bal(&env, &asset) - get_asset_res(&env, &asset)
    }

    /// Reconciles the circle's recorded balance against the contract's actual on-chain token balance.
    pub fn reconcile_circle_balance(env: Env, id: u32) -> Result<ReconciliationResult, Error> {
        let c = load(&env, id)?;
        let actual = token::Client::new(&env, &c.token).balance(&env.current_contract_address());
        Ok(ReconciliationResult {
            on_chain_token_balance: actual,
            recorded_balance: c.balance,
            is_reconciled: actual >= c.balance,
        })
    }

    /// Reconciles the tracked total asset balance against the contract's actual on-chain token balance.
    pub fn reconcile_asset_balance(env: Env, asset: Address) -> ReconciliationResult {
        let recorded = get_asset_bal(&env, &asset);
        let actual = token::Client::new(&env, &asset).balance(&env.current_contract_address());
        ReconciliationResult {
            on_chain_token_balance: actual,
            recorded_balance: recorded,
            is_reconciled: actual >= recorded,
        }
    }
}

#[cfg(test)]
mod test;
