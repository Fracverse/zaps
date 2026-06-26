# Zaps Backend API Server (Axum & Rust)

Welcome to the rebranded Zaps Backend codebase! This is a high-performance Rust web server written using the **Axum** framework and **SQLx** for database persistence.

## Architecture

The backend handles the social layers of Zaps and indexes transaction states from Stellar:
- **`api/`**: Axum endpoints grouped by features (User profiles/friends, Social feed parsing, Like/Comment registry, Allbridge integration proxies).
- **`db/`**: SQL database models and PostgreSQL initialization script (`schema.sql`).
- **`indexer/`**: Background worker daemon that polls Soroban events and caches them in the PostgreSQL database.
- **`services/`**: Integration clients for the Allbridge API and Stellar network calls.

## How to Contribute

This codebase is structured with stubs and `todo!()` macros so that contributors can implement features step-by-step.
1. Check the local issues under `/issues/backend/` or GitHub Issues.
2. Locate the corresponding file in `src/` (e.g., `src/api/feed.rs` for feed pagination).
3. Replace the placeholder stubs with proper Rust/SQLx implementation logic.
4. Add unit tests where appropriate.

## Local Development

Ensure PostgreSQL is running locally and set the `DATABASE_URL` environment variable.

To verify compiling:
```bash
cargo check
```

To run the development server:
```bash
cargo run
```
