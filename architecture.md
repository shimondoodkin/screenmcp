# PhoneMCP — Architecture

## Services Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                        phonemcp.com                              │
│                                                                  │
│  ┌──────────────┐  ┌──────────────┐  ┌────────────────────────┐ │
│  │   Website     │  │  Worker      │  │  Rust WS Server(s)     │ │
│  │   (Next.js)   │  │  Service     │  │  (phone connections)   │ │
│  │               │  │  (subdomain) │  │                        │ │
│  │  - Landing    │  │              │  │  - Holds phone WS      │ │
│  │  - Dashboard  │  │  - MCP/SSE   │  │  - Executes commands   │ │
│  │  - Docs       │  │    endpoint  │  │  - Resumable sessions  │ │
│  │  - Login      │  │  - Routes    │  │                        │ │
│  │  - Account    │  │    commands  │  │  worker1.phonemcp.com  │ │
│  │  - Payments   │  │    to phones │  │  worker2.phonemcp.com  │ │
│  │  - API keys   │  │              │  │  ...                   │ │
│  │  - Devices    │  │              │  │                        │ │
│  │  - APK dl     │  │              │  │                        │ │
│  └──────┬───────┘  └──────┬───────┘  └────────┬───────────────┘ │
│         │                  │                    │                 │
│         └──────────────────┼────────────────────┘                │
│                            │                                     │
│                     ┌──────┴──────┐                              │
│                     │   Redis     │                              │
│                     │  + Postgres │                              │
│                     │             │                              │
│                     │ - Sessions  │                              │
│                     │ - Workers   │                              │
│                     │ - Users     │                              │
│                     │ - Devices   │                              │
│                     │ - Cmd queue │                              │
│                     └─────────────┘                              │
│                                                                  │
│                     ┌─────────────┐                              │
│                     │  Firebase   │                              │
│                     │  Auth       │                              │
│                     │  (Google)   │                              │
│                     └─────────────┘                              │
└─────────────────────────────────────────────────────────────────┘
```

## Auth Flow — Google Login via Firebase

Firebase Auth handles Google Sign-In for both phone app and website. The server never sees Google credentials — it only verifies Firebase ID tokens.

### Website (Next.js)

```
Browser                    Firebase             Next.js API           Postgres
  │                           │                     │                    │
  ├── Google Sign-In popup ──►│                     │                    │
  │◄── Firebase ID token ────┤                     │                    │
  │                           │                     │                    │
  ├── POST /api/auth/login ──────────────────────►│                    │
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
  ├── GET /api/discover ─────────────────────────►│                    │
  │   Authorization: Bearer {idToken}              │                    │
  │                           │     verify token ──►│                    │
  │◄── { wsUrl, sessionToken } ──────────────────┤                    │
  │                           │                     │                    │
  ├── WS connect ───────────────────────────────────────────────────►│
  │   { type: "auth", token: sessionToken }        │                    │
  │◄── { type: "auth_ok" } ────────────────────────────────────────┤
  │                           │                     │                    │
```

### Token Chain
```
Google credentials
    → Firebase ID token (short-lived, ~1hr)
        → Session token (server-issued JWT, for WS auth)
            → Redis session (maps to user + device)
```

## Firebase Setup Requirements

1. Create Firebase project
2. Enable Google Sign-In provider in Firebase Console → Authentication
3. Add Android app (package: com.phonemcp.app, SHA-1 fingerprint)
4. Add Web app (for Next.js dashboard)
5. Download `google-services.json` → Android app
6. Copy web config → Next.js env vars

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

### devices
```sql
CREATE TABLE devices (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID REFERENCES users(id),
    device_name TEXT,
    device_model TEXT,
    last_seen TIMESTAMPTZ,
    worker_id UUID REFERENCES workers(id),
    connected BOOLEAN DEFAULT false,
    created_at TIMESTAMPTZ DEFAULT now()
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

## Implementation Order

### Phase 1 — Done ✓
- [x] Android accessibility service
- [x] Screenshot, click, drag, type, clipboard, UI tree
- [x] Test UI (MainActivity)

### Phase 2 — Firebase Auth
- [ ] Create Firebase project, enable Google Sign-In
- [ ] Add Firebase to Android app (google-services.json)
- [ ] Google Sign-In screen in phone app
- [ ] Next.js project setup
- [ ] Firebase web auth (Google Sign-In on website)
- [ ] Server-side token verification
- [ ] User table in Postgres

### Phase 3 — Website Dashboard
- [ ] Landing page
- [ ] Login/signup flow
- [ ] Dashboard (account, devices, API keys)
- [ ] APK download page
- [ ] Documentation pages

### Phase 4 — WebSocket + Workers
- [ ] Rust WS server (phone connections)
- [ ] Discovery API
- [ ] Protocol implementation (see protocol.md)
- [ ] Session resumption
- [ ] Phone app WS client + auto-reconnect

### Phase 5 — MCP Integration
- [ ] MCP endpoint on worker (SSE / streamable HTTP)
- [ ] API key auth for MCP clients
- [ ] Route MCP commands → phone → results back

### Phase 6 — Payments & Polish
- [ ] Stripe integration
- [ ] Rate limiting per plan
- [ ] Usage tracking
- [ ] Audit logs
