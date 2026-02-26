# BLINKS Backend

A Rust-based backend for the BLINKS payment system, built on Stellar network with Soroban smart contracts.

## Architecture

The BLINKS backend provides the following services:

- **Identity & Wallet Service**: User management and Stellar address resolution
- **Payment Orchestrator**: QR/NFC payment processing and validation
- **Bridge & Settlement Coordinator**: Cross-chain asset bridging to Stellar
- **Anchor Integration Service**: SEP-24/SEP-31 integration for fiat on/off ramps
- **Compliance & Risk Engine**: Sanctions screening and transaction monitoring
- **Transaction Log & Audit Service**: Immutable audit trails
- **Admin Dashboard API**: System monitoring and management
- **Indexer / Ledger Listener**: Stellar network event monitoring

## Quick Start

### Prerequisites

- Rust 1.70+
- PostgreSQL 13+
- Stellar CLI (optional, for development)

### Setup

1. **Clone and navigate to backend:**
   ```bash
   cd backend
   ```

2. **Install dependencies:**
   ```bash
   cargo build
   ```

3. **Database setup:**
   ```bash
   # Create PostgreSQL database
   createdb BLINKS

   # Set environment variables
   cp env.example .env
   # Edit .env with your database URL and other settings
   ```

4. **Run migrations:**
   ```bash
   cargo run --bin migrate
   ```

5. **Start the server:**
   ```bash
   cargo run
   ```

The server will start on `http://localhost:3000`.

### Configuration

Configuration is loaded from:
1. `config/default.toml` - Default configuration
2. `config/{RUN_ENV}.toml` - Environment-specific overrides
3. Environment variables with `BLINKS_` prefix

### API Endpoints

#### Health Check
- `GET /health` - Basic health check
- `GET /ready` - Readiness check with database connectivity

#### Authentication
- `POST /auth/login` - User login
- `POST /auth/register` - User registration
- `POST /auth/refresh` - Token refresh

#### Identity & Wallet (Protected)
- `POST /identity/users` - Create user
- `GET /identity/users/{user_id}` - Get user details
- `GET /identity/users/{user_id}/wallet` - Get user wallet
- `GET /identity/resolve/{user_id}` - Resolve User ID to Stellar address

#### Payments (Protected)
- `POST /payments` - Create payment
- `GET /payments/{id}` - Get payment details
- `GET /payments/{id}/status` - Get payment status
- `POST /payments/qr/generate` - Generate QR payment
- `POST /payments/nfc/validate` - Validate NFC payment

#### User-to-User Transfers (Protected)

Direct transfers between two BLINKS users are exposed via the **Transfers** API. These endpoints construct an **unsigned Stellar transaction XDR** that the client signs and submits, keeping funds non-custodial.

- `POST /transfers/transfers` - Build an unsigned user-to-user transfer XDR
- `GET /transfers/transfers/{id}` - Get transfer details (skeletal, subject to extension)
- `GET /transfers/transfers/{id}/status` - Get transfer status (skeletal, subject to extension)

##### `POST /transfers/transfers` – Build unsigned XDR for a direct transfer

**Purpose**

- Create an **unsigned Stellar transaction XDR** representing a transfer from the authenticated user (`from_user_id`) to another BLINKS user (`to_user_id`).
- Validate that the recipient exists and has a structurally valid Stellar address.
- Support an optional memo for business / reconciliation needs.

**Authentication**

- Requires a valid JWT access token.
- The authenticated user is resolved from the token (`sub`) and injected as `AuthenticatedUser` by the auth middleware.

**Request Body**

```json
{
  "to_user_id": "alice",
  "amount": 1000000,
  "asset": "USDC",
  "memo": "Rent payment January"
}
```

- **`to_user_id`**: Target BLINKS user identifier. Must correspond to an existing row in the `users` table.
- **`amount`**: Integer amount in the smallest unit for the given asset (e.g. 1 USDC = 1_000_000 if using 7 decimals). Must be `> 0`.
- **`asset`**: Logical asset code used by your application (e.g. `USDC`).
- **`memo`** *(optional)*: Free-text memo attached to the logical transfer for downstream reconciliation and user UX.

**Validation Rules**

- `amount` must be strictly greater than zero; otherwise the handler returns:
  - `400 BAD_REQUEST` with `error="VALIDATION_ERROR"`.
- `to_user_id` must resolve to an existing user:
  - Backed by `IdentityService::get_user_by_id`.
  - If not found, the handler returns:
    - `404 NOT_FOUND` with `error="NOT_FOUND"`.
- The recipient must have a structurally valid Stellar address:
  - Current implementation checks that the address is non-empty and starts with `G`.
  - If invalid, the handler returns:
    - `400 BAD_REQUEST` with `error="VALIDATION_ERROR"` and message `"Recipient has an invalid Stellar address"`.
