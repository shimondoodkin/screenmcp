# Design: `screenmcp/python-cli` — Standalone Local stdio MCP

**Date:** 2026-06-03
**Status:** Approved (design phase)

## Summary

A self-contained Python MCP server that runs over **stdio** and executes desktop
commands **directly on the local machine** — no worker, no relay, no auth, no
network. It is the single-process local cousin of the existing Rust/Android
clients. Targets Windows, macOS, and Linux. The `ui_tree` command is intentionally
out of scope.

Ships as **one self-contained file**: `screenmcp/python-cli/screenmcp_cli.py`,
organized into clearly-commented banner sections. Optional dependencies are
imported lazily inside the handlers that need them so the server always starts.

## Goals

- Drop-in local desktop control for any MCP client (Claude Code, Claude Desktop, etc.).
- Zero required infrastructure: `python screenmcp_cli.py` over stdio.
- Behavioral parity with the other ScreenMCP clients (command names, params,
  result shapes, coordinate scaling).
- Cross-platform: Windows, macOS, Linux.

## Non-Goals

- `ui_tree` / accessibility tree (explicitly skipped).
- Connecting to the worker/relay or any remote transport.
- Auth (stdio is local-trust).

## Transport & Protocol (raw JSON-RPC, no MCP library)

- Newline-delimited JSON-RPC 2.0 over stdin/stdout (MCP stdio convention: one
  JSON message per line).
- Methods handled:
  - `initialize` → advertise `{capabilities:{tools:{}}, serverInfo:{name,version}}`.
  - `notifications/initialized` → no-op ack.
  - `tools/list` → array of `{name, description, inputSchema}` built from the registry.
  - `tools/call` → dispatch to handler, return `content` blocks.
  - `ping` → `{}`.
  - Unknown method → JSON-RPC error `-32601`.
- `tools/call` results:
  - Screenshots → `{type:"image", data:<base64>, mimeType:"image/webp"}` so the
    model sees the image directly.
  - Other commands → `{type:"text", text:<json-string>}`.
  - Failures → `{content:[{type:"text", text:<msg>}], isError:true}`.
- Malformed input lines are skipped; parse errors on a request id return
  JSON-RPC `-32700`.

## File Structure (single file, section banners)

`screenmcp/python-cli/screenmcp_cli.py`:

```
# === IMPORTS & CONSTANTS ===      stdlib + lazy-optional notes; VERSION, DEFAULT_W/H=1456x819
# === PLATFORM DETECTION ===       IS_WIN/IS_MAC/IS_LINUX; primary modifier (cmd vs ctrl); nav keymap
# === COORDINATE SCALING ===       to_native(x,y,maxw,maxh) / from_native(...); screen-size helpers
# === VISION ===                   screenshot, screenshot_region, screenshot_window, active_window, get_screen_size
# === MOUSE ===                    click, right_click, double_click, middle_click, long_click, mouse_move, drag, scroll, mouse_scroll
# === KEYBOARD ===                 type, press_key, hold_key, release_key, hotkey  (+ key-name → pynput Key map)
# === CLIPBOARD / TEXT ===         get_text, select_all, copy, paste, get_clipboard, set_clipboard
# === WINDOW MANAGEMENT ===        list_windows, focus_window  (platform-guarded impls)
# === NAVIGATION ===               back, home, recents  (platform-specific hotkeys)
# === SYSTEM ===                   is_elevated, elevate, camera, list_cameras, play_audio
# === REGISTRY ===                 TOOLS dict: name → {description, inputSchema, handler}
# === JSON-RPC STDIO SERVER ===    read loop, method dispatch, framing, error wrapping
# === MAIN ===                     if __name__ == "__main__": run()
```

Platform-specific window/nav code lives in functions guarded by `sys.platform`,
not separate files.

## Library Stack

Required: `mss` (screenshots), `pynput` (mouse + keyboard), `Pillow` (encode
WebP / resize), `pyperclip` (clipboard).

Window management (the genuinely platform-specific part):
- **Windows:** `pygetwindow`.
- **macOS:** Quartz `CGWindowList` via `pyobjc`; focus via `osascript` AppleScript.
- **Linux:** `wmctrl` / `xdotool` via `subprocess`; if absent → `unsupported`.

Optional extras (lazy-imported inside handlers; `unsupported` if missing):
- `camera` / `list_cameras` → `opencv-python` (`cv2`).
- `play_audio` → `simpleaudio`.

