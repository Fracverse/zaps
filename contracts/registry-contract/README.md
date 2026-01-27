# Registry Contract

A central on-chain registry for mapping human-readable names to contract addresses on Soroban.

## Overview

The Registry Contract provides a simple, admin-controlled way to maintain a registry of contract addresses. This enables other contracts and clients to discover contract addresses by name rather than hardcoding addresses.

## Features

- **Admin-controlled registration**: Only the designated admin can register or update contract mappings
- **Human-readable names**: Map arbitrary string names to contract addresses
- **Contract discovery**: Look up addresses by name or list all registered contracts
- **Event emission**: All state changes emit events for off-chain indexing

## Contract Interface

### Functions

#### `initialize(admin: Address) -> Result<(), RegistryError>`

Initializes the registry with an admin address. Can only be called once.

**Parameters:**

- `admin`: The address that will have permission to register contracts

**Errors:**

- `AlreadyInitialized`: Registry has already been initialized

---

#### `register_contract(name: String, address: Address) -> Result<(), RegistryError>`

Registers or updates a contract address with a human-readable name. Requires admin authorization.

**Parameters:**

- `name`: Human-readable identifier for the contract
- `address`: The contract address to register

**Errors:**

- `NotInitialized`: Registry has not been initialized

**Events:**

- Emits `register` event with name and address

---

#### `get_contract(name: String) -> Option<Address>`

Retrieves a contract address by its registered name.

**Parameters:**

- `name`: The registered name to look up

**Returns:**

- `Some(Address)`: The contract address if found
- `None`: If no contract is registered with that name

---

#### `list_contracts() -> Result<Vec<ContractEntry>, RegistryError>`

Lists all registered contracts.

**Returns:**

- Vector of `ContractEntry` structs containing name and address pairs

**Errors:**

- `NotInitialized`: Registry has not been initialized

---

#### `get_admin() -> Result<Address, RegistryError>`

Returns the admin address.

**Errors:**

- `NotInitialized`: Registry has not been initialized

## Project Structure

```text
registry-contract/
├── Cargo.toml
├── README.md
└── src/
    ├── lib.rs      # Contract implementation
    ├── storage.rs  # Storage keys and data types
    ├── events.rs   # Event emission helpers
    ├── error.rs    # Error definitions
    └── test.rs     # Unit tests
```

## Testing

Run tests with:

```bash
cargo test -p registry-contract
```
