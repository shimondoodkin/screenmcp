# ScreenMCP Initiation Protocol

## Overview

How phones and controllers discover, connect, authenticate, and maintain sessions with the WebSocket server. Sessions are resumable across server changes and disconnects.

For the wire protocol (command format and full command catalog), see [wire-protocol.md](wire-protocol.md).

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

See [wire-protocol.md](wire-protocol.md) for command wire format and examples, and [commands.md](commands.md) for the full param reference.

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

## SSE Notifications (Worker)

Devices can connect to the worker via Server-Sent Events to receive push notifications (e.g. "connect" events from discover). This replaces the MCP-server's broadcast-to-all model with targeted per-device delivery.

### GET /events — SSE stream

```
GET /events?device_id=a1b2c3d4 HTTP/1.1
Authorization: Bearer <token>
```

- **Auth**: Bearer token verified against TOML config (`user.id` or `api_keys`). Device ID verified against allowed list.
- **Response**: `text/event-stream` with `Cache-Control: no-cache`, `Connection: keep-alive`.
- One SSE connection per device_id. A new connection replaces the old one (same as phone WebSocket behavior).
- Initial event on connect:
  ```
  data: {"type":"connected","timestamp":1234567890}
  ```
- Heartbeat comment every 30s: `: heartbeat`
- Events pushed via `/notify` arrive as:
  ```
  data: {"device_id":"a1b2c3d4","type":"connect","wsUrl":"ws://...","target_device_id":"a1b2c3d4","timestamp":1234567890}
  ```

### POST /notify — push event to device

```
POST /notify HTTP/1.1
Content-Type: application/json
Authorization: Bearer <notify_secret>

{
  "device_id": "a1b2c3d4",
  "type": "connect",
  "wsUrl": "ws://localhost:8080",
  "target_device_id": "a1b2c3d4"
}
```

- **Auth**: If `notify_secret` is configured (in `[auth].notify_secret` in worker.toml or `NOTIFY_SECRET` env var), the request must include `Authorization: Bearer <notify_secret>`. Returns 401 if the secret doesn't match. If not configured, no auth is required (backwards compat / dev).
- Adds `timestamp` if not present in the request body.
- Returns `200 {"ok":true}` if the device has an active SSE connection and the event was delivered.
- Returns `404 {"error":"device not connected"}` if no SSE client is connected for that device_id.

### SSE Notification Flow

```
MCP-server                        Worker                         Phone/Desktop
    │                                │                                │
    │  POST /notify {device_id, ..}  │                                │
    ├───────────────────────────────►│                                │
    │◄── 200 {"ok":true} ───────────┤  data: {event}\n\n             │
    │                                ├───────────────────────────────►│
    │                                │                                │
    │                                │     (device connects to WS)    │
    │                                │◄──── WS auth ─────────────────┤
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
