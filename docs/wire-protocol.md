# ScreenMCP Wire Protocol

How commands flow between controllers, the worker, and devices over WebSocket.

For connection setup, authentication, session resumption, and SSE notifications, see [initiation-protocol.md](initiation-protocol.md).
For a reference table of all commands (params, types, defaults), see [commands.md](commands.md).

## Command Flow

```
Controller (SDK/MCP)          Worker (Rust relay)           Device (Phone/Desktop)
        │                            │                            │
        ├── { cmd: "click",          │                            │
        │     params: {x,y} } ──────►│                            │
        │                            │                            │
        │◄── { type: "cmd_accepted", │                            │
        │     id: 7 } ──────────────┤                            │
        │                            ├── { id: 7, cmd: "click",  │
        │                            │     params: {x,y} } ──────►│
        │                            │                            │
        │                            │     (device executes tap)  │
        │                            │                            │
        │                            │◄── { id: 7, status: "ok", │
        │                            │     result: {} } ──────────┤
        │◄── { id: 7, status: "ok", │                            │
        │     result: {} } ──────────┤                            │
```

1. **Controller sends** a command — just `cmd` and optional `params`, no `id` yet
2. **Worker assigns** a numeric `id` and replies with `cmd_accepted`
3. **Worker forwards** the command (with `id`) to the target device
4. **Device executes** the command and sends back a response with the same `id`
5. **Worker relays** the response back to the controller

## Message Format

All messages are JSON over WebSocket. No framing — one JSON object per WebSocket text frame.

### Controller → Worker (Command)

```json
{ "cmd": "screenshot", "params": { "quality": 80 } }
```

The `params` field is omitted when the command takes no parameters:

```json
{ "cmd": "back" }
```

### Worker → Controller (Command Accepted)

```json
{ "type": "cmd_accepted", "id": 7 }
```

### Worker → Device (Command)

```json
{ "id": 7, "cmd": "screenshot", "params": { "quality": 80 } }
```

### Device → Worker → Controller (Response)

Success:
```json
{ "id": 7, "status": "ok", "result": { "image": "<base64>" } }
```

Error:
```json
{ "id": 7, "status": "error", "error": "no active window" }
```

Unsupported (desktop-only commands on Android, or vice versa):
```json
{ "id": 7, "status": "ok", "unsupported": true }
```

### Server Messages (typed)

```json
{ "type": "phone_status", "connected": true }
{ "type": "ping" }
{ "type": "error", "error": "rate limit exceeded" }
```

Client must respond to pings:
```json
{ "type": "pong" }
```

---

## All Commands — Full Examples

Every command is shown twice: first with **all optional params included**, then with **only required params** (minimal form).

### screenshot

Full:
```json
{ "cmd": "screenshot", "params": { "quality": 80, "max_width": 1080, "max_height": 1920 } }
```

Minimal:
```json
{ "cmd": "screenshot" }
```

Response:
```json
{ "id": 1, "status": "ok", "result": { "image": "<base64 webp>" } }
```

### ui_tree

Full (no optional params):
```json
{ "cmd": "ui_tree" }
```

Response:
```json
{ "id": 2, "status": "ok", "result": { "tree": [ { "className": "FrameLayout", "bounds": { "left": 0, "top": 0, "right": 1080, "bottom": 1920 }, "children": [] } ] } }
```

### click

Full:
```json
{ "cmd": "click", "params": { "x": 540, "y": 1200, "duration": 200 } }
```

Minimal:
```json
{ "cmd": "click", "params": { "x": 540, "y": 1200 } }
```

Response:
```json
{ "id": 3, "status": "ok", "result": {} }
```

### long_click

Full (no optional params):
```json
{ "cmd": "long_click", "params": { "x": 540, "y": 1200 } }
```

Response:
```json
{ "id": 4, "status": "ok", "result": {} }
```

### drag

Full:
```json
{ "cmd": "drag", "params": { "startX": 200, "startY": 800, "endX": 200, "endY": 400, "duration": 500 } }
```

Minimal:
```json
{ "cmd": "drag", "params": { "startX": 200, "startY": 800, "endX": 200, "endY": 400 } }
```

Response:
```json
{ "id": 5, "status": "ok", "result": {} }
```

### scroll

Full:
```json
{ "cmd": "scroll", "params": { "x": 540, "y": 960, "dx": 0, "dy": -500 } }
```

Minimal:
```json
{ "cmd": "scroll", "params": { "x": 540, "y": 960 } }
```

