# Zaps Smart Contracts (Soroban Workspace)

Welcome to the rebranded Zaps Smart Contracts repository! Zaps is a social payments platform running on the Stellar blockchain, turning payments into interactive social experiences.

## Architecture

This workspace contains the following Soroban smart contracts:
- **`user_registry`**: Maps Stellar addresses to user profiles and custom Zaps IDs (e.g., `ebube.zaps`).
- **`social_payment`**: Executes peer-to-peer payments on-chain with detailed transaction notes, amounts, and visibility settings (Public, Friends-only, Private) and emits social transaction events.
- **`naira_token`**: A custom Naira-pegged stablecoin anchor interface (₦) used as the primary fiat token in our social ecosystem.
- **`social_graph`**: Keeps track of friend links and relationships on-chain to handle privacy permissions.
- **`allbridge_receiver`**: Interacts with the Allbridge relayer to receive cross-chain token deposits and fund user wallets from other blockchains (Solana, EVM, etc.).

## How to Contribute

This codebase is structured with stubs and `todo!()` macros so that contributors can implement features step-by-step.
1. Check the local issues under `/issues/smart-contracts/` or GitHub Issues.
2. Locate the corresponding contract in the `contracts/` directory.
3. Replace the placeholder stubs with proper Soroban implementation logic.
4. Add unit tests inside the contract modules (using `#[test]` / `testutils` from the SDK).

## Building & Testing

To compile the smart contracts:
```bash
cargo build --target wasm32-unknown-unknown --release
```

To run unit tests:
```bash
cargo test
```
