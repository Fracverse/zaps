# SMART-CONTRACT [SC-007]: Implement on-chain like tracking in payment contract

## Description
Allow users to send a 'like' transaction referencing a payment's on-chain transaction hash.

## Files to Edit/Create
- `contracts/contracts/social_payment/src/lib.rs`

## Acceptance Criteria
- Require sender auth.
- Emit a PaymentLiked event containing tx hash and user address.

## Guidance / Hints
Emit a clean event that the indexer can parse, keeping storage costs minimal.
