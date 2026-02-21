# ScreenMCP Python SDK

A Python library for controlling Android phones programmatically through the
ScreenMCP platform.  Fully async, built on `websockets` and `httpx`.

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
    async with ScreenMCPClient(api_key="pk_your_key_here") as phone:
        # Take a screenshot
        result = await phone.screenshot()
        print(f"Got image: {len(result['image'])} bytes base64")

        # Tap on the screen
        await phone.click(540, 960)

        # Type some text
        await phone.type_text("Hello from Python!")

        # Scroll down
        await phone.scroll("down", amount=800)

        # Get the UI tree for inspection
        tree = await phone.ui_tree()
        print(tree)

asyncio.run(main())
```

## Configuration

```python
client = ScreenMCPClient(
    api_key="pk_...",                                # required
    api_url="https://server10.doodkin.com",          # default
    device_id="your-device-uuid",                    # optional; auto-selects if omitted
    command_timeout=30.0,                            # seconds (default 30)
    auto_reconnect=True,                             # default True
)
```

## Available Commands

| Method | Description |
|---|---|
| `screenshot()` | Capture the screen (returns base64 JPEG) |
| `click(x, y)` | Tap at coordinates |
| `long_click(x, y)` | Long-press at coordinates |
| `drag(start_x, start_y, end_x, end_y)` | Drag gesture |
| `scroll(direction, amount)` | Scroll up/down/left/right |
| `type_text(text)` | Type into focused input |
| `get_text()` | Read text from focused element |
| `select_all()` | Select all text |
| `copy()` | Copy selection |
| `paste()` | Paste from clipboard |
| `back()` | Press Back |
| `home()` | Press Home |
| `recents()` | Open app switcher |
| `ui_tree()` | Get accessibility tree |
| `camera(facing)` | Take a photo ("rear" or "front") |

All methods are async and return a `dict` with the command result.

## Generic Commands

Use `send_command()` for any command, including future ones:

```python
resp = await phone.send_command("screenshot", {"quality": 50})
print(resp.result)
```

## Error Handling

```python
from screenmcp import ScreenMCPClient, AuthError, CommandError, ConnectionError

try:
    async with ScreenMCPClient(api_key="pk_...") as phone:
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
phone = ScreenMCPClient(api_key="pk_...")
await phone.connect()
try:
    await phone.screenshot()
finally:
    await phone.disconnect()
```

## Requirements

- Python 3.10+
- `websockets >= 12.0`
- `httpx >= 0.25.0`
