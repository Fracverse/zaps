# BACKEND [BE-008]: Create Feed Fetch API endpoint for Personal/Private payments

## Description
Return personal social payment history (Private visibility items).

## Files to Edit/Create
- `backend/src/api/feed.rs`

## Acceptance Criteria
- Strictly check that only sender or receiver can retrieve private items.

## Guidance / Hints
Enforce `WHERE (sender_id = :me OR receiver_id = :me)` in private feed queries.
