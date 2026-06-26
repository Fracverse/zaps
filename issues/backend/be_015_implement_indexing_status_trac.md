# BACKEND [BE-015]: Implement indexing status tracker to prevent double-processing

## Description
Save latest ledger checkpoint sequence in DB to survive worker restarts.

## Files to Edit/Create
- `backend/src/indexer/worker.rs`

## Acceptance Criteria
- Store sequence on successful block scan.
- Read checkpoint on boot.

## Guidance / Hints
Write ledger sequence updates inside the transaction scope of parsed events.
