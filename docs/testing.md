# SDK Testing

ScreenMCP includes end-to-end tests for all three SDKs (Python, TypeScript, Rust) using a fake device that simulates a real phone/desktop client.

## Quick Start

Run all SDK tests with a single command:

```bash
cd fake-device
./run_tests.sh
```

This script is fully self-contained — it builds everything, starts the stack, runs all tests, and cleans up automatically. No manual setup required.

### What it does

1. Writes a temporary `~/.screenmcp/worker.toml` test config
2. Builds the worker (Rust) and MCP server (Node.js)
3. Builds the Rust SDK test binary
4. Installs Python packages (fake-device + Python SDK)
5. Starts worker on port 8199, MCP server on port 3199
6. Starts the fake device (registers + connects via SSE)
7. Runs Python SDK tests (28 tests)
8. Runs TypeScript SDK tests (30 tests)
9. Runs Rust SDK tests (22 tests)
10. Prints summary and exits with 0 (all pass) or 1 (any fail)
11. Stops all background processes on exit

### Prerequisites

- Rust toolchain (`cargo`)
- Node.js 18+ (`node`, `npm`, `npx`)
- Python 3.10+ (`python3`, `pip`)
- `curl` (for health checks)

### Ports

Tests use non-standard ports to avoid conflicts with a running instance:

| Service    | Port |
|------------|------|
| Worker     | 8199 |
| MCP Server | 3199 |

## Test Coverage

### Python SDK (28 tests)

Tests the `screenmcp` Python package via `ScreenMCPClient` + `DeviceConnection`:

- All commands: screenshot, click, long_click, drag, scroll, type, get_text, select_all, copy, paste, back, home, recents, ui_tree, camera, list_cameras, press_key, get_clipboard, set_clipboard
- Selector engine: `text:`, `role:`, `desc:`, `id:`, negation (`!text:X&&role:Y`)
- Fluent API: `find("text:Chrome").element()`
- `exists()` for present and absent elements
- Unknown command error handling (`CommandError`)

### TypeScript SDK (30 tests)

Tests the `@screenmcp/sdk` package — same coverage as Python plus TypeScript-specific selector tests.

### Rust SDK (22 tests)

Tests the `screenmcp` Rust crate — all commands. No selector engine (not idiomatic for Rust).

## Running Individual SDK Tests

If you already have the stack running (worker + MCP server + fake device), you can run individual test scripts:

### Start the stack manually

```bash
# Terminal 1 — worker
cd worker && cargo run

# Terminal 2 — MCP server
cd mcp-server && npm run start

# Terminal 3 — fake device
cd fake-device
pip install -e .
python -m fake_device \
    --api-url http://localhost:3000 \
    --device-id faketest001 \
    --user-id local-user
```

### Run individual tests

```bash
# Python
cd fake-device
pip install -e ../sdk/python
python test_with_sdk.py \
    --api-url http://localhost:3000 \
    --api-key pk_test123 \
    --device-id faketest001

# TypeScript
cd sdk/typescript/examples/cli
npx tsx test_fake_device.ts \
    --api-url http://localhost:3000 \
    --api-key pk_test123 \
    --device-id faketest001

# Rust
cd sdk/rust
cargo run --release --example test_fake_device -- \
    --api-url http://localhost:3000 \
    --api-key pk_test123 \
    --device-id faketest001
```

## Fake Device

The fake device (`fake-device/`) is a Python client that simulates a real phone:

- Registers with the MCP server
- Listens on SSE for connect events
- Connects to the worker via WebSocket as a "phone"
- Returns hardcoded responses for all commands (PNG screenshot, UI tree with realistic nodes, etc.)

### Error mode flags

```bash
python -m fake_device --screen-off       # screenshot returns "Screen is off" error
python -m fake_device --typing-fails     # type returns "No focused text element" error
```

To run tests against error modes:

```bash
python test_with_sdk.py --test-error-modes
```

## Test Script Structure

All three SDK test scripts follow the same pattern:

1. **Parse CLI args** (`--api-url`, `--api-key`, `--device-id`)
2. **Create `ScreenMCPClient`** and call `connect(device_id)` to get a `DeviceConnection`
3. **Wait for phone** (fake device) to connect via SSE → WebSocket
4. **Run each test** on the connection in a try/catch block, recording pass/fail
5. **Print summary** and exit with 0 or 1

### Test file locations

| SDK | Test file |
|-----|-----------|
| Python | `fake-device/test_with_sdk.py` |
| TypeScript | `sdk/typescript/examples/cli/test_fake_device.ts` |
| Rust | `sdk/rust/examples/test_fake_device.rs` |

