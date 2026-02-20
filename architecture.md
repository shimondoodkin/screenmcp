# PhoneMCP — Architecture

## Services Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                    server10.doodkin.com                          │
│                                                                  │
│  ┌──────────────┐                 ┌────────────────────────────┐ │
│  │   Web API     │                │  Rust WS Worker             │ │
│  │   (Next.js)   │                │  (phone + controller conns) │ │
│  │               │                │                              │ │
│  │  - Landing    │   HTTP verify  │  - Holds phone WS            │ │
│  │  - Dashboard  │◄──────────────►│  - Routes commands           │ │
│  │  - Login      │                │  - Session resumption (Redis)│ │
│  │  - API keys   │                │  - Self-registers on start   │ │
│  │  - Discovery  │                │                              │ │
│  │  - Devices    │                │  :8080 (internal)            │ │
│  │  :3000 (int)  │                │  :8443 (WSS via socat)       │ │
│  └──────┬───────┘                └────────┬──────────────────┘  │
│         │                                  │                     │
│         └──────────┬───────────────────────┘                     │
│                    │                                              │
│             ┌──────┴──────┐                                      │
│             │  Redis      │                                      │
│             │  + Postgres │                                      │
│             │             │                                      │
│             │ - Sessions  │                                      │
│             │ - Workers   │                                      │
│             │ - Users     │                                      │
│             │ - Devices   │                                      │
│             │ - Cmd queue │                                      │
│             └─────────────┘                                      │
│                                                                  │
│             ┌─────────────┐                                      │
│             │  Firebase   │                                      │
│             │  Auth       │                                      │
│             │  (Google)   │                                      │
│             └─────────────┘                                      │
└─────────────────────────────────────────────────────────────────┘
```

## Auth Flow — Google Login via Firebase

Firebase Auth handles Google Sign-In for both phone app and website. The server never sees Google credentials — it only verifies Firebase ID tokens.

Dual auth system (resolved by `web/src/lib/resolve-auth.ts`):
- **Firebase ID tokens**: verified via Firebase Admin SDK
- **API keys**: `pk_` + 64 hex chars, stored as SHA-256 hash in `api_keys` table

### Website (Next.js)

```
Browser                    Firebase             Next.js API           Postgres
  │                           │                     │                    │
  ├── Google Sign-In popup ──►│                     │                    │
  │◄── Firebase ID token ────┤                     │                    │
  │                           │                     │                    │
  ├── POST /api/auth/login ─────────────────────►│                    │
  │   { idToken }             │                     │                    │
  │                           │     verify token ──►│                    │
  │                           │◄── uid, email ──────┤                    │
  │                           │                     ├── upsert user ───►│
  │                           │                     │◄── user record ───┤
  │◄── { session cookie } ───────────────────────┤                    │
  │                           │                     │                    │
```

### Phone App (Android)

```
Phone App                  Firebase             Discovery API         Worker WS
  │                           │                     │                    │
  ├── Google Sign-In ────────►│                     │                    │
  │◄── Firebase ID token ────┤                     │                    │
  │                           │                     │                    │
  ├── POST /api/discover ──────────────────────►│                    │
  │   Authorization: Bearer {idToken|apiKey}      │                    │
  │                           │     verify token ──►│                    │
  │◄── { wsUrl } ─────────────────────────────────┤                    │
  │                           │                     │                    │
  ├── WS connect ───────────────────────────────────────────────────►│
  │   { type: "auth", token, role: "phone", last_ack: 5 }          │
  │                           │                     │   verify via API──►│
  │◄── { type: "auth_ok" } ────────────────────────────────────────┤
  │                           │                     │                    │
```

### Token Chain
```
Google credentials
    → Firebase ID token (short-lived, ~1hr)
        → Used directly for WS auth (worker verifies via POST /api/auth/verify)

Alternative:
    API key (pk_...) → worker verifies via POST /api/auth/verify → resolves to user
