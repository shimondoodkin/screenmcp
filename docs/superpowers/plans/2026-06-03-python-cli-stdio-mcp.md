# Python CLI Standalone stdio MCP — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a single-file Python stdio MCP server (`screenmcp/python-cli/screenmcp_cli.py`) that executes desktop commands directly on the local machine across Windows/macOS/Linux, with behavioral parity to the existing ScreenMCP clients (minus `ui_tree`).

**Architecture:** One self-contained Python file organized into commented banner sections. A pure JSON-RPC-over-stdio loop reads newline-delimited messages, dispatches `tools/call` to handlers via a `TOOLS` registry. Pure logic (scaling, registry, JSON-RPC framing) is unit-tested directly; side-effecting handlers (mouse/keyboard/screenshot/window) are tested by mocking `pynput`/`mss`/`pygetwindow`. Optional deps (`cv2`, `simpleaudio`) are lazy-imported inside their handlers.

**Tech Stack:** Python 3.9+, `mss`, `pynput`, `Pillow`, `pyperclip`, `pygetwindow` (Windows), Quartz/`osascript` (macOS), `wmctrl`/`xdotool` (Linux). Optional: `opencv-python`, `simpleaudio`. Tests: `pytest`.

---

## File Structure

- Create: `screenmcp/python-cli/screenmcp_cli.py` — the entire server (all sections).
- Create: `screenmcp/python-cli/pyproject.toml` — deps, console script, extras.
- Create: `screenmcp/python-cli/README.md` — install/run/config + macOS permissions.
- Create: `screenmcp/python-cli/tests/test_scaling.py` — pure scaling math.
- Create: `screenmcp/python-cli/tests/test_jsonrpc.py` — protocol framing/dispatch.
- Create: `screenmcp/python-cli/tests/test_registry.py` — registry completeness.
- Create: `screenmcp/python-cli/tests/test_handlers.py` — handlers via mocks.
- Create: `screenmcp/python-cli/tests/test_stdio.py` — end-to-end subprocess.
- Modify: `screenmcp/docs/adding-new-command.md` — add python-cli section + checklist item.

**Note on TDD for side-effecting code:** `pynput`, `mss`, and `pygetwindow` are injected through module-level singletons (`_mouse`, `_keyboard`, `_grabber`, `_win_backend`) created lazily by accessor functions. Tests monkeypatch those accessors so handlers can be exercised without a real display.

All paths below are relative to the repo root `screenmcp/`. Run all commands from `screenmcp/python-cli/`.

---

### Task 1: Project scaffold + JSON-RPC stdio core

**Files:**
- Create: `screenmcp/python-cli/screenmcp_cli.py`
- Create: `screenmcp/python-cli/pyproject.toml`
- Create: `screenmcp/python-cli/tests/test_jsonrpc.py`

- [ ] **Step 1: Write the failing test**

`tests/test_jsonrpc.py`:

```python
import io
import json
import screenmcp_cli as app


def run_lines(lines):
    """Feed JSON-RPC request dicts, return list of response dicts."""
    stdin = io.StringIO("".join(json.dumps(l) + "\n" for l in lines))
    stdout = io.StringIO()
    app.serve(stdin, stdout)
    out = []
    for raw in stdout.getvalue().splitlines():
        if raw.strip():
            out.append(json.loads(raw))
    return out


def test_initialize_advertises_tools():
    resp = run_lines([{"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}}])
    assert resp[0]["id"] == 1
    assert resp[0]["result"]["capabilities"]["tools"] == {}
    assert resp[0]["result"]["serverInfo"]["name"] == "screenmcp-cli"


def test_notifications_initialized_produces_no_response():
    resp = run_lines([{"jsonrpc": "2.0", "method": "notifications/initialized"}])
    assert resp == []


def test_unknown_method_returns_method_not_found():
    resp = run_lines([{"jsonrpc": "2.0", "id": 7, "method": "bogus"}])
    assert resp[0]["error"]["code"] == -32601


def test_ping_returns_empty_result():
    resp = run_lines([{"jsonrpc": "2.0", "id": 3, "method": "ping"}])
    assert resp[0]["result"] == {}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd screenmcp/python-cli && python -m pytest tests/test_jsonrpc.py -v`
Expected: FAIL — `AttributeError: module 'screenmcp_cli' has no attribute 'serve'`

- [ ] **Step 3: Write minimal implementation**

`screenmcp_cli.py` (start the file with these sections):

```python
#!/usr/bin/env python3
# === IMPORTS & CONSTANTS ===
import sys
import json

VERSION = "0.1.0"
SERVER_NAME = "screenmcp-cli"
DEFAULT_W = 1456
DEFAULT_H = 819

# Populated in the REGISTRY section near the bottom of the file.
TOOLS = {}


# === JSON-RPC STDIO SERVER ===
def _result(rpc_id, result):
    return {"jsonrpc": "2.0", "id": rpc_id, "result": result}


def _error(rpc_id, code, message):
    return {"jsonrpc": "2.0", "id": rpc_id, "error": {"code": code, "message": message}}


def _handle(msg):
    """Return a response dict, or None for notifications/no-reply."""
    method = msg.get("method")
    rpc_id = msg.get("id")
    if method == "initialize":
        return _result(rpc_id, {
            "protocolVersion": "2024-11-05",
            "capabilities": {"tools": {}},
            "serverInfo": {"name": SERVER_NAME, "version": VERSION},
        })
    if method == "notifications/initialized":
        return None
    if method == "ping":
        return _result(rpc_id, {})
    if method == "tools/list":
        return _result(rpc_id, {"tools": _list_tools()})
    if method == "tools/call":
        return _call_tool(rpc_id, msg.get("params") or {})
    if rpc_id is None:
        return None
    return _error(rpc_id, -32601, f"Method not found: {method}")


def _list_tools():
    return [
        {"name": n, "description": t["description"], "inputSchema": t["inputSchema"]}
        for n, t in TOOLS.items()
    ]


def _call_tool(rpc_id, params):
    name = params.get("name")
    args = params.get("arguments") or {}
    tool = TOOLS.get(name)
    if tool is None:
        return _error(rpc_id, -32602, f"Unknown tool: {name}")
    try:
        return _result(rpc_id, tool["handler"](args))
    except Exception as exc:  # surfaced as an MCP tool error, not a transport error
        return _result(rpc_id, {
            "content": [{"type": "text", "text": f"{type(exc).__name__}: {exc}"}],
            "isError": True,
        })


def serve(stdin, stdout):
    for raw in stdin:
        raw = raw.strip()
        if not raw:
            continue
        try:
            msg = json.loads(raw)
        except json.JSONDecodeError:
            stdout.write(json.dumps(_error(None, -32700, "Parse error")) + "\n")
            stdout.flush()
            continue
        resp = _handle(msg)
        if resp is not None:
            stdout.write(json.dumps(resp) + "\n")
            stdout.flush()


# === MAIN ===
def run():
    serve(sys.stdin, sys.stdout)


if __name__ == "__main__":
    run()
```

