# Privy Authentication Endpoint

## Overview

The `POST /api/auth/privy` endpoint creates new user accounts linked to Privy identities. It verifies a Privy token, establishes a bidirectional link between a Privy DID (Decentralized Identity) and a Stellar address, and returns JWT access credentials for subsequent API calls.

## Endpoint

```
POST /api/auth/privy
Content-Type: application/json
```

## Request

```json
{
  "privy_token": "string",
  "privy_did": "did:privy:user_abc123",
  "stellar_address": "GBPK7THXDEPNBQB5K3EMQL5FZAQLHJ4XPBWJFNV3EPJN7CVPQGJZ6PBN"
}
```

### Parameters

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `privy_token` | string | Yes | Privy authentication token to verify |
| `privy_did` | string | Yes | Privy Decentralized Identity (format: `did:privy:*`) |
| `stellar_address` | string | Yes | Stellar G-address (56 characters, starts with G) |

## Response

### Success (201 Created)

```json
{
  "token": "eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9...",
  "username": "u_GBPK7THXDEPNBQB5K",
  "privy_did": "did:privy:user_abc123"
}
```

### Error Responses

#### 400 Bad Request

Invalid Stellar address format:
```json
{
  "error": "Invalid Stellar address format"
}
```

Invalid Privy DID format:
```json
{
  "error": "Invalid Privy DID format"
}
```

#### 401 Unauthorized

Token verification failed:
```json
{
  "error": "Privy token verification failed"
}
```

#### 409 Conflict

Stellar address already linked to different Privy identity:
```json
{
  "error": "This Stellar address is already linked to a different Privy identity"
}
```

Privy DID already linked to different Stellar address:
```json
{
  "error": "This Privy identity is already linked to a different Stellar address"
}
```

Privy identity already linked to another account (due to UNIQUE constraint):
```json
{
  "error": "This Privy identity is already linked to another account"
}
```

#### 500 Internal Server Error

Database or JWT generation error:
```json
{
  "error": "Internal database error"
}
```

## Implementation Details

### Constraint Checking

The endpoint enforces strict one-to-one mapping between Privy DIDs and Stellar addresses:

1. **Address→DID check**: Verifies the target Stellar address is not linked to a different Privy DID
   - Allows re-authentication with the same address+DID pair
   - Rejects attempts to link one address to multiple DIDs

2. **DID→Address check**: Verifies the Privy DID is not linked to a different Stellar address
   - Rejects attempts to link one DID to multiple addresses
   - Caught both before INSERT and via UNIQUE constraint

### Data Model

Database schema (`users` table):

```sql
ALTER TABLE users ADD COLUMN privy_did VARCHAR(255) UNIQUE;
ALTER TABLE users ADD COLUMN privy_linked_at TIMESTAMP;
CREATE INDEX idx_users_privy_did ON users(privy_did) WHERE privy_did IS NOT NULL;
```

### JWT Token

- Issued with 24-hour expiration
- Contains `sub` claim set to Stellar address
- Uses `JWT_SECRET` environment variable
- Same format as `/api/auth/verify` endpoint

### User Creation Flow

1. Generate username from Stellar address: `u_{first_14_chars}`
2. Create or update user with:
   - `address` (unique)
   - `username` (unique)
   - `privy_did` (unique)
   - `privy_linked_at` (timestamp)
3. Return JWT token + username + privy_did

## Security Considerations

### Privy Token Verification

**⚠️ PLACEHOLDER IMPLEMENTATION**: The current implementation includes a stub `verify_privy_token()` function that performs basic validation. In production, you must:

1. Integrate with Privy's token verification API
   - Reference: [Privy API Docs](https://docs.privy.com/reference)
   - Endpoint: `POST https://auth.privy.io/api/v1/verify_token`

2. Verify JWT signature using Privy's public key

3. Extract and validate DID claim from token

4. Check token expiration

5. Consider caching Privy verification results for performance

### Stellar Address Validation

- Validates address format (56 characters, G prefix)
- Verifies CRC16 checksum
- Extracts and validates Ed25519 public key
- Does NOT verify that the address actually exists on Stellar network

### Rate Limiting

The endpoint is protected by the auth route rate limiter:
- 5 requests per second per IP
- Maximum burst of 10 requests
- Applied to all `/api/auth/*` routes

## Usage Example

### Client Request

```bash
curl -X POST http://localhost:8080/api/auth/privy \
  -H "Content-Type: application/json" \
  -d '{
    "privy_token": "privy_token_from_privy_sdk",
    "privy_did": "did:privy:user_xyz789",
    "stellar_address": "GBPK7THXDEPNBQB5K3EMQL5FZAQLHJ4XPBWJFNV3EPJN7CVPQGJZ6PBN"
  }'
```

### Response

```json
{
  "token": "eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJHQlBLN1RIWERFUE5CUUIzRU1RRjVGWkFRTEhKNFhQQldKRk5WM0VQSk43Q1ZQUUdKWjZQQk4iLCJleHAiOjE3MjI5NTI5MzV9.xyz...",
  "username": "u_GBPK7THXDEPNBQB5K",
  "privy_did": "did:privy:user_xyz789"
}
```

Use the returned JWT in subsequent API calls:

```bash
curl -H "Authorization: Bearer eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9..." \
  http://localhost:8080/api/users/profile
```

## Testing

### Unit Tests

Located in `tests/privy_auth_tests.rs`:

1. Valid Privy auth creates user with DID linkage
2. Reject address linked to different DID
3. Reject DID linked to different address
4. Allow re-authentication with same pair
5. Reject invalid Stellar address format
6. Reject invalid Privy DID format
7. Reject invalid token

Run tests:
```bash
cargo test privy_auth -- --nocapture --ignored
```

### Integration Testing

1. **Create new Privy identity** in your test environment
2. **Link to Stellar address** via `/api/auth/privy`
3. **Verify JWT token** works with authenticated endpoints
4. **Test constraint violations**:
   - Link address to multiple DIDs (should fail)
   - Link DID to multiple addresses (should fail)

## Migration

Applied via `migrations/20260726000001_privy_identity.sql`:

```sql
ALTER TABLE users
ADD COLUMN privy_did VARCHAR(255) UNIQUE,
ADD COLUMN privy_linked_at TIMESTAMP;

CREATE INDEX idx_users_privy_did
    ON users(privy_did)
    WHERE privy_did IS NOT NULL;
```

## Future Enhancements

1. **Implement full Privy token verification** with real API calls
2. **Add DID unlinking** endpoint (admin-only or user-initiated)
3. **Support DID rotation** for users switching Privy accounts
4. **Add audit logging** for sensitive identity changes
5. **Implement token revocation** mechanism
6. **Add rate limiting per unique DID/address** to prevent abuse

## Related Endpoints

- `GET /api/auth/challenge` - Get challenge for Stellar signature auth
- `POST /api/auth/verify` - Stellar address authentication (alternative)
- `GET /api/users/profile` - Get authenticated user profile
- `PUT /api/users/profile` - Update user profile

## References

- [Privy Documentation](https://docs.privy.com/)
- [Stellar Address Format](https://developers.stellar.org/docs/tutorials/create-account#understanding-stellar-account-id-types)
- [JWT.io](https://jwt.io/)
