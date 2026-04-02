# ScreenMCP Windows Local Mode

## Overview

Add a local mode to the Windows desktop client that embeds an HTTP server directly in the app, enabling AI assistants and scripts to control the desktop without needing the worker/relay infrastructure. The app serves both a plain REST API and MCP Streamable HTTP on the same port.

## Architecture

```
AI Assistant (Claude Code, Cursor, etc.)
    │
    ├── MCP Streamable HTTP ──→ POST /mcp  ──┐
    │                                         │
    └── (or scripts via REST) ──→ POST /command ──┤
                                                   │
                                          ┌────────┴────────┐
                                          │  Windows Client  │
                                          │  axum on :6767   │
                                          │  (embedded)      │
                                          └─────────────────┘
```

No separate process. The HTTP server runs inside the existing Windows tray app alongside the existing WebSocket connection (both can be active simultaneously).

## Config

New fields in `~/.screenmcp/config.toml`:

```toml
local_mode_key = ""          # empty = local mode disabled
local_mode_port = 6767       # default port
```

- When `local_mode_key` is non-empty, the app spawns an axum HTTP server on `127.0.0.1:{local_mode_port}`.
- When `local_mode_key` is empty, local mode is disabled and the server is not started.
- Config changes (via tray settings window) start/stop/restart the local server dynamically.

## HTTP Endpoints

### `POST /command` — Plain REST API

Direct command execution for scripts and simple integrations.

**Request:**
```
POST http://127.0.0.1:6767/command
Authorization: Bearer <local_mode_key>
Content-Type: application/json

{"cmd": "screenshot", "params": {"quality": 80}}
```

**Response:**
```json
{"status": "ok", "result": {"image": "base64...", "width": 1920, "height": 1080}}
```

**Error:**
```json
{"status": "error", "error": "description of what went wrong"}
```

**Auth failure:** Returns HTTP 401.

### `/mcp` — MCP Streamable HTTP

Full MCP protocol for AI assistant integration.

- `POST /mcp` — JSON-RPC 2.0 requests (`initialize`, `tools/list`, `tools/call`)
- `GET /mcp` — SSE stream for server-initiated messages
- `DELETE /mcp` — session termination
- Auth: `Authorization: Bearer <local_mode_key>`

Implemented directly in Rust using JSON-RPC 2.0 over HTTP + SSE. No Rust MCP SDK dependency — the protocol surface is small.

**Claude Code config:**
```json
{
  "mcpServers": {
    "screenmcp": {
      "type": "url",
      "url": "http://127.0.0.1:6767/mcp",
      "headers": {
        "Authorization": "Bearer <key>"
      }
    }
  }
}
```

## Tray Menu Changes

New "Local Mode" section in the tray menu:

- **Status line** — `Local: Listening on :6767` or `Local: Disabled` (greyed)
- **Local Mode Settings** — opens a settings window with:
  - Text field for API key (masked)
  - "Generate" button — fills with random 32-char hex string
  - Port field (defaults to 6767)
  - Save button — starts/stops/restarts server as needed
- **Run as Administrator** — relaunches the app elevated (hidden when already elevated, shows "Running as Administrator" greyed out instead)

## Commands

### Existing Commands (all exposed)

| Command | Parameters | Description |
|---------|-----------|-------------|
| `screenshot` | `quality?`, `max_width?`, `max_height?` | Take screenshot, returns base64 WebP |
| `click` | `x`, `y`, `duration?` | Click at coordinates |
| `right_click` | `x`, `y` | Right-click at coordinates |
| `middle_click` | `x`, `y` | Middle-click at coordinates |
| `long_click` | `x`, `y` | Long press at coordinates (1000ms) |
| `drag` | `startX`, `startY`, `endX`, `endY`, `duration?` | Drag between points |
| `scroll` | `x`, `y`, `dx`, `dy` | Scroll with delta |
| `mouse_scroll` | `x`, `y`, `dx`, `dy` | Mouse wheel scroll |
| `type` | `text` | Type text |
| `press_key` | `key` | Press and release a key |
| `hold_key` | `key` | Hold a key down |
| `release_key` | `key` | Release a held key |
| `get_text` | | Get selected text |
| `select_all` | | Select all (Ctrl+A) |
| `copy` | | Copy selection (Ctrl+C) |
| `paste` | `text?` | Paste (Ctrl+V), optionally set clipboard first |
| `get_clipboard` | | Get clipboard contents |
| `set_clipboard` | `text` | Set clipboard contents |
| `ui_tree` | | Get accessibility tree |
| `back` | | Browser back (Alt+Left) |
| `home` | | Windows key |
| `recents` | | Alt+Tab |
| `camera` | `camera_id?`, `max_width?`, `max_height?` | Capture camera frame |
| `list_cameras` | | List available cameras |

### New Commands

