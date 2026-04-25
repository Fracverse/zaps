#![no_std]

use soroban_sdk::{
    contract, contractclient, contracterror, contractimpl, contracttype, Address, Env, Symbol,
    Val, Vec,
};

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum UpgradeError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    ContractPaused = 3,
    AdminCannotFallback = 4,
    InvalidVersion = 5,
    PendingUpgradeExists = 6,
    NoPendingUpgrade = 7,
    NoRollbackAvailable = 8,
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    Paused,
    CurrentImplementation,
    CurrentVersion,
    PreviousImplementation,
    PreviousVersion,
    PendingUpgrade,
    VersionImplementation(u32),
    UpgradeHistory,
    State(Symbol),
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MigrationStep {
    pub key: Symbol,
    pub script: Address,
    pub data: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingUpgrade {
    pub from_version: u32,
    pub target_version: u32,
    pub old_implementation: Address,
    pub new_implementation: Address,
    pub steps: Vec<MigrationStep>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpgradeRecord {
    pub from_version: u32,
    pub to_version: u32,
    pub old_implementation: Address,
    pub new_implementation: Address,
    pub rollback: bool,
}

#[contractclient(name = "MigrationScriptClient")]
pub trait MigrationScript {
    fn migrate(
        env: Env,
        key: Symbol,
        current_value: i128,
        data: i128,
        from_version: u32,
        to_version: u32,
    ) -> i128;
}

#[contract]
pub struct UpgradeableProxy;

#[contractimpl]
impl UpgradeableProxy {
    pub fn initialize(
        env: Env,
        admin: Address,
        implementation: Address,
        version: u32,
    ) -> Result<(), UpgradeError> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(UpgradeError::AlreadyInitialized);
        }

        if version == 0 {
            return Err(UpgradeError::InvalidVersion);
        }

        admin.require_auth();

        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::CurrentImplementation, &implementation);
        env.storage().instance().set(&DataKey::CurrentVersion, &version);
        env.storage()
            .instance()
            .set(&DataKey::VersionImplementation(version), &implementation);
        env.storage().instance().set(&DataKey::Paused, &false);
        env.storage()
            .instance()
            .set(&DataKey::UpgradeHistory, &Vec::<UpgradeRecord>::new(&env));
        Ok(())
    }

    pub fn forward(
        env: Env,
        caller: Address,
        method: Symbol,
        args: Vec<Val>,
    ) -> Result<Val, UpgradeError> {
        if is_paused(&env) {
            return Err(UpgradeError::ContractPaused);
        }

        let admin = read_admin(&env)?;
        caller.require_auth();

        if caller == admin {
            return Err(UpgradeError::AdminCannotFallback);
        }

        let implementation = read_current_implementation(&env)?;
        Ok(env.invoke_contract(&implementation, &method, args))
    }

    pub fn stage_upgrade(
        env: Env,
        new_implementation: Address,
        target_version: u32,
        steps: Vec<MigrationStep>,
    ) -> Result<(), UpgradeError> {
        require_admin(&env)?;

        if env.storage().instance().has(&DataKey::PendingUpgrade) {
            return Err(UpgradeError::PendingUpgradeExists);
        }

        let from_version = read_current_version(&env)?;
        if target_version <= from_version {
            return Err(UpgradeError::InvalidVersion);
        }

        let old_implementation = read_current_implementation(&env)?;
        let pending = PendingUpgrade {
            from_version,
            target_version,
            old_implementation,
            new_implementation,
            steps,
        };

        env.storage().instance().set(&DataKey::PendingUpgrade, &pending);
        Ok(())
    }

    pub fn execute_upgrade(env: Env) -> Result<(), UpgradeError> {
        require_admin(&env)?;

        let pending: PendingUpgrade = env
            .storage()
            .instance()
            .get(&DataKey::PendingUpgrade)
            .ok_or(UpgradeError::NoPendingUpgrade)?;

        for step in pending.steps.iter() {
            let key = step.key.clone();
            let state_key = DataKey::State(key.clone());
            let current_value: i128 = env.storage().persistent().get(&state_key).unwrap_or(0);

            let migrated = MigrationScriptClient::new(&env, &step.script).migrate(
                &key,
                &current_value,
                &step.data,
                &pending.from_version,
                &pending.target_version,
            );

            env.storage().persistent().set(&state_key, &migrated);
        }

        let current_implementation = read_current_implementation(&env)?;
        let current_version = read_current_version(&env)?;

        env.storage()
            .instance()
            .set(&DataKey::PreviousImplementation, &current_implementation);
        env.storage()
            .instance()
            .set(&DataKey::PreviousVersion, &current_version);
        env.storage()
            .instance()
            .set(&DataKey::CurrentImplementation, &pending.new_implementation);
        env.storage()
            .instance()
            .set(&DataKey::CurrentVersion, &pending.target_version);
        env.storage().instance().set(
            &DataKey::VersionImplementation(pending.target_version),
            &pending.new_implementation,
        );

        append_history(
            &env,
            UpgradeRecord {
                from_version: current_version,
                to_version: pending.target_version,
                old_implementation: current_implementation,
                new_implementation: pending.new_implementation,
                rollback: false,
            },
        );

        env.storage().instance().remove(&DataKey::PendingUpgrade);
        Ok(())
    }

    pub fn rollback(env: Env) -> Result<(), UpgradeError> {
        require_admin(&env)?;

        let previous_implementation: Address = env
            .storage()
            .instance()
            .get(&DataKey::PreviousImplementation)
            .ok_or(UpgradeError::NoRollbackAvailable)?;
        let previous_version: u32 = env
            .storage()
            .instance()
            .get(&DataKey::PreviousVersion)
            .ok_or(UpgradeError::NoRollbackAvailable)?;

        let current_implementation = read_current_implementation(&env)?;
        let current_version = read_current_version(&env)?;

        env.storage()
            .instance()
            .set(&DataKey::CurrentImplementation, &previous_implementation);
        env.storage()
            .instance()
            .set(&DataKey::CurrentVersion, &previous_version);
        env.storage()
            .instance()
            .set(&DataKey::PreviousImplementation, &current_implementation);
        env.storage()
            .instance()
            .set(&DataKey::PreviousVersion, &current_version);

        append_history(
            &env,
            UpgradeRecord {
                from_version: current_version,
                to_version: previous_version,
                old_implementation: current_implementation,
                new_implementation: previous_implementation,
                rollback: true,
            },
        );

        env.storage().instance().remove(&DataKey::PendingUpgrade);
        Ok(())
    }

    pub fn pause(env: Env) -> Result<(), UpgradeError> {
        require_admin(&env)?;
        env.storage().instance().set(&DataKey::Paused, &true);
        Ok(())
    }

    pub fn unpause(env: Env) -> Result<(), UpgradeError> {
        require_admin(&env)?;
        env.storage().instance().set(&DataKey::Paused, &false);
        Ok(())
    }

    pub fn transfer_admin(env: Env, new_admin: Address) -> Result<(), UpgradeError> {
        require_admin(&env)?;
        env.storage().instance().set(&DataKey::Admin, &new_admin);
        Ok(())
    }

    pub fn set_state_value(env: Env, key: Symbol, value: i128) -> Result<(), UpgradeError> {
        require_admin(&env)?;
        env.storage()
            .persistent()
            .set(&DataKey::State(key), &value);
        Ok(())
    }

    pub fn get_state_value(env: Env, key: Symbol) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::State(key))
            .unwrap_or(0)
    }

    pub fn get_admin(env: Env) -> Result<Address, UpgradeError> {
        read_admin(&env)
    }

    pub fn is_paused(env: Env) -> bool {
        is_paused(&env)
    }

    pub fn current_implementation(env: Env) -> Result<Address, UpgradeError> {
        read_current_implementation(&env)
    }

    pub fn current_version(env: Env) -> Result<u32, UpgradeError> {
        read_current_version(&env)
    }

    pub fn implementation_for_version(env: Env, version: u32) -> Option<Address> {
        env.storage()
            .instance()
            .get(&DataKey::VersionImplementation(version))
    }

    pub fn get_pending_upgrade(env: Env) -> Option<PendingUpgrade> {
        env.storage().instance().get(&DataKey::PendingUpgrade)
    }

    pub fn get_upgrade_history(env: Env) -> Vec<UpgradeRecord> {
        env.storage()
            .instance()
            .get(&DataKey::UpgradeHistory)
            .unwrap_or_else(|| Vec::new(&env))
    }
}

fn read_admin(env: &Env) -> Result<Address, UpgradeError> {
    env.storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(UpgradeError::NotInitialized)
}

fn require_admin(env: &Env) -> Result<Address, UpgradeError> {
    let admin = read_admin(env)?;
    admin.require_auth();
    Ok(admin)
}

fn read_current_implementation(env: &Env) -> Result<Address, UpgradeError> {
    env.storage()
        .instance()
        .get(&DataKey::CurrentImplementation)
        .ok_or(UpgradeError::NotInitialized)
}

fn read_current_version(env: &Env) -> Result<u32, UpgradeError> {
    env.storage()
        .instance()
        .get(&DataKey::CurrentVersion)
        .ok_or(UpgradeError::NotInitialized)
}

fn is_paused(env: &Env) -> bool {
    env.storage().instance().get(&DataKey::Paused).unwrap_or(false)
}

fn append_history(env: &Env, record: UpgradeRecord) {
    let mut history: Vec<UpgradeRecord> = env
        .storage()
        .instance()
        .get(&DataKey::UpgradeHistory)
        .unwrap_or_else(|| Vec::new(env));
    history.push_back(record);
    env.storage().instance().set(&DataKey::UpgradeHistory, &history);
}

mod test;
