# BACKEND [BE-005]: Implement regex-based user search endpoint

## Description
Support searching other users by their unique Zaps ID (alphanumeric search) for payment routing.

## Files to Edit/Create
- `backend/src/api/user.rs`

## Acceptance Criteria
- Match prefixes and return paginated user profiles.

## Guidance / Hints
Use `LIKE 'query%'` in SQL queries for fast indexed lookups.
