# DEVOPS [DO-002]: Setup Github Actions CI workflow for compiling Soroban smart contracts

## Description
Add workflow to compile contracts and verify checks on commits.

## Files to Edit/Create
- `.github/workflows/ci-contracts.yml`

## Acceptance Criteria
- Build wasm files automatically on pull requests.

## Guidance / Hints
Install Rust target wasm32-unknown-unknown before compiling.
