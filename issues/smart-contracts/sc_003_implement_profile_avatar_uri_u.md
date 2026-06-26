# SMART-CONTRACT [SC-003]: Implement profile avatar URI update function in Registry

## Description
Allow registered users to update their avatar URI in the user registry contract.

## Files to Edit/Create
- `contracts/contracts/user_registry/src/lib.rs`

## Acceptance Criteria
- Require authorization from the user's address.
- Store the avatar URI mapping successfully.

## Guidance / Hints
Call `user.require_auth()` before writing update.
