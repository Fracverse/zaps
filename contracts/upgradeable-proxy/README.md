# Upgradeable Proxy Contract

This contract provides a safe upgrade framework for Soroban-based contracts with:

- Transparent proxy-style forwarding (`forward`) to a current implementation.
- Admin-only staged upgrades (`stage_upgrade` + `execute_upgrade`).
- Data migration steps executed during upgrades.
- On-chain version tracking and upgrade history.
- Rollback support to the previous implementation/version.
- Emergency pause for runtime call forwarding.

## Core Design

### 1. Transparent Proxy Pattern
- `forward(caller, method, args)` dispatches calls to the current implementation.
- The `caller` must authenticate (`caller.require_auth()`).
- Admin is blocked from fallback forwarding (`AdminCannotFallback`) to keep control-plane and data-plane concerns separated.

### 2. Upgrade Authorization
- All privileged operations require stored admin auth:
  - `stage_upgrade`
  - `execute_upgrade`
  - `rollback`
  - `pause`
  - `unpause`
  - `transfer_admin`
  - `set_state_value`

### 3. Version Management
- Tracks current implementation and version:
  - `current_implementation()`
  - `current_version()`
- Keeps per-version implementation mapping:
  - `implementation_for_version(version)`
- Persists historical upgrade records:
  - `get_upgrade_history()`

### 4. Data Migration Framework
- Upgrade is staged with `Vec<MigrationStep>`.
- Each step targets a state key and migration script contract:
  - `key: Symbol`
  - `script: Address`
  - `data: i128` (script-specific data)
- During `execute_upgrade`, each migration script is invoked before implementation/version switch.

Migration script interface:

```rust
fn migrate(
    env: Env,
    key: Symbol,
    current_value: i128,
    data: i128,
    from_version: u32,
    to_version: u32,
) -> i128
```

### 5. Rollback
- `rollback()` restores the immediately previous implementation/version.
- Rollbacks are appended to upgrade history with `rollback = true`.

### 6. Emergency Pause
- `pause()` blocks `forward` calls.
- `unpause()` re-enables forwarding.
- Control-plane operations remain admin-controlled.

## Upgrade Runbook

1. Upload/deploy new implementation contract.
2. Deploy migration script contracts (if required).
3. Build migration steps (`Vec<MigrationStep>`).
4. `stage_upgrade(new_impl, target_version, steps)`.
5. Validate staged payload via `get_pending_upgrade()`.
6. Execute `execute_upgrade()`.
7. Validate:
   - `current_version()`
   - `current_implementation()`
   - `get_state_value(key)` / app-level invariants
8. If needed, execute `rollback()`.
9. If incident response is needed during runtime, call `pause()`.

## Testing

`src/test.rs` includes upgrade scenario tests for:

- Proxy forwarding behavior.
- Migration execution correctness.
- On-chain version tracking.
- Rollback behavior.
- Admin-only authorization.
- State preservation across upgrades.
- Emergency pause behavior.
