# BACKEND [BE-010]: Implement Comment creation and deletion endpoints

## Description
Allow users to write/delete comment logs attached to payments.

## Files to Edit/Create
- `backend/src/api/social.rs`

## Acceptance Criteria
- Check comment owner matches authenticated user before deletion.

## Guidance / Hints
Perform comment validation in Axum routes before updating db.
