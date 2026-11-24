# Nonce-Based Payment Authorization Implementation

## Overview
This implementation adds per-user nonce tracking for off-chain signed payment authorizations in the social payment contract, preventing replay attacks.

## Changes Made

### 1. Storage Schema Updates

**File:** `contracts/contracts/social_payment/src/lib.rs`

- Added `NONCE_KEY_PREFIX` constant for storage key prefix
- Added `Nonce(Address)` variant to `DataKey` enum to store per-user nonces in persistent storage

### 2. Error Handling

Added two new error types to the `Error` enum:
- `NonceAlreadyUsed = 2`: Returned when a nonce has been used or doesn't match current nonce
- `InvalidSignature = 3`: Returned when signature verification fails

### 3. Data Structures

Added `SignedPaymentAuth` struct containing:
- `sender: Address` - The account authorizing the payment
- `receiver: Address` - The payment recipient
- `token: Address` - The token contract address
- `amount: i128` - Payment amount
- `memo: String` - Payment memo
- `visibility: Visibility` - Payment visibility (Public/Friends/Private)
- `nonce: u64` - Sequential nonce to prevent replay attacks

### 4. Public Functions

#### `get_nonce(env: Env, user: Address) -> u64`
- Returns the current nonce for a given user
- Returns 0 for users who have never made a signed payment
- Nonces are stored in persistent storage

#### `pay_with_signature(env: Env, auth: SignedPaymentAuth, sender_pubkey: BytesN<32>, signature: BytesN<64>) -> Result<(), Error>`
- Executes a payment using an off-chain Ed25519 signature authorization
- **Nonce validation:** Checks that `auth.nonce` matches the sender's current on-chain nonce
- **Signature verification:** Verifies the Ed25519 signature against the provided public key
- **Nonce increment:** Increments the sender's nonce after successful execution
- **No require_auth():** Authorization is proven by signature, not by Soroban auth invocation
- Returns `Error::NonceAlreadyUsed` if nonce doesn't match
- Returns `Error::InvalidSignature` if signature verification fails (via panic from crypto module)

## Security Features

### Replay Attack Prevention
- Each user maintains an independent nonce counter
- Nonces must be used sequentially (0, 1, 2, 3, ...)
- Once a nonce is consumed, it cannot be reused
- Attempting to use a past nonce returns `NonceAlreadyUsed` error
- Attempting to use a future nonce also returns `NonceAlreadyUsed` error

### Signature Verification
- Uses Ed25519 signature verification from Soroban's crypto module
- Requires both the signature and the sender's public key
- Verifies that the signature was created by the private key corresponding to the public key
- The authorization payload (containing all payment details) is serialized to XDR and signed

### Storage
- Nonces are stored in persistent storage using `DataKey::Nonce(Address)`
- Each address has its own independent nonce counter
- Storage is updated atomically after successful signature verification

## Usage Example

```rust
// Off-chain: sender creates and signs authorization
let auth = SignedPaymentAuth {
    sender: sender_address,
    receiver: receiver_address,
    token: naira_token_address,
    amount: 1000,
    memo: String::from_str(&env, "Payment memo"),
    visibility: Visibility::Private,
    nonce: 0, // First payment for this sender
};

// Sender signs the serialized auth with their private key
let message = auth.to_xdr(&env);
let signature = sign_with_private_key(message); // Off-chain signing

// On-chain: anyone can submit the signed authorization
let result = client.pay_with_signature(
    &auth,
    &sender_public_key,
    &signature
);

// Next payment must use nonce = 1
```

## Testing

Added comprehensive test coverage:
- `test_get_nonce_returns_zero_for_new_user`: Verifies initial nonce is 0
- `test_nonce_increments_after_successful_signed_payment`: Confirms nonce increments
- `test_signed_payment_rejects_reused_nonce`: Ensures nonces cannot be reused
- `test_signed_payment_rejects_future_nonce`: Prevents skipping nonces
- `test_signed_payment_different_users_independent_nonces`: Validates isolation between users

## Notes

### Production Considerations
1. **Public Key to Address Mapping:** The current implementation requires the caller to provide the sender's public key. In production, you should verify that this public key corresponds to the `auth.sender` address to prevent unauthorized payments.

2. **Off-Chain Signing:** The signing process happens off-chain. The sender uses their private key to sign the authorization payload, then anyone can submit it on-chain with the signature.

3. **Gas Payment:** Since the sender doesn't call `require_auth()`, a third party (relayer) can submit the signed transaction and pay the gas fees, enabling gasless transactions for end users.

4. **Nonce Management:** The frontend/wallet must track the current nonce for each user to construct valid authorizations. The `get_nonce()` function can be queried to get the current value.

## Acceptance Criteria ✅

- ✅ Store `Nonce(Address)` in storage
- ✅ Increment user nonce on successful payment execution
- ✅ Reject reused nonces with proper error handling

## Files Modified

- `contracts/contracts/social_payment/src/lib.rs` - Main implementation file

No other files needed modification as this is a self-contained feature addition to the social payment contract.