- The sender (`from_user_id`) is taken from the authenticated JWT subject and resolved via `IdentityService::get_user_wallet` to obtain the sender’s Stellar address.

**Backend Flow**

Implementation lives in `backend/src/http/transfers.rs`:

- Extracts `AuthenticatedUser` (via middleware) to obtain `from_user_id`.
- Uses `IdentityService` to:
  - Fetch the sender wallet (`get_user_wallet`) and derive the sender Stellar address.
  - Resolve `to_user_id` into a `User` and obtain the recipient Stellar address.
- Performs a lightweight Stellar address validation for the recipient.
- Constructs a `BuildTransactionDto` with:
  - `contract_id = "user_to_user_transfer"` – a logical identifier for the user-to-user transfer contract or flow.
  - `method = "transfer"` – the contract method being invoked.
  - `args` – JSON-encoded argument list containing:
    - `from_user_id`, `from_address`
    - `to_user_id`, `to_address`
    - `asset`, `amount`
    - `memo` (optional)
- Invokes `SorobanService::build_transaction(dto)` which returns a **mock unsigned XDR** string (in production this would be a real Stellar/Soroban transaction XDR).
- Synthesizes a **transient transfer identifier** (`Uuid::new_v4()`) and returns it alongside the unsigned XDR.

**Response**

```json
{
  "id": "00000000-0000-0000-0000-000000000000",
  "from_user_id": "bob",
  "to_user_id": "alice",
  "amount": 1000000,
  "asset": "USDC",
  "status": "pending",
  "memo": "Rent payment January",
  "unsigned_xdr": "mock_xdr_invoke_user_to_user_transfer_transfer_[...]"
}
```

- **`id`**: UUID generated server-side for this transfer request. Currently ephemeral until a full persistence layer for transfers is added.
- **`from_user_id`**: The authenticated user (JWT subject).
- **`to_user_id`**: Recipient BLINKS user ID.
- **`amount`**: Requested transfer amount.
- **`asset`**: Asset code.
- **`status`**: Currently fixed to `"pending"` to reflect that the transfer has not yet been signed or submitted.
- **`memo`**: Echoes the request memo, if provided.
- **`unsigned_xdr`**: Base64-encoded (mock) unsigned transaction XDR that the client must sign and submit to the Stellar network.

**Client Responsibilities**

- Sign the `unsigned_xdr` with the user’s Stellar private key on the client side.
- Submit the signed XDR to the Stellar network (e.g. via Horizon/Soroban RPC or another backend endpoint).
- Optionally store or correlate the returned `id` and `memo` for user receipts and history views.

#### Admin (Protected, Admin Only)
- `GET /admin/dashboard/stats` - Dashboard statistics
- `GET /admin/transactions` - Transaction listing
- `GET /admin/users/{user_id}/activity` - User activity log
- `GET /admin/system/health` - System health status

## Development

### Running Tests

```bash
cargo test
```

### Code Formatting

```bash
cargo fmt
```

### Linting

```bash
cargo clippy
```

### Database Migrations

Migrations are automatically run on startup. 

**Creating a new migration:**
```bash
# Using the Rust binary (recommended)
cargo run --bin new_migration -- <description>
```

**Important:** Migration filenames must have unique timestamps to avoid conflicts.

To manually run migrations:
```bash
cargo run --bin migrate
```

## Security Considerations

- JWT tokens expire after 24 hours by default
- All user funds remain non-custodial
- Transactions are signed client-side
- Compliance checks are performed on all transactions
- Audit logs are immutable and comprehensive

## Deployment

The backend is designed to be deployed as a single binary:

```bash
cargo build --release
./target/release/BLINKS-backend
```

Use environment variables or config files to configure for different environments.

## Architecture Details

### Service Layer

The backend follows a modular service architecture:

- Each service handles a specific domain (identity, payments, compliance, etc.)
- Services are stateless and receive database connections via dependency injection
- All business logic is contained within service methods

### Middleware

- **Authentication**: JWT-based user authentication
- **Authorization**: Role-based access control
- **Metrics**: Prometheus metrics collection
- **Request ID**: Request tracing and correlation
- **CORS**: Cross-origin resource sharing

### Database Schema

The PostgreSQL database contains the following main tables:

- `users` - User accounts and Stellar addresses
- `merchants` - Merchant configurations and vaults
- `payments` - Payment transactions
- `transfers` - User-to-user transfers
- `withdrawals` - Withdrawal transactions
- `balances` - Account balances
- `audit_logs` - Audit trail
- `bridge_transactions` - Cross-chain bridge transactions

## Contributing

1. Follow Rust best practices and idioms
2. Write tests for new functionality
3. Update documentation for API changes
4. Ensure code passes `cargo clippy` and `cargo fmt`

## License

This project is part of the BLINKS ecosystem.