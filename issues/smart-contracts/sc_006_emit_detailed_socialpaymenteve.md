# SMART-CONTRACT [SC-006]: Emit detailed SocialPaymentEvent on successful payment

## Description
Emit a rich event on payment so the background indexer can parse it and write to database.

## Files to Edit/Create
- `contracts/contracts/social_payment/src/lib.rs`

## Acceptance Criteria
- Event must contain sender, receiver, amount, memo note, and visibility tier.

## Guidance / Hints
Use `env.events().publish()` to broadcast structured payment tags.