A `pyproject.toml` is provided only as a convenience for `pip install`
(`screenmcp-cli` console script + extras `[camera]`, `[audio]`); the file itself
runs standalone.

## Coordinate Scaling (parity kept)

Mirrors the other clients. Screenshots default to **1456×819**. Click / drag /
scroll / region coordinates are interpreted in that space and scaled to the real
screen. `max_width` / `max_height` override (0 = native, no scaling).
`get_screen_size` returns the scaled dims plus the original native resolution.

Scaling math (per axis): `native = coord * (native_dim / scaled_dim)`, where
`scaled_dim` is the screenshot space dimension (`max_width`/`max_height` or the
defaults), computed against the captured monitor's native size.

## Command Coverage & Behavior

Real implementations: `screenshot`, `screenshot_region`, `screenshot_window`,
`active_window`, `get_screen_size`, all mouse commands, all keyboard commands,
`select_all`, `copy`, `paste`, `get_clipboard`, `set_clipboard`, `list_windows`,
`focus_window`, `back`, `home`, `recents`, `is_elevated`.

Decisions on ambiguous/infeasible commands:

1. **`get_text`** (no accessibility tree): best-effort heuristic — save clipboard
   → `select_all` → `copy` → read clipboard → **restore original clipboard**.
   Documented as heuristic; non-destructive to the user's clipboard.
2. **`elevate`**: returns `{"status":"ok","unsupported":true}` with a message
   (cannot cleanly re-launch a live stdio process elevated). `is_elevated` is
   real: `ctypes.windll.shell32.IsUserAnAdmin()` on Windows, `os.geteuid()==0`
   elsewhere.
3. **`camera` / `list_cameras` / `play_audio`**: optional-dependency; return
   `unsupported` when the extra is not installed.

`unsupported` result shape matches the Mac/Linux Rust clients:
`{"status":"ok","unsupported":true,"reason":"<why>"}`.

## Platform-Specific Mappings

| Concept            | Windows        | macOS              | Linux           |
|--------------------|----------------|--------------------|-----------------|
| primary modifier   | ctrl           | cmd                | ctrl            |
| `back`             | alt+left       | cmd+`[`            | alt+left        |
| `home`             | win            | F3 (Mission Ctrl)  | super           |
| `recents`          | alt+tab        | cmd+tab            | alt+tab         |
| window enumerate   | pygetwindow    | CGWindowList       | wmctrl          |
| window focus       | pygetwindow    | osascript          | wmctrl -a       |
| is_elevated        | IsUserAnAdmin  | geteuid()==0       | geteuid()==0    |

`copy`/`paste`/`select_all` use the primary modifier so they work on macOS (cmd)
without special-casing each call site.

## macOS Permissions

A plain CLI launched from a terminal inherits TCC permissions from the launching
app (Terminal/iTerm). First screenshot triggers a **Screen Recording** prompt;
first input event triggers an **Accessibility** prompt — both attached to the
parent app, granted once in System Settings → Privacy & Security.

There is no way to programmatically pre-request these from a non-bundled CLI. The
server handles denial gracefully: on `PermissionError` or an empty/black capture
it returns an `isError` result naming the exact permission and where to grant it.
The README documents the one-time setup. (A `check_permissions` probe tool can be
added later if desired — out of scope for v1.)

## Documentation Changes

- **New file:** `screenmcp/python-cli/README.md` — install, run, MCP client
  config snippet (`claude mcp add` / JSON), optional extras, macOS permissions note.
- **Update:** `screenmcp/docs/adding-new-command.md` — add a new section/checklist
  item for the python-cli: when adding a command, touch `screenmcp_cli.py` (add the
  handler in the relevant section + an entry in the `TOOLS` registry). It is a
  self-contained client like the desktop clients, minus `ui_tree`.

## Testing

A `screenmcp/python-cli/test_stdio.py` that launches the server as a subprocess and
pipes a JSON-RPC sequence: `initialize` → `notifications/initialized` →
`tools/list` → `tools/call get_screen_size`, asserting framing and response shape.
Headless-safe commands (`get_screen_size`, clipboard round-trip) run in CI;
mouse/screenshot/window tests are opt-in (require a display) and skipped when no
display is present.

## Risks / Open Items

- Linux window management depends on external `wmctrl`/`xdotool` and X11; Wayland
  is best-effort and may return `unsupported`. Documented.
- macOS window focus via `osascript` matches by app/window title substring; exotic
  apps may not focus reliably. Acceptable for v1.
- `get_text` heuristic momentarily uses the clipboard; restored after. Acceptable.
```