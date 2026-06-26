# BACKEND [BE-019]: Create rate limiter middleware for sensitive endpoints

## Description
Protect login and search endpoints from spam.

## Files to Edit/Create
- `backend/src/main.rs`

## Acceptance Criteria
- Return 429 Too Many Requests if threshold is crossed.

## Guidance / Hints
Use simple token bucket algorithm using Redis or memory state.
