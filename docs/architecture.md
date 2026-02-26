# ScreenMCP — Architecture

## Services Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                         Local Machine                            │
│                                                                  │
│  ┌──────────────┐                 ┌────────────────────────────┐ │
│  │  MCP Server   │                │  Rust WS Worker             │ │
│  │  (Node.js)    │                │  (phone + controller conns) │ │
│  │               │                │                              │ │
│  │  - MCP tools  │   reads        │  - Holds phone WS            │ │
│  │  - Device reg │◄─────────────►│  - Routes commands           │ │
│  │  - SSE events │  worker.toml   │  - In-memory cmd queue       │ │
│  │  - Discovery  │                │  - Token verify from TOML    │ │
│  │               │                │                              │ │
│  │  :3000        │                │  :8080                       │ │
│  └──────┬───────┘                └────────┬──────────────────┘  │
│         │                                  │                     │
│         └──────────┬───────────────────────┘                     │
│                    │                                              │
│             ┌──────┴──────┐                                      │
│             │ worker.toml │                                      │
│             │             │                                      │
│             │ - user.id   │                                      │
│             │ - api_keys  │                                      │
│             │ - devices   │                                      │
│             │ - server    │                                      │
│             └─────────────┘                                      │
└─────────────────────────────────────────────────────────────────┘
```

## Auth Flow

Auth is based on a shared secret (`user.id`) and optional API keys, all stored in `~/.screenmcp/worker.toml`.

```toml
[user]
id = "my-secret-token"

[auth]
api_keys = ["pk_abc123", "pk_def456"]
```

Both `user.id` and any `api_keys` entry are accepted as Bearer tokens by the worker and mcp-server.

### Phone/Desktop App

```
Phone/Desktop               MCP Server              Worker WS
  │                            │                        │
  ├── POST /api/devices/register ──►│                   │
  │   Authorization: Bearer {user.id}                   │
  │◄── { device_number: 1 } ──┤                        │
  │                            │                        │
  │   (listens on SSE)         │                        │
  ├── GET /api/events ────────►│                        │
  │   Authorization: Bearer ..  │                        │
  │◄── SSE stream ────────────┤                        │
  │                            │                        │
  │   ... SSE: { type: "connect", wsUrl, target_device_id } ...  │
  │                            │                        │
  ├── WS connect ─────────────────────────────────────►│
  │   { "type":"auth", "user_id":"my-secret-token",   │
  │     "role":"phone", "device_id":"a1b2..." }        │
  │                            │   verify against TOML──►│
  │◄── { "type":"auth_ok" } ──────────────────────────┤
  │                            │                        │
```

### Controller (MCP Client / CLI)

```
MCP Client                  MCP Server              Worker WS
  │                            │                        │
  ├── POST /api/discover ─────►│                        │
  │   { device_id: "a1b2..." } │                        │
  │   Authorization: Bearer ..  │── SSE: connect event──►│ (to phone)
  │◄── { wsUrl } ─────────────┤                        │
  │                            │                        │
  ├── WS connect ─────────────────────────────────────►│
  │   { "type":"auth", "key":"pk_abc123",              │
  │     "role":"controller", "target_device_id":"a1b2.."│
  │◄── { "type":"auth_ok", "phone_connected": true } ──┤
  │                            │                        │
  │── MCP tool calls (via Streamable HTTP) ───────────►│
  │                            │                        │
```

### Token Chain
```
~/.screenmcp/worker.toml
    → user.id used as Bearer token (phones/desktops)
    → auth.api_keys used as Bearer token (controllers/MCP clients)
    → Worker and MCP Server both verify locally against the same file
```

## Config File (`~/.screenmcp/worker.toml`)

```toml
[user]
id = "local-user"

[auth]
api_keys = ["pk_abc123"]

[devices]
allowed = ["hexid1 My Phone", "hexid2 My Desktop"]   # "device_id optional_name" format

[server]
port = 3000
worker_url = "ws://localhost:8080"
```

- **user.id**: Shared secret used by phones/desktops as Bearer token
- **auth.api_keys**: Tokens for controllers and MCP clients
- **devices.allowed**: Registered devices. Format is `"hex_device_id Optional Description"`. Empty list = accept all devices (worker) or no devices registered yet (mcp-server)
- **server.port**: MCP server listen port
- **server.worker_url**: WebSocket URL of the worker

Device registration via `POST /api/devices/register` persists back to this file.

## Implementation Status

### Android App ✓
- [x] Accessibility service (ScreenMcpService)
- [x] Screenshot, click, long_click, drag, scroll, type, clipboard, UI tree
- [x] Camera capture (front/rear)
- [x] Phone lock detection
- [x] WebP screenshot format with quality/scaling params
- [x] Open Source Server mode (SSE + user_id auth)

### Desktop Clients ✓
- [x] Windows — Win32 APIs for ui_tree, system tray
- [x] macOS — CoreGraphics for ui_tree, Cmd shortcuts
- [x] Linux — wmctrl/xdotool for ui_tree, Ctrl shortcuts
- [x] All: Open Source Server settings (checkbox + user_id + API URL)
- [x] All: SSE event listening with target_device_id filtering

### Worker ✓
- [x] Rust WS server (phone + controller connections)
- [x] Protocol implementation (see protocol.md)
- [x] File backend: auth from TOML, in-memory state
- [x] API backend: auth via web API, state in Redis (--features api)

### MCP Server ✓
- [x] MCP endpoint (Streamable HTTP)
- [x] Route MCP commands → phone → results back
- [x] `list_devices` tool — discover registered devices
- [x] All phone tools require explicit `device_id` (integer device_number)
- [x] Device registration API (persists to TOML)
- [x] SSE event notifications with target_device_id
- [x] Discovery endpoint (POST /api/discover)

### Remote CLI ✓
- [x] TypeScript CLI client + library
- [x] Interactive REPL shell mode
- [x] Worker discovery via API
