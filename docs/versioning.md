# ScreenMCP Versioning Protocol

How version negotiation and compatibility checking works across all ScreenMCP components.

## Version Format

Each component uses **semver major.minor** versioning:

```json
{
  "major": 1,
  "minor": 0,
  "component": "android"
}
```

- **major** -- Breaking changes. Incremented when wire protocol, command format, or auth flow changes in a backwards-incompatible way. The worker enforces major version compatibility.
- **minor** -- Non-breaking additions. New commands, optional fields, UI changes. The worker does not reject based on minor version.
- **component** -- Identifies the client type. One of: `android`, `windows`, `linux`, `mac`, `remote`, `sdk-ts`, `sdk-py`, `sdk-rust`, `worker`.

## How Version Info Is Sent

Clients include a `version` field in their WebSocket auth message:

```json
{
  "type": "auth",
  "token": "...",
  "role": "phone",
  "device_id": "...",
  "version": {
    "major": 1,
    "minor": 0,
    "component": "android"
  }
}
```

The `version` field is **optional** for backwards compatibility. Clients that do not send it are allowed to connect (they are assumed to be compatible).

## Compatibility Matrix

The worker maintains a compatibility table in `worker/src/protocol.rs`:

```rust
pub const COMPATIBILITY: &[(&str, u32, u32)] = &[
    ("android", 1, 1),   // min_major=1, max_major=1
    ("windows", 1, 1),
    ("linux", 1, 1),
    ("mac", 1, 1),
    ("remote", 1, 1),
    ("sdk-ts", 1, 1),
    ("sdk-py", 1, 1),
    ("sdk-rust", 1, 1),
    ("worker", 1, 1),
];
```

Each entry is `(component_name, min_major_inclusive, max_major_inclusive)`. A client's major version must fall within `[min_major, max_major]` to be accepted.

Unknown component names are allowed by default (forward compatible).

## Version Checks

### On connect (auth phase)

When a client sends an auth message with version info, the worker checks:

1. Is the client's major version within the allowed range for its component?
2. If not, the worker sends a version error and closes the connection.

### On controller connect (cross-check)

When a controller connects and targets a specific phone:

1. The worker checks if the target phone's stored version is compatible.
2. If the phone is outdated, the controller receives an `outdated_remote` error.
3. If both controller and phone are outdated, the controller receives a `both_outdated` error.

### Version storage

The worker stores version info per `device_id` in the in-memory connection registry. Version info is removed when a device disconnects.

## Error Messages

Version errors are sent as structured JSON before closing the connection:

```json
{
  "type": "error",
  "code": "outdated_client",
  "message": "Your Android app (v0.9) is outdated. Please update to version 1.x or later.",
  "update_url": "https://screenmcp.com/download"
}
```

### Error Codes

| Code | Meaning |
|------|---------|
| `outdated_client` | The connecting client's major version is below the minimum. Update the client. |
| `outdated_remote` | The target device (phone/desktop) that the controller wants to talk to is outdated. The device owner needs to update. |
| `both_outdated` | Both the controller and the target device are outdated. Both need updates. |

## How to Bump Versions

### Adding a new command (minor bump)

No compatibility change needed. Just increment the minor version in the client that supports the new command. The worker does not enforce minor versions.

### Breaking protocol change (major bump)

1. Increment the major version in all affected clients.
2. Update the `COMPATIBILITY` table in `worker/src/protocol.rs`:
   - Set `min_major` to the new version if old clients cannot work at all.
   - Set `max_major` to the new version.
   - To support a transition period, keep `min_major` at the old value temporarily.
3. Deploy the updated worker first, then roll out client updates.

### Example: bumping Android from v1 to v2

```rust
// During transition (accept both v1 and v2):
("android", 1, 2),

// After all users have updated:
("android", 2, 2),
```

### Adding a new component

Add a new entry to the `COMPATIBILITY` table. Unknown components are allowed by default, so existing workers will accept new components even before the table is updated.

## Current Versions

| Component | Major | Minor |
|-----------|-------|-------|
| android   | 1     | 0     |
| windows   | 1     | 0     |
| linux     | 1     | 0     |
| mac       | 1     | 0     |
| remote    | 1     | 0     |
| sdk-ts    | 1     | 0     |
| sdk-py    | 1     | 0     |
| sdk-rust  | 1     | 0     |
| worker    | 1     | 0     |
