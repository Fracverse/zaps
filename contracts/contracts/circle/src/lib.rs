#![no_std]
#![allow(unexpected_cfgs)]
//! Zaps Circles: pooled contributions with payout limits, approval governance
//! and membership exit rules. Each circle holds one token.
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
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Payout {
    pub requester: Address,
    pub to: Address,
    pub amount: i128,
    /// Threshold in force when the request was made; later governance changes do not alter it.
    pub threshold: u32,
    pub approvals: Vec<Address>,
    pub executed: bool,
}

#[contracttype]
enum Key {
    Count,
    Circle(u32),
    Member(u32, Address),
    Contribution(u32, Address),
    Pending(u32, Address),
    Payout(u32, u32),
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

#[contractimpl]
impl CircleContract {
    /// Creates a circle. The admin becomes the first member and sole approver (threshold 1).
    pub fn create_circle(env: Env, admin: Address, token: Address, max_payout: i128) -> Result<u32, Error> {
        admin.require_auth();
        if max_payout < 0 {
            return Err(Error::InvalidAmount);
        }
        let id: u32 = env.storage().instance().get(&Key::Count).unwrap_or(0);
        env.storage().instance().set(&Key::Count, &(id + 1));
        let c = Circle {
            admin: admin.clone(),
            token,
            balance: 0,
            reserved: 0,
            max_payout,
            threshold: 1,
            approvers: Vec::from_array(&env, [admin.clone()]),
            next_request: 0,
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
        c.balance += amount;
        save(&env, id, &c);
        let key = Key::Contribution(id, member.clone());
        let total: i128 = env.storage().persistent().get(&key).unwrap_or(0);
        env.storage().persistent().set(&key, &(total + amount));
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

    /// Admin-only. Existing requests keep the threshold they were created with.
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

    pub fn request_payout(env: Env, id: u32, member: Address, to: Address, amount: i128) -> Result<u32, Error> {
        member.require_auth();
        let mut c = load(&env, id)?;
        if !is_member(&env, id, &member) {
            return Err(Error::NotMember);
        }
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        if c.max_payout > 0 && amount > c.max_payout {
            return Err(Error::ExceedsLimit);
        }
        if amount > c.balance - c.reserved {
            return Err(Error::InsufficientBalance);
        }
        let rid = c.next_request;
        c.next_request += 1;
        c.reserved += amount;
        let p = Payout {
            requester: member.clone(),
            to,
            amount,
            threshold: c.threshold,
            approvals: Vec::new(&env),
            executed: false,
        };
        env.storage().persistent().set(&Key::Payout(id, rid), &p);
        let n = pending(&env, id, &member);
        env.storage().persistent().set(&Key::Pending(id, member), &(n + 1));
        save(&env, id, &c);
        env.events().publish((symbol_short!("request"), id, rid), amount);
        Ok(rid)
    }

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
        env.storage().persistent().set(&Key::Payout(id, rid), &p);
        env.events().publish((symbol_short!("approved"), id, rid), approver);
        Ok(())
    }

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
        p.executed = true;
        c.balance -= p.amount;
        c.reserved -= p.amount;
        env.storage().persistent().set(&Key::Payout(id, rid), &p);
        let n = pending(&env, id, &p.requester);
        env.storage().persistent().set(&Key::Pending(id, p.requester.clone()), &(n - 1));
        save(&env, id, &c);
        token::Client::new(&env, &c.token).transfer(&env.current_contract_address(), &p.to, &p.amount);
        env.events().publish((symbol_short!("paid"), id, rid), p.amount);
        Ok(())
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

    pub fn get_circle(env: Env, id: u32) -> Result<Circle, Error> {
        load(&env, id)
    }

    pub fn is_member(env: Env, id: u32, who: Address) -> bool {
        is_member(&env, id, &who)
    }

    pub fn contribution_of(env: Env, id: u32, who: Address) -> i128 {
        env.storage().persistent().get(&Key::Contribution(id, who)).unwrap_or(0)
    }
}

#[cfg(test)]
mod test;
