# SMART-CONTRACT [SC-014]: Implement contract upgradeability interface

## Description
Support hot upgrading contract logic without losing stored state.

## Files to Edit/Create
- `contracts/contracts/social_payment/src/lib.rs`

## Acceptance Criteria
- Enforce upgrade can only be triggered by the multisig admin address.

## Guidance / Hints
Use `env.deployer().update_current_contract_wasm()`.
