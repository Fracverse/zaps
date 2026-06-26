# BACKEND [BE-018]: Add structured request logging middleware

## Description
Integrate JSON/tracing log details to monitor server requests and errors.

## Files to Edit/Create
- `backend/src/main.rs`

## Acceptance Criteria
- Print status code, path, duration, errors in clean format.

## Guidance / Hints
Setup `tower_http::trace::TraceLayer`.
