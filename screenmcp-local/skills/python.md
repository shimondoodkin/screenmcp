# Controlling ScreenMCP from Python

Use the REST API at `POST http://127.0.0.1:6767/command` to control the desktop from Python scripts.

## Setup

```python
import requests
import base64
import json
from io import BytesIO

SCREENMCP_URL = "http://127.0.0.1:6767"
SCREENMCP_KEY = "your-api-key-here"

HEADERS = {
    "Authorization": f"Bearer {SCREENMCP_KEY}",
    "Content-Type": "application/json",
}

def cmd(name: str, params: dict = None) -> dict:
    """Send a command to ScreenMCP and return the result."""
    body = {"cmd": name}
    if params:
        body["params"] = params
    resp = requests.post(f"{SCREENMCP_URL}/command", json=body, headers=HEADERS)
    resp.raise_for_status()
    data = resp.json()
    if data.get("status") == "error":
        raise RuntimeError(data.get("error", "unknown error"))
    return data.get("result", {})
```

## Coordinate Scaling

All coordinates auto-scale between screenshot space (default 1456x819) and actual screen. You don't need to know the screen resolution — just use pixel coordinates from the screenshot.

```python
# Screenshot and click use the same coordinate space automatically
result = cmd("screenshot")  # returns 1456x819 image by default
# If you see a button at pixel (400, 300) in the image, just click there:
cmd("click", {"x": 400, "y": 300})  # auto-scaled to actual screen
```

To use a different resolution, pass `max_width`/`max_height` to both screenshot and click:
```python
result = cmd("screenshot", {"max_width": 1092, "max_height": 1092})
cmd("click", {"x": 200, "y": 300, "max_width": 1092, "max_height": 1092})
```

## Important: Always Focus First

**Call `focus_window` before clicking on an app.** Otherwise clicks land on whatever window is in front.

```python
cmd("focus_window", {"title": "Paint"})  # bring Paint to front
cmd("click", {"x": 400, "y": 300})       # now clicks Paint's canvas
```

## Taking Screenshots

```python
# Take a screenshot and save to file
result = cmd("screenshot", {"max_width": 1920, "max_height": 1080})
image_bytes = base64.b64decode(result["image"])
with open("screenshot.webp", "wb") as f:
    f.write(image_bytes)

# Convert to PIL Image for processing
from PIL import Image
img = Image.open(BytesIO(image_bytes))
print(f"Screenshot: {img.width}x{img.height}")
```

## Mouse and Keyboard

```python
# Click
cmd("click", {"x": 500, "y": 300})

# Right-click
cmd("right_click", {"x": 500, "y": 300})

# Double-click
cmd("double_click", {"x": 500, "y": 300})

# Type text
cmd("type", {"text": "Hello, world!"})

# Press a key
cmd("press_key", {"key": "enter"})

# Keyboard shortcut
cmd("hotkey", {"keys": ["ctrl", "c"]})

# Scroll down
cmd("scroll", {"x": 500, "y": 500, "dy": -3})

# Drag
cmd("drag", {"startX": 100, "startY": 200, "endX": 400, "endY": 200})
```

## Reading the Screen

```python
# Get accessibility tree
result = cmd("ui_tree")
for node in result.get("tree", []):
    print(f"  {node.get('ct', '')} - {node.get('name', '')} @ {node.get('bounds', '')}")

# Get screen size
result = cmd("get_screen_size")
print(f"Screen: {result['width']}x{result['height']}")

# List windows
result = cmd("list_windows")
for w in result.get("windows", []):
    print(f"  [{w['index']}] {w['title']} ({w['width']}x{w['height']})")
```

## Clipboard

```python
# Copy and read
cmd("select_all")
result = cmd("copy", {"return_text": True})
print(f"Text: {result['text']}")

# Set clipboard and paste
cmd("set_clipboard", {"text": "Pasted from Python"})
cmd("paste")
```

## Window Management

```python
# Focus a window by title
cmd("focus_window", {"title": "Notepad"})

# Focus by index
result = cmd("list_windows")
cmd("focus_window", {"index": 0})  # Focus the first window
```

## Automation Example

```python
import time

def open_app(name: str):
    """Open an app via Start menu search."""
    cmd("hotkey", {"keys": ["win"]})
    time.sleep(0.5)
    cmd("type", {"text": name})
    time.sleep(1)
    cmd("press_key", {"key": "enter"})
    time.sleep(2)

def save_file(filename: str):
    """Save current document with a specific name."""
    cmd("hotkey", {"keys": ["ctrl", "s"]})
    time.sleep(0.5)
    cmd("select_all")
    cmd("type", {"text": filename})
    cmd("press_key", {"key": "enter"})

# Example: Open Notepad, type something, save
open_app("notepad")
cmd("type", {"text": "Hello from ScreenMCP Python!"})
save_file("test.txt")
```
