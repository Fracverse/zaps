#![no_std]
use soroban_sdk::{contract, contracterror, contractimpl, contracttype, Address, Env, Symbol};

#[contracttype]
pub enum DataKey {
    Balance(Address), // merchant_id => balance
    PaymentRouter,    // authorized payment router
    PayoutContract,   // authorized payout contract
    Admin,            // contract administrator
}

#[contracttype]
pub struct BalanceCreditedEvent {
    pub merchant_id: Address,
    pub amount: i128,
    pub resulting_balance: i128,
}

#[contracttype]
pub struct BalanceDebitedEvent {
    pub merchant_id: Address,
    pub amount: i128,
    pub resulting_balance: i128,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    NegativeAmount = 1,
    InsufficientBalance = 2,
    UnauthorizedCaller = 3,
    MerchantNotInitialized = 4,
    AlreadyInitialized = 5,
    NotInitialized = 6,
}

#[contract]
pub struct MerchantVault;

#[contractimpl]
impl MerchantVault {
    /// Initialize the contract with admin and authorized contracts
    pub fn initialize(
        env: Env,
        admin: Address,
        payment_router: Address,
        payout_contract: Address,
    ) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }

        admin.require_auth();

        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::PaymentRouter, &payment_router);
        env.storage()
            .instance()
            .set(&DataKey::PayoutContract, &payout_contract);

        Ok(())
    }

    /// Initialize a merchant account with zero balance
    pub fn init_merchant(env: Env, merchant_id: Address) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;

        admin.require_auth();

        if env
            .storage()
            .persistent()
            .has(&DataKey::Balance(merchant_id.clone()))
        {
            return Err(Error::AlreadyInitialized);
        }

        env.storage()
            .persistent()
            .set(&DataKey::Balance(merchant_id), &0i128);

        Ok(())
    }

    /// Credit merchant balance
    pub fn credit(env: Env, merchant_id: Address, amount: i128) -> Result<i128, Error> {
        let payment_router: Address = env
            .storage()
            .instance()
            .get(&DataKey::PaymentRouter)
            .ok_or(Error::NotInitialized)?;

        payment_router.require_auth();

        if amount < 0 {
            return Err(Error::NegativeAmount);
        }

        if !env
            .storage()
            .persistent()
            .has(&DataKey::Balance(merchant_id.clone()))
        {
            return Err(Error::MerchantNotInitialized);
        }

        let current_balance: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::Balance(merchant_id.clone()))
            .unwrap_or(0);

        let new_balance = current_balance
            .checked_add(amount)
            .expect("Balance overflow");

        env.storage()
            .persistent()
            .set(&DataKey::Balance(merchant_id.clone()), &new_balance);

        env.events().publish(
            (Symbol::new(&env, "balance_credited"), merchant_id.clone()),
            BalanceCreditedEvent {
                merchant_id,
                amount,
                resulting_balance: new_balance,
            },
        );

        Ok(new_balance)
    }

    /// Debit merchant balance
    /// Only callable by authorized payout/refund contract
    pub fn debit(env: Env, merchant_id: Address, amount: i128) -> Result<i128, Error> {
        let payout_contract: Address = env
            .storage()
            .instance()
            .get(&DataKey::PayoutContract)
            .ok_or(Error::NotInitialized)?;

        payout_contract.require_auth();

        if amount < 0 {
            return Err(Error::NegativeAmount);
        }

        if !env
            .storage()
            .persistent()
            .has(&DataKey::Balance(merchant_id.clone()))
        {
            return Err(Error::MerchantNotInitialized);
        }

        let current_balance: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::Balance(merchant_id.clone()))
            .unwrap_or(0);

        if current_balance < amount {
            return Err(Error::InsufficientBalance);
        }

        let new_balance = current_balance - amount;

        env.storage()
            .persistent()
            .set(&DataKey::Balance(merchant_id.clone()), &new_balance);

        env.events().publish(
            (Symbol::new(&env, "balance_debited"), merchant_id.clone()),
            BalanceDebitedEvent {
                merchant_id,
                amount,
                resulting_balance: new_balance,
            },
        );

        Ok(new_balance)
    }

    /// Get merchant balance (read-only, public)
    pub fn balance_of(env: Env, merchant_id: Address) -> Result<i128, Error> {
        if !env
            .storage()
            .persistent()
            .has(&DataKey::Balance(merchant_id.clone()))
        {
            return Err(Error::MerchantNotInitialized);
        }

        let balance: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::Balance(merchant_id))
            .unwrap_or(0);

        Ok(balance)
    }

    /// Update authorized payment router (admin only)
    pub fn update_payment_router(env: Env, new_router: Address) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;

        admin.require_auth();

        env.storage()
            .instance()
            .set(&DataKey::PaymentRouter, &new_router);

        Ok(())
    }

    /// Update authorized payout contract (admin only)
    pub fn update_payout_contract(env: Env, new_payout: Address) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;

        admin.require_auth();

        env.storage()
            .instance()
            .set(&DataKey::PayoutContract, &new_payout);

        Ok(())
    }
}
mod test;