| Command | Parameters | Description |
|---------|-----------|-------------|
| `mouse_move` | `x`, `y` | Move mouse cursor to coordinates without clicking |
| `double_click` | `x`, `y` | Double-click at coordinates |
| `hotkey` | `keys` (string array) | Press key combination atomically, e.g. `["ctrl", "c"]`, `["alt", "tab"]`, `["win", "d"]` |
| `get_screen_size` | | Returns `{width, height}` of primary screen |
| `list_windows` | | List visible on-screen windows with title, position, size, state. Filters out offscreen/hidden windows (same logic as `ui_tree` — `IsOffscreen` check + viewport bounds). Includes taskbar items. |
| `focus_window` | `title?`, `index?` | Bring window to foreground by title substring or index from `list_windows` |
| `elevate` | | Request admin elevation. Shows confirmation dialog to user ("The AI assistant is requesting administrator privileges. Allow?" OK/Cancel). If OK, relaunches app as admin (triggers UAC). If Cancel, returns error. If already elevated, returns `{"status": "ok", "already_elevated": true}`. |
| `is_elevated` | | Returns `{"status": "ok", "elevated": true/false}` |

### `list_windows` Filtering

Uses the same filtering as `ui_tree` to exclude ghost windows:
- Skip windows where `CurrentIsOffscreen() == true`
- Skip windows whose bounds are entirely outside the virtual screen viewport
- Skip zero-size or invisible windows
- Include taskbar items

## Elevation

- `elevate` command shows a native Windows dialog asking user confirmation before triggering UAC
- `is_elevated` command returns current elevation status
- Tray menu has "Run as Administrator" entry (shows greyed "Running as Administrator" when already elevated)
- When elevated, the app can interact with elevated windows (Task Manager, admin prompts, UAC dialogs)

## Skills

Located in `screenmcp/screenmcp-local/skills/`:

### `skill.md` — Main Skill

- What ScreenMCP local mode is and how to use it
- Available MCP tools with parameters and return types
- How to interpret screenshots (base64 WebP)
- How to read and use `ui_tree` results (element names, types, bounds, clickable state)
- Both `screenshot` and `ui_tree` are first-class tools — use whichever fits the situation, or both together
- Coordinate system explanation
- Error handling patterns
- Workflow patterns: understanding the screen, taking actions, validating results

### `windows.md` — Comprehensive Windows Usage Guide

**Desktop anatomy:**
- Taskbar (bottom): pinned apps, running apps indicator, system tray, notification area, clock
- Desktop icons
- Start menu structure and navigation

**Window anatomy:**
- Title bar, menu bar, toolbar, status bar, scrollbars
- Close (X), maximize/restore, minimize buttons — top-right corner
- Resizing by dragging edges and corners

**Common UI patterns:**
- Dialog boxes (OK/Cancel/Apply)
- File picker / Save As dialogs
- Dropdown menus, context menus (right-click)
- Tabs, checkboxes, radio buttons, tree views
- Ribbons (Office-style)
- Settings app navigation

**File Explorer:**
- Address bar, navigation pane, breadcrumbs
- File/folder operations

**Keyboard shortcuts (comprehensive, via `hotkey` command):**
- Clipboard: `["ctrl","c"]`, `["ctrl","v"]`, `["ctrl","x"]`, `["ctrl","a"]`
- Undo/redo: `["ctrl","z"]`, `["ctrl","y"]`
- Save: `["ctrl","s"]`
- Find: `["ctrl","f"]`
- Window management: `["alt","tab"]`, `["alt","F4"]`, `["win","d"]`, `["win","e"]`, `["win","r"]`
- Snap: `["win","left"]`, `["win","right"]`, `["win","up"]`
- Task manager: `["ctrl","shift","esc"]`
- Screenshot: `["win","shift","s"]`
- Lock screen: `["win","l"]`
- And more...

**Multi-monitor:** coordinates extend beyond primary screen bounds

**Common app patterns:** browser tabs, Office ribbon, Settings app

**Text editing patterns:**
- Click to position cursor
- Click + Shift+click for selection range
- Triple-click for line selection
- Ctrl+A for select all
- Click field → select_all → type to replace content

**Drag and drop:** using `drag` command for file moving, window arranging

**Notification handling:** notification center, toast dismissal

**UAC / elevated prompts:** what they look like, when `elevate` command is needed

### `python.md` — Python Scripting Guide

- Making requests to `POST http://127.0.0.1:6767/command`
- Auth header setup
- Code examples for common flows: screenshot, click, type, read ui_tree
- Parsing base64 WebP screenshots into PIL images
- Simple automation patterns (loop: screenshot → analyze → act)

## Dependencies

**New Rust dependencies for the Windows client:**
- `axum` — HTTP server framework (already tokio-based)
- `tower-http` — CORS middleware (if needed)

**No new packages or processes.** Everything runs inside the existing Windows client binary.

## Implementation Notes

- The local HTTP server runs on the existing tokio runtime (spawned in main.rs)
- Commands execute via the existing `commands::execute_command()` function
- A new `local_server.rs` module handles axum setup, routing, auth middleware, and MCP protocol
- The MCP Streamable HTTP implementation handles JSON-RPC 2.0 directly — `initialize`, `tools/list`, `tools/call`
- Server lifecycle (start/stop/restart) is managed via a channel from the tray, similar to `WsCommand`
- New commands (`mouse_move`, `double_click`, `hotkey`, `get_screen_size`, `list_windows`, `focus_window`, `elevate`, `is_elevated`) are added to `commands.rs`
