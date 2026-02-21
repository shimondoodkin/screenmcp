# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Run Commands

### Docker (full stack)
```bash
docker compose up --build          # all services
docker compose logs web --tail 50  # view web logs
docker compose restart web         # restart single service
```

### Android App
```bash
./gradlew.bat assembleDebug                                    # build (Windows)
adb install -r app/build/outputs/apk/debug/app-debug.apk      # install
```

### Web (Next.js API Server)
```bash
cd web && npm install && npm run dev    # dev server on :3000
cd web && npm run build                 # production build
```

### Worker (Rust WebSocket Relay)
```bash
cd worker && cargo build --release              # build (file backend, no Redis needed)
cd worker && cargo build --release --features api  # build with API backend (Redis + web API)
cd worker && cargo run                          # dev run on :8080
```

### MCP Server (Open Source API)
```bash
cd mcp-server && npm install && npm run build   # compile TS
cd mcp-server && npm run start                  # run on :3000
cd mcp-server && npx tsc --noEmit               # type-check only
```

### Remote CLI Client
```bash
cd remote && npm install && npm run build    # compile TS
cd remote && npm run dev -- screenshot       # run directly via tsx
npx tsc --noEmit                             # type-check only
```

## Architecture

### Cloud Mode (Firebase + Postgres)
```
Phone (Android) ←──WSS──→ Worker (Rust, --features api) ←──HTTP──→ Web API (Next.js) ←→ Postgres
                              ↑                                          ↑
                         CLI Client (TS)                              Redis
```

### Open Source Mode (single-user, no infrastructure)
```
Phone/Desktop ←──WSS──→ Worker (Rust, default) ←── reads ──→ ~/.screenmcp/worker.toml
                              ↑
                    MCP Server (Node.js) ←── reads ──→ ~/.screenmcp/worker.toml
                         ↑
                    SSE events + MCP tools
```

**Web** (`web/`) — Next.js API server. Handles auth, device/worker registry, API key CRUD, worker discovery. Dashboard at `/dashboard`. Uses Firebase + Postgres.

**MCP Server** (`mcp-server/`) — Lightweight Node.js HTTP server for open source mode. MCP tools over Streamable HTTP, device registration API, SSE event notifications. No database, no Firebase — reads `~/.screenmcp/worker.toml` for auth and devices. Persists device registration back to the TOML file.

**Worker** (`worker/`) — Rust tokio WebSocket relay. Phones and CLI controllers connect here. Routes commands between controllers and phones. Has two backend modes selected via Cargo features:
- **Default (file backend)**: Auth from TOML config, in-memory state. Zero external dependencies.
- **`--features api`**: Auth via web API, state in Redis. Used with the cloud deployment.

**App** (`app/`) — Android Kotlin app. Runs an AccessibilityService to execute UI automation (taps, drags, screenshots, text input, camera). Connects to worker via WebSocket. Supports "Open Source Server" mode (SSE instead of FCM).

**Remote** (`remote/`) — TypeScript CLI client + library. Discovers worker via API, connects via WebSocket, sends commands. Has interactive REPL shell mode.

**Desktop Clients** — Rust system tray apps that connect as "phone" devices for desktop control:
- `windows/` — Windows (uses Win32 APIs for ui_tree)
- `mac/` — macOS (uses CoreGraphics for ui_tree, Cmd shortcuts)
- `linux/` — Linux (uses wmctrl/xdotool for ui_tree, Ctrl shortcuts)

All clients support "Open Source Server" mode via settings: `opensource_server_enabled`, `opensource_user_id`, `opensource_api_url`.

## Auth System

### Cloud Mode (web/)
Two auth methods, resolved by `web/src/lib/resolve-auth.ts`:
- **Firebase ID tokens** — verified via Firebase Admin SDK
- **API keys** — `pk_` + 64 hex chars, stored as SHA-256 hash in `api_keys` table. Only Firebase-authed users can create/manage API keys.

