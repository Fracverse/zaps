#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, symbol_short,
    token::Client as TokenClient, Address, Env, Symbol, Vec,
};

const MAX_BATCH_SIZE: u32 = 10;

#[contracterror]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum BatchPaymentError {
    InvalidBatchSize = 1,
    InvalidAmount = 2,
    TokenTransferFailed = 3,
}

#[contracttype]
#[derive(Clone)]
pub struct BatchPaymentRequest {
    pub recipient: Address,
    pub asset: Address,
    pub amount: i128,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct BatchPaymentResult {
    pub payer: Address,
    pub recipient: Address,
    pub asset: Address,
    pub amount: i128,
    pub success: bool,
    pub error_code: Option<u32>,
}

#[contracttype]
pub struct BatchPaymentEvent {
    pub payer: Address,
    pub recipient: Address,
    pub asset: Address,
    pub amount: i128,
    pub success: bool,
    pub error_code: Option<u32>,
}

#[contract]
pub struct PaymentBatchContract;

#[contractimpl]
impl PaymentBatchContract {
    pub fn batch_payments(
        env: Env,
        payer: Address,
        requests: Vec<BatchPaymentRequest>,
    ) -> Vec<BatchPaymentResult> {
        payer.require_auth();
        let count = requests.len();
        if count == 0 || count > MAX_BATCH_SIZE {
            panic_with_error!(env, BatchPaymentError::InvalidBatchSize);
        }

        let mut results = Vec::new(&env);

        for i in 0..count {
            let request = requests.get(i).unwrap();
            let result = process_payment(&env, &payer, &request);
            emit_payment_event(&env, &payer, &request, result.success, result.error_code);
            results.push_back(result);
        }

        results
    }
}

fn process_payment(
    env: &Env,
    payer: &Address,
    request: &BatchPaymentRequest,
) -> BatchPaymentResult {
    let success = request.amount > 0
        && TokenClient::new(env, &request.asset)
            .try_transfer(payer, &request.recipient, &request.amount)
            .is_ok();

    let error_code = if request.amount <= 0 {
        Some(BatchPaymentError::InvalidAmount as u32)
    } else if success {
        None
    } else {
        Some(BatchPaymentError::TokenTransferFailed as u32)
    };

    BatchPaymentResult {
        payer: payer.clone(),
        recipient: request.recipient.clone(),
        asset: request.asset.clone(),
        amount: request.amount,
        success,
        error_code,
    }
}

fn emit_payment_event(
    env: &Env,
    payer: &Address,
    request: &BatchPaymentRequest,
    success: bool,
    error_code: Option<u32>,
) {
    env.events().publish(
        (
            symbol_short!("batchpay"),
            Symbol::new(env, if success { "success" } else { "failure" }),
        ),
        BatchPaymentEvent {
            payer: payer.clone(),
            recipient: request.recipient.clone(),
            asset: request.asset.clone(),
            amount: request.amount,
            success,
            error_code,
        },
    );
}

mod test;
