# SMART-CONTRACT [SC-004]: Define core Social Payment data structures

## Description
Define structs for Payment representing the details of social peer-to-peer payments.

## Files to Edit/Create
- `contracts/contracts/social_payment/src/lib.rs`

## Acceptance Criteria
- Struct must contain sender, receiver, token, amount, memo description, visibility enum.

## Guidance / Hints
Derive `#[contracttype]` for custom structs in Soroban.
