# BACKEND [BE-003]: Implement Stellar wallet-based authentication challenge-response route

## Description
Generate cryptographically secure mock challenges for client signatures and verify them.

## Files to Edit/Create
- `backend/src/api/auth.rs`

## Acceptance Criteria
- Verify signature matches wallet address.
- Return JWT token on successful sign-in.

## Guidance / Hints
Verify messages using standard Stellar signature verification algorithms (Ed25519).
