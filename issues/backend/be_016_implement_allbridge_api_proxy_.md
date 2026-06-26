# BACKEND [BE-016]: Implement Allbridge API proxy endpoint to get bridge fee quotes

## Description
Provide client route fetching swap fee schedules from Allbridge Core REST endpoints.

## Files to Edit/Create
- `backend/src/api/bridge.rs` 
- `backend/src/services/allbridge.rs`

## Acceptance Criteria
- Correctly format request and parse Allbridge fee responses.

## Guidance / Hints
Use `reqwest` client to call `core-api.allbridge.io`.
