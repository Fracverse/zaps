#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, symbol_short,
    token::Client as TokenClient, Address, Env, Symbol, Vec,
};

const KEY_ADMIN: Symbol = symbol_short!("admin");
const KEY_TOKEN: Symbol = symbol_short!("token");
const KEY_RECIPIENTS: Symbol = symbol_short!("recips");
const KEY_SPLIT_MODE: Symbol = symbol_short!("mode");
const KEY_VERSION: Symbol = symbol_short!("version");

#[contracttype]
#[derive(Clone)]
pub struct Recipient {
    pub address: Address,
    pub share: i128,
}

#[contracttype]
#[derive(Clone)]
pub struct SplitConfig {
    pub recipients: Vec<Recipient>,
    pub split_type: SplitType,
}

#[contracttype]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum SplitType {
    Percentage = 1,
    Fixed = 2,
}

#[contracterror]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum SplitterError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    InvalidRecipients = 4,
    SharesExceedTotal = 5,
    ZeroAmount = 6,
    RecipientMismatch = 7,
    NoRemainderRecipient = 8,
}

fn bump_instance(env: &Env) {
    let ttl = 6_307_200u32;
    let threshold = 100_000u32;
    env.storage().instance().extend_ttl(threshold, ttl);
}

fn require_admin(env: &Env) -> Address {
    let admin: Address = env
        .storage()
        .instance()
        .get(&KEY_ADMIN)
        .unwrap_or_else(|| panic_with_error!(env, SplitterError::NotInitialized));
    admin.require_auth();
    admin
}

fn validate_config(recipients: &Vec<Recipient>, split_type: SplitType) -> bool {
    if recipients.is_empty() {
        return false;
    }
    match split_type {
        SplitType::Percentage => {
            let mut total: i128 = 0;
            for r in recipients.iter() {
                if r.share <= 0 {
                    return false;
                }
                total += r.share;
            }
            total <= 100_00
        }
        SplitType::Fixed => {
            for r in recipients.iter() {
                if r.share <= 0 {
                    return false;
                }
            }
            true
        }
    }
}

#[contract]
pub struct PaymentSplitter;

#[contractimpl]
impl PaymentSplitter {
    pub fn initialize(
        env: Env,
        admin: Address,
        token: Address,
        split_type: SplitType,
        recipients: Vec<Recipient>,
    ) {
        if env.storage().instance().has(&KEY_ADMIN) {
            panic_with_error!(env, SplitterError::AlreadyInitialized);
        }
        admin.require_auth();

        if !validate_config(&recipients, split_type) {
            panic_with_error!(env, SplitterError::InvalidRecipients);
        }

        env.storage().instance().set(&KEY_ADMIN, &admin);
        env.storage().instance().set(&KEY_TOKEN, &token);
        env.storage().instance().set(&KEY_SPLIT_MODE, &split_type);
        env.storage().instance().set(&KEY_RECIPIENTS, &recipients);
        env.storage().instance().set(&KEY_VERSION, &1u32);
        bump_instance(&env);
    }

    pub fn split(env: Env, from: Address, amount: i128) {
        from.require_auth();

        if amount <= 0 {
            panic_with_error!(env, SplitterError::ZeroAmount);
        }

        let token: Address = env
            .storage()
            .instance()
            .get(&KEY_TOKEN)
            .unwrap_or_else(|| panic_with_error!(env, SplitterError::NotInitialized));
        let split_type: SplitType = env.storage().instance().get(&KEY_SPLIT_MODE).unwrap();
        let recipients: Vec<Recipient> = env.storage().instance().get(&KEY_RECIPIENTS).unwrap();

        let token_client = TokenClient::new(&env, &token);
        let contract_addr = env.current_contract_address();

        token_client.transfer(&from, &contract_addr, &amount);

        match split_type {
            SplitType::Percentage => Self::distribute_percentage(&env, &token_client, &contract_addr, amount, &recipients),
            SplitType::Fixed => Self::distribute_fixed(&env, &token_client, &contract_addr, amount, &recipients),
        }

        env.events().publish(
            (symbol_short!("splitter"), symbol_short!("split")),
            (from, amount, recipients.len() as u32),
        );
    }

    fn distribute_percentage(env: &Env, token: &TokenClient, from: &Address, total: i128, recipients: &Vec<Recipient>) {
        let n = recipients.len();
        let mut distributed: i128 = 0;

        for i in 0..n {
            let r = recipients.get(i).unwrap();
            let share_amount = total * r.share / 100_00;
            if share_amount > 0 {
                token.transfer(from, &r.address, &share_amount);
                distributed += share_amount;
            }
        }

        let remainder = total - distributed;
        if remainder > 0 {
            let first = recipients.get(0).unwrap();
            token.transfer(from, &first.address, &remainder);
        }
    }

    fn distribute_fixed(env: &Env, token: &TokenClient, from: &Address, total: i128, recipients: &Vec<Recipient>) {
        let n = recipients.len();
        let mut total_shares: i128 = 0;
        for i in 0..n {
            let r = recipients.get(i).unwrap();
            total_shares += r.share;
        }

        if total < total_shares {
            panic_with_error!(env, SplitterError::SharesExceedTotal);
        }

        let mut distributed: i128 = 0;
        for i in 0..n {
            let r = recipients.get(i).unwrap();
            token.transfer(from, &r.address, &r.share);
            distributed += r.share;
        }

        let remainder = total - distributed;
        if remainder > 0 {
            let first = recipients.get(0).unwrap();
            token.transfer(from, &first.address, &remainder);
        }
    }

    pub fn set_recipients(env: Env, split_type: SplitType, recipients: Vec<Recipient>) {
        require_admin(&env);
        if !validate_config(&recipients, split_type) {
            panic_with_error!(env, SplitterError::InvalidRecipients);
        }
        env.storage().instance().set(&KEY_SPLIT_MODE, &split_type);
        env.storage().instance().set(&KEY_RECIPIENTS, &recipients);
        bump_instance(&env);

        env.events().publish(
            (symbol_short!("splitter"), symbol_short!("recips_up")),
            recipients.len() as u32,
        );
    }

    pub fn transfer_admin(env: Env, new_admin: Address) {
        require_admin(&env);
        env.storage().instance().set(&KEY_ADMIN, &new_admin);
        env.events().publish(
            (symbol_short!("splitter"), symbol_short!("adm_xfer")),
            new_admin,
        );
    }

    pub fn get_config(env: Env) -> SplitConfig {
        let split_type: SplitType = env.storage().instance().get(&KEY_SPLIT_MODE).unwrap();
        let recipients: Vec<Recipient> = env.storage().instance().get(&KEY_RECIPIENTS).unwrap();
        SplitConfig { recipients, split_type }
    }

    pub fn get_admin(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&KEY_ADMIN)
            .unwrap_or_else(|| panic_with_error!(env, SplitterError::NotInitialized))
    }

    pub fn get_token(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&KEY_TOKEN)
            .unwrap_or_else(|| panic_with_error!(env, SplitterError::NotInitialized))
    }
}

mod test;
