# BACKEND [BE-006]: Create Feed Fetch API endpoint for Public payments

## Description
Return a scrollable, paginated public feed of social payments across the platform.

## Files to Edit/Create
- `backend/src/api/feed.rs`

## Acceptance Criteria
- Exclude transactions marked private or friends-only.

## Guidance / Hints
Filter in SQL: `WHERE visibility = 'PUBLIC'` order by `created_at DESC`.
