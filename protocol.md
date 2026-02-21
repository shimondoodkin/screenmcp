# ScreenMCP Protocol

## Overview

Phone connects to a WebSocket server, receives commands, executes them, and sends results back. Sessions are resumable across server changes and disconnects.

## Connection Flow

```
Phone/CLI                 Discovery API              WS Server             Redis
  │                            │                        │                    │
  ├── POST /api/discover ─────►│                        │                    │
  │   Authorization: Bearer ..  ├── find least-loaded ──►│                    │
  │◄── { wsUrl } ─────────────┤                        │                    │
  │                            │                        │                    │
  ├── WS connect ─────────────────────────────────────►│                    │
  ├── { "type":"auth", "token":"...",                  │                    │
  │     "role":"phone", "device_id":"a1b2...",         │                    │
  │     "last_ack": 5 } ─────────────────────────────►│                    │
  │                            │                        ├── verify via API──►│
  │                            │                        ├── SET user:server──►│
  │                            │                        ├── GET pending cmds─►│
  │◄── replay commands id > 5 ─────────────────────────┤                    │
  │                            │                        │                    │
```

## Message Format

All messages are JSON over WebSocket.

### Server → Phone (Commands)

```json
{
  "id": 7,
  "cmd": "click",
  "params": { "x": 540, "y": 1200 }
}
```

### Phone → Server (Responses)

```json
{
  "id": 7,
  "status": "ok",
  "result": {}
}
```

```json
{
  "id": 7,
  "status": "error",
  "error": "no active window"
}
```

### Phone → Server (ACK shorthand)

```json
{ "ack": 7 }
```

## Commands

### click
```json
{ "id": 1, "cmd": "click", "params": { "x": 540, "y": 1200 } }
{ "id": 1, "cmd": "click", "params": { "x": 540, "y": 1200, "duration": 500 } }
```
Optional `duration` in ms (default 100). Use higher values for long-press effects.

### long_click
```json
{ "id": 2, "cmd": "long_click", "params": { "x": 540, "y": 1200 } }
```
Fixed 1000ms press duration.

### drag
```json
{ "id": 3, "cmd": "drag", "params": { "startX": 540, "startY": 1200, "endX": 540, "endY": 400, "duration": 300 } }
```

### scroll
```json
{ "id": 4, "cmd": "scroll", "params": { "x": 540, "y": 1200, "dx": 0, "dy": -300 } }
```
Finger-drag gesture from (x,y) to (x+dx, y+dy). Use negative dy to scroll content up.

### type
```json
{ "id": 5, "cmd": "type", "params": { "text": "hello world" } }
```
Appends text to currently focused input field.

### select_all / copy / paste
```json
{ "id": 6, "cmd": "select_all" }
{ "id": 7, "cmd": "copy" }
{ "id": 8, "cmd": "paste" }
```

### get_text
```json
{ "id": 9, "cmd": "get_text" }
→ { "id": 9, "status": "ok", "result": { "text": "field contents" } }
```

### screenshot
```json
{ "id": 10, "cmd": "screenshot" }
{ "id": 10, "cmd": "screenshot", "params": { "quality": 50, "max_width": 720, "max_height": 1280 } }
→ { "id": 10, "status": "ok", "result": { "image": "<base64 webp>" } }
→ { "id": 10, "status": "error", "error": "phone is locked" }
```
Returns WebP image as base64. Default: lossless WebP (quality omitted or 100). Quality 1-99: lossy WebP. Optional `max_width`/`max_height` for scaling (aspect ratio preserved). Returns error if phone is locked (keyguard active).

### ui_tree
```json
{ "id": 11, "cmd": "ui_tree" }
→ { "id": 11, "status": "ok", "result": { "tree": [ ...nodes ] } }
```

### UI Tree Node Format
```json
{
  "className": "EditText",
  "resourceId": "com.example:id/search_input",
  "text": "hello",
  "contentDescription": "Search",
  "bounds": { "left": 0, "top": 100, "right": 1080, "bottom": 200 },
  "clickable": true,
  "editable": true,
  "focused": true,
  "scrollable": false,
  "checkable": false,
  "checked": false,
  "children": [ ... ]
}
```

### back / home / recents
```json
{ "id": 12, "cmd": "back" }
{ "id": 13, "cmd": "home" }
{ "id": 14, "cmd": "recents" }
```

### camera
```json
{ "id": 15, "cmd": "camera" }
{ "id": 15, "cmd": "camera", "params": { "camera": "0", "quality": 80, "max_width": 1280, "max_height": 960 } }
→ { "id": 15, "status": "ok", "result": { "image": "<base64 webp>" } }
```
Camera ID: "0" = rear (default), "1" = front. Returns empty image string if camera not available. Quality default: 80 (lossy). Optional `max_width`/`max_height` for scaling.