`pyproject.toml`:

```toml
[project]
name = "screenmcp-cli"
version = "0.1.0"
description = "Standalone local stdio MCP server for desktop control (ScreenMCP)."
requires-python = ">=3.9"
dependencies = ["mss", "pynput", "Pillow", "pyperclip", "pygetwindow; sys_platform == 'win32'"]

[project.optional-dependencies]
camera = ["opencv-python"]
audio = ["simpleaudio"]
dev = ["pytest"]

[project.scripts]
screenmcp-cli = "screenmcp_cli:run"

[tool.pytest.ini_options]
pythonpath = ["."]
```

- [ ] **Step 4: Run test to verify it passes**

Run: `python -m pytest tests/test_jsonrpc.py -v`
Expected: PASS (4 passed)

- [ ] **Step 5: Commit**

```bash
git add screenmcp/python-cli/screenmcp_cli.py screenmcp/python-cli/pyproject.toml screenmcp/python-cli/tests/test_jsonrpc.py
git commit -m "feat(python-cli): JSON-RPC stdio core + project scaffold"
```

---

### Task 2: Coordinate scaling

**Files:**
- Modify: `screenmcp/python-cli/screenmcp_cli.py` (add `# === COORDINATE SCALING ===` section after constants)
- Create: `screenmcp/python-cli/tests/test_scaling.py`

- [ ] **Step 1: Write the failing test**

`tests/test_scaling.py`:

```python
import screenmcp_cli as app


def test_scaled_space_defaults():
    assert app.scaled_space(None, None) == (1456, 819)


def test_scaled_space_override():
    assert app.scaled_space(728, 0) == (728, 0)


def test_to_native_scales_up():
    # screenshot space 1456x819, native 2912x1638 -> 2x
    nx, ny = app.to_native(100, 200, native=(2912, 1638), maxw=None, maxh=None)
    assert (nx, ny) == (200, 400)


def test_to_native_zero_dim_means_native_passthrough():
    nx, ny = app.to_native(100, 200, native=(2912, 1638), maxw=0, maxh=0)
    assert (nx, ny) == (100, 200)


def test_to_native_independent_axes():
    nx, ny = app.to_native(728, 819, native=(1456, 1638), maxw=None, maxh=None)
    assert (nx, ny) == (728, 1638)
```

- [ ] **Step 2: Run test to verify it fails**

Run: `python -m pytest tests/test_scaling.py -v`
Expected: FAIL — `AttributeError: ... has no attribute 'scaled_space'`

- [ ] **Step 3: Write minimal implementation**

Add after `# === IMPORTS & CONSTANTS ===` in `screenmcp_cli.py`:

```python
# === COORDINATE SCALING ===
def scaled_space(maxw, maxh):
    """Resolve the screenshot-space dimensions. None -> default; 0 kept as 0 (native)."""
    w = DEFAULT_W if maxw is None else maxw
    h = DEFAULT_H if maxh is None else maxh
    return (w, h)


def _axis(coord, native_dim, scaled_dim):
    if scaled_dim == 0:           # 0 => native passthrough, no scaling
        return int(round(coord))
    return int(round(coord * native_dim / scaled_dim))


def to_native(x, y, native, maxw=None, maxh=None):
    """Map a coordinate from screenshot space to native screen pixels."""
    sw, sh = scaled_space(maxw, maxh)
    return (_axis(x, native[0], sw), _axis(y, native[1], sh))
```

- [ ] **Step 4: Run test to verify it passes**

Run: `python -m pytest tests/test_scaling.py -v`
Expected: PASS (5 passed)

- [ ] **Step 5: Commit**

```bash
git add screenmcp/python-cli/screenmcp_cli.py screenmcp/python-cli/tests/test_scaling.py
git commit -m "feat(python-cli): coordinate scaling helpers"
```

---

### Task 3: Platform detection + lazy backend accessors

**Files:**
- Modify: `screenmcp/python-cli/screenmcp_cli.py` (add `# === PLATFORM DETECTION ===` + accessor stubs)
- Create: `screenmcp/python-cli/tests/test_handlers.py`

- [ ] **Step 1: Write the failing test**

`tests/test_handlers.py`:

```python
import screenmcp_cli as app


def test_primary_modifier_is_a_string():
    assert app.PRIMARY_MOD in ("ctrl", "cmd")


def test_nav_keymap_has_expected_actions():
    km = app.nav_keymap()
    assert set(km) == {"back", "home", "recents"}
    # each maps to a non-empty list of key names
    for combo in km.values():
        assert isinstance(combo, list) and combo
```

- [ ] **Step 2: Run test to verify it fails**

Run: `python -m pytest tests/test_handlers.py -v`
Expected: FAIL — `AttributeError: ... has no attribute 'PRIMARY_MOD'`

- [ ] **Step 3: Write minimal implementation**

Add `# === PLATFORM DETECTION ===` after the scaling section:

```python
# === PLATFORM DETECTION ===
IS_WIN = sys.platform.startswith("win")
IS_MAC = sys.platform == "darwin"
IS_LINUX = sys.platform.startswith("linux")

PRIMARY_MOD = "cmd" if IS_MAC else "ctrl"


def nav_keymap():
    if IS_MAC:
        return {"back": ["cmd", "["], "home": ["f3"], "recents": ["cmd", "tab"]}
    if IS_WIN:
        return {"back": ["alt", "left"], "home": ["cmd"], "recents": ["alt", "tab"]}
    return {"back": ["alt", "left"], "home": ["cmd"], "recents": ["alt", "tab"]}


# Lazy backend singletons (overridable in tests).
_mouse = None
_keyboard = None
_grabber = None


def mouse():
    global _mouse
    if _mouse is None:
        from pynput.mouse import Controller
        _mouse = Controller()
    return _mouse


def keyboard():
    global _keyboard
    if _keyboard is None:
        from pynput.keyboard import Controller
        _keyboard = Controller()
    return _keyboard


def grabber():
    global _grabber
    if _grabber is None:
        import mss
        _grabber = mss.mss()
    return _grabber
```

