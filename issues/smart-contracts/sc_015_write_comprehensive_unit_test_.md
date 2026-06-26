# SMART-CONTRACT [SC-015]: Write comprehensive unit test suite for Social Payment contract

## Description
Create mock test ledger environment to test full pay, like, and comment flows.

## Files to Edit/Create
- `contracts/contracts/social_payment/src/lib.rs` (plus tests module)

## Acceptance Criteria
- Coverage should include public, friends-only, and private payment validations.

## Guidance / Hints
Use `Env::default()` and register client contract instances.
