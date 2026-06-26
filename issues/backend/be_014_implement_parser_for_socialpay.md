# BACKEND [BE-014]: Implement parser for SocialPaymentEvent ledger logs

## Description
Decode blockchain event payloads into fields: sender, receiver, amount, memo, visibility.

## Files to Edit/Create
- `backend/src/indexer/worker.rs`

## Acceptance Criteria
- Decrypted event fields must match database format.

## Guidance / Hints
Map on-chain addresses to database user UUIDs before inserts.
