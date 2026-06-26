# BACKEND [BE-013]: Design and build Stellar Event Indexer worker to poll Soroban RPC

## Description
Poller process querying Soroban RPC for payment events on the ledger.

## Files to Edit/Create
- `backend/src/indexer/worker.rs`

## Acceptance Criteria
- Loop consistently, handle RPC disconnections, track cursor position.

## Guidance / Hints
Implement backoff logic if RPC endpoint returns errors.
