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
cd worker && cargo build --release    # build
cd worker && cargo run                # dev run on :8080
```

### Remote CLI Client
```bash
cd remote && npm install && npm run build    # compile TS
cd remote && npm run dev -- screenshot       # run directly via tsx
npx tsc --noEmit                             # type-check only
```

## Architecture

```
Phone (Android) ←──WSS──→ Worker (Rust) ←──HTTP──→ Web API (Next.js) ←→ Postgres
                              ↑                          ↑
                         CLI Client (TS)              Redis
```

**Web** (`web/`) — Next.js API server. Handles auth, device/worker registry, API key CRUD, worker discovery. Dashboard at `/dashboard`.

**Worker** (`worker/`) — Rust tokio WebSocket relay. Phones and CLI controllers connect here. Routes commands between controllers and phones. Self-registers with the web API on startup. Stores pending commands in Redis for session resumption.

**App** (`app/`) — Android Kotlin app. Runs an AccessibilityService to execute UI automation (taps, drags, screenshots, text input, camera). Connects to worker via WebSocket.

**Remote** (`remote/`) — TypeScript CLI client + library. Discovers worker via API, connects via WebSocket, sends commands. Has interactive REPL shell mode.

## Auth System

Two auth methods, resolved by `web/src/lib/resolve-auth.ts`:
- **Firebase ID tokens** — verified via Firebase Admin SDK
- **API keys** — `pk_` + 64 hex chars, stored as SHA-256 hash in `api_keys` table. Only Firebase-authed users can create/manage API keys.

Worker verifies tokens by calling `POST /api/auth/verify` on the web API.

## Connection Flow

1. Client calls `POST /api/discover` → gets least-loaded worker URL
2. Client opens WebSocket to worker, sends `{type: "auth", token, role, last_ack}`
3. Worker verifies token via web API, registers connection in memory
4. Commands flow: controller → worker (Redis queue) → phone → response back
5. On reconnect, phone sends `last_ack` and worker replays unacked commands from Redis

## Key Stability Patterns

- **Connection generation counter** in Android WebSocket client prevents stale callbacks from triggering reconnect loops
- **Worker replaces** (not rejects) duplicate phone connections — drops old channel seamlessly
- **3-second cooldown** between disconnect/reconnect for same device_id
- **Heartbeat**: server pings every 30s, drops after 60s no pong

## Deployment

- **Server**: server10.doodkin.com
- **SSL**: socat terminates TLS on :443 → localhost:3000 and :8443 → localhost:8080
- **Certs**: Let's Encrypt at `/etc/letsencrypt/live/server10.doodkin.com/`
- **Start**: `screen -dmS phonemcp bash start-server.sh`
- **Secrets** (not in git): `.env` (Docker Compose vars), `web/.env.local` (Firebase + PayPal keys), `firebase-service-account.json`, `app/google-services.json`
- Docker ports bound to 127.0.0.1 only; socat handles public access

## Database

Schema in `db/init.sql`. Tables: `users`, `workers`, `devices`, `api_keys`. All IDs are UUID v4. Auto-initialized on first Docker Compose run.

## Supported Phone Commands

`screenshot`, `click`, `long_click`, `drag`, `scroll`, `type`, `get_text`, `select_all`, `copy`, `paste`, `back`, `home`, `recents`, `ui_tree`, `camera`. Unsupported PC-style commands (`right_click`, `middle_click`, `mouse_scroll`) return `{status: "ok", unsupported: true}`.
