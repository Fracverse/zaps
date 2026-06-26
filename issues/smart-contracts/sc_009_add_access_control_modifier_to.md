# SMART-CONTRACT [SC-009]: Add access control modifier to registry contract actions

## Description
Ensure admin actions or registration locks are guarded by appropriate authorizations.

## Files to Edit/Create
- `contracts/contracts/user_registry/src/lib.rs`

## Acceptance Criteria
- Non-authorized users should fail to execute update profiles on behalf of others.

## Guidance / Hints
Implement authorization checks using `Address::require_auth()`.
