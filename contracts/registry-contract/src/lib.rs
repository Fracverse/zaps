#![no_std]

mod error;
mod events;
mod storage;

use soroban_sdk::{contract, contractimpl, Address, Env, String, Vec};

pub use error::RegistryError;
pub use storage::{ContractEntry, DataKey};

#[contract]
pub struct Registry;

#[contractimpl]
impl Registry {
    pub fn initialize(env: Env, admin: Address) -> Result<(), RegistryError> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(RegistryError::AlreadyInitialized);
        }

        env.storage().instance().set(&DataKey::Admin, &admin);
        let names: Vec<String> = Vec::new(&env);
        env.storage()
            .instance()
            .set(&DataKey::ContractNames, &names);

        events::emit_initialized(&env, &admin);
        Ok(())
    }

    pub fn register_contract(
        env: Env,
        name: String,
        address: Address,
    ) -> Result<(), RegistryError> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(RegistryError::NotInitialized)?;

        admin.require_auth();

        let mut names: Vec<String> = env
            .storage()
            .instance()
            .get(&DataKey::ContractNames)
            .ok_or(RegistryError::NotInitialized)?;

        if !names.iter().any(|n| n == name) {
            names.push_back(name.clone());
            env.storage()
                .instance()
                .set(&DataKey::ContractNames, &names);
        }

        env.storage()
            .instance()
            .set(&DataKey::Registry(name.clone()), &address);

        events::emit_registered(&env, &name, &address);
        Ok(())
    }

    pub fn get_contract(env: Env, name: String) -> Option<Address> {
        env.storage().instance().get(&DataKey::Registry(name))
    }

    pub fn list_contracts(env: Env) -> Result<Vec<ContractEntry>, RegistryError> {
        let names: Vec<String> = env
            .storage()
            .instance()
            .get(&DataKey::ContractNames)
            .ok_or(RegistryError::NotInitialized)?;

        let mut entries = Vec::new(&env);
        for name in names.iter() {
            if let Some(address) = env
                .storage()
                .instance()
                .get(&DataKey::Registry(name.clone()))
            {
                entries.push_back(ContractEntry {
                    name: name.clone(),
                    address,
                });
            }
        }
        Ok(entries)
    }

    pub fn get_admin(env: Env) -> Result<Address, RegistryError> {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(RegistryError::NotInitialized)
    }
}

mod test;
