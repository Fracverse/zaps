# DEVOPS [DO-003]: Setup Github Actions CI workflow for cargo test and clippy on backend

## Description
Verify backend code compilation and quality standards on pull request.

## Files to Edit/Create
- `.github/workflows/ci-backend.yml`

## Acceptance Criteria
- Enforce compilation success, formatting checks, and tests passes.

## Guidance / Hints
Run `cargo fmt --check` and `cargo clippy -- -D warnings`.
