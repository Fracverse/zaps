# SMART-CONTRACT [SC-005]: Implement P2P payment execution function with Naira token

## Description
Build the core pay function transferring Naira stablecoin tokens between two users.

## Files to Edit/Create
- `contracts/contracts/social_payment/src/lib.rs`

## Acceptance Criteria
- Transfer naira tokens from sender to receiver.
- Support custom currency checks.

## Guidance / Hints
Instantiate TokenClient for the Naira token address and call `transfer`.
