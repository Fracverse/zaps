#![no_std]
#![allow(unexpected_cfgs)]
use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, token, Address, Env, Symbol,
};

// ─── SC-016: Storage keys & configuration ────────────────────────────────────

const OWNER_KEY: Symbol = symbol_short!("owner");
const TOKEN_KEY: Symbol = symbol_short!("token");
const APY_KEY: Symbol = symbol_short!("apy");
const SHARES_KEY: Symbol = symbol_short!("tot_shr");
const ASSETS_KEY: Symbol = symbol_short!("tot_ast");
const IDX_KEY: Symbol = symbol_short!("yld_idx");
const IDX_LED_KEY: Symbol = symbol_short!("idx_led");
const PROTO_BAL_KEY: Symbol = symbol_short!("p_bal");
const PROTO_REW_KEY: Symbol = symbol_short!("p_rew");
const PROTO_LED_KEY: Symbol = symbol_short!("p_led");

/// Precision factor used in all fixed-point math (1e8).
const PRECISION: i128 = 100_000_000;

/// Mock lending protocol annualized reward rate (basis points).
#[cfg(any(feature = "mock-protocol", test))]
const MOCK_PROTOCOL_REWARD_BPS: i128 = 650; // 6.50%

#[cfg(any(feature = "mock-protocol", test))]
mod sandbox_protocol {
    use super::{
        Env, MOCK_PROTOCOL_REWARD_BPS, PROTO_BAL_KEY, PROTO_LED_KEY, PROTO_REW_KEY, Symbol,
    };

    const LEDGERS_PER_YEAR: i128 = 6_307_200; // ~5s per ledger

    fn checkpoint(env: &Env) {
        let now = env.ledger().sequence();
        let last: u32 = env.storage().instance().get(&PROTO_LED_KEY).unwrap_or(now);
        let delta = (now - last) as i128;
        if delta > 0 {
            let supplied: i128 = env.storage().instance().get(&PROTO_BAL_KEY).unwrap_or(0);
            if supplied > 0 {
                let accrued = supplied
                    .checked_mul(MOCK_PROTOCOL_REWARD_BPS)
                    .expect("overflow")
                    .checked_mul(delta)
                    .expect("overflow")
                    / (10_000i128
                        .checked_mul(LEDGERS_PER_YEAR)
                        .expect("overflow"));
                if accrued > 0 {
                    let rewards: i128 = env.storage().instance().get(&PROTO_REW_KEY).unwrap_or(0);
                    env.storage()
                        .instance()
                        .set(&PROTO_REW_KEY, &(rewards + accrued));
                }
            }
        }
        env.storage().instance().set(&PROTO_LED_KEY, &now);
    }

    pub fn supply(env: &Env, amount: i128) {
        if amount <= 0 {
            return;
        }
        checkpoint(env);
        let current: i128 = env.storage().instance().get(&PROTO_BAL_KEY).unwrap_or(0);
        env.storage().instance().set(&PROTO_BAL_KEY, &(current + amount));
        env.events().publish(
            (Symbol::new(env, "MockSupply"),),
            (amount, current + amount),
        );
    }

    pub fn redeem(env: &Env, amount: i128) -> i128 {
        if amount <= 0 {
            return 0;
        }
        checkpoint(env);
        let current: i128 = env.storage().instance().get(&PROTO_BAL_KEY).unwrap_or(0);
        let redeemed = if amount > current { current } else { amount };
        env.storage()
            .instance()
            .set(&PROTO_BAL_KEY, &(current - redeemed));
        env.events().publish(
            (Symbol::new(env, "MockRedeem"),),
            (amount, redeemed, current - redeemed),
        );
        redeemed
    }

    pub fn pending_rewards(env: &Env) -> i128 {
        let now = env.ledger().sequence();
        let last: u32 = env.storage().instance().get(&PROTO_LED_KEY).unwrap_or(now);
        let delta = (now - last) as i128;
        let supplied: i128 = env.storage().instance().get(&PROTO_BAL_KEY).unwrap_or(0);
        let current_rewards: i128 = env.storage().instance().get(&PROTO_REW_KEY).unwrap_or(0);
        if delta <= 0 || supplied <= 0 {
            return current_rewards;
        }
        let incremental = supplied
            .checked_mul(MOCK_PROTOCOL_REWARD_BPS)
            .expect("overflow")
            .checked_mul(delta)
            .expect("overflow")
            / (10_000i128
                .checked_mul(LEDGERS_PER_YEAR)
                .expect("overflow"));
        current_rewards + incremental
    }

    pub fn claim_rewards(env: &Env) -> i128 {
        checkpoint(env);
        let rewards: i128 = env.storage().instance().get(&PROTO_REW_KEY).unwrap_or(0);
        env.storage().instance().set(&PROTO_REW_KEY, &0i128);
        env.events()
            .publish((Symbol::new(env, "MockClaim"),), (rewards,));
        rewards
    }

