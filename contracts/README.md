# Zaps Smart Contracts (Soroban Workspace)

Welcome to the rebranded Zaps Smart Contracts repository! Zaps is a social payments platform running on the Stellar blockchain, turning payments into interactive social experiences.

## Architecture

This workspace contains the following Soroban smart contracts:
- **`user_registry`**: Maps Stellar addresses to user profiles and custom Zaps IDs (e.g., `ebube.zaps`).
- **`social_payment`**: Executes peer-to-peer payments on-chain with detailed transaction notes, amounts, and visibility settings (Public, Friends-only, Private) and emits social transaction events.
- **`naira_token`**: A custom Naira-pegged stablecoin anchor interface (₦) used as the primary fiat token in our social ecosystem.
- **`social_graph`**: Keeps track of friend links and relationships on-chain to handle privacy permissions.
- **`allbridge_receiver`**: Interacts with the Allbridge relayer to receive cross-chain token deposits and fund user wallets from other blockchains (Solana, EVM, etc.).
- **`circle`**: Pools member contributions, enforces governed payouts, and tracks balances by supported token contract.
- **`circle_factory`**: Provides an authorized entry point for creating circles and querying their factory registrations.

## Circle asset support

Before creating circles, initialize the `circle` contract with its asset-registry administrator, then have that administrator register each supported token contract. Circle creation, contributions (including goal contributions), and payout requests/execution reject unregistered assets. The registry and per-asset accounting are queryable on-chain; an asset can only be removed when its tracked balance and payout reservations are both zero.

Registration is an administrator assertion that the address implements the Soroban fungible-token interface used by the contract (`balance` and `transfer`) and transfers the requested amount exactly. Circle amounts are integer token base units, not decimal-adjusted display amounts; callers should supply Stellar asset contracts with Stellar's 7-decimal precision. Fee-on-transfer, rebasing, or otherwise non-exact tokens are not compatible because circle accounting records the requested transfer amount.

Circle administrators can configure a fixed amount, minimum amount, and maximum amount per contribution. A value of `0` disables that rule; a fixed amount must fall within any configured bounds. Rules are validated when set and checked before funds move through regular, recorded, or goal contributions. Rule changes affect subsequent contributions only.

Circle configuration queries include the circle asset and its support status, contribution rules, membership count and optional maximum member count, payout cap, and withdrawal approval threshold/list. The member cap defaults to unlimited (`0`), includes the circle admin, and cannot be lowered below the current member count. Only the circle admin can change it; joins and departures update the stored count.

The `circle_factory` must be initialized with the deployed `circle` contract address. Callers authorize circle creation with their own address; the factory delegates creation to the configured contract, assigns a monotonically increasing factory ID, stores the contract/ID mapping, and emits a registration event. Factory IDs can be queried with `get_circle` and `get_circle_count`.

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
