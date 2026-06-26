# SMART-CONTRACT [SC-001]: Implement Username Registration mapping Address -> Zaps ID

## Description
Implement the core registration function to allow users to register a unique Zaps username (e.g. ebube.zaps).

## Files to Edit/Create
- `contracts/contracts/user_registry/src/lib.rs`

## Acceptance Criteria
- Ensure Address to String map is correctly written and retrieved.
- Validate that registrations are unique.

## Guidance / Hints
Use Soroban SDK `env.storage().persistent()` to store address-username mappings.
