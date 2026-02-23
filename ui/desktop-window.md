# Desktop Window & Auth Flow

Specification for the desktop client window and browser-based Google sign-in. This must be implemented in all desktop clients (Windows, macOS, Linux).

## Overview

Desktop clients run as system tray apps with:

1. **Google Sign-In callback** — local HTTP server receives auth token from browser redirect
2. **Native Test Window** — egui/eframe window with device control buttons (like Android's main screen)

## Local HTTP Server

Started on app launch. Binds to `127.0.0.1:0` (random port). Handles auth callback only:

| Route | Method | Purpose |
|-------|--------|---------|
| `/callback?token=pk_...&email=user@gmail.com` | GET | Google sign-in callback |

The test window is a native egui window (not browser-based), so no HTTP routes are needed for it.

## Google Sign-In Flow (Browser Redirect)

Desktop clients cannot use Firebase SDK directly (no webview). Instead:

```
Desktop App                    Browser                     screenmcp.com
    │                             │                              │
    │  1. Start localhost:PORT    │                              │
    │  2. Open browser ──────────>│                              │
    │                             │  3. Navigate to              │
    │                             │     /auth/desktop?port=PORT ─>│
    │                             │                              │  4. Show Google sign-in
    │                             │                              │  5. User signs in
    │                             │                              │  6. Create API key (pk_...)
    │                             │  7. Redirect to              │
    │                             │     localhost:PORT/callback   │
    │  8. Receive token <─────────│     ?token=pk_...&email=...  │
    │  9. Save to config.toml    │                              │
    │  10. Show success           │                              │
```

### Implementation Steps

1. **Start local server** at app launch (random port, bound to 127.0.0.1 only).
2. **"Sign in with Google"** tray menu item → calls `open::that()` to open:
   ```
   https://screenmcp.com/auth/desktop?port={PORT}
   ```
3. **Website page** (`/auth/desktop`) shows Google sign-in button, creates API key via `POST /api/keys`, redirects to:
   ```
   http://localhost:{PORT}/callback?token={API_KEY}&email={EMAIL}
   ```
4. **Local server** receives the callback:
   - Parses `token` and `email` from query string
   - Saves `token` and `email` to config.toml
   - Serves success HTML page ("Signed in! You can close this tab.")
   - Sends event to tray to update UI
5. **Tray** updates to show "Signed in: user@gmail.com" and refreshes config.

### Security Notes

- Local server only binds to `127.0.0.1` (localhost) — not accessible from network.
- Token is a persistent API key (`pk_...`), not a short-lived Firebase token.
- Browser → localhost redirect is safe (only local machine receives it).

## Native Test Window

A native egui/eframe window opened via "Test Window" tray menu item. Spawned on a separate thread. Provides the same device control buttons as the Android main screen.

### Layout

```
┌───────────────────────────────────┐
│  ScreenMCP Test Window            │
│                                   │
│  ─── Screenshot ───────────────── │
│  [ Take Screenshot ]              │
│  ┌─────────────────────────────┐  │
│  │      (preview image)        │  │
│  └─────────────────────────────┘  │
│                                   │
│  ─── Click / Tap ──────────────── │
│  [ X ] [ Y ]  [ Click at (X,Y) ] │
│                                   │
│  ─── Drag / Swipe ─────────────── │
│  [SX] [SY] [EX] [EY]  [ Drag ]   │
│                                   │
│  ─── Type ─────────────────────── │
│  [ Text to type        ] [ Type ] │
│  [ Get Text ]                     │
│                                   │
│  ─── Clipboard ────────────────── │
│  [Select All] [Copy] [Paste]      │
│                                   │
│  ─── Navigation ───────────────── │
│  [ Back ] [ Home ] [ Recents ]    │
│                                   │
│  ─── UI Tree ──────────────────── │
│  [ Get UI Tree ]                  │
│  (scrollable tree output)         │
│                                   │
│  ─── Log ──────────────────────── │
│  [08:12:33] screenshot: ok        │
│  [08:12:35] click: ok             │
└───────────────────────────────────┘
```

### How It Works

- Calls `commands::execute_command()` directly — no HTTP/WebSocket involved.
- Commands run in a background thread to avoid blocking the egui event loop.
- Screenshot results are decoded from base64 PNG and displayed as egui textures.
- UI tree results are shown in a scrollable monospace text area.
- All results are logged at the bottom.

## Tray Menu

```
ScreenMCP                        ← title (disabled)
─────────────
Status: Connected                ← disabled, updated dynamically
Signed in: user@gmail.com       ← disabled, shown when logged in
─────────────
Sign in with Google              ← opens browser OAuth flow
Sign Out                         ← enabled only when signed in
─────────────
Test Connection                  ← does discover + connect to worker
Register Device                  ← enabled when signed in
Unregister Device                ← enabled when device registered
─────────────
Test Window                      ← opens native egui test window
─────────────
Open Source Server  ▸            ← submenu
  ├─ [x] Open Source Server
  ├─ User ID: local-user
  ├─ API URL: http://...
  └─ Edit Open Source Settings...
Open Config File
─────────────
About ScreenMCP.com              ← opens website
Quit
```

- **Sign in with Google**: Disabled when Open Source mode is enabled or already signed in.
- **Sign Out**: Clears token + email from config, updates menu labels.
- **Test Connection**: Reloads config, runs discover + connect (same as auto-connect flow).
- **Register Device**: Calls `POST /api/discover` with device_id.
- **Unregister Device**: Calls `DELETE /api/devices/{device_id}`.
- **Test Window**: Opens native egui window on a new thread.
- **About**: Opens https://screenmcp.com in browser.

## Config

```toml
api_url = "https://screenmcp.com"
token = "pk_abc123..."            # Set by Google sign-in flow
email = "user@gmail.com"          # Set by Google sign-in flow (display only)
auto_connect = true
screenshot_quality = 80
device_id = "a1b2c3..."
opensource_server_enabled = false
opensource_user_id = ""
opensource_api_url = ""
```

## Files (Per Client)

| File | Purpose |
|------|---------|
| `auth.rs` | Local HTTP server, auth callback only |
| `test_window.rs` | Native egui/eframe test window |
| `config.rs` | Config with `email` field |
| `tray.rs` | Menu items, handlers, auth/status events |
| `main.rs` | Start local server, wire channels |

### Reference Implementation

Windows client (`screenmcp/windows/src/`) is the reference implementation. Port to macOS and Linux following the same pattern.
