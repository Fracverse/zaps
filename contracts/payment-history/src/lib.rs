#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, symbol_short,
    Address, Bytes, Env, Symbol, Vec,
};

const KEY_ADMIN: Symbol = symbol_short!("admin");
const KEY_VERSION: Symbol = symbol_short!("version");
const KEY_MAX_RECORDS: Symbol = symbol_short!("max_rec");
const KEY_TOTAL_PAYMENTS: Symbol = symbol_short!("tot_pay");
const KEY_RETENTION_LEDGERS: Symbol = symbol_short!("ret_ldg");

#[contracttype]
#[derive(Clone)]
pub struct PaymentRecord {
    pub id: u64,
    pub payer: Address,
    pub payee: Address,
    pub token: Address,
    pub amount: i128,
    pub timestamp_ledger: u32,
    pub memo: Bytes,
    pub merchant_id: Option<Bytes>,
}

#[contracttype]
#[derive(Clone)]
pub struct PaymentFilter {
    pub payer: Option<Address>,
    pub payee: Option<Address>,
    pub merchant_id: Option<Bytes>,
    pub token: Option<Address>,
    pub min_amount: Option<i128>,
    pub max_amount: Option<i128>,
    pub from_ledger: Option<u32>,
    pub to_ledger: Option<u32>,
}

#[contracttype]
#[derive(Clone)]
pub struct PaginatedResult {
    pub records: Vec<PaymentRecord>,
    pub total: u64,
    pub page: u32,
    pub page_size: u32,
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    PaymentRecord(u64),
    PayerIndex(Address, u64),
    PayeeIndex(Address, u64),
    MerchantIndex(Bytes, u64),
    PayerCount(Address),
    PayeeCount(Address),
    MerchantCount(Bytes),
}

#[contracterror]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum HistoryError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    RecordNotFound = 4,
    MaxRecordsExceeded = 5,
    InvalidAmount = 6,
    InvalidPage = 7,
}

fn bump_instance(env: &Env) {
    let ttl = 6_307_200u32;
    let threshold = 100_000u32;
    env.storage().instance().extend_ttl(threshold, ttl);
}

fn bump_persistent<K>(env: &Env, key: &K)
where
    K: soroban_sdk::IntoVal<Env, soroban_sdk::Val>,
{
    env.storage()
        .persistent()
        .extend_ttl(key, 50_000u32, 3_153_600u32);
}

fn require_admin(env: &Env) -> Address {
    let admin: Address = env
        .storage()
        .instance()
        .get(&KEY_ADMIN)
        .unwrap_or_else(|| panic_with_error!(env, HistoryError::NotInitialized));
    admin.require_auth();
    admin
}

fn record_exists(env: &Env, record_id: u64) -> bool {
    env.storage().persistent().has(&DataKey::PaymentRecord(record_id))
}

fn matches_filter(record: &PaymentRecord, filter: &PaymentFilter) -> bool {
    if let Some(ref payer) = filter.payer {
        if record.payer != *payer { return false; }
    }
    if let Some(ref payee) = filter.payee {
        if record.payee != *payee { return false; }
    }
    if let Some(ref merchant_id) = filter.merchant_id {
        match &record.merchant_id {
            Some(ref mid) => if mid != merchant_id { return false; }
            None => return false,
        }
    }
    if let Some(ref token) = filter.token {
        if record.token != *token { return false; }
    }
    if let Some(min) = filter.min_amount {
        if record.amount < min { return false; }
    }
    if let Some(max) = filter.max_amount {
        if record.amount > max { return false; }
    }
    if let Some(from) = filter.from_ledger {
        if record.timestamp_ledger < from { return false; }
    }
    if let Some(to) = filter.to_ledger {
        if record.timestamp_ledger > to { return false; }
    }
    true
}

#[contract]
pub struct PaymentHistory;

#[contractimpl]
impl PaymentHistory {
    pub fn initialize(env: Env, admin: Address, max_records: u64, retention_ledgers: u32) {
        if env.storage().instance().has(&KEY_ADMIN) {
            panic_with_error!(env, HistoryError::AlreadyInitialized);
        }
        admin.require_auth();

        let max = if max_records == 0 { 10_000 } else { max_records };

        env.storage().instance().set(&KEY_ADMIN, &admin);
        env.storage().instance().set(&KEY_MAX_RECORDS, &max);
        env.storage().instance().set(&KEY_RETENTION_LEDGERS, &retention_ledgers);
        env.storage().instance().set(&KEY_TOTAL_PAYMENTS, &0u64);
        env.storage().instance().set(&KEY_VERSION, &1u32);
        bump_instance(&env);

        env.events().publish(
            (symbol_short!("history"), symbol_short!("init")),
            admin,
        );
    }

    pub fn record_payment(
        env: Env,
        recorder: Address,
        payer: Address,
        payee: Address,
        token: Address,
        amount: i128,
        memo: Bytes,
        merchant_id: Option<Bytes>,
    ) -> u64 {
        recorder.require_auth();

        if amount <= 0 {
            panic_with_error!(env, HistoryError::InvalidAmount);
        }

        let max_records: u64 = env.storage().instance().get(&KEY_MAX_RECORDS).unwrap_or(10_000);
        let total: u64 = env.storage().instance().get(&KEY_TOTAL_PAYMENTS).unwrap_or(0);

        if total >= max_records {
            panic_with_error!(env, HistoryError::MaxRecordsExceeded);
        }

        let record_id = total + 1;
        let record = PaymentRecord {
            id: record_id,
            payer: payer.clone(),
            payee: payee.clone(),
            token,
            amount,
            timestamp_ledger: env.ledger().sequence(),
            memo,
            merchant_id: merchant_id.clone(),
        };

        env.storage()
            .persistent()
            .set(&DataKey::PaymentRecord(record_id), &record);
        bump_persistent(&env, &DataKey::PaymentRecord(record_id));

        env.storage().instance().set(&KEY_TOTAL_PAYMENTS, &record_id);

        let payer_count: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::PayerCount(payer.clone()))
            .unwrap_or(0);
        env.storage()
            .persistent()
            .set(&DataKey::PayerIndex(payer.clone(), payer_count), &record_id);
        env.storage()
            .persistent()
            .set(&DataKey::PayerCount(payer.clone()), &(payer_count + 1));
        bump_persistent(&env, &DataKey::PayerCount(payer.clone()));

