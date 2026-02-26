# ScreenMCP Python SDK

A Python library for controlling phones and desktops programmatically through
ScreenMCP. Fully async, built on `websockets` and `httpx`.

## Installation

```bash
pip install screenmcp
```

Or install from source:

```bash
cd sdk/python
pip install -e .
```

## Quick Start

```python
import asyncio
from screenmcp import ScreenMCPClient

async def main():
    # 1. Create API client with your API key
    client = ScreenMCPClient(api_key="pk_your_key_here")

    # 2. List available devices
    devices = await client.list_devices()
    print(f"Found {len(devices)} devices")

    # 3. Connect to a device — returns a DeviceConnection
    phone = await client.connect(device_id="a1b2c3d4e5f67890abcdef1234567890")
    print(f"Connected to worker: {phone.worker_url}")
    print(f"Phone online: {phone.phone_connected}")

    # 4. Send commands on the connection
    result = await phone.screenshot()
    print(f"Got image: {len(result['image'])} bytes base64")

    await phone.click(540, 960)
    await phone.type_text("Hello from Python!")
    await phone.scroll("down", amount=800)

    tree = await phone.ui_tree()
    print(tree)

    # 5. Disconnect
    await phone.disconnect()

asyncio.run(main())
```

The connection flow is:
1. **Create client** with API key (no device ID yet)
2. **`list_devices()`** to see available devices
3. **`connect(device_id=...)`** discovers a worker, opens a WebSocket, and returns a `DeviceConnection`
4. **Send commands** on the connection — each command goes through the WebSocket to the device and back
5. **`disconnect()`** closes the WebSocket

You can also use `async with` for automatic disconnect:

```python
async with await client.connect(device_id="...") as phone:
    await phone.screenshot()
```

## Configuration

```python
client = ScreenMCPClient(
    api_key="pk_...",                                # required
    api_url="https://api.screenmcp.com",             # cloud mode (default)
    command_timeout=30.0,                            # seconds (default 30)
    auto_reconnect=True,                             # default True
)
```

### Open Source Mode

For self-hosted ScreenMCP (no cloud account needed):

```python
client = ScreenMCPClient(
    api_key="pk_your_local_key",
    api_url="http://localhost:3000",     # your MCP server URL
)
```

## Available Commands

All command methods are on the `DeviceConnection` object returned by `client.connect()`.

### Screen & UI

| Method | Description |
|---|---|
| `screenshot()` | Capture the screen (returns base64 PNG) |
| `ui_tree()` | Get the accessibility tree |
| `click(x, y)` | Tap at coordinates |
| `long_click(x, y)` | Long-press at coordinates |
| `drag(start_x, start_y, end_x, end_y)` | Drag gesture |
| `scroll(direction, amount)` | Scroll up/down/left/right |

### Text Input

| Method | Description |
|---|---|
| `type_text(text)` | Type into focused input |
| `get_text()` | Read text from focused element |
| `select_all()` | Select all text |
| `copy(return_text=False)` | Copy selection (optionally return copied text) |
| `paste(text=None)` | Paste from clipboard (or paste given text) |
| `get_clipboard()` | Read clipboard contents |
| `set_clipboard(text)` | Set clipboard contents |

### Navigation

| Method | Description |
|---|---|
| `back()` | Press Back |
| `home()` | Press Home |
| `recents()` | Open app switcher |

### Keyboard (Desktop)

| Method | Description |
|---|---|
| `press_key(key)` | Press and release a key (e.g. "Enter", "Tab") |
| `hold_key(key)` | Hold a key down (e.g. "Shift") |
| `release_key(key)` | Release a held key |

### Camera

| Method | Description |
|---|---|
| `list_cameras()` | List available cameras |
| `camera(camera_id)` | Take a photo with a specific camera |

All methods are async and return a `dict` with the command result.

## Selector Engine

Find UI elements by text, role, description, or resource ID:

```python
from screenmcp import find_elements

# Get the UI tree first
result = await phone.ui_tree()
tree = result["tree"]

# Find by text
elements = find_elements(tree, "text:Settings")

# Find by role/class
elements = find_elements(tree, "role:EditText")

# Find by content description
elements = find_elements(tree, "desc:Home button")

# Find by resource ID
elements = find_elements(tree, "id:com.android.chrome:id/search_box")

# Combine with AND
elements = find_elements(tree, "role:TextView&&text:Settings")

# Negate
elements = find_elements(tree, "!text:Settings&&role:TextView")
```

Each returned `FoundElement` has: `x`, `y` (center coordinates), `text`, `class_name`, `content_description`, `resource_id`, `bounds`.

### Fluent API

```python
# Find an element and get its info
element = await phone.find("text:Settings", timeout=2.0).element()
print(f"Settings is at ({element.x}, {element.y})")

# Find and click in one step
await phone.find("text:Settings").click()

# Check if an element exists
if await phone.exists("text:Settings", timeout=1.0):
    print("Settings button is visible")

# Wait for an element to appear
await phone.wait_for("text:Loading complete", timeout=10.0)

# Wait for an element to disappear
await phone.wait_for_gone("text:Loading...", timeout=10.0)
```

## Generic Commands

Use `send_command()` for any command, including future ones:

```python
resp = await phone.send_command("screenshot", {"quality": 50})
print(resp.result)
```

## Error Handling

```python
from screenmcp import ScreenMCPClient, AuthError, CommandError, ConnectionError

client = ScreenMCPClient(api_key="pk_...")
try:
    async with await client.connect(device_id="...") as phone:
        await phone.click(100, 200)
except AuthError:
    print("Invalid API key")
except ConnectionError:
    print("Could not connect to worker")
except CommandError as e:
    print(f"Command failed: {e}")
```

## Manual Lifecycle

If you prefer not to use the context manager:

```python
client = ScreenMCPClient(api_key="pk_...")
phone = await client.connect(device_id="...")
try:
    await phone.screenshot()
finally:
    await phone.disconnect()
```

## Requirements

- Python 3.10+
- `websockets >= 12.0`
- `httpx >= 0.25.0`
