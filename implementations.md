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

## Command Support Matrix

The worker relays all commands — it does not interpret them. The matrix below covers device-side handlers and client-side typed methods.

### Screen & UI

| Command | Android | Windows | macOS | Linux | MCP Server | TS SDK | Python SDK |
|---------|---------|---------|-------|-------|------------|--------|------------|
| `screenshot` | yes | yes | yes | yes | yes | yes | yes |
| `ui_tree` | yes | yes | yes | yes | yes | yes | yes |

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

### Keyboard (Desktop Only)

| Command | Android | Windows | macOS | Linux | MCP Server | TS SDK | Python SDK |
|---------|---------|---------|-------|-------|------------|--------|------------|
| `hold_key` | error | yes | yes | yes | yes | yes | yes |
| `release_key` | error | yes | yes | yes | yes | yes | yes |
| `press_key` | error | yes | yes | yes | yes | yes | yes |

### Mouse (Desktop Only)

| Command | Android | Windows | macOS | Linux | MCP Server | TS SDK | Python SDK |
|---------|---------|---------|-------|-------|------------|--------|------------|
| `right_click` | unsupported | yes | yes | yes | yes | generic | generic |
| `middle_click` | unsupported | yes | yes | yes | yes | generic | generic |
| `mouse_scroll` | unsupported | yes | yes | yes | yes | generic | generic |

- SDKs can send these via `sendCommand()`. CLI example has shell commands for them.

## Gaps Summary

All gaps resolved. Every command has full coverage across all projects.