```

## Firebase Setup Requirements

1. Create Firebase project
2. Enable Google Sign-In provider in Firebase Console → Authentication
3. Add Android app (package: com.phonemcp.app, SHA-1 fingerprint)
4. Add Web app (for Next.js dashboard)
5. Download `google-services.json` → Android app
6. Copy web config → `web/.env.local` (NEXT_PUBLIC_FIREBASE_* vars)

## Data Models (Postgres)

### users
```sql
CREATE TABLE users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    firebase_uid TEXT UNIQUE NOT NULL,
    email TEXT NOT NULL,
    name TEXT,
    created_at TIMESTAMPTZ DEFAULT now(),
    plan TEXT DEFAULT 'free'
);
```

### workers
```sql
CREATE TABLE workers (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    domain TEXT UNIQUE NOT NULL,
    active BOOLEAN DEFAULT true,
    connection_count INT DEFAULT 0,
    region TEXT,
    created_at TIMESTAMPTZ DEFAULT now()
);
```

### devices
```sql
CREATE TABLE devices (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID REFERENCES users(id),
    device_name TEXT NOT NULL DEFAULT 'default',
    device_model TEXT,
    fcm_token TEXT,
    last_seen TIMESTAMPTZ,
    worker_id UUID REFERENCES workers(id),
    connected BOOLEAN DEFAULT false,
    created_at TIMESTAMPTZ DEFAULT now(),
    UNIQUE(user_id, device_name)
);
```

### api_keys
```sql
CREATE TABLE api_keys (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID REFERENCES users(id),
    key_hash TEXT NOT NULL,
    key_prefix TEXT NOT NULL,
    name TEXT,
    created_at TIMESTAMPTZ DEFAULT now(),
    last_used TIMESTAMPTZ
);
```

## Redis Keys

```
# Worker status (for discovery)
worker:{id}:status        → "ready" | "draining"
worker:{id}:connections   → 142

# User sessions (for WS routing)
user:{uid}:server         → "worker-3"
user:{uid}:pending        → [cmd7, cmd8, ...]
user:{uid}:last_ack       → 6
user:{uid}:cmd_counter    → 13

# Session tokens
session:{token}           → { user_id, device_id }  TTL 24h
```

## Implementation Status

### Phase 1 — Android App ✓
- [x] Accessibility service (PhoneMcpService)
- [x] Screenshot, click, long_click, drag, scroll, type, clipboard, UI tree
- [x] Camera capture (front/rear)
- [x] Phone lock detection
- [x] WebP screenshot format with quality/scaling params
- [x] Test UI (MainActivity)

### Phase 2 — Firebase Auth ✓
- [x] Firebase project, Google Sign-In enabled
- [x] Firebase in Android app (google-services.json)
- [x] Google Sign-In screen in phone app
- [x] Next.js project setup
- [x] Firebase web auth (Google Sign-In on website)
- [x] Server-side token verification
- [x] User table in Postgres
- [x] API key auth system (pk_ prefix, SHA-256 hash)

### Phase 3 — Website Dashboard ✓
- [x] Login/signup flow
- [x] Dashboard (devices, API keys CRUD)

### Phase 4 — WebSocket + Workers ✓
- [x] Rust WS server (phone + controller connections)
- [x] Discovery API (POST /api/discover, least-loaded worker)
- [x] Protocol implementation (see protocol.md)
- [x] Session resumption via Redis
- [x] Phone app WS client + auto-reconnect
- [x] Remote CLI client (TypeScript)
- [x] Docker Compose deployment

### Phase 5 — MCP Integration
- [ ] MCP endpoint (SSE / streamable HTTP)
- [ ] Route MCP commands → phone → results back

### Phase 6 — Payments & Polish
- [ ] PayPal subscription integration
- [ ] Rate limiting per plan
- [ ] Usage tracking
- [ ] Landing page, docs, use cases
