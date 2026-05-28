#![no_std]

//! # Loyalty Points Contract
//!
//! Merchants award points to users on payments; users redeem them.
//!
//! ## Design
//! * Points are earned at a configurable rate (points per 1 000 token units).
//! * Points expire after a configurable TTL (ledger-based).
//! * Redemption burns points and transfers tokens from a funded reserve.
//! * Transfers between users are disabled (non-transferable by default).
//! * Only the admin can award points, set the earn rate, and manage the reserve.

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype,
    symbol_short, token::Client as TokenClient,
    Address, Env, Map, Symbol,
};

// ---------------------------------------------------------------------------
// Storage keys
// ---------------------------------------------------------------------------

const KEY_ADMIN: Symbol = symbol_short!("admin");
const KEY_TOKEN: Symbol = symbol_short!("token");
/// Points earned per 1 000 token units paid.
const KEY_EARN_RATE: Symbol = symbol_short!("earn_rate");
/// Ledgers until points expire (0 = never).
const KEY_TTL: Symbol = symbol_short!("ttl");
/// Map<Address, PointBalance>
const KEY_BALANCES: Symbol = symbol_short!("balances");
/// Tokens per point for redemption.
const KEY_REDEEM_RATE: Symbol = symbol_short!("rdm_rate");

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct PointBalance {
    pub points: i128,
    /// Ledger sequence at which these points expire (0 = never).
    pub expires_at: u32,
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
    InsufficientPoints = 5,
    PointsExpired = 6,
    InvalidRate = 7,
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct LoyaltyPoints;

#[contractimpl]
impl LoyaltyPoints {

    // -----------------------------------------------------------------------
    // Initialisation
    // -----------------------------------------------------------------------

    /// * `earn_rate`  – points awarded per 1 000 token units (e.g. 10 = 1 %)
    /// * `redeem_rate`– token units returned per point redeemed
    /// * `ttl_ledgers` – ledgers until points expire; 0 = never
    pub fn initialize(
        env: Env,
        admin: Address,
        token: Address,
        earn_rate: i128,
        redeem_rate: i128,
        ttl_ledgers: u32,
    ) -> Result<(), Error> {
        if env.storage().instance().has(&KEY_ADMIN) {
            return Err(Error::AlreadyInitialized);
        }
        if earn_rate <= 0 || redeem_rate <= 0 {
            return Err(Error::InvalidRate);
        }
        admin.require_auth();

        env.storage().instance().set(&KEY_ADMIN, &admin);
        env.storage().instance().set(&KEY_TOKEN, &token);
        env.storage().instance().set(&KEY_EARN_RATE, &earn_rate);
        env.storage().instance().set(&KEY_REDEEM_RATE, &redeem_rate);
        env.storage().instance().set(&KEY_TTL, &ttl_ledgers);
        env.storage()
            .instance()
            .set(&KEY_BALANCES, &Map::<Address, PointBalance>::new(&env));

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Earning
    // -----------------------------------------------------------------------

    /// Award points to `user` based on `payment_amount`.
    /// `points = payment_amount * earn_rate / 1_000`
    pub fn earn(env: Env, user: Address, payment_amount: i128) -> Result<i128, Error> {
        Self::require_initialized(&env)?;
        let admin: Address = env.storage().instance().get(&KEY_ADMIN).unwrap();
        admin.require_auth();

        if payment_amount <= 0 {
            return Err(Error::ZeroAmount);
        }

        let earn_rate: i128 = env.storage().instance().get(&KEY_EARN_RATE).unwrap();
        let points = payment_amount * earn_rate / 1_000;
        if points == 0 {
            return Ok(0);
        }

        let ttl: u32 = env.storage().instance().get(&KEY_TTL).unwrap();
        let expires_at = if ttl == 0 {
            0u32
        } else {
            env.ledger().sequence() + ttl
        };

        let mut balances: Map<Address, PointBalance> =
            env.storage().instance().get(&KEY_BALANCES).unwrap();

        let current = balances.get(user.clone()).unwrap_or(PointBalance {
            points: 0,
            expires_at,
        });

        // If existing points haven't expired, add to them; otherwise reset.
        let new_balance = if current.expires_at == 0
            || env.ledger().sequence() <= current.expires_at
        {
            PointBalance {
                points: current.points + points,
                expires_at: if expires_at == 0 { current.expires_at } else { expires_at },
            }
        } else {
            PointBalance { points, expires_at }
        };

        balances.set(user.clone(), new_balance);
        env.storage().instance().set(&KEY_BALANCES, &balances);

        env.events().publish(
            (symbol_short!("loyalty"), symbol_short!("earned")),
            (user, points),
        );

        Ok(points)
    }

    // -----------------------------------------------------------------------
    // Redemption
    // -----------------------------------------------------------------------

    /// Burn `points` and transfer `points * redeem_rate` tokens to `user`.
    /// The contract must hold sufficient token balance (funded by admin).
    pub fn redeem(env: Env, user: Address, points: i128) -> Result<i128, Error> {
        user.require_auth();
        if points <= 0 {
            return Err(Error::ZeroAmount);
        }
        Self::require_initialized(&env)?;

        let mut balances: Map<Address, PointBalance> =
            env.storage().instance().get(&KEY_BALANCES).unwrap();

        let bal = balances
            .get(user.clone())
            .unwrap_or(PointBalance { points: 0, expires_at: 0 });

        // Check expiry.
        if bal.expires_at != 0 && env.ledger().sequence() > bal.expires_at {
            return Err(Error::PointsExpired);
        }

        if bal.points < points {
            return Err(Error::InsufficientPoints);
        }

        let redeem_rate: i128 = env.storage().instance().get(&KEY_REDEEM_RATE).unwrap();
        let token_amount = points * redeem_rate;

        // Update state before transfer (CEI).
        balances.set(
            user.clone(),
            PointBalance {
                points: bal.points - points,
                expires_at: bal.expires_at,
            },
        );
        env.storage().instance().set(&KEY_BALANCES, &balances);

        let token: Address = env.storage().instance().get(&KEY_TOKEN).unwrap();
        TokenClient::new(&env, &token)
            .transfer(&env.current_contract_address(), &user, &token_amount);

        env.events().publish(
            (symbol_short!("loyalty"), symbol_short!("redeemed")),
            (user, points, token_amount),
        );

        Ok(token_amount)
    }

    // -----------------------------------------------------------------------
    // Admin: fund reserve
    // -----------------------------------------------------------------------

    pub fn fund_reserve(env: Env, from: Address, amount: i128) -> Result<(), Error> {
        from.require_auth();
        if amount <= 0 {
            return Err(Error::ZeroAmount);
        }
        Self::require_initialized(&env)?;
        let token: Address = env.storage().instance().get(&KEY_TOKEN).unwrap();
        TokenClient::new(&env, &token)
            .transfer(&from, &env.current_contract_address(), &amount);
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Admin: configuration
    // -----------------------------------------------------------------------

    pub fn set_earn_rate(env: Env, earn_rate: i128) -> Result<(), Error> {
        Self::require_initialized(&env)?;
        if earn_rate <= 0 {
            return Err(Error::InvalidRate);
        }
        let admin: Address = env.storage().instance().get(&KEY_ADMIN).unwrap();
        admin.require_auth();
        env.storage().instance().set(&KEY_EARN_RATE, &earn_rate);
        Ok(())
    }

    pub fn set_redeem_rate(env: Env, redeem_rate: i128) -> Result<(), Error> {
        Self::require_initialized(&env)?;
        if redeem_rate <= 0 {
            return Err(Error::InvalidRate);
        }
        let admin: Address = env.storage().instance().get(&KEY_ADMIN).unwrap();
        admin.require_auth();
        env.storage().instance().set(&KEY_REDEEM_RATE, &redeem_rate);
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

    pub fn balance_of(env: Env, user: Address) -> Result<PointBalance, Error> {
        Self::require_initialized(&env)?;
        let balances: Map<Address, PointBalance> =
            env.storage().instance().get(&KEY_BALANCES).unwrap();
        Ok(balances.get(user).unwrap_or(PointBalance { points: 0, expires_at: 0 }))
    }

    pub fn get_earn_rate(env: Env) -> Result<i128, Error> {
        Self::require_initialized(&env)?;
        Ok(env.storage().instance().get(&KEY_EARN_RATE).unwrap())
    }

    pub fn get_redeem_rate(env: Env) -> Result<i128, Error> {
        Self::require_initialized(&env)?;
        Ok(env.storage().instance().get(&KEY_REDEEM_RATE).unwrap())
    }

    // -----------------------------------------------------------------------
    // Internal
    // -----------------------------------------------------------------------

    fn require_initialized(env: &Env) -> Result<(), Error> {
        if !env.storage().instance().has(&KEY_ADMIN) {
            return Err(Error::NotInitialized);
        }
        Ok(())
    }
}

mod test;
