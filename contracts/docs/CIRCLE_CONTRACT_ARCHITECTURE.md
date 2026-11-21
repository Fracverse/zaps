# Zaps Circle Smart Contract Architecture

## Overview

Zaps Circles enables pooled savings and contribution groups on the Stellar network,
governed by multi-member approval and backed by Soroban smart contracts.

This document describes the on-chain architecture, storage layout, authorization
model, asset handling, and upgrade considerations for the Circle contract suite.

---

## Architecture Decision: Single Contract per Circle

Each circle is deployed as an **independent Soroban contract instance** rather
than a factory pattern.  This design was chosen because:

- **Isolation** — a bug or exploit in one circle cannot drain another circle's
  funds.
- **Parallelism** — Soroban's fee model charges per ledger entry accessed; a
  single fat factory contract touching every circle's storage would pay for all
  of them on every invocation.
- **Upgrade granularity** — individual circles can be migrated on a per-instance
  basis without coordinating a global upgrade.
- **Authorization simplicity** — each contract's admin/approver set is self-
  contained and does not interact with other circles.

A lightweight **off-chain registry** (the backend `circles` table) maps human-
readable circle IDs to their Stellar contract addresses.

---

## Contracts

### `circle` — Core Circle Contract

**Location:** `contracts/contracts/circle`

Manages the full lifecycle of a savings circle:

| Responsibility | Details |
|---|---|
| Membership | `add_member`, `remove_member`, `is_member` |
| Contributions | `contribute(token, amount)` — transfers tokens in, records per-member totals |
| Payout requests | `request_payout(to, amount, asset)` — creates a pending request with an auto-incrementing `request_id` |
| Multi-sig approvals | `approve(request_id)` — each approver casts one vote; executes automatically when `threshold` is reached |
| Savings goals | `create_goal`, `update_goal_progress`, `complete_goal` |
| Governance | `set_threshold`, `set_max_payout`, `add_approver`, `remove_approver` |
| Balance queries | `get_balance(asset)` — returns `CircleBalance` |

---

## Storage Structures

All storage is keyed off the Soroban `Env` persistent ledger.

```rust
// Stored under DataKey::CircleInfo
pub struct Circle {
    pub admin: Address,
    pub token: Address,
    pub balance: i128,
    pub reserved: i128,     // Locked by pending payout requests
    pub max_payout: i128,   // 0 = unlimited
    pub threshold: u32,     // Minimum approvals to execute a payout
    pub approvers: Vec<Address>,
    pub next_request: u32,
    pub total_contributed: i128,
    pub total_paid: i128,
}

// Stored under DataKey::Payout(request_id)
pub struct Payout {
    pub requester: Address,
    pub to: Address,
    pub amount: i128,
    pub asset: Address,
    pub status: RequestStatus,
    pub threshold: u32,     // Captured at request time; governance changes don't alter it
    pub approvals: Vec<Address>,
    pub executed: bool,
}

// Stored under DataKey::Goal(goal_id)
pub struct SavingsGoal {
    pub name: soroban_sdk::Bytes,
    pub target: i128,
    pub current: i128,
    pub deadline: Option<u64>,   // ledger sequence
    pub completed: bool,
}

// Stored under DataKey::Member(address)
pub struct MemberInfo {
    pub total_contributed: i128,
    pub joined_ledger: u32,
}
```

### DataKey Enum

```rust
#[contracttype]
pub enum DataKey {
    CircleInfo,
    Payout(u32),
    Member(Address),
    AssetBalance(Address),
    Goal(u32),
    NextGoal,
}
```

---

## Authorization Requirements

| Operation | Required authorization |
|---|---|
| `initialize` | Deployer (called once at deploy time) |
| `contribute` | Any member (`require_auth` on caller; must be in members list) |
| `request_payout` | Any member |
| `approve` | Must be a registered approver (`require_auth` on caller) |
| `add_member` | Admin only (`require_auth` on `circle.admin`) |
| `remove_member` | Admin or the member themselves; blocked if pending obligations exist |
| `set_threshold` | Admin only |
| `set_max_payout` | Admin only |
| `add_approver` / `remove_approver` | Admin only |
| `create_goal` | Admin only |
| `upgrade` | Admin only (via contract `upgrade` host function) |

All Soroban `require_auth` calls use the standard Stellar authorization framework
(passkey / ed25519 / multisig).  No custom session token or off-chain JWT is
accepted by the contract itself.

---

## Supported Assets

The contract is **asset-agnostic**: any SEP-41-compatible token can be used as
the circle's primary token, and payout requests may target a different asset
(e.g., the circle collects USDC but pays out XLM).

- `Circle.token` is the default contribution asset.
- `Payout.asset` is the asset to be sent on execution and may differ.
- Asset balances are tracked per-asset via `DataKey::AssetBalance(asset_address)`.
- The contract calls the standard SEP-41 `transfer` function; no custom token
  logic is required.

---

## Contribution and Payout Workflow

```
Member                   Circle Contract         Token Contract
  |                            |                       |
  |-- contribute(amount) ----->|                       |
  |                            |-- transfer_from ----->|
  |                            |<-- ok ---------------|
  |                            | +balance, +contributed
  |
  |-- request_payout(to, amt)->|
  |                            | reserve(amt)
  |                            | store Payout{status: Pending}
  |<-- request_id -------------|
  |
  |-- approve(request_id) ---->|  (repeated for each approver)
  |                            | approvals += 1
  |                            | if approvals >= threshold:
  |                            |   execute_payout()
  |                            |   -- transfer(to, amt) -->|
  |                            |   unreserve(amt)
  |                            |   total_paid += amt
  |<-- ApprovalProgress -------|
```

---

## Security Risks and Mitigations

| Risk | Mitigation |
|---|---|
| **Double-spend on pending payouts** | The `reserved` balance is incremented atomically when a request is created and can never be re-spent until the request is executed or cancelled. |
| **Threshold manipulation** | Each `Payout` captures `threshold` at creation time; a governance change cannot lower the bar for an in-flight request. |
| **Admin key compromise** | Admin can be rotated by calling `set_admin` with the existing admin's signature. The deployer should use a hardware key or multisig wallet for the admin address. |
| **Malicious approver** | Approvers are limited to a registered list controlled by the admin. A single approver cannot unilaterally execute if `threshold > 1`. |
| **Re-entrancy** | Soroban's execution model is deterministic and single-threaded per transaction; re-entrancy attacks are not applicable. |
| **Integer overflow** | All arithmetic uses checked Rust i128 operations; the `Error::Overflow` variant is returned instead of panicking. |
| **Expired deadlines** | Payout requests can be given a ledger-sequence deadline; the `Error::DeadlineExpired` path cancels them and releases the reserved amount. |
| **Griefing via many small requests** | `max_payout` is enforced per request; an admin can set a minimum to prevent spam. Per-member contribution tracking disincentivises bad actors. |

---

## Upgrade and Migration Considerations

- Each contract instance stores a **schema version** in `DataKey::SchemaVersion`.
- Upgrades are performed via `env.deployer().upgrade_contract_wasm(new_wasm_hash)`,
  callable only by the admin.
- State migration logic (if needed) runs in the new WASM's `__migrate` entry
  point, executed once per instance on first invocation after an upgrade.
- Because each circle is an independent contract, upgrades can be rolled out
  gradually (e.g., opt-in per circle) rather than all-at-once.
- The backend tracks `contract_wasm_hash` per circle so the indexer can
  emit a `ContractUpgraded` event and prompt members to review the change.

---

## Running the Circle Contract Locally

```sh
# Build
cd contracts
cargo build --target wasm32-unknown-unknown --release

# Run unit tests
cargo test -p circle

# Deploy to Testnet (requires Stellar CLI and funded account)
stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/circle.wasm \
  --source <ADMIN_SECRET_KEY> \
  --network testnet

# Initialize
stellar contract invoke \
  --id <CONTRACT_ADDRESS> \
  --source <ADMIN_SECRET_KEY> \
  --network testnet \
  -- initialize \
  --admin <ADMIN_ADDRESS> \
  --token <TOKEN_ADDRESS> \
  --max_payout 0 \
  --threshold 2 \
  --approvers '["<APPROVER_1>","<APPROVER_2>"]'
```
