# SMART-CONTRACT [SC-013]: Implement fee distribution logic on social transfers

## Description
Deduct a 0.1% platform fee on public payments and route it to a treasury address.

## Files to Edit/Create
- `contracts/contracts/social_payment/src/lib.rs`

## Acceptance Criteria
- Calculate fee accurately (amount * 1 / 1000).
- Transfer fee to the designated address.

## Guidance / Hints
Handle integer division rounding down safely.
