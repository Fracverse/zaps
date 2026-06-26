# BACKEND [BE-001]: Setup PostgreSQL database connection pool and database migration framework

## Description
Initialize SQLx connection pool and load migrations successfully on startup.

## Files to Edit/Create
- `backend/src/main.rs` 
- `backend/src/db/mod.rs`

## Acceptance Criteria
- Server must read DATABASE_URL and launch migrations.

## Guidance / Hints
Use `sqlx::migrate!()` to run the migration directory on bootstrap.
