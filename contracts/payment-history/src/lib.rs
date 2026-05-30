#![no_std] // Crucial: Fixes the duplicate panic_impl error

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Vec, vec};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Payment {
    pub id: u64,
    pub merchant: Address,
    pub customer: Address,
    pub amount: i128,
    pub timestamp: u64,
    pub is_private: bool,
}

#[contracttype]
pub enum DataKey {
    NextId,
    Payments,
}

#[contract]
pub struct PaymentHistoryContract;

#[contractimpl]
impl PaymentHistoryContract {
    /// Records a new payment on-chain
    pub fn record_payment(env: Env, merchant: Address, customer: Address, amount: i128, is_private: bool) -> u64 {
        customer.require_auth();

        let mut next_id: u64 = env.storage().instance().get(&DataKey::NextId).unwrap_or(0);
        let mut payments: Vec<Payment> = env.storage().instance().get(&DataKey::Payments).unwrap_or(vec![&env]);

        let new_payment = Payment {
            id: next_id,
            merchant: merchant.clone(),
            customer: customer.clone(),
            amount,
            timestamp: env.ledger().timestamp(),
            is_private,
        };

        payments.push_back(new_payment);
        
        env.storage().instance().set(&DataKey::Payments, &payments);
        env.storage().instance().set(&DataKey::NextId, &(next_id + 1));

        next_id
    }

    /// Fetches history. We return the inner raw elements to comply with macro rules.
    pub fn get_history(env: Env, caller: Address, offset: u32, limit: u32) -> Vec<Payment> {
        let payments: Vec<Payment> = env.storage().instance().get(&DataKey::Payments).unwrap_or(vec![&env]);
        let total = payments.len();
        let mut result = vec![&env];

        if offset >= total || limit == 0 {
            return result;
        }

        let end = core::cmp::min(offset + limit, total);

        for i in offset..end {
            let p = payments.get(i).unwrap();
            
            // Privacy Controls: Mask metrics if marked private and viewer isn't customer/merchant
            if p.is_private && caller != p.customer && caller != p.merchant {
                let masked = Payment {
                    id: p.id,
                    merchant: p.merchant.clone(),
                    customer: p.customer.clone(), // Keep addresses but wipe the value
                    amount: 0,                    // Mask transaction size
                    timestamp: p.timestamp,
                    is_private: true,
                };
                result.push_back(masked);
            } else {
                result.push_back(p);
            }
        }

        result
    }
}