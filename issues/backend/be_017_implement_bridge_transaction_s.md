# BACKEND [BE-017]: Implement bridge transaction status polling backend task

## Description
Track cross-chain deposits and poll Allbridge status periodically.

## Files to Edit/Create
- `backend/src/api/bridge.rs` 
- `backend/src/services/allbridge.rs`

## Acceptance Criteria
- Return bridge status (PENDING, SUCCESS, FAILED) dynamically.

## Guidance / Hints
Expose status endpoint called by the client UI stepper.