Worker (with `--features api`) verifies tokens by calling `POST /api/auth/verify` on the web API.

### Open Source Mode (mcp-server/)
Auth from `~/.screenmcp/worker.toml`:
- **user.id** — shared secret, acts as both user identifier and auth token for phones
- **auth.api_keys** — list of API keys for controllers/MCP clients
- Both `user.id` and any `api_keys` entry are accepted as Bearer tokens

Worker (default build) verifies tokens locally against the same TOML file.

## Connection Flow

### Cloud Mode
1. Client calls `POST /api/discover {device_id}` → gets least-loaded worker URL, FCM sent to target device
2. Client opens WebSocket to worker, sends `{type: "auth", token, role, device_id, last_ack}`
3. Worker verifies token via web API, routes by `device_id`
4. Commands flow: controller → worker (Redis queue) → phone → response back
5. On reconnect, phone sends `last_ack` and worker replays unacked commands from Redis

### Open Source Mode
1. Client calls `POST /api/discover {device_id}` → gets worker URL, SSE event sent to target device
2. Phone/desktop listens on `GET /api/events` (SSE) for `{type: "connect", wsUrl, target_device_id}`
3. Device checks `target_device_id` matches its own ID, connects to worker at `wsUrl`
4. Worker verifies token against TOML config, routes by `device_id`
5. Commands flow: controller → worker (in-memory queue) → phone → response back

## Config File (`~/.screenmcp/worker.toml`)

Used by both the worker (file backend) and mcp-server:
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

Device registration via API persists back to this file. Empty `allowed` list = accept all devices (worker) or no devices registered yet (mcp-server).

## Key Stability Patterns

- **Connection generation counter** in Android WebSocket client prevents stale callbacks from triggering reconnect loops
- **Worker replaces** (not rejects) duplicate phone connections — drops old channel seamlessly
- **3-second cooldown** between disconnect/reconnect for same device_id
- **Heartbeat**: server pings every 30s, drops after 60s no pong

## Deployment

- **Server**: server10.doodkin.com
- **SSL**: socat terminates TLS on :443 → localhost:3000 and :8443 → localhost:8080
- **Certs**: Let's Encrypt at `/etc/letsencrypt/live/server10.doodkin.com/`
- **Start**: `screen -dmS screenmcp bash start-server.sh`
- **Secrets** (not in git): `.env` (Docker Compose vars), `web/.env.local` (Firebase + PayPal keys), `firebase-service-account.json`, `app/google-services.json`
- Docker ports bound to 127.0.0.1 only; socat handles public access

## Open Source Client Settings

All clients (Android, Windows, Mac, Linux) have an "Open Source Server" checkbox in their UI. When enabled, two fields appear:
- **User ID** — the `user.id` from `worker.toml`, used as Bearer token
- **API Server URL** — the mcp-server's URL

Setting names (consistent across all clients):
- `opensource_server_enabled` (bool)
- `opensource_user_id` (string)
- `opensource_api_url` (string)

When enabled: auth uses `opensource_user_id` as Bearer token, SSE replaces FCM/push for "connect" events, Firebase sign-in is skipped.

## Database (Cloud Mode Only)

Schema in `db/init.sql`. Tables: `users`, `workers`, `devices`, `api_keys`, `usage_logs`. Device IDs are client-generated cryptographic hex strings (128 bits). `devices` has a `device_number` column (integer, user-facing, used in MCP). Auto-initialized on first Docker Compose run. Not used in open source mode.

## Supported Phone Commands

`screenshot`, `click`, `long_click`, `drag`, `scroll`, `type`, `get_text`, `select_all`, `copy`, `paste`, `back`, `home`, `recents`, `ui_tree`, `camera`. Desktop-only keyboard commands: `hold_key`, `release_key`, `press_key` (PC/Mac/Linux). Unsupported desktop-only commands (`right_click`, `middle_click`, `mouse_scroll`) return `{status: "ok", unsupported: true}`.
