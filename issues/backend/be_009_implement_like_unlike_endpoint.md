# BACKEND [BE-009]: Implement Like/Unlike endpoint for payments

## Description
Store a user's like reaction in the database and handle toggling state.

## Files to Edit/Create
- `backend/src/api/social.rs`

## Acceptance Criteria
- Toggle like state and update like counter.
- Avoid double likes via unique constraint checks.

## Guidance / Hints
Use `INSERT INTO likes ... ON CONFLICT DO NOTHING` or handle delete on double tap.
