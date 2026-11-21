#![no_std]
#![allow(unexpected_cfgs)]

use soroban_sdk::{
    contract, contractclient, contracterror, contractimpl, contracttype, symbol_short, Address, Env,
};

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    NotInitialized = 1,
    AlreadyInitialized = 2,
    Unauthorized = 3,
    InvalidConfiguration = 4,
    InvalidAmount = 5,
    CircleCreationFailed = 6,
    DuplicateCircleId = 7,
    NotFound = 8,
    Overflow = 9,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CircleRegistration {
    pub id: u32,
    pub circle_contract: Address,
    pub circle_id: u32,
    pub creator: Address,
    pub asset: Address,
    pub max_payout: i128,
    pub created_at: u64,
}

#[contracttype]
enum Key {
    CircleContract,
    Count,
    Circle(u32),
}

#[contractclient(name = "CircleClient")]
pub trait CircleInterface {
    fn create_circle(env: Env, admin: Address, token: Address, max_payout: i128) -> u32;
    fn get_supported_assets(env: Env) -> soroban_sdk::Vec<Address>;
}

#[contract]
pub struct CircleFactory;

fn circle_contract(env: &Env) -> Result<Address, Error> {
    env.storage()
        .instance()
        .get(&Key::CircleContract)
        .ok_or(Error::NotInitialized)
}

#[contractimpl]
impl CircleFactory {
    /// Initializes the factory once with the address of the deployed Circle contract.
    pub fn initialize(env: Env, admin: Address, circle_contract: Address) -> Result<(), Error> {
        admin.require_auth();
        if env.storage().instance().has(&Key::CircleContract) {
            return Err(Error::AlreadyInitialized);
        }
        if circle_contract == env.current_contract_address() {
            return Err(Error::InvalidConfiguration);
        }
        if CircleClient::new(&env, &circle_contract)
            .try_get_supported_assets()
            .is_err()
        {
            return Err(Error::InvalidConfiguration);
        }
        env.storage()
            .instance()
            .set(&Key::CircleContract, &circle_contract);
        env.storage().instance().set(&Key::Count, &0u32);
        Ok(())
    }

    /// Creates a circle through the configured contract and registers a unique factory ID.
    pub fn create_circle(
        env: Env,
        creator: Address,
        asset: Address,
        max_payout: i128,
    ) -> Result<u32, Error> {
        creator.require_auth();
        if max_payout < 0 {
            return Err(Error::InvalidAmount);
        }
        let circle_contract = circle_contract(&env)?;
        let id: u32 = env
            .storage()
            .instance()
            .get(&Key::Count)
            .ok_or(Error::NotInitialized)?;
        let next_count = id.checked_add(1).ok_or(Error::Overflow)?;
        if env.storage().persistent().has(&Key::Circle(id)) {
            return Err(Error::DuplicateCircleId);
        }

        let client = CircleClient::new(&env, &circle_contract);
        let circle_id = match client.try_create_circle(&creator, &asset, &max_payout) {
            Ok(circle_id) => circle_id,
            Err(_) => return Err(Error::CircleCreationFailed),
        };
        let registration = CircleRegistration {
            id,
            circle_contract,
            circle_id,
            creator: creator.clone(),
            asset,
            max_payout,
            created_at: env.ledger().timestamp(),
        };
        env.storage()
            .persistent()
            .set(&Key::Circle(id), &registration);
        env.storage().instance().set(&Key::Count, &next_count);
        env.events()
            .publish((symbol_short!("circ_crt"), id), registration);
        Ok(id)
    }

    /// Queries a registered circle by its factory-generated ID.
    pub fn get_circle(env: Env, id: u32) -> Result<CircleRegistration, Error> {
        circle_contract(&env)?;
        env.storage()
            .persistent()
            .get(&Key::Circle(id))
            .ok_or(Error::NotFound)
    }

    /// Queries the number of circles registered by this factory.
    pub fn get_circle_count(env: Env) -> Result<u32, Error> {
        circle_contract(&env)?;
        Ok(env.storage().instance().get(&Key::Count).unwrap_or(0))
    }
}

#[cfg(test)]
mod test;
