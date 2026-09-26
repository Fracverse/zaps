# Social Payment Contract Changes

## Added

- **#976** — bulk transaction fee discount for `batch_payout`
  (`contracts/contracts/social_payment/src/lib.rs`). The platform fee is now tiered
  on the number of payouts a batch settles, on top of the existing volume-based fee,
  so batching is cheaper per recipient than sending the same payments individually:

  | Settled payouts | Discount | Effective coefficient at the default 10 bps |
  | --- | --- | --- |
  | 1–4 | 0% | 10 bps |
  | 5–9 | 5% | 9.5 bps |
  | 10–24 | 10% | 9 bps |
  | 25–49 | 20% | 8 bps |
  | 50–100 | 30% | 7 bps |

- Added `batch_discount_bps()` and `discounted_fee()` helpers plus the
  `BATCH_DISCOUNT_MIN_ITEMS` threshold, so the tier table lives in one place.
- Implemented `comment_payment` in `contracts/contracts/social_payment/src/lib.rs`.
- Added validation to reject comments longer than 120 characters.
- Added emission of a Soroban event named `PaymentCommented` when a comment is posted.

## Notes

- The batch fee quote used for the balance check is now computed with
  `calculate_fee()`, the same expression single payments use, so the quote can never
  disagree with a `pay` call and it no longer overflows on large volumes.
- The fee is still charged on settled volume only, and the 1-stroop minimum fee is
  re-applied after the discount, so a settled batch can never route a zero fee to the
  treasury.
- Batch sizes below `BATCH_DISCOUNT_MIN_ITEMS` (5 payouts) are charged exactly the
  single-payment rate, which keeps existing batch behaviour unchanged at small sizes.
- The event payload includes the target transaction identifier (`tx_id`) and the comment text.
- The change keeps the existing authorization requirement with `sender.require_auth()`.
- A new test file was added at `contracts/contracts/social_payment/tests/comment_payment.rs` to exercise valid comment submission and over-length rejection.