        let payee_count: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::PayeeCount(payee.clone()))
            .unwrap_or(0);
        env.storage()
            .persistent()
            .set(&DataKey::PayeeIndex(payee.clone(), payee_count), &record_id);
        env.storage()
            .persistent()
            .set(&DataKey::PayeeCount(payee.clone()), &(payee_count + 1));
        bump_persistent(&env, &DataKey::PayeeCount(payee.clone()));

        if let Some(ref mid) = merchant_id {
            let merchant_count: u64 = env
                .storage()
                .persistent()
                .get(&DataKey::MerchantCount(mid.clone()))
                .unwrap_or(0);
            env.storage()
                .persistent()
                .set(&DataKey::MerchantIndex(mid.clone(), merchant_count), &record_id);
            env.storage()
                .persistent()
                .set(&DataKey::MerchantCount(mid.clone()), &(merchant_count + 1));
            bump_persistent(&env, &DataKey::MerchantCount(mid.clone()));
        }

        env.events().publish(
            (symbol_short!("history"), symbol_short!("recorded")),
            (record_id, payer, payee, amount as u128),
        );

        record_id
    }

    pub fn get_record(env: Env, record_id: u64) -> PaymentRecord {
        env.storage()
            .persistent()
            .get(&DataKey::PaymentRecord(record_id))
            .unwrap_or_else(|| panic_with_error!(env, HistoryError::RecordNotFound))
    }

    pub fn query(
        env: Env,
        filter: PaymentFilter,
        page: u32,
        page_size: u32,
    ) -> PaginatedResult {
        if page_size == 0 || page_size > 100 {
            panic_with_error!(env, HistoryError::InvalidPage);
        }

        let total: u64 = env.storage().instance().get(&KEY_TOTAL_PAYMENTS).unwrap_or(0);
        let mut matching: Vec<PaymentRecord> = Vec::new(&env);

        let start = total.saturating_sub(1);
        let end: i64 = if total > (page as u64 * page_size as u64) {
            (total - (page as u64 * page_size as u64)) as i64
        } else {
            0
        };

        let mut count = 0u64;
        let skip = page * page_size as u32;
        let mut collected = 0u32;

        for i in (end as u64..=start).rev() {
            if collected >= page_size {
                break;
            }
            let record_id = i + 1;
            if !record_exists(&env, record_id) {
                continue;
            }
            let record = env.storage()
                .persistent()
                .get::<DataKey, PaymentRecord>(&DataKey::PaymentRecord(record_id))
                .unwrap();

            if matches_filter(&record, &filter) {
                if count >= skip as u64 {
                    matching.push_back(record);
                    collected += 1;
                }
                count += 1;
            }
        }

        PaginatedResult {
            records: matching,
            total: count,
            page,
            page_size,
        }
    }

    pub fn get_payment_count(env: Env) -> u64 {
        env.storage().instance().get(&KEY_TOTAL_PAYMENTS).unwrap_or(0)
    }

    pub fn get_payer_payments(env: Env, payer: Address, page: u32, page_size: u32) -> PaginatedResult {
        let count: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::PayerCount(payer.clone()))
            .unwrap_or(0);

        let start = page as u64 * page_size as u64;
        let mut records = Vec::new(&env);
        let mut collected = 0u32;

        for i in start..count {
            if collected >= page_size {
                break;
            }
            let record_id: u64 = env.storage()
                .persistent()
                .get(&DataKey::PayerIndex(payer.clone(), i))
                .unwrap();
            let record = env.storage()
                .persistent()
                .get::<DataKey, PaymentRecord>(&DataKey::PaymentRecord(record_id))
                .unwrap();
            records.push_back(record);
            collected += 1;
        }

        PaginatedResult { records, total: count, page, page_size }
    }

    pub fn get_payee_payments(env: Env, payee: Address, page: u32, page_size: u32) -> PaginatedResult {
        let count: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::PayeeCount(payee.clone()))
            .unwrap_or(0);

        let start = page as u64 * page_size as u64;
        let mut records = Vec::new(&env);
        let mut collected = 0u32;

        for i in start..count {
            if collected >= page_size {
                break;
            }
            let record_id: u64 = env.storage()
                .persistent()
                .get(&DataKey::PayeeIndex(payee.clone(), i))
                .unwrap();
            let record = env.storage()
                .persistent()
                .get::<DataKey, PaymentRecord>(&DataKey::PaymentRecord(record_id))
                .unwrap();
            records.push_back(record);
            collected += 1;
        }

        PaginatedResult { records, total: count, page, page_size }
    }

    pub fn transfer_admin(env: Env, new_admin: Address) {
        require_admin(&env);
        env.storage().instance().set(&KEY_ADMIN, &new_admin);
        env.events().publish(
            (symbol_short!("history"), symbol_short!("adm_xfer")),
            new_admin,
        );
    }
}

mod test;
