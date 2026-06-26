# SMART-CONTRACT [SC-008]: Implement on-chain comment event emission

## Description
Allow users to post comment messages targeting a payment's on-chain identifier.

## Files to Edit/Create
- `contracts/contracts/social_payment/src/lib.rs`

## Acceptance Criteria
- Limit comment string length to 120 chars.
- Emit PaymentCommented event containing comment text.

## Guidance / Hints
Validate comment string length using `comment.len()` before emitting.
