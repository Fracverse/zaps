# BACKEND [BE-007]: Create Feed Fetch API endpoint for Friend-only payments

## Description
Return a feed of social payments involving the authenticated user or their friends.

## Files to Edit/Create
- `backend/src/api/feed.rs`

## Acceptance Criteria
- Enforce friends visibility rules (check friendship table before returning records).

## Guidance / Hints
Use a SQL `JOIN` on friendships where status is 'ACCEPTED'.
