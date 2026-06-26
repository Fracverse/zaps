# BACKEND [BE-012]: Implement friend list retrieval API endpoint

## Description
Return all friends of the authenticated user to select for transfers.

## Files to Edit/Create
- `backend/src/api/user.rs`

## Acceptance Criteria
- Only return friends with ACCEPTED status.

## Guidance / Hints
Perform quick index search in friendships table.
