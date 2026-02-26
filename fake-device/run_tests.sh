#!/usr/bin/env bash
# Self-contained E2E test runner for ScreenMCP SDKs.
# Starts worker + MCP server + fake device, runs SDK tests, tears down.
# Usage: ./run_tests.sh
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

# Config
API_KEY="pk_test123"
USER_ID="test-user-123"
DEVICE_ID="faketest001"
MCP_PORT=3199
WORKER_PORT=8199
PIDS=()

cleanup() {
    echo ""
    echo "=== Cleaning up ==="
    for pid in "${PIDS[@]}"; do
        kill "$pid" 2>/dev/null && echo "Stopped PID $pid" || true
    done
    wait 2>/dev/null || true
    rm -f /tmp/screenmcp-test-*.log
    echo "Done."
}
trap cleanup EXIT

# ── Setup config ──────────────────────────────────────────────────────
echo "=== Setting up test config ==="
mkdir -p ~/.screenmcp
cat > ~/.screenmcp/worker.toml <<EOF
[user]
id = "$USER_ID"

[auth]
api_keys = ["$API_KEY"]

[devices]
allowed = []

[server]
port = $MCP_PORT
worker_url = "ws://localhost:$WORKER_PORT"
EOF
echo "Config written to ~/.screenmcp/worker.toml"

# ── Build worker if needed ────────────────────────────────────────────
echo ""
echo "=== Building worker ==="
cd "$REPO_DIR/worker"
cargo build --release 2>&1 | tail -3

# ── Build MCP server if needed ────────────────────────────────────────
echo ""
echo "=== Building MCP server ==="
cd "$REPO_DIR/mcp-server"
npm install --silent 2>&1 | tail -1
npm run build 2>&1 | tail -1

# ── Build Rust SDK test ──────────────────────────────────────────────
echo ""
echo "=== Building Rust SDK test ==="
cd "$REPO_DIR/sdk/rust"
cargo build --release --example test_fake_device 2>&1 | tail -3

# ── Install Python deps ──────────────────────────────────────────────
echo ""
echo "=== Installing Python packages ==="
pip install --break-system-packages -q -e "$REPO_DIR/fake-device" -e "$REPO_DIR/sdk/python" 2>&1 | tail -2

# ── Start worker ──────────────────────────────────────────────────────
echo ""
echo "=== Starting worker on port $WORKER_PORT ==="
cd "$REPO_DIR/worker"
PORT=$WORKER_PORT RUST_LOG=warn ./target/release/screenmcp-worker > /tmp/screenmcp-test-worker.log 2>&1 &
PIDS+=($!)
echo "Worker PID: ${PIDS[-1]}"

# Wait for worker to be ready
for i in $(seq 1 30); do
    if curl -sf "http://localhost:$WORKER_PORT" >/dev/null 2>&1 || [ -s /tmp/screenmcp-test-worker.log ]; then
        break
    fi
    sleep 0.5
done
sleep 1
echo "Worker started"

# ── Start MCP server ─────────────────────────────────────────────────
echo ""
echo "=== Starting MCP server on port $MCP_PORT ==="
cd "$REPO_DIR/mcp-server"
PORT=$MCP_PORT WORKER_WS_URL="ws://localhost:$WORKER_PORT" node dist/server.js > /tmp/screenmcp-test-mcp.log 2>&1 &
PIDS+=($!)
echo "MCP server PID: ${PIDS[-1]}"

# Wait for MCP server
for i in $(seq 1 20); do
    if curl -sf "http://localhost:$MCP_PORT/api/auth/verify" -X POST -H "Content-Type: application/json" -d "{\"token\":\"$USER_ID\"}" >/dev/null 2>&1; then
        break
    fi
    sleep 0.5
done
echo "MCP server started"

# ── Start fake device ────────────────────────────────────────────────
echo ""
echo "=== Starting fake device ==="
python3 -m fake_device \
    --api-url "http://localhost:$MCP_PORT" \
    --user-id "$USER_ID" \
    --device-id "$DEVICE_ID" \
    > /tmp/screenmcp-test-fakedev.log 2>&1 &
PIDS+=($!)
echo "Fake device PID: ${PIDS[-1]}"

# Wait for fake device to register and connect to SSE
for i in $(seq 1 20); do
    if grep -q "SSE connected" /tmp/screenmcp-test-fakedev.log 2>/dev/null; then
        break
    fi
    sleep 0.5
done
echo "Fake device connected to SSE"

# ── Run Python SDK tests ─────────────────────────────────────────────
echo ""
echo "========================================"
echo "=== Running Python SDK tests ==="
echo "========================================"
cd "$SCRIPT_DIR"
PYTHON_EXIT=0
python3 test_with_sdk.py \
    --api-url "http://localhost:$MCP_PORT" \
    --api-key "$API_KEY" \
    --device-id "$DEVICE_ID" \
    || PYTHON_EXIT=$?

# ── Run TypeScript SDK tests ─────────────────────────────────────────
echo ""
echo "========================================"
echo "=== Running TypeScript SDK tests ==="
echo "========================================"
cd "$REPO_DIR/sdk/typescript"
npx tsc 2>&1 | tail -1 || true
cd "$REPO_DIR/sdk/typescript/examples/cli"
TS_EXIT=0
npx tsx test_fake_device.ts \
    --api-url "http://localhost:$MCP_PORT" \
    --api-key "$API_KEY" \
    --device-id "$DEVICE_ID" \
    || TS_EXIT=$?

# ── Run Rust SDK tests ────────────────────────────────────────────────
echo ""
echo "========================================"
echo "=== Running Rust SDK tests ==="
echo "========================================"
RUST_EXIT=0
"$REPO_DIR/sdk/rust/target/release/examples/test_fake_device" \
    --api-url "http://localhost:$MCP_PORT" \
    --api-key "$API_KEY" \
    --device-id "$DEVICE_ID" \
    || RUST_EXIT=$?

# ── Summary ──────────────────────────────────────────────────────────
echo ""
echo "========================================"
echo "=== Final Results ==="
echo "========================================"
if [ "$PYTHON_EXIT" -eq 0 ] && [ "$TS_EXIT" -eq 0 ] && [ "$RUST_EXIT" -eq 0 ]; then
    echo "ALL TESTS PASSED"
    EXIT_CODE=0
else
    [ "$PYTHON_EXIT" -ne 0 ] && echo "Python SDK tests FAILED (exit $PYTHON_EXIT)"
    [ "$TS_EXIT" -ne 0 ] && echo "TypeScript SDK tests FAILED (exit $TS_EXIT)"
    [ "$RUST_EXIT" -ne 0 ] && echo "Rust SDK tests FAILED (exit $RUST_EXIT)"
    EXIT_CODE=1
fi

# cleanup happens via trap
exit $EXIT_CODE