### hold_key / release_key / press_key (desktop only)
```json
{ "id": 16, "cmd": "hold_key", "params": { "key": "alt" } }
{ "id": 17, "cmd": "press_key", "params": { "key": "tab" } }
{ "id": 18, "cmd": "release_key", "params": { "key": "alt" } }
```
Desktop keyboard control. `hold_key` presses and holds a key, `release_key` releases it, `press_key` does press+release in one action. Supported key names: `shift`, `ctrl`/`control`, `alt`, `meta`/`cmd`/`win`/`command`/`super`, `tab`, `enter`/`return`, `escape`/`esc`, `space`, `backspace`, `delete`/`del`, `home`, `end`, `pageup`, `pagedown`, `up`, `down`, `left`, `right`, `f1`–`f12`, or any single character. On Android these return `{status: "error"}` (unsupported).

### Unsupported desktop-only commands
These are accepted but return unsupported flag (for cross-platform CLI compatibility):
```json
{ "id": 16, "cmd": "right_click", "params": { "x": 540, "y": 1200 } }
{ "id": 17, "cmd": "middle_click", "params": { "x": 540, "y": 1200 } }
{ "id": 18, "cmd": "mouse_scroll", "params": { "x": 540, "y": 1200, "dx": 0, "dy": -120 } }
→ { "id": 16, "status": "ok", "result": { "unsupported": true } }
```

## Authentication

### WS Auth — Phone
```json
{ "type": "auth", "user_id": "firebase-id-token", "role": "phone", "device_id": "a1b2c3d4e5f67890...", "last_ack": 5 }
→ { "type": "auth_ok" }
→ { "type": "auth_fail", "error": "invalid token" }
```
Phones authenticate with a Firebase ID token in `user_id`. The `device_id` is a cryptographically secure random hex string (32 chars / 128 bits), generated once per client and persisted. It uniquely identifies this device for routing.

### WS Auth — Controller
```json
{ "type": "auth", "key": "pk_...", "role": "controller", "target_device_id": "a1b2c3d4e5f67890...", "last_ack": 0 }
→ { "type": "auth_ok", "phone_connected": true }
```
Controllers authenticate with an API key (`pk_` prefix) in `key`. The `target_device_id` is the device's cryptographic ID (the same value the phone sent as `device_id` when it connected).

Worker verifies tokens by calling `POST /api/auth/verify` on the web API server.

## Session Resumption

### Problem
Phone disconnects (network drop, server drain, deploy). Must not lose commands.

### State in Redis
`{device_id}` below is the client-generated cryptographic hex ID (e.g. `a1b2c3d4e5f67890abcdef1234567890`).
```
device:{device_id}:server       → "worker-uuid"       # which server holds connection
device:{device_id}:pending      → [cmd7, cmd8]        # unacked commands (list)
device:{device_id}:last_ack     → 6                   # last command phone confirmed
device:{device_id}:cmd_counter  → 13                  # next command ID to assign
```

### Reconnect Sequence
1. Phone detects disconnect
2. Phone calls `POST /api/discover` with auth token
3. Discovery returns a WS server URL (skips servers marked "draining")
4. Phone opens WS, sends auth message with `last_ack`
5. Server looks up pending commands in Redis where `id > last_ack`
6. Server replays those commands in order
7. Phone processes and ACKs each one

### Command Lifecycle
```
Controller sends command
    → Worker assigns ID, appends to device:{device_id}:pending in Redis
    → Worker forwards to phone over WS
    → Phone executes, sends response + ack
    → Worker removes from pending, updates last_ack
```

## Server Drain

```
1. Set server status to "draining" in Redis
2. Discovery stops routing new connections to it
3. Drop all WebSocket connections (hard close)
4. Phones auto-reconnect via discovery → land on other servers
5. New server replays unacked commands from Redis
6. Deploy/restart drained server
7. Set status back to "ready"
```

## Heartbeat

```
Server → { "type": "ping" }        every 30s
Phone  → { "type": "pong" }

No pong in 60s → server drops connection, cleans up
No ping in 60s → phone reconnects via discovery
```

## Error Codes

| Code | Meaning |
|------|---------|
| `ok` | Command executed successfully |
| `error` | Command failed (see `error` field) |
| `not_ready` | Accessibility service not enabled |
| `no_focus` | No focused input field for text operations |
| `timeout` | Command took too long to execute |

## Rate Limits

- Max 10 commands/second per user
- Max 1 screenshot/second per user
- Max pending (unacked) commands: 50
- Command payload max: 1MB