    pub fn supplied_balance(env: &Env) -> i128 {
        env.storage().instance().get(&PROTO_BAL_KEY).unwrap_or(0)
    }
}

#[cfg(not(any(feature = "mock-protocol", test)))]
mod sandbox_protocol {
    use super::Env;

    pub fn supply(_env: &Env, _amount: i128) {}
    pub fn redeem(_env: &Env, amount: i128) -> i128 {
        amount
    }
    pub fn pending_rewards(_env: &Env) -> i128 {
        0
    }
    pub fn claim_rewards(_env: &Env) -> i128 {
        0
    }
    pub fn supplied_balance(_env: &Env) -> i128 {
        0
    }
}

#[contracttype]
enum DataKey {
    UserShares(Address),
}

#[contract]
pub struct YieldVaultContract;

#[contractimpl]
impl YieldVaultContract {
    fn require_owner(env: &Env, caller: &Address) {
        let owner: Address = env
            .storage()
            .instance()
            .get(&OWNER_KEY)
            .expect("not initialized");
        assert!(caller == &owner, "only owner");
    }

    /// SC-016: One-time initializer. Sets owner, token address, and initial APY.
    /// `apy_bps` is the annual percentage yield in basis points (e.g. 500 = 5%).
    pub fn initialize(env: Env, owner: Address, token: Address, apy_bps: u32) {
        if env.storage().instance().has(&OWNER_KEY) {
            panic!("already initialized");
        }
        env.storage().instance().set(&OWNER_KEY, &owner);
        env.storage().instance().set(&TOKEN_KEY, &token);
        env.storage().instance().set(&APY_KEY, &apy_bps);
        // Yield index starts at 1.0 (represented as PRECISION)
        env.storage().instance().set(&IDX_KEY, &PRECISION);
        env.storage()
            .instance()
            .set(&IDX_LED_KEY, &env.ledger().sequence());
        env.storage().instance().set(&SHARES_KEY, &0i128);
        env.storage().instance().set(&ASSETS_KEY, &0i128);
        env.storage().instance().set(&PROTO_BAL_KEY, &0i128);
        env.storage().instance().set(&PROTO_REW_KEY, &0i128);
        env.storage()
            .instance()
            .set(&PROTO_LED_KEY, &env.ledger().sequence());
    }

    // ─── SC-019: Yield compounding math ──────────────────────────────────────

    /// Compute the current yield index by accruing yield since the last update.
    /// Uses integer arithmetic scaled by PRECISION to avoid overflow.
    /// Formula: new_index = old_index * (1 + apy_bps/10000 * delta_ledgers / ledgers_per_year)
    /// Approximated as: new_index = old_index + old_index * apy_bps * delta / (10000 * LEDGERS_PER_YEAR)
    fn current_index(env: &Env) -> i128 {
        const LEDGERS_PER_YEAR: i128 = 6_307_200; // ~5s per ledger
        let old_index: i128 = env.storage().instance().get(&IDX_KEY).unwrap_or(PRECISION);
        let last_ledger: u32 = env.storage().instance().get(&IDX_LED_KEY).unwrap_or(0);
        let apy_bps: u32 = env.storage().instance().get(&APY_KEY).unwrap_or(0);
        let delta = (env.ledger().sequence() - last_ledger) as i128;
        if delta == 0 || apy_bps == 0 {
            return old_index;
        }
        // Scaled addition; all intermediate values stay within i128 for realistic APYs and time windows
        let accrued = old_index
            .checked_mul(apy_bps as i128)
            .expect("overflow")
            .checked_mul(delta)
            .expect("overflow")
            / (10_000i128.checked_mul(LEDGERS_PER_YEAR).expect("overflow"));
        old_index.checked_add(accrued).expect("overflow")
    }

    /// Persist the latest yield index and reset the reference ledger.
    fn checkpoint_index(env: &Env) {
        let idx = Self::current_index(env);
        env.storage().instance().set(&IDX_KEY, &idx);
        env.storage()
            .instance()
            .set(&IDX_LED_KEY, &env.ledger().sequence());
    }

    // ─── SC-017: Deposit ──────────────────────────────────────────────────────

