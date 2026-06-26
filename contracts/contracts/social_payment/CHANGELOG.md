# Social Payment Contract Changes

## Added

- Implemented `comment_payment` in `contracts/contracts/social_payment/src/lib.rs`.
- Added validation to reject comments longer than 120 characters.
- Added emission of a Soroban event named `PaymentCommented` when a comment is posted.

## Notes

- The event payload includes the target transaction identifier (`tx_id`) and the comment text.
- The change keeps the existing authorization requirement with `sender.require_auth()`.
- A new test file was added at `contracts/contracts/social_payment/tests/comment_payment.rs` to exercise valid comment submission and over-length rejection.
