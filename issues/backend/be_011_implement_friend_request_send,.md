# BACKEND [BE-011]: Implement Friend Request send, accept, and reject routes

## Description
Handle user social links status logic (PENDING -> ACCEPTED / REJECTED).

## Files to Edit/Create
- `backend/src/api/user.rs`

## Acceptance Criteria
- Update database state and prevent sending double requests.

## Guidance / Hints
Insert friendship record with PENDING status. Accept changes it to ACCEPTED.