    /// Deposit `amount` tokens from `depositor` into the vault.
    /// Mints vault shares proportional to the current yield index.
    /// shares_minted = amount * PRECISION / current_index
    pub fn deposit(env: Env, depositor: Address, amount: i128) {
        depositor.require_auth();
        assert!(amount > 0, "amount must be positive");

        Self::checkpoint_index(&env);

        let token_addr: Address = env
            .storage()
            .instance()
            .get(&TOKEN_KEY)
            .expect("not initialized");
        let vault_addr = env.current_contract_address();

        // Pull tokens from depositor into vault
        token::Client::new(&env, &token_addr).transfer(&depositor, &vault_addr, &amount);
        // Simulate routing liquidity into an external lending protocol adapter.
        sandbox_protocol::supply(&env, amount);

        let index = Self::current_index(&env);
        let shares = amount.checked_mul(PRECISION).expect("overflow") / index;
        assert!(shares > 0, "deposit too small");

        // Update user shares
        let user_key = DataKey::UserShares(depositor.clone());
        let prev_shares: i128 = env.storage().persistent().get(&user_key).unwrap_or(0);
        env.storage()
            .persistent()
            .set(&user_key, &(prev_shares + shares));

        // Update totals
        let tot_shares: i128 = env.storage().instance().get(&SHARES_KEY).unwrap_or(0);
        let tot_assets: i128 = env.storage().instance().get(&ASSETS_KEY).unwrap_or(0);
        env.storage()
            .instance()
            .set(&SHARES_KEY, &(tot_shares + shares));
        env.storage()
            .instance()
            .set(&ASSETS_KEY, &(tot_assets + amount));

        env.events().publish(
            (Symbol::new(&env, "Deposited"),),
            (depositor, amount, shares),
        );
    }

    // ─── SC-018: Withdraw ─────────────────────────────────────────────────────

    /// Burn `shares` from `user` and return the equivalent tokens (principal + yield).
    /// assets_out = shares * current_index / PRECISION
    pub fn withdraw(env: Env, user: Address, shares: i128) {
        user.require_auth();
        assert!(shares > 0, "shares must be positive");

        Self::checkpoint_index(&env);

        let user_key = DataKey::UserShares(user.clone());
        let user_shares: i128 = env.storage().persistent().get(&user_key).unwrap_or(0);
        assert!(user_shares >= shares, "insufficient shares");

        let index = Self::current_index(&env);
        let assets_out = shares.checked_mul(index).expect("overflow") / PRECISION;
        assert!(assets_out > 0, "withdrawal too small");
        // Simulate retrieving liquidity from the external lending protocol adapter.
        sandbox_protocol::redeem(&env, assets_out);

        // Deduct shares
        env.storage()
            .persistent()
            .set(&user_key, &(user_shares - shares));

        // Update totals (clamp to zero to guard against rounding drift)
        let tot_shares: i128 = env.storage().instance().get(&SHARES_KEY).unwrap_or(0);
        let tot_assets: i128 = env.storage().instance().get(&ASSETS_KEY).unwrap_or(0);
        env.storage()
            .instance()
            .set(&SHARES_KEY, &(tot_shares - shares).max(0));
        env.storage()
            .instance()
            .set(&ASSETS_KEY, &(tot_assets - assets_out).max(0));

        let token_addr: Address = env
            .storage()
            .instance()
            .get(&TOKEN_KEY)
            .expect("not initialized");
        let vault_addr = env.current_contract_address();
        token::Client::new(&env, &token_addr).transfer(&vault_addr, &user, &assets_out);

        env.events().publish(
            (Symbol::new(&env, "Withdrawn"),),
            (user, shares, assets_out),
        );
    }

    // ─── Mock protocol adapter interface ──────────────────────────────────────

    /// Owner-controlled mock protocol supply entrypoint for sandbox/testing use.
    pub fn mock_protocol_supply(env: Env, caller: Address, amount: i128) {
        caller.require_auth();
        Self::require_owner(&env, &caller);
        assert!(amount > 0, "amount must be positive");
        sandbox_protocol::supply(&env, amount);
    }

    /// Owner-controlled mock protocol redeem entrypoint for sandbox/testing use.
    pub fn mock_protocol_redeem(env: Env, caller: Address, amount: i128) -> i128 {
        caller.require_auth();
        Self::require_owner(&env, &caller);
        assert!(amount > 0, "amount must be positive");
        sandbox_protocol::redeem(&env, amount)
    }

    /// Returns current pending simulated rewards from the mock protocol.
    pub fn mock_protocol_pending_rewards(env: Env) -> i128 {
        sandbox_protocol::pending_rewards(&env)
    }

    /// Claims accrued simulated rewards from the mock protocol.
    pub fn mock_protocol_claim_rewards(env: Env, caller: Address) -> i128 {
        caller.require_auth();
        Self::require_owner(&env, &caller);
        sandbox_protocol::claim_rewards(&env)
    }

    /// Returns the amount currently supplied to the mock protocol.
    pub fn mock_protocol_supplied_balance(env: Env) -> i128 {
        sandbox_protocol::supplied_balance(&env)
    }

    // ─── View helpers ─────────────────────────────────────────────────────────

    pub fn shares_of(env: Env, user: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::UserShares(user))
            .unwrap_or(0)
    }

    pub fn total_shares(env: Env) -> i128 {
        env.storage().instance().get(&SHARES_KEY).unwrap_or(0)
    }

    pub fn total_assets(env: Env) -> i128 {
        env.storage().instance().get(&ASSETS_KEY).unwrap_or(0)
    }

    pub fn yield_index(env: Env) -> i128 {
        Self::current_index(&env)
    }
}
