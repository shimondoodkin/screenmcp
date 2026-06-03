# Implementations

Command support matrix across all projects. Reference: [commands.md](commands.md)

## Projects

| # | Project | Path | Language | Type |
|---|---------|------|----------|------|
| 1 | Worker | `worker/` | Rust | WebSocket relay server |
| 2 | MCP Server | `mcp-server/` | TypeScript | MCP tools HTTP server |
| 3 | Android App | `android/` | Kotlin | Mobile client (AccessibilityService) |
| 4 | Windows Client | `windows/` | Rust | Desktop client (Win32) |
| 5 | macOS Client | `mac/` | Rust | Desktop client (CoreGraphics) |
| 6 | Linux Client | `linux/` | Rust | Desktop client (wmctrl/xdotool) |
| 7 | TypeScript SDK | `sdk/typescript/` | TypeScript | Client library + CLI example |
| 8 | Python SDK | `sdk/python/` | Python | Client library (async) |
| 9 | Rust SDK | `sdk/rust/` | Rust | Client library (async) |

## Command Support Matrix

The worker relays all commands — it does not interpret them. The matrix below covers device-side handlers and client-side typed methods.

### Screen & UI

| Command | Android | Windows | macOS | Linux | MCP Server | TS SDK | Python SDK |
|---------|---------|---------|-------|-------|------------|--------|------------|
| `screenshot` | yes | yes | yes | yes | yes | yes | yes |
| `ui_tree` | yes | yes | yes | yes | yes | yes | yes |

- `ui_tree` extended params (`window`, `region`, `region_mode`, `types`, `text_match`, `regex`, `max_depth`, `format`, `fields`) are Windows-only. Other platforms accept and ignore them.
- `screenshot` / `screenshot_region` estimated-click overlay params: `dots` (paint markers) are supported on **all** clients (Android, Windows, macOS, Linux, Python CLI). `cursor` (draw the real cursor) is supported on **Windows / macOS / Linux only** — Android has no cursor and ignores it; the Python CLI does not draw the cursor. `dot_radius` defaults to 3.

### Touch & Gestures

| Command | Android | Windows | macOS | Linux | MCP Server | TS SDK | Python SDK |
|---------|---------|---------|-------|-------|------------|--------|------------|
| `click` | yes | yes | yes | yes | yes | yes | yes |
| `long_click` | yes | yes | yes | yes | yes | yes | yes |
| `drag` | yes | yes | yes | yes | yes | yes | yes |
| `scroll` | yes | yes | yes | yes | yes | yes | yes |

### Text Input

| Command | Android | Windows | macOS | Linux | MCP Server | TS SDK | Python SDK |
|---------|---------|---------|-------|-------|------------|--------|------------|
| `type` | yes | yes | yes | yes | yes | yes | yes |
| `get_text` | yes | yes | yes | yes | yes | yes | yes |
| `select_all` | yes | yes | yes | yes | yes | yes | yes |
| `copy` | yes | yes | yes | yes | yes | yes | yes |
| `paste` | yes | yes | yes | yes | yes | yes | yes |

- `copy` supports optional `return_text` param — returns copied text in response.
- `paste` supports optional `text` param — sets clipboard before pasting.

### Clipboard

| Command | Android | Windows | macOS | Linux | MCP Server | TS SDK | Python SDK |
|---------|---------|---------|-------|-------|------------|--------|------------|
| `get_clipboard` | yes | yes | yes | yes | yes | yes | yes |
| `set_clipboard` | yes | yes | yes | yes | yes | yes | yes |

### Navigation

| Command | Android | Windows | macOS | Linux | MCP Server | TS SDK | Python SDK |
|---------|---------|---------|-------|-------|------------|--------|------------|
| `back` | yes | yes | yes | yes | yes | yes | yes |
| `home` | yes | yes | yes | yes | yes | yes | yes |
| `recents` | yes | yes | yes | yes | yes | yes | yes |

### Camera

| Command | Android | Windows | macOS | Linux | MCP Server | TS SDK | Python SDK |
|---------|---------|---------|-------|-------|------------|--------|------------|
| `list_cameras` | yes | empty | empty | empty | yes | yes | yes |
| `camera` | yes | unsupported | unsupported | unsupported | yes | yes | yes |

- Camera accepts any camera ID string. Use `list_cameras` to discover available IDs.

### Audio

| Command | Android | Windows | macOS | Linux | MCP Server | TS SDK | Python SDK | Rust SDK |
|---------|---------|---------|-------|-------|------------|--------|------------|----------|
| `play_audio` | yes | yes | yes | yes | yes | yes | yes | yes |

### Keyboard (Desktop Only)

| Command | Android | Windows | macOS | Linux | MCP Server | TS SDK | Python SDK | Rust SDK |
|---------|---------|---------|-------|-------|------------|--------|------------|----------|
| `hold_key` | unsupported | yes | yes | yes | yes | yes | yes | yes |
| `release_key` | unsupported | yes | yes | yes | yes | yes | yes | yes |
| `press_key` | unsupported | yes | yes | yes | yes | yes | yes | yes |
| `hotkey` | unsupported | yes | yes | yes | yes | yes | yes | yes |

### Mouse (Desktop Only)

| Command | Android | Windows | macOS | Linux | MCP Server | TS SDK | Python SDK | Rust SDK |
|---------|---------|---------|-------|-------|------------|--------|------------|----------|
| `right_click` | unsupported | yes | yes | yes | yes | yes | yes | yes |
| `middle_click` | unsupported | yes | yes | yes | yes | yes | yes | yes |
| `mouse_scroll` | unsupported | yes | yes | yes | yes | yes | yes | yes |
| `mouse_move` | unsupported | yes | yes | yes | yes | yes | yes | yes |
| `double_click` | yes | yes | yes | yes | yes | yes | yes | yes |

### Window Management

| Command | Android | Windows | macOS | Linux | MCP Server | TS SDK | Python SDK | Rust SDK |
|---------|---------|---------|-------|-------|------------|--------|------------|----------|
| `get_screen_size` | yes | yes | yes | yes | yes | yes | yes | yes |
| `list_windows` | yes | yes | yes | yes | yes | yes | yes | yes |
| `focus_window` | yes | yes | yes | yes | yes | yes | yes | yes |
| `active_window` | yes | yes | yes | yes | yes | yes | yes | yes |
| `screenshot_window` | unsupported | yes | yes | yes | yes | yes | yes | yes |
| `screenshot_region` | yes | yes | yes | yes | yes | yes | yes | yes |

### System (Desktop Only)

| Command | Android | Windows | macOS | Linux | MCP Server | TS SDK | Python SDK | Rust SDK |
|---------|---------|---------|-------|-------|------------|--------|------------|----------|
| `is_elevated` | unsupported | yes | yes | yes | yes | yes | yes | yes |
| `elevate` | unsupported | yes | yes | yes | yes | yes | yes | yes |

### Coordinate Scaling

All coordinate-based commands (`click`, `drag`, `scroll`, `mouse_move`, `double_click`, `right_click`, `middle_click`, `mouse_scroll`, `ui_tree`, `list_windows`, `get_screen_size`, `active_window`) support `max_width`/`max_height` parameters for automatic coordinate scaling. Default: 1456x819 (landscape).

## Gaps Summary

All commands have full coverage across all platforms and SDKs. Android returns `{unsupported: true}` for desktop-only commands.