### Python test structure (`test_with_sdk.py`)

The Python test has two test paths:
- **SDK path** (default): Creates `ScreenMCPClient`, calls `connect()` to get a `DeviceConnection`, then tests commands on it
- **Raw WebSocket path** (fallback): Uses `httpx` + `websockets` directly if the SDK isn't installed

A `TestResults` class tracks pass/fail/skip counts and prints a summary at the end. Each test is a try/except block:

```python
# ── Test: new_command ────────────────────────────────────────
try:
    result = await phone.new_command(param1, param2)
    # validate result
    results.ok("new_command(param1, param2)")
except Exception as e:
    results.fail("new_command", str(e))
```

### TypeScript test structure (`test_fake_device.ts`)

Same pattern with `pass()`/`fail()` helper functions:

```typescript
// new_command
try {
  const result = await phone.newCommand(param1, param2);
  // validate result
  pass("newCommand(param1, param2)");
} catch (e) {
  fail("newCommand", (e as Error).message);
}
```

### Rust test structure (`test_fake_device.rs`)

Uses a `TestResults` struct with `pass()`/`fail()` methods. Each test is a match expression:

```rust
// new_command
match phone.new_command(param1, param2).await {
    Ok(r) => results.pass("new_command(param1, param2)"),
    Err(e) => results.fail("new_command", &e.to_string()),
}
```

## Adding a Test for a New Command

When you add a new command to ScreenMCP (see [adding-new-command.md](adding-new-command.md)), you need to update **4 files** to get test coverage:

### 1. Fake device response (`fake-device/src/fake_device/commands.py`)

Add a handler in the `handle_command()` function. There are three categories:

**Simple commands** (return `{status: "ok"}` with no result data):
```python
# Add to the simple_commands set:
simple_commands = {
    "click", "long_click", ..., "new_command",
}
```

**Commands with result data** (return structured response):
```python
if cmd == "new_command":
    return {"status": "ok", "result": {"key": "fake value"}}
```

**Commands that check params**:
```python
if cmd == "new_command":
    p = params or {}
    if p.get("some_flag"):
        return {"status": "ok", "result": {"extra": "data"}}
    return {"status": "ok", "result": {}}
```

### 2. Python SDK test (`fake-device/test_with_sdk.py`)

Add a test block inside `test_with_python_sdk()`:

```python
# ── Test: new_command ────────────────────────────────────────
try:
    result = await phone.new_command(param1, param2)
    if result.get("expected_key"):
        results.ok("new_command(param1, param2)")
    else:
        results.fail("new_command", "Missing expected_key")
except Exception as e:
    results.fail("new_command", str(e))
```

If you also want the raw WebSocket fallback path tested, add it to the `commands_to_test` list in `test_with_raw_ws()`:

```python
commands_to_test = [
    ...,
    ("new_command", {"param1": "value"}),
]
```

### 3. TypeScript SDK test (`sdk/typescript/examples/cli/test_fake_device.ts`)

Add a test block inside `runTests()`:

```typescript
// newCommand
try {
  const result = await phone.newCommand(param1, param2);
  pass("newCommand(param1, param2)");
} catch (e) {
  fail("newCommand", (e as Error).message);
}
```

### 4. Rust SDK test (`sdk/rust/examples/test_fake_device.rs`)

Add a test block inside `main()`:

```rust
// new_command
match phone.new_command(param1, param2).await {
    Ok(r) => results.pass("new_command(param1, param2)"),
    Err(e) => results.fail("new_command", &e.to_string()),
}
```

### Verify

Run the full test suite to confirm:

```bash
cd fake-device && ./run_tests.sh
```

All three SDK test counts should increase by 1.

## Troubleshooting

**Tests timeout waiting for phone connection:**
The fake device needs a moment to register, receive the SSE connect event, and join the worker via WebSocket. The test scripts wait up to 15 seconds. If this isn't enough, check the fake device log at `/tmp/screenmcp-test-fakedev.log`.

**Port conflicts:**
`run_tests.sh` uses ports 8199/3199. If something is already using those ports, either stop it or edit the `WORKER_PORT`/`MCP_PORT` variables in the script.

**pip install fails:**
On system Python (e.g. Debian/Ubuntu), you may need `--break-system-packages` flag or use a virtualenv.

**Logs:**
During `run_tests.sh`, service logs are at:
- `/tmp/screenmcp-test-worker.log`
- `/tmp/screenmcp-test-mcp.log`
- `/tmp/screenmcp-test-fakedev.log`
