# SMART-CONTRACT [SC-012]: Implement social friendship graph registry on-chain

## Description
Track user friends lists on-chain to handle friends-only payments permission check.

## Files to Edit/Create
- `contracts/contracts/social_graph/src/lib.rs`

## Acceptance Criteria
- Actions `add_friend` and `remove_friend` must succeed with user's signatures.

## Guidance / Hints
Store as persistent mapping of address pairs.
