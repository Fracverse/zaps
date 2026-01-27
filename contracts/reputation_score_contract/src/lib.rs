#![no_std]
use soroban_sdk::{contract, contractimpl, contracttype, log, Address, Env};

#[contracttype]
#[derive(Clone)]
enum DataKey {
    Admin,
    Score(Address),
}

#[contract]
pub struct ReputationScoreContract;

#[contractimpl]
impl ReputationScoreContract {
    /// Initialize the contract with an admin address.
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("Already initialized");
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
    }

    /// Increase the reputation score of a user. Only Callable by Admin.
    pub fn increase_score(env: Env, user: Address, value: u32) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized");
        admin.require_auth();

        let key = DataKey::Score(user.clone());
        let current_score: u32 = env.storage().persistent().get(&key).unwrap_or(0);

        let new_score = current_score.checked_add(value).expect("Score overflow");

        env.storage().persistent().set(&key, &new_score);
        log!(
            &env,
            "Score increased for {}: new score {}",
            user,
            new_score
        );
    }

    /// Decrease the reputation score of a user. Only Callable by Admin.
    /// Prevents underflow by capping the minimum score at 0.
    pub fn decrease_score(env: Env, user: Address, value: u32) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized");
        admin.require_auth();

        let key = DataKey::Score(user.clone());
        let current_score: u32 = env.storage().persistent().get(&key).unwrap_or(0);

        let new_score = current_score.saturating_sub(value);

        env.storage().persistent().set(&key, &new_score);
        log!(
            &env,
            "Score decreased for {}: new score {}",
            user,
            new_score
        );
    }

    /// Get the reputation score of a user.
    pub fn get_score(env: Env, user: Address) -> u32 {
        let key = DataKey::Score(user);
        env.storage().persistent().get(&key).unwrap_or(0)
    }
}

mod test;