(Note: `"cmd"` is pynput's name for the Windows/Super key too; the hotkey key-map in Task 5 resolves it per-platform.)

- [ ] **Step 4: Run test to verify it passes**

Run: `python -m pytest tests/test_handlers.py -v`
Expected: PASS (2 passed)

- [ ] **Step 5: Commit**

```bash
git add screenmcp/python-cli/screenmcp_cli.py screenmcp/python-cli/tests/test_handlers.py
git commit -m "feat(python-cli): platform detection + lazy backend accessors"
```

---

### Task 4: Vision commands (screenshot, region, window, active_window, get_screen_size)

**Files:**
- Modify: `screenmcp/python-cli/screenmcp_cli.py` (add `# === VISION ===` section)
- Modify: `screenmcp/python-cli/tests/test_handlers.py`

- [ ] **Step 1: Write the failing test**

Append to `tests/test_handlers.py`:

```python
class _FakeShot:
    def __init__(self, w, h):
        self.size = (w, h)
        self.rgb = b"\x00\x00\x00" * (w * h)


class _FakeGrabber:
    monitors = [{"left": 0, "top": 0, "width": 2912, "height": 1638},
                {"left": 0, "top": 0, "width": 2912, "height": 1638}]

    def grab(self, region):
        return _FakeShot(region["width"], region["height"])


def test_get_screen_size_reports_scaled_and_native(monkeypatch):
    monkeypatch.setattr(app, "grabber", lambda: _FakeGrabber())
    out = app.cmd_get_screen_size({})
    text = out["content"][0]["text"]
    assert '"width": 1456' in text and '"height": 819' in text
    assert '"original_width": 2912' in text


def test_screenshot_returns_webp_image_block(monkeypatch):
    monkeypatch.setattr(app, "grabber", lambda: _FakeGrabber())
    out = app.cmd_screenshot({})
    block = out["content"][0]
    assert block["type"] == "image"
    assert block["mimeType"] == "image/webp"
    assert len(block["data"]) > 0
```

- [ ] **Step 2: Run test to verify it fails**

Run: `python -m pytest tests/test_handlers.py -k screen -v`
Expected: FAIL — `AttributeError: ... has no attribute 'cmd_get_screen_size'`

- [ ] **Step 3: Write minimal implementation**

Add `# === VISION ===` section. Helpers + five handlers:

```python
# === VISION ===
import base64 as _b64
from io import BytesIO


def _primary_native():
    """Native (width, height) of the primary monitor."""
    mon = grabber().monitors[1]
    return (mon["width"], mon["height"])


def _encode_webp(shot, target=None):
    """shot: mss screenshot. target: optional (w,h) to resize the encoded image to."""
    from PIL import Image
    img = Image.frombytes("RGB", shot.size, shot.rgb)
    if target and target[0] and target[1]:
        img = img.resize(target)
    buf = BytesIO()
    img.save(buf, format="WEBP")
    return _b64.b64encode(buf.getvalue()).decode("ascii")


def _image_result(b64):
    return {"content": [{"type": "image", "data": b64, "mimeType": "image/webp"}]}


def _text_result(obj):
    return {"content": [{"type": "text", "text": json.dumps(obj, indent=2)}]}


def cmd_get_screen_size(args):
    nw, nh = _primary_native()
    sw, sh = scaled_space(args.get("max_width"), args.get("max_height"))
    return _text_result({"width": sw or nw, "height": sh or nh,
                         "original_width": nw, "original_height": nh})


def cmd_screenshot(args):
    nw, nh = _primary_native()
    sw, sh = scaled_space(args.get("max_width"), args.get("max_height"))
    mon = grabber().monitors[1]
    shot = grabber().grab(mon)
    target = None if (sw == 0 or sh == 0) else (sw, sh)
    return _image_result(_encode_webp(shot, target))


def cmd_screenshot_region(args):
    nw, nh = _primary_native()
    x1, y1 = to_native(args["min_x"], args["min_y"], (nw, nh),
                       args.get("max_width"), args.get("max_height"))
    x2, y2 = to_native(args["max_x"], args["max_y"], (nw, nh),
                       args.get("max_width"), args.get("max_height"))
    region = {"left": x1, "top": y1, "width": max(1, x2 - x1), "height": max(1, y2 - y1)}
    shot = grabber().grab(region)
    return _image_result(_encode_webp(shot))  # native resolution, no resize


def cmd_active_window(args):
    info = active_window_info()  # provided by Task 7 window backend
    return _text_result(info)


def cmd_screenshot_window(args):
    win = find_window(args.get("title"), args.get("index"))  # Task 7
    if win is None:
        return _text_result({"status": "error", "error": "window not found"})
    region = {"left": win["x"], "top": win["y"], "width": win["width"], "height": win["height"]}
    shot = grabber().grab(region)
    return _image_result(_encode_webp(shot))
```

> Dependency note: `cmd_active_window` and `cmd_screenshot_window` call
> `active_window_info()` / `find_window()` defined in Task 7. Register them in the
> registry (Task 9). Until Task 7 lands, the two tests added here
> (`get_screen_size`, `screenshot`) do not touch those functions and pass.

- [ ] **Step 4: Run test to verify it passes**

Run: `python -m pytest tests/test_handlers.py -k screen -v`
Expected: PASS (2 passed)

- [ ] **Step 5: Commit**

```bash
git add screenmcp/python-cli/screenmcp_cli.py screenmcp/python-cli/tests/test_handlers.py
git commit -m "feat(python-cli): vision commands (screenshot/region/window/size)"
```

---

### Task 5: Keyboard commands + key-name resolution

**Files:**
- Modify: `screenmcp/python-cli/screenmcp_cli.py` (add `# === KEYBOARD ===` section)
- Modify: `screenmcp/python-cli/tests/test_handlers.py`

- [ ] **Step 1: Write the failing test**

Append to `tests/test_handlers.py`:

```python
class _RecKeyboard:
    def __init__(self):
        self.events = []
        self.typed = []

    def press(self, k):
        self.events.append(("press", k))

    def release(self, k):
        self.events.append(("release", k))

    def type(self, s):
        self.typed.append(s)


def test_type_sends_text(monkeypatch):
    rec = _RecKeyboard()
    monkeypatch.setattr(app, "keyboard", lambda: rec)
    app.cmd_type({"text": "hi"})
    assert rec.typed == ["hi"]


def test_hotkey_presses_then_releases_in_reverse(monkeypatch):
    rec = _RecKeyboard()
    monkeypatch.setattr(app, "keyboard", lambda: rec)
    app.cmd_hotkey({"keys": ["ctrl", "c"]})
    kinds = [e[0] for e in rec.events]
    assert kinds == ["press", "press", "release", "release"]


def test_resolve_key_maps_named_keys():
    from pynput.keyboard import Key
    assert app.resolve_key("enter") == Key.enter
    assert app.resolve_key("a") == "a"
```

- [ ] **Step 2: Run test to verify it fails**

Run: `python -m pytest tests/test_handlers.py -k "type or hotkey or resolve" -v`
Expected: FAIL — `AttributeError: ... has no attribute 'cmd_type'`

- [ ] **Step 3: Write minimal implementation**

Add `# === KEYBOARD ===`:

```python
# === KEYBOARD ===
def resolve_key(name):
    """Map a key name to a pynput key object, or return a 1-char string as-is."""
    from pynput.keyboard import Key
    name = str(name).lower()
    special = {
        "shift": Key.shift, "ctrl": Key.ctrl, "control": Key.ctrl,
        "alt": Key.alt, "meta": Key.cmd, "win": Key.cmd, "cmd": Key.cmd,
        "super": Key.cmd, "tab": Key.tab, "enter": Key.enter, "return": Key.enter,
        "escape": Key.esc, "esc": Key.esc, "space": Key.space,
        "backspace": Key.backspace, "delete": Key.delete, "del": Key.delete,
        "home": Key.home, "end": Key.end, "pageup": Key.page_up,
        "pagedown": Key.page_down, "up": Key.up, "down": Key.down,
        "left": Key.left, "right": Key.right,
    }
    if name in special:
        return special[name]
    if len(name) > 1 and name[0] == "f" and name[1:].isdigit():
        return getattr(Key, name)  # f1..f12
    return name  # single character


def _ok(extra=None):
    obj = {"status": "ok"}
    if extra:
        obj.update(extra)
    return _text_result(obj)


def cmd_type(args):
    keyboard().type(args.get("text", ""))
    return _ok()


def cmd_press_key(args):
    k = resolve_key(args["key"])
    keyboard().press(k)
    keyboard().release(k)
    return _ok()


def cmd_hold_key(args):
    keyboard().press(resolve_key(args["key"]))
    return _ok()


def cmd_release_key(args):
    keyboard().release(resolve_key(args["key"]))
    return _ok()


def cmd_hotkey(args):
    keys = [resolve_key(k) for k in args["keys"]]
    for k in keys:
        keyboard().press(k)
    for k in reversed(keys):
        keyboard().release(k)
    return _ok()
```

- [ ] **Step 4: Run test to verify it passes**

Run: `python -m pytest tests/test_handlers.py -k "type or hotkey or resolve" -v`
Expected: PASS (3 passed)

- [ ] **Step 5: Commit**

```bash
git add screenmcp/python-cli/screenmcp_cli.py screenmcp/python-cli/tests/test_handlers.py
git commit -m "feat(python-cli): keyboard commands + key-name resolution"
```

---

### Task 6: Mouse commands

**Files:**
- Modify: `screenmcp/python-cli/screenmcp_cli.py` (add `# === MOUSE ===` section)
- Modify: `screenmcp/python-cli/tests/test_handlers.py`

- [ ] **Step 1: Write the failing test**

Append to `tests/test_handlers.py`:

```python
class _RecMouse:
    def __init__(self):
        self.position = (0, 0)
        self.calls = []

    def click(self, button, count=1):
        self.calls.append(("click", str(button), count))

    def press(self, button):
        self.calls.append(("press", str(button)))

    def release(self, button):
        self.calls.append(("release", str(button)))

    def scroll(self, dx, dy):
        self.calls.append(("scroll", dx, dy))


def _patch_mouse(monkeypatch, native=(1456, 819)):
    rec = _RecMouse()
    monkeypatch.setattr(app, "mouse", lambda: rec)
    monkeypatch.setattr(app, "_primary_native", lambda: native)
    return rec


def test_click_moves_to_scaled_point_and_clicks(monkeypatch):
    rec = _patch_mouse(monkeypatch, native=(2912, 1638))  # 2x
    app.cmd_click({"x": 100, "y": 200})
    assert rec.position == (200, 400)
    assert rec.calls[0][0] == "click"


def test_double_click_count_two(monkeypatch):
    rec = _patch_mouse(monkeypatch)
    app.cmd_double_click({"x": 10, "y": 10})
    assert rec.calls[0] == ("click", str(rec.calls[0][1]), 2) or rec.calls[0][2] == 2


def test_scroll_direction_down_is_negative_dy(monkeypatch):
    rec = _patch_mouse(monkeypatch)
    app.cmd_scroll({"x": 10, "y": 10, "direction": "down", "amount": 3})
    assert ("scroll", 0, -3) in rec.calls
```

- [ ] **Step 2: Run test to verify it fails**

Run: `python -m pytest tests/test_handlers.py -k "click or scroll" -v`
Expected: FAIL — `AttributeError: ... has no attribute 'cmd_click'`

- [ ] **Step 3: Write minimal implementation**

Add `# === MOUSE ===`:

```python
# === MOUSE ===
import time as _time


def _move(args):
    nw, nh = _primary_native()
    nx, ny = to_native(args["x"], args["y"], (nw, nh),
                      args.get("max_width"), args.get("max_height"))
    mouse().position = (nx, ny)
    return nx, ny


def _button(name="left"):
    from pynput.mouse import Button
    return {"left": Button.left, "right": Button.right, "middle": Button.middle}[name]


def cmd_mouse_move(args):
    _move(args)
    return _ok()


def cmd_click(args):
    _move(args)
    mouse().click(_button("left"), 1)
    return _ok()


def cmd_right_click(args):
    _move(args)
    mouse().click(_button("right"), 1)
    return _ok()


def cmd_middle_click(args):
    _move(args)
    mouse().click(_button("middle"), 1)
    return _ok()


def cmd_double_click(args):
    _move(args)
    mouse().click(_button("left"), 2)
    return _ok()


def cmd_long_click(args):
    _move(args)
    mouse().press(_button("left"))
    _time.sleep(args.get("duration", 1000) / 1000.0)
    mouse().release(_button("left"))
    return _ok()


def cmd_drag(args):
    nw, nh = _primary_native()
    sx, sy = to_native(args["startX"], args["startY"], (nw, nh),
                      args.get("max_width"), args.get("max_height"))
    ex, ey = to_native(args["endX"], args["endY"], (nw, nh),
                      args.get("max_width"), args.get("max_height"))
    mouse().position = (sx, sy)
    mouse().press(_button("left"))
    _time.sleep(args.get("duration", 300) / 1000.0)
    mouse().position = (ex, ey)
    mouse().release(_button("left"))
    return _ok()


def cmd_scroll(args):
    _move(args)
    direction = args.get("direction")
    if direction:
        amount = args.get("amount", 3)
        dx, dy = {"up": (0, amount), "down": (0, -amount),
                  "left": (-amount, 0), "right": (amount, 0)}[direction]
    else:
        dx, dy = args.get("dx", 0), args.get("dy", 0)
    mouse().scroll(dx, dy)
    return _ok()


def cmd_mouse_scroll(args):
    return cmd_scroll(args)
```

- [ ] **Step 4: Run test to verify it passes**

Run: `python -m pytest tests/test_handlers.py -k "click or scroll" -v`
Expected: PASS (3 passed)

- [ ] **Step 5: Commit**

```bash
git add screenmcp/python-cli/screenmcp_cli.py screenmcp/python-cli/tests/test_handlers.py
git commit -m "feat(python-cli): mouse commands"
```

---

### Task 7: Window management + navigation

**Files:**
- Modify: `screenmcp/python-cli/screenmcp_cli.py` (add `# === WINDOW MANAGEMENT ===` and `# === NAVIGATION ===`)
- Modify: `screenmcp/python-cli/tests/test_handlers.py`

- [ ] **Step 1: Write the failing test**

Append to `tests/test_handlers.py`:

```python
def test_nav_back_uses_keymap(monkeypatch):
    rec = _RecKeyboard()
    monkeypatch.setattr(app, "keyboard", lambda: rec)
    monkeypatch.setattr(app, "nav_keymap",
                        lambda: {"back": ["alt", "left"], "home": ["a"], "recents": ["b"]})
    app.cmd_back({})
    assert [e[0] for e in rec.events] == ["press", "press", "release", "release"]


def test_focus_window_not_found_returns_error(monkeypatch):
    monkeypatch.setattr(app, "_win_list", lambda: [])
    out = app.cmd_focus_window({"title": "nope"})
    assert "error" in out["content"][0]["text"]


def test_list_windows_maps_bounds_to_scaled_space(monkeypatch):
    monkeypatch.setattr(app, "_primary_native", lambda: (2912, 1638))  # 2x
    monkeypatch.setattr(app, "_win_list", lambda: [
        {"title": "A", "x": 200, "y": 400, "width": 1000, "height": 500, "index": 0, "state": "normal"}
    ])
    out = app.cmd_list_windows({})
    text = out["content"][0]["text"]
    assert '"x": 100' in text and '"y": 200' in text  # halved into 1456x819 space
```

- [ ] **Step 2: Run test to verify it fails**

Run: `python -m pytest tests/test_handlers.py -k "nav or window" -v`
Expected: FAIL — `AttributeError: ... has no attribute 'cmd_back'`

- [ ] **Step 3: Write minimal implementation**

Add `# === WINDOW MANAGEMENT ===`. `_win_list()` is the single platform-specific
seam; everything else scales/filters its output:

```python
# === WINDOW MANAGEMENT ===
def _win_list():
    """Return list of native-pixel window dicts: {title,x,y,width,height,index,state}."""
    if IS_WIN:
        return _win_list_windows()
    if IS_MAC:
        return _win_list_mac()
    if IS_LINUX:
        return _win_list_linux()
    return []


def _win_list_windows():
    import pygetwindow as gw
    out = []
    for i, w in enumerate(gw.getAllWindows()):
        if not w.title:
            continue
        state = "minimized" if w.isMinimized else ("maximized" if w.isMaximized else "normal")
        out.append({"title": w.title, "x": w.left, "y": w.top,
                    "width": w.width, "height": w.height, "index": i, "state": state})
    return out


def _win_list_mac():
    try:
        from Quartz import (CGWindowListCopyWindowInfo, kCGWindowListOptionOnScreenOnly,
                            kCGNullWindowID)
    except Exception:
        return []
    info = CGWindowListCopyWindowInfo(kCGWindowListOptionOnScreenOnly, kCGNullWindowID)
    out = []
    for i, w in enumerate(info or []):
        name = w.get("kCGWindowName") or w.get("kCGWindowOwnerName") or ""
        b = w.get("kCGWindowBounds") or {}
        if not name:
            continue
        out.append({"title": name, "x": int(b.get("X", 0)), "y": int(b.get("Y", 0)),
                    "width": int(b.get("Width", 0)), "height": int(b.get("Height", 0)),
                    "index": i, "state": "normal"})
    return out


def _win_list_linux():
    import subprocess
    try:
        raw = subprocess.check_output(["wmctrl", "-lG"], text=True)
    except Exception:
        return []
    out = []
    for i, line in enumerate(raw.splitlines()):
        parts = line.split(None, 7)
        if len(parts) < 8:
            continue
        _, _, x, y, w, h, _, title = parts
        out.append({"title": title, "x": int(x), "y": int(y),
                    "width": int(w), "height": int(h), "index": i, "state": "normal"})
    return out


def _scale_bounds_to_space(win):
    nw, nh = _primary_native()
    fx, fy = (DEFAULT_W / nw if nw else 1), (DEFAULT_H / nh if nh else 1)
    return {**win,
            "x": int(round(win["x"] * fx)), "y": int(round(win["y"] * fy)),
            "width": int(round(win["width"] * fx)), "height": int(round(win["height"] * fy))}


def find_window(title=None, index=None):
    wins = _win_list()
    if index is not None and 0 <= index < len(wins):
        return wins[index]
    if title:
        for w in wins:
            if title.lower() in w["title"].lower():
                return w
    return None


def active_window_info():
    if IS_WIN:
        import pygetwindow as gw
        w = gw.getActiveWindow()
        if not w:
            return {"status": "error", "error": "no active window"}
        return {"title": w.title, "x": w.left, "y": w.top, "width": w.width, "height": w.height}
    if IS_MAC:
        wins = _win_list_mac()
        return wins[0] if wins else {"status": "error", "error": "no active window"}
    if IS_LINUX:
        import subprocess
        try:
            wid = subprocess.check_output(["xdotool", "getactivewindow", "getwindowname"], text=True).strip()
            return {"title": wid}
        except Exception:
            return {"status": "ok", "unsupported": True, "reason": "xdotool not available"}
    return {"status": "ok", "unsupported": True, "reason": "unsupported platform"}


def cmd_list_windows(args):
    return _text_result({"windows": [_scale_bounds_to_space(w) for w in _win_list()]})


def cmd_focus_window(args):
    win = find_window(args.get("title"), args.get("index"))
    if win is None:
        return _text_result({"status": "error", "error": "window not found"})
    if IS_WIN:
        import pygetwindow as gw
        target = gw.getWindowsWithTitle(win["title"])
        if target:
            try:
                target[0].activate()
            except Exception:
                target[0].minimize(); target[0].restore()
    elif IS_MAC:
        import subprocess
        subprocess.run(["osascript", "-e",
                        f'tell application "System Events" to set frontmost of '
                        f'(first process whose name contains "{win["title"]}") to true'],
                       check=False)
    elif IS_LINUX:
        import subprocess
        subprocess.run(["wmctrl", "-a", win["title"]], check=False)
    return _ok({"focused": win["title"]})
```

Add `# === NAVIGATION ===`:

```python
# === NAVIGATION ===
def _send_combo(keys):
    resolved = [resolve_key(k) for k in keys]
    for k in resolved:
        keyboard().press(k)
    for k in reversed(resolved):
        keyboard().release(k)


def cmd_back(args):
    _send_combo(nav_keymap()["back"])
    return _ok()


def cmd_home(args):
    _send_combo(nav_keymap()["home"])
    return _ok()


def cmd_recents(args):
    _send_combo(nav_keymap()["recents"])
    return _ok()
```

- [ ] **Step 4: Run test to verify it passes**

Run: `python -m pytest tests/test_handlers.py -k "nav or window" -v`
Expected: PASS (3 passed)

- [ ] **Step 5: Commit**

```bash
git add screenmcp/python-cli/screenmcp_cli.py screenmcp/python-cli/tests/test_handlers.py
git commit -m "feat(python-cli): window management + navigation"
```

---

### Task 8: Clipboard/text + system commands

**Files:**
- Modify: `screenmcp/python-cli/screenmcp_cli.py` (add `# === CLIPBOARD / TEXT ===` and `# === SYSTEM ===`)
- Modify: `screenmcp/python-cli/tests/test_handlers.py`

- [ ] **Step 1: Write the failing test**

Append to `tests/test_handlers.py`:

```python
def test_set_and_get_clipboard(monkeypatch):
    store = {"v": ""}
    monkeypatch.setattr(app, "_clip_set", lambda s: store.__setitem__("v", s))
    monkeypatch.setattr(app, "_clip_get", lambda: store["v"])
    app.cmd_set_clipboard({"text": "hello"})
    out = app.cmd_get_clipboard({})
    assert "hello" in out["content"][0]["text"]


def test_get_text_restores_original_clipboard(monkeypatch):
    store = {"v": "ORIGINAL"}
    monkeypatch.setattr(app, "_clip_set", lambda s: store.__setitem__("v", s))
    monkeypatch.setattr(app, "_clip_get", lambda: store["v"])
    # select_all + copy simulated: copy writes "SELECTED" into the clipboard
    monkeypatch.setattr(app, "cmd_select_all", lambda a: app._ok())
    def fake_copy(a):
        store["v"] = "SELECTED"
        return app._ok()
    monkeypatch.setattr(app, "cmd_copy", fake_copy)
    out = app.cmd_get_text({})
    assert "SELECTED" in out["content"][0]["text"]
    assert store["v"] == "ORIGINAL"  # restored


def test_camera_unsupported_when_cv2_missing(monkeypatch):
    monkeypatch.setattr(app, "_import_cv2", lambda: None)
    out = app.cmd_camera({})
    assert '"unsupported": true' in out["content"][0]["text"]
```

- [ ] **Step 2: Run test to verify it fails**

Run: `python -m pytest tests/test_handlers.py -k "clipboard or get_text or camera" -v`
Expected: FAIL — `AttributeError: ... has no attribute 'cmd_set_clipboard'`

- [ ] **Step 3: Write minimal implementation**

Add `# === CLIPBOARD / TEXT ===`:

```python
# === CLIPBOARD / TEXT ===
def _clip_get():
    import pyperclip
    return pyperclip.paste()


def _clip_set(text):
    import pyperclip
    pyperclip.copy(text)


def cmd_get_clipboard(args):
    return _text_result({"text": _clip_get()})


def cmd_set_clipboard(args):
    _clip_set(args.get("text", ""))
    return _ok()


def cmd_select_all(args):
    _send_combo([PRIMARY_MOD, "a"])
    return _ok()


def cmd_copy(args):
    _send_combo([PRIMARY_MOD, "c"])
    _time.sleep(0.05)
    if args.get("return_text"):
        return _text_result({"text": _clip_get()})
    return _ok()


def cmd_paste(args):
    if "text" in args:
        _clip_set(args["text"])
    _send_combo([PRIMARY_MOD, "v"])
    return _ok()


def cmd_get_text(args):
    original = _clip_get()
    try:
        cmd_select_all({})
        cmd_copy({})
        _time.sleep(0.05)
        text = _clip_get()
    finally:
        _clip_set(original)  # restore — heuristic must not clobber clipboard
    return _text_result({"text": text})
```

Add `# === SYSTEM ===`:

```python
# === SYSTEM ===
import os as _os


def cmd_is_elevated(args):
    if IS_WIN:
        try:
            import ctypes
            elevated = bool(ctypes.windll.shell32.IsUserAnAdmin())
        except Exception:
            elevated = False
    else:
        elevated = (hasattr(_os, "geteuid") and _os.geteuid() == 0)
    return _text_result({"elevated": elevated})


def cmd_elevate(args):
    return _text_result({"status": "ok", "unsupported": True,
                         "reason": "cannot re-launch a live stdio process elevated"})


def _import_cv2():
    try:
        import cv2
        return cv2
    except Exception:
        return None


def cmd_list_cameras(args):
    cv2 = _import_cv2()
    if cv2 is None:
        return _text_result({"status": "ok", "unsupported": True, "reason": "opencv-python not installed"})
    found = []
    for i in range(5):
        cap = cv2.VideoCapture(i)
        if cap is not None and cap.isOpened():
            found.append(i)
            cap.release()
    return _text_result({"cameras": found})


def cmd_camera(args):
    cv2 = _import_cv2()
    if cv2 is None:
        return _text_result({"status": "ok", "unsupported": True, "reason": "opencv-python not installed"})
    index = args.get("index", 0)
    cap = cv2.VideoCapture(index)
    try:
        ok, frame = cap.read()
        if not ok:
            return _text_result({"status": "error", "error": f"camera {index} read failed"})
        ok, buf = cv2.imencode(".webp", frame)
        return _image_result(_b64.b64encode(buf.tobytes()).decode("ascii"))
    finally:
        cap.release()


def cmd_play_audio(args):
    try:
        import simpleaudio
    except Exception:
        return _text_result({"status": "ok", "unsupported": True, "reason": "simpleaudio not installed"})
    data = _b64.b64decode(args["audio"])
    wave_obj = simpleaudio.WaveObject(data, num_channels=args.get("channels", 2),
                                      bytes_per_sample=args.get("bytes_per_sample", 2),
                                      sample_rate=args.get("sample_rate", 44100))
    wave_obj.play().wait_done()
    return _ok()
```

- [ ] **Step 4: Run test to verify it passes**

Run: `python -m pytest tests/test_handlers.py -k "clipboard or get_text or camera" -v`
Expected: PASS (3 passed)

- [ ] **Step 5: Commit**

```bash
git add screenmcp/python-cli/screenmcp_cli.py screenmcp/python-cli/tests/test_handlers.py
git commit -m "feat(python-cli): clipboard/text + system commands"
```

---

### Task 9: Registry wiring + completeness test

**Files:**
- Modify: `screenmcp/python-cli/screenmcp_cli.py` (add `# === REGISTRY ===` before the server section)
- Create: `screenmcp/python-cli/tests/test_registry.py`

- [ ] **Step 1: Write the failing test**

`tests/test_registry.py`:

```python
import screenmcp_cli as app

EXPECTED = {
    "screenshot", "screenshot_region", "screenshot_window", "active_window",
    "get_screen_size", "click", "right_click", "double_click", "long_click",
    "middle_click", "mouse_move", "drag", "scroll", "mouse_scroll", "type",
    "press_key", "hold_key", "release_key", "hotkey", "get_text", "select_all",
    "copy", "paste", "get_clipboard", "set_clipboard", "list_windows",
    "focus_window", "get_screen_size", "back", "home", "recents", "elevate",
    "is_elevated", "camera", "list_cameras", "play_audio",
}


def test_every_expected_command_is_registered():
    missing = EXPECTED - set(app.TOOLS)
    assert not missing, f"missing tools: {missing}"


def test_ui_tree_is_not_registered():
    assert "ui_tree" not in app.TOOLS


def test_every_tool_has_description_schema_and_callable_handler():
    for name, tool in app.TOOLS.items():
        assert tool["description"], name
        assert tool["inputSchema"]["type"] == "object", name
        assert callable(tool["handler"]), name


def test_tools_list_method_returns_all(tmp_path):
    listed = app._list_tools()
    assert len(listed) == len(app.TOOLS)
```

- [ ] **Step 2: Run test to verify it fails**

Run: `python -m pytest tests/test_registry.py -v`
Expected: FAIL — `test_every_expected_command_is_registered` (TOOLS is empty)

- [ ] **Step 3: Write minimal implementation**

Add `# === REGISTRY ===` just before `# === JSON-RPC STDIO SERVER ===`. Define a
small schema helper, then populate `TOOLS`:

```python
# === REGISTRY ===
def _schema(props=None, required=None):
    return {"type": "object", "properties": props or {}, "required": required or []}


_XY = {"x": {"type": "number"}, "y": {"type": "number"}}
_MAXWH = {"max_width": {"type": "number"}, "max_height": {"type": "number"}}

TOOLS.update({
    "screenshot": {"description": "Take a screenshot (default 1456x819, WebP).",
                   "inputSchema": _schema(_MAXWH), "handler": cmd_screenshot},
    "screenshot_region": {"description": "Capture a region at native resolution.",
                          "inputSchema": _schema({**_MAXWH, "min_x": {"type": "number"},
                                                  "min_y": {"type": "number"}, "max_x": {"type": "number"},
                                                  "max_y": {"type": "number"}},
                                                 ["min_x", "min_y", "max_x", "max_y"]),
                          "handler": cmd_screenshot_region},
    "screenshot_window": {"description": "Capture a specific window by title or index.",
                          "inputSchema": _schema({"title": {"type": "string"}, "index": {"type": "number"}}),
                          "handler": cmd_screenshot_window},
    "active_window": {"description": "Get the currently focused window info.",
                      "inputSchema": _schema(), "handler": cmd_active_window},
    "get_screen_size": {"description": "Get screen dimensions (scaled + native).",
                        "inputSchema": _schema(_MAXWH), "handler": cmd_get_screen_size},
    "click": {"description": "Left-click at (x, y).",
              "inputSchema": _schema({**_XY, **_MAXWH}, ["x", "y"]), "handler": cmd_click},
    "right_click": {"description": "Right-click at (x, y).",
                    "inputSchema": _schema({**_XY, **_MAXWH}, ["x", "y"]), "handler": cmd_right_click},
    "double_click": {"description": "Double-click at (x, y).",
                     "inputSchema": _schema({**_XY, **_MAXWH}, ["x", "y"]), "handler": cmd_double_click},
    "middle_click": {"description": "Middle-click at (x, y).",
                     "inputSchema": _schema({**_XY, **_MAXWH}, ["x", "y"]), "handler": cmd_middle_click},
    "long_click": {"description": "Long press at (x, y). Optional duration ms (default 1000).",
                   "inputSchema": _schema({**_XY, **_MAXWH, "duration": {"type": "number"}}, ["x", "y"]),
                   "handler": cmd_long_click},
    "mouse_move": {"description": "Move cursor to (x, y) without clicking.",
                   "inputSchema": _schema({**_XY, **_MAXWH}, ["x", "y"]), "handler": cmd_mouse_move},
    "drag": {"description": "Drag from (startX,startY) to (endX,endY). Optional duration ms.",
             "inputSchema": _schema({"startX": {"type": "number"}, "startY": {"type": "number"},
                                     "endX": {"type": "number"}, "endY": {"type": "number"},
                                     "duration": {"type": "number"}, **_MAXWH},
                                    ["startX", "startY", "endX", "endY"]), "handler": cmd_drag},
    "scroll": {"description": "Scroll at (x,y) with dx/dy, or direction+amount.",
               "inputSchema": _schema({**_XY, "dx": {"type": "number"}, "dy": {"type": "number"},
                                       "direction": {"type": "string"}, "amount": {"type": "number"}, **_MAXWH},
                                      ["x", "y"]), "handler": cmd_scroll},
    "mouse_scroll": {"description": "Raw mouse wheel scroll at coordinates.",
                     "inputSchema": _schema({**_XY, "dx": {"type": "number"}, "dy": {"type": "number"}, **_MAXWH},
                                            ["x", "y"]), "handler": cmd_mouse_scroll},
    "type": {"description": "Type text into the focused field.",
             "inputSchema": _schema({"text": {"type": "string"}}, ["text"]), "handler": cmd_type},
    "press_key": {"description": "Press and release a single key.",
                  "inputSchema": _schema({"key": {"type": "string"}}, ["key"]), "handler": cmd_press_key},
    "hold_key": {"description": "Hold a key down.",
                 "inputSchema": _schema({"key": {"type": "string"}}, ["key"]), "handler": cmd_hold_key},
    "release_key": {"description": "Release a held key.",
                    "inputSchema": _schema({"key": {"type": "string"}}, ["key"]), "handler": cmd_release_key},
    "hotkey": {"description": "Press a key combination atomically, e.g. [ctrl, c].",
               "inputSchema": _schema({"keys": {"type": "array", "items": {"type": "string"}}}, ["keys"]),
               "handler": cmd_hotkey},
    "get_text": {"description": "Get text from focused field (clipboard heuristic, restores clipboard).",
                 "inputSchema": _schema(), "handler": cmd_get_text},
    "select_all": {"description": "Select all text (primary modifier + A).",
                   "inputSchema": _schema(), "handler": cmd_select_all},
    "copy": {"description": "Copy selection. Set return_text:true to get the text.",
             "inputSchema": _schema({"return_text": {"type": "boolean"}}), "handler": cmd_copy},
    "paste": {"description": "Paste. Optionally pass text to set clipboard first.",
              "inputSchema": _schema({"text": {"type": "string"}}), "handler": cmd_paste},
    "get_clipboard": {"description": "Read clipboard contents.",
                      "inputSchema": _schema(), "handler": cmd_get_clipboard},
    "set_clipboard": {"description": "Set clipboard to given text.",
                      "inputSchema": _schema({"text": {"type": "string"}}, ["text"]), "handler": cmd_set_clipboard},
    "list_windows": {"description": "List visible windows (bounds in screenshot space).",
                     "inputSchema": _schema(), "handler": cmd_list_windows},
    "focus_window": {"description": "Bring a window to front by title (substring) or index.",
                     "inputSchema": _schema({"title": {"type": "string"}, "index": {"type": "number"}}),
                     "handler": cmd_focus_window},
    "back": {"description": "Back navigation (platform hotkey).",
             "inputSchema": _schema(), "handler": cmd_back},
    "home": {"description": "Home / Start (platform hotkey).",
             "inputSchema": _schema(), "handler": cmd_home},
    "recents": {"description": "Recent windows / app switcher.",
                "inputSchema": _schema(), "handler": cmd_recents},
    "is_elevated": {"description": "Check if running with admin privileges.",
                    "inputSchema": _schema(), "handler": cmd_is_elevated},
    "elevate": {"description": "Request admin privileges (unsupported in stdio mode).",
                "inputSchema": _schema(), "handler": cmd_elevate},
    "camera": {"description": "Capture a frame from a camera (requires [camera] extra).",
               "inputSchema": _schema({"index": {"type": "number"}}), "handler": cmd_camera},
    "list_cameras": {"description": "List available cameras (requires [camera] extra).",
                     "inputSchema": _schema(), "handler": cmd_list_cameras},
    "play_audio": {"description": "Play base64 WAV audio (requires [audio] extra).",
                   "inputSchema": _schema({"audio": {"type": "string"}, "sample_rate": {"type": "number"},
                                           "channels": {"type": "number"}, "bytes_per_sample": {"type": "number"}},
                                          ["audio"]), "handler": cmd_play_audio},
})
```

- [ ] **Step 4: Run test to verify it passes**

Run: `python -m pytest tests/test_registry.py -v`
Expected: PASS (4 passed)

- [ ] **Step 5: Run the full suite + commit**

Run: `python -m pytest -v`
Expected: PASS (all tasks' tests green)

```bash
git add screenmcp/python-cli/screenmcp_cli.py screenmcp/python-cli/tests/test_registry.py
git commit -m "feat(python-cli): wire command registry"
```

---

### Task 10: End-to-end stdio smoke test

**Files:**
- Create: `screenmcp/python-cli/tests/test_stdio.py`

- [ ] **Step 1: Write the failing test**

`tests/test_stdio.py`:

```python
import json
import subprocess
import sys
import os

HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def test_end_to_end_initialize_list_and_call():
    reqs = [
        {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}},
        {"jsonrpc": "2.0", "method": "notifications/initialized"},
        {"jsonrpc": "2.0", "id": 2, "method": "tools/list"},
        {"jsonrpc": "2.0", "id": 3, "method": "tools/call",
         "params": {"name": "get_screen_size", "arguments": {}}},
    ]
    stdin = "".join(json.dumps(r) + "\n" for r in reqs)
    proc = subprocess.run([sys.executable, "screenmcp_cli.py"],
                          input=stdin, capture_output=True, text=True, cwd=HERE, timeout=30)
    responses = [json.loads(l) for l in proc.stdout.splitlines() if l.strip()]
    by_id = {r.get("id"): r for r in responses}
    assert by_id[1]["result"]["serverInfo"]["name"] == "screenmcp-cli"
    assert len(by_id[2]["result"]["tools"]) >= 30
    # get_screen_size returns a text content block with width/height
    assert "width" in by_id[3]["result"]["content"][0]["text"]
```

- [ ] **Step 2: Run test to verify it fails (if run before deps installed) / passes**

Run: `python -m pytest tests/test_stdio.py -v`
Expected: PASS once `mss`/`pynput`/`Pillow`/`pyperclip` are installed in the env.
(If a headless CI has no display, `get_screen_size` may need `xvfb-run`; document in README. The assertion on tools/list still validates protocol.)

- [ ] **Step 3: Commit**

```bash
git add screenmcp/python-cli/tests/test_stdio.py
git commit -m "test(python-cli): end-to-end stdio smoke test"
```

---

### Task 11: README + adding-new-command docs

**Files:**
- Create: `screenmcp/python-cli/README.md`
- Modify: `screenmcp/docs/adding-new-command.md`

- [ ] **Step 1: Write the README**

`screenmcp/python-cli/README.md`:

```markdown
# ScreenMCP — Python CLI (Standalone Local stdio MCP)

A single-file MCP server that controls **this** computer's desktop directly over
stdio. No worker, no relay, no auth, no network. Windows, macOS, Linux.

## Install

    cd screenmcp/python-cli
    pip install -e .            # core
    pip install -e ".[camera]"  # + camera (opencv-python)
    pip install -e ".[audio]"   # + play_audio (simpleaudio)

Or just run the file with the deps available:

    pip install mss pynput Pillow pyperclip pygetwindow
    python screenmcp_cli.py

## Configure an MCP client

Claude Code:

    claude mcp add screenmcp-local -- python /abs/path/to/screenmcp_cli.py

Or JSON (Claude Desktop `mcpServers`):

    {
      "mcpServers": {
        "screenmcp-local": {
          "command": "python",
          "args": ["/abs/path/to/screenmcp_cli.py"]
        }
      }
    }

## Coordinates

Screenshots default to 1456x819. Click/drag/scroll coordinates are in that space
and auto-scale to your real screen. Override per call with `max_width`/`max_height`
(0 = native pixels).

## macOS permissions

The first screenshot triggers a **Screen Recording** prompt; the first click/keypress
triggers an **Accessibility** prompt. Both attach to the app that launched python
(Terminal/iTerm/your MCP client). Grant them in
**System Settings → Privacy & Security → Screen Recording / Accessibility**, then
restart the launching app. Denied permissions surface as an `isError` tool result
naming the missing permission.

## Linux notes

Window management uses `wmctrl` (and `xdotool` for active window). Install them:
`sudo apt install wmctrl xdotool`. Wayland is best-effort; some window ops may
report `unsupported`.

## Unsupported / stubbed

- `ui_tree` — not implemented (out of scope).
- `elevate` — returns `unsupported` (cannot re-launch a live stdio process elevated).
- `camera` / `play_audio` — require the optional `[camera]` / `[audio]` extras.
```

- [ ] **Step 2: Update `adding-new-command.md`**

In `screenmcp/docs/adding-new-command.md`, add a new component section after
section 4 (Linux Desktop Client). Insert:

```markdown
### 4b. Python CLI — Standalone Local stdio MCP

| File | Purpose |
|------|---------|
| `python-cli/screenmcp_cli.py` | Add a `cmd_<name>(args)` handler in the relevant `# === SECTION ===`, then add an entry to the `TOOLS` registry (name, description, inputSchema, handler) |

**Pattern**: Single self-contained file. Handlers return MCP `content` blocks via
`_text_result(...)` / `_image_result(...)` / `_ok(...)`. Coordinate params scale
via `to_native(...)`. Platform-specific behavior is guarded by `IS_WIN`/`IS_MAC`/
`IS_LINUX`. `ui_tree` is intentionally not implemented here. Update the completeness
set in `python-cli/tests/test_registry.py`.
```

Then add to the Checklist Template block:

```markdown
[ ] Python CLI: screenmcp_cli.py — add handler + TOOLS entry; update test_registry.py
```

- [ ] **Step 3: Commit**

```bash
git add screenmcp/python-cli/README.md screenmcp/docs/adding-new-command.md
git commit -m "docs(python-cli): README + adding-new-command entry"
```

---

## Self-Review Notes

- **Spec coverage:** transport (T1), scaling (T2), platform/permissions seams (T3),
  vision (T4), keyboard (T5), mouse (T6), window+nav (T7), clipboard/text+system
  incl. get_text heuristic / elevate stub / camera+audio optional (T8), registry &
  ui_tree exclusion (T9), e2e (T10), README+docs incl. mac permissions & adding-new-command (T11).
  All spec sections map to a task.
- **Naming consistency:** handlers `cmd_<name>`; seams `mouse()/keyboard()/grabber()`,
  `_win_list()/find_window()/active_window_info()`, `_clip_get()/_clip_set()`,
  `_import_cv2()`, `_primary_native()`, `to_native()/scaled_space()`,
  result builders `_text_result/_image_result/_ok/_image_result`. Used consistently
  across tasks.
- **Test seams:** every side-effecting handler reaches hardware only through an
  accessor that tests monkeypatch — no real display needed for the unit suite.
```