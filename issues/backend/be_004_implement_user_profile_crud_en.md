# BACKEND [BE-004]: Implement User Profile CRUD endpoints

## Description
Allow users to read and update their display name, bio, and avatar URLs.

## Files to Edit/Create
- `backend/src/api/user.rs`

## Acceptance Criteria
- Updates must only affect the authenticated user's records.

## Guidance / Hints
Extract the current user address from JWT authorization header.
