use soroban_sdk::{contracttype, Address, String};

#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    Admin,
    Registry(String),
    ContractNames,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractEntry {
    pub name: String,
    pub address: Address,
}