Response:
```json
{ "id": 6, "status": "ok", "result": {} }
```

### type

Full (no optional params):
```json
{ "cmd": "type", "params": { "text": "Hello world" } }
```

Response:
```json
{ "id": 7, "status": "ok", "result": {} }
```

### get_text

Full (no params):
```json
{ "cmd": "get_text" }
```

Response:
```json
{ "id": 8, "status": "ok", "result": { "text": "current field contents" } }
```

### select_all

Full (no params):
```json
{ "cmd": "select_all" }
```

Response:
```json
{ "id": 9, "status": "ok", "result": {} }
```

### copy

Full:
```json
{ "cmd": "copy", "params": { "return_text": true } }
```

Minimal:
```json
{ "cmd": "copy" }
```

Response (with `return_text`):
```json
{ "id": 10, "status": "ok", "result": { "text": "copied content" } }
```

Response (without `return_text`):
```json
{ "id": 10, "status": "ok", "result": {} }
```

### paste

Full:
```json
{ "cmd": "paste", "params": { "text": "text to paste" } }
```

Minimal:
```json
{ "cmd": "paste" }
```

Response:
```json
{ "id": 11, "status": "ok", "result": {} }
```

### get_clipboard

Full (no params):
```json
{ "cmd": "get_clipboard" }
```

Response:
```json
{ "id": 12, "status": "ok", "result": { "text": "clipboard contents" } }
```

### set_clipboard

Full (no optional params):
```json
{ "cmd": "set_clipboard", "params": { "text": "new clipboard value" } }
```

Response:
```json
{ "id": 13, "status": "ok", "result": {} }
```

### back

```json
{ "cmd": "back" }
```

Response:
```json
{ "id": 14, "status": "ok", "result": {} }
```

### home

```json
{ "cmd": "home" }
```

Response:
```json
{ "id": 15, "status": "ok", "result": {} }
```

### recents

```json
{ "cmd": "recents" }
```

Response:
```json
{ "id": 16, "status": "ok", "result": {} }
```

### list_cameras

Full (no params):
```json
{ "cmd": "list_cameras" }
```

Response (Android):
```json
{ "id": 17, "status": "ok", "result": { "cameras": [ { "id": "0", "facing": "back" }, { "id": "1", "facing": "front" }, { "id": "2", "facing": "external" } ] } }
```

Response (Desktop):
```json
{ "id": 17, "status": "ok", "result": { "cameras": [] } }
```

### camera

Full:
```json
{ "cmd": "camera", "params": { "camera": "1", "quality": 90, "max_width": 1920, "max_height": 1080 } }
```

Minimal:
```json
{ "cmd": "camera" }
```

Response:
```json
{ "id": 18, "status": "ok", "result": { "image": "<base64 webp>" } }
```

### hold_key (desktop only)

```json
{ "cmd": "hold_key", "params": { "key": "alt" } }
```

Response:
```json
{ "id": 19, "status": "ok", "result": {} }
```

### release_key (desktop only)

```json
{ "cmd": "release_key", "params": { "key": "alt" } }
```

Response:
```json
{ "id": 20, "status": "ok", "result": {} }
```

### press_key (desktop only)

```json
{ "cmd": "press_key", "params": { "key": "enter" } }
```

Response:
```json
{ "id": 21, "status": "ok", "result": {} }
```

### right_click (desktop only)

```json
{ "cmd": "right_click", "params": { "x": 540, "y": 960 } }
```

Response:
```json
{ "id": 22, "status": "ok", "result": {} }
```

### middle_click (desktop only)

```json
{ "cmd": "middle_click", "params": { "x": 540, "y": 960 } }
```

Response:
```json
{ "id": 23, "status": "ok", "result": {} }
```

### mouse_scroll (desktop only)

Full:
```json
{ "cmd": "mouse_scroll", "params": { "x": 540, "y": 960, "dx": 0, "dy": -120 } }
```

Minimal:
```json
{ "cmd": "mouse_scroll", "params": { "x": 540, "y": 960 } }
```

Response:
```json
{ "id": 24, "status": "ok", "result": {} }
```

---

## Error Codes

| Status | Meaning |
|--------|---------|
| `ok` | Command executed successfully |
| `error` | Command failed — see `error` field for details |

Common error strings: `"phone is locked"`, `"no active window"`, `"no focused input"`, `"accessibility service not enabled"`, `"command timed out"`.

## Rate Limits

- Max 10 commands/second per user
- Max 1 screenshot/second per user
- Max pending (unacked) commands: 50
- Command payload max: 1MB
