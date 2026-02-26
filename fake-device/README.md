# Fake Device

A Python fake device client for automated testing of the ScreenMCP system. It pretends to be a real device (like an Android phone) so you can test the full controller-to-worker-to-device pipeline without a physical device.

## Install

```bash
cd fake-device
pip install -e .
```

## Usage

```bash
# Basic open-source mode (default)
python -m fake_device \
  --api-url http://localhost:3000 \
  --device-id test-device-001 \
  --user-id local-user \
  --mode opensource

# With test modes
python -m fake_device \
  --api-url http://localhost:3000 \
  --device-id test-device-001 \
  --user-id local-user \
  --screen-off                   # screenshot returns error
  --typing-fails                 # type returns error
  --slow-response 2.0            # 2 second delay on every response
  --disconnect-after 5           # disconnect after 5 commands

# Verbose logging
python -m fake_device --verbose --device-id test-device-001
```

## Environment Variables

All CLI flags have environment variable equivalents:

| Variable | Default | Description |
|----------|---------|-------------|
| `FAKE_DEVICE_API_URL` | `http://localhost:3000` | MCP server API URL |
| `FAKE_DEVICE_ID` | `fake-device-001` | Device ID |
| `FAKE_DEVICE_NAME` | `Fake Test Device` | Human-readable device name |
| `FAKE_DEVICE_USER_ID` | `local-user` | User ID / auth token |
| `FAKE_DEVICE_MODE` | `opensource` | `opensource` or `cloud` |
| `FAKE_DEVICE_SLOW_RESPONSE` | `0` | Response delay in seconds |
| `FAKE_DEVICE_DISCONNECT_AFTER` | `0` | Disconnect after N commands |
| `FAKE_DEVICE_VERBOSE` | `` | Set to `1`/`true`/`yes` for debug logging |

## How It Works

1. **Registers** itself with the MCP server (`POST /api/devices/register`)
2. **Listens** on the SSE endpoint (`GET /api/events`) for `connect` events
3. When a controller triggers a discover, the MCP server sends a `connect` event
4. The fake device **connects via WebSocket** to the worker URL from the event
5. **Authenticates** as a phone (`role: "phone"`)
6. **Handles commands** with hardcoded responses (screenshot returns a PNG, ui_tree returns a realistic tree, etc.)
7. On shutdown, **unregisters** itself (`POST /api/devices/delete`)

## Test Modes

- `--screen-off`: `screenshot` returns `{status: "error", error: "Screen is off"}`
- `--typing-fails`: `type` returns `{status: "error", error: "No focused text element"}`
- `--slow-response N`: Adds N seconds of delay before every response
- `--disconnect-after N`: Disconnects after N commands (tests reconnection logic)
