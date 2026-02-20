# PhoneMCP Protocol

## Overview

Phone connects to a WebSocket server, receives commands, executes them, and sends results back. Sessions are resumable across server changes and disconnects.

## Connection Flow

```
Phone                     Discovery API              WS Server             Redis
  │                            │                        │                    │
  ├── GET /discover ──────────►│                        │                    │
  │   (auth token)             ├── find least-loaded ──►│                    │
  │◄── wss://ws-4.example.com─┤                        │                    │
  │                            │                        │                    │
  ├── WS connect ─────────────────────────────────────►│                    │
  ├── { "type":"auth", "token":"...", "last_ack": 5 }─►│                    │
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
```

### long_click
```json
{ "id": 2, "cmd": "long_click", "params": { "x": 540, "y": 1200 } }
```

### drag
```json
{ "id": 3, "cmd": "drag", "params": { "startX": 540, "startY": 1200, "endX": 540, "endY": 400, "duration": 300 } }
```

### type
```json
{ "id": 4, "cmd": "type", "params": { "text": "hello world" } }
```

### select_all
```json
{ "id": 5, "cmd": "select_all" }
```

### copy
```json
{ "id": 6, "cmd": "copy" }
```

### paste
```json
{ "id": 7, "cmd": "paste" }
```

### get_text
```json
{ "id": 8, "cmd": "get_text" }
→ { "id": 8, "status": "ok", "result": { "text": "field contents" } }
```

### screenshot
```json
{ "id": 9, "cmd": "screenshot" }
→ { "id": 9, "status": "ok", "result": { "data": "<base64 png>" } }
```

### get_ui_tree
```json
{ "id": 10, "cmd": "get_ui_tree" }
→ { "id": 10, "status": "ok", "result": { "tree": [ ...nodes ] } }
```

### UI Tree Node Format
```json
{
  "cls": "EditText",
  "id": "search_input",
  "text": "hello",
  "desc": "Search",
  "bounds": [0, 100, 1080, 200],
  "flags": ["click", "edit", "focused"],
  "children": [ ... ]
}
```

### back / home / recents
```json
{ "id": 11, "cmd": "back" }
{ "id": 12, "cmd": "home" }
{ "id": 13, "cmd": "recents" }
```

## Session Resumption

### Problem
Phone disconnects (network drop, server drain, deploy). Must not lose commands.

### State in Redis
```
user:{uid}:server       → "ws-4"           # which server holds connection
user:{uid}:pending      → [cmd7, cmd8]     # unacked commands (list)
user:{uid}:last_ack     → 6                # last command phone confirmed
user:{uid}:cmd_counter  → 13               # next command ID to assign
```

### Reconnect Sequence
1. Phone detects disconnect
2. Phone calls `GET /discover` with auth token
3. Discovery returns a WS server URL (skips servers marked "draining")
4. Phone opens WS, sends auth message with `last_ack`
5. Server looks up pending commands in Redis where `id > last_ack`
6. Server replays those commands in order
7. Phone processes and ACKs each one

### Command Lifecycle
```
Dashboard sends command
    → Server assigns ID, appends to user:{uid}:pending in Redis
    → Server forwards to phone over WS
    → Phone executes, sends response + ack
    → Server removes from pending, updates last_ack
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

## Authentication

### Initial Auth (HTTP)
```
POST /auth/google
{ "google_id_token": "..." }
→ { "token": "jwt-session-token", "expires": 3600 }
```

### WS Auth (on connect)
```json
{ "type": "auth", "token": "jwt-session-token", "last_ack": 5 }
→ { "type": "auth_ok", "resume_from": 6 }
```

```json
{ "type": "auth", "token": "expired-token" }
→ { "type": "auth_fail", "error": "token expired" }
→ server closes connection
```

## Error Codes

| Code | Meaning |
|------|---------|
| `ok` | Command executed successfully |
| `error` | Command failed (see `error` field) |
| `not_ready` | Accessibility service not enabled |
| `no_focus` | No focused input field for text operations |
| `timeout` | Command took too long to execute |

## Binary Messages

Screenshots can be large. Two options:

### Option A: Base64 in JSON (simple, ~33% overhead)
```json
{ "id": 9, "status": "ok", "result": { "data": "<base64>" } }
```

### Option B: Binary frame (efficient)
```
[4 bytes: command ID][rest: raw PNG bytes]
```

Server decides based on negotiation at auth time. Default: base64.

## Rate Limits

- Max 10 commands/second per user
- Max 1 screenshot/second per user
- Max pending (unacked) commands: 50
- Command payload max: 1MB
