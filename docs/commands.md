# ScreenMCP Commands

Canonical reference for all device commands. See [wire-protocol.md](wire-protocol.md) for wire format and [initiation-protocol.md](initiation-protocol.md) for auth, sessions, and connection flow.

## Command Format

Commands are sent as JSON over WebSocket:

```json
{ "id": 1, "cmd": "command_name", "params": { ... } }
```

Responses:

```json
{ "id": 1, "status": "ok", "result": { ... } }
{ "id": 1, "status": "error", "error": "message" }
```

---

## Coordinate Scaling

All commands that accept or return coordinates support optional `max_width` and `max_height` parameters for automatic coordinate scaling. This lets AI assistants use screenshot pixel coordinates directly without knowing the device's DPI or resolution.

**Default values:** `max_width: 1456`, `max_height: 819` (landscape). These defaults apply automatically to all coordinate commands — screenshots, clicks, ui_tree, list_windows, get_screen_size. You don't need to pass them explicitly.

**How it works:**
1. Take a screenshot — get a 1456x819 image by default
2. Click at screenshot pixel coordinates — auto-scaled to actual screen
3. `ui_tree` and `list_windows` — returned bounds are in 1456x819 space
4. `get_screen_size` — returns scaled dimensions (1456x819) plus originals

**Disabling:** Set `max_width: 0` and `max_height: 0` to use raw screen coordinates.

**Config override:** If `max_screenshot_width` or `max_screenshot_height` are set in the config file, they override the built-in defaults.

**Applies to:** `click`, `long_click`, `right_click`, `middle_click`, `double_click`, `mouse_move`, `drag`, `scroll`, `mouse_scroll`, `ui_tree`, `list_windows`, `get_screen_size`.

---

## Screen & UI

### screenshot

Take a screenshot of the device screen. Returns base64 WebP image.

| Param | Type | Default | Description |
|-------|------|---------|-------------|
| `quality` | integer | 100 | 1–99 lossy WebP, 100 = lossless |
| `max_width` | integer | — | Max width in pixels (aspect ratio preserved) |
| `max_height` | integer | — | Max height in pixels (aspect ratio preserved) |

**Returns:** `{ "image": "<base64 webp>" }`
**Errors:** `"phone is locked"` if keyguard active.

### ui_tree

Get the accessibility tree of the current screen.

| Param | Type | Default | Description |
|-------|------|---------|-------------|
| `max_width` | integer | 0 | Scale returned bounds to match screenshot space |
| `max_height` | integer | 0 | Scale returned bounds to match screenshot space |
| `window` | string \| number | — | (Windows) Title substring or hwnd. Scopes the walk to one top-level window. |
| `region` | object | — | (Windows) `{min_x, min_y, max_x, max_y}` in screenshot space. Filter by bounds. |
| `region_mode` | string | `"inside"` | (Windows) `"inside"` (fully inside) or `"intersect"` (any overlap). |
| `types` | string[] | all | (Windows) Whitelist of `controlType` values. Case-insensitive. |
| `text_match` | string | — | (Windows) Filter on `text`. Substring (case-insensitive) by default. |
| `regex` | boolean | false | (Windows) If true, `text_match` is treated as a regex. |
| `max_depth` | integer | 10 | (Windows) Cap recursion depth. |
| `format` | string | `"nested"` | (Windows) `"nested"` (default) or `"flat"`. |
| `fields` | string[] | per-format | (Windows) Per-node fields to emit. See node fields below. |

**Returns (nested):** `{ "tree": [ ...nodes ] }`

**Returns (flat):** `{ "nodes": [ {controlType, text, cx, cy, hwnd, path}, ... ] }`

**Available `fields` values:** `text, value, controlType, className, resourceId, contentDescription, bounds, cx, cy, enabled, clickable, editable, scrollable, checked, focused, hwnd, path`. `controlType` is always emitted; `children` is always emitted in nested mode.

**Examples:**

Scope to one window, list buttons only:

```json
{"cmd":"ui_tree","params":{"window":"Notepad","types":["button"]}}
```

Flat mode, ready-to-click center coords:

```json
{"cmd":"ui_tree","params":{"format":"flat","window":"Notepad"}}
```

Region filter, intersect:

```json
{"cmd":"ui_tree","params":{"region":{"min_x":0,"min_y":0,"max_x":500,"max_y":500},"region_mode":"intersect"}}
```

**Android node fields:**

| Field | Type | Description |
|-------|------|-------------|
| `className` | string | Widget class (e.g. `EditText`) |
| `resourceId` | string | Android resource ID |
| `text` | string | Displayed text |
| `contentDescription` | string | Accessibility label |
| `bounds` | object | `{ left, top, right, bottom }` |
| `clickable` | boolean | Whether the node is clickable |
| `editable` | boolean | Whether the node is editable |
| `focused` | boolean | Whether the node has focus |
| `scrollable` | boolean | Whether the node is scrollable |
| `checkable` | boolean | Whether the node is checkable |
| `checked` | boolean | Whether the node is checked |
| `children` | array | Child nodes |

**Desktop node fields:**

| Field | Type | Description |
|-------|------|-------------|
| `title` | string | Window title |
| `x` | number | Window X position |
| `y` | number | Window Y position |
| `width` | number | Window width |
| `height` | number | Window height |

---

## Touch & Gestures

### click

Tap on the screen at coordinates.

| Param | Type | Default | Description |
|-------|------|---------|-------------|
| `x` | number | — | X coordinate (required) |
| `y` | number | — | Y coordinate (required) |
| `duration` | integer | 100 | Press duration in ms |
| `max_width` | integer | 0 | Screenshot width for auto-scaling (see Coordinate Scaling) |
| `max_height` | integer | 0 | Screenshot height for auto-scaling |

### long_click

Long press at coordinates. Fixed 1000ms press duration.

| Param | Type | Default | Description |
|-------|------|---------|-------------|
| `x` | number | — | X coordinate (required) |
| `y` | number | — | Y coordinate (required) |

### drag

Drag from one point to another.

| Param | Type | Default | Description |
|-------|------|---------|-------------|
| `startX` | number | — | Starting X (required) |
| `startY` | number | — | Starting Y (required) |
| `endX` | number | — | Ending X (required) |
| `endY` | number | — | Ending Y (required) |
| `duration` | integer | 300 | Duration in ms |

### scroll

Finger-drag scroll gesture from (x,y) to (x+dx, y+dy).

| Param | Type | Default | Description |
|-------|------|---------|-------------|
| `x` | number | — | Start X (required) |
| `y` | number | — | Start Y (required) |
| `dx` | number | 0 | Horizontal delta |
| `dy` | number | 0 | Vertical delta (negative = scroll content up) |

---

## Text Input

### type

Type text into the currently focused input field.

| Param | Type | Description |
|-------|------|-------------|
| `text` | string | Text to type (required) |

### get_text

Get text from the currently focused input field.

No parameters.

**Returns:** `{ "text": "field contents" }`

### select_all

Select all text in the focused field. No parameters.

### copy

Copy selected text to clipboard.

| Param | Type | Default | Description |
|-------|------|---------|-------------|
| `return_text` | boolean | false | If true, return the copied text in the response |

**Returns (when `return_text` is true):** `{ "text": "copied content" }`

### paste

Paste into the focused field. Optionally set clipboard contents before pasting.

| Param | Type | Default | Description |
|-------|------|---------|-------------|
| `text` | string | — | If provided, set clipboard to this text before pasting |

---

## Clipboard

### get_clipboard

Get the current clipboard text contents.

No parameters.

**Returns:** `{ "text": "clipboard contents" }`

### set_clipboard

Set the clipboard to the given text.

| Param | Type | Description |
|-------|------|-------------|
| `text` | string | Text to put in the clipboard (required) |

---

## Navigation

### back

Press the back button. Android: system Back. Desktop: Alt+Left (Win/Linux), Cmd+Left (Mac).

### home

Press the home button. Android: system Home. Desktop: Win key (Windows), Cmd+H (Mac), Super (Linux).

### recents

Open the app switcher. Android: recent apps. Desktop: Alt+Tab (Win/Linux), Cmd+Tab (Mac).

---

## Camera

### list_cameras

List available cameras on the device. Use this to discover camera IDs before calling `camera`.

No parameters.

**Returns:**
```json
{
  "cameras": [
    { "id": "0", "facing": "back" },
    { "id": "1", "facing": "front" },
    { "id": "2", "facing": "external" }
  ]
}
```

Desktop clients return `{ "cameras": [] }` (no cameras).

### camera

Take a photo with the device camera.

| Param | Type | Default | Description |
|-------|------|---------|-------------|
| `camera` | string | `"0"` | Camera ID (use `list_cameras` to discover available IDs) |
| `quality` | integer | 80 | Image quality 1–99 lossy, 100 lossless |
| `max_width` | integer | — | Max width in pixels (aspect ratio preserved) |
| `max_height` | integer | — | Max height in pixels (aspect ratio preserved) |

**Returns:** `{ "image": "<base64 webp>" }`
Returns empty image string if camera not available. Desktop clients return `{ "unsupported": true }`.

---

## Audio

### play_audio

Play an audio file (WAV or MP3) on the device speaker.

| Param | Type | Default | Description |
|-------|------|---------|-------------|
| `audio_data` | string | — | Base64-encoded audio file (WAV or MP3) (required) |
| `volume` | number | 1.0 | Playback volume 0.0–1.0 |

---

## Keyboard (Desktop Only)

These commands are supported by desktop clients (Windows, Mac, Linux). On Android they return `{status: "error"}`.

### hold_key

Press and hold a key until `release_key` is called.

| Param | Type | Description |
|-------|------|-------------|
| `key` | string | Key name (required) |

### release_key

Release a held key.

| Param | Type | Description |
|-------|------|-------------|
| `key` | string | Key name (required) |

### press_key

Press and release a key in one action.

| Param | Type | Description |
|-------|------|-------------|
| `key` | string | Key name (required) |

**Supported key names:** `shift`, `ctrl`/`control`, `alt`, `meta`/`cmd`/`win`/`command`/`super`, `tab`, `enter`/`return`, `escape`/`esc`, `space`, `backspace`, `delete`/`del`, `home`, `end`, `pageup`, `pagedown`, `up`, `down`, `left`, `right`, `f1`–`f12`, or any single character.

---

## Mouse (Desktop Only)

These are accepted but return unsupported on Android (for cross-platform CLI compatibility).

### right_click

| Param | Type | Description |
|-------|------|-------------|
| `x` | number | X coordinate (required) |
| `y` | number | Y coordinate (required) |

### middle_click

| Param | Type | Description |
|-------|------|-------------|
| `x` | number | X coordinate (required) |
| `y` | number | Y coordinate (required) |

### mouse_scroll

| Param | Type | Description |
|-------|------|-------------|
| `x` | number | X coordinate (required) |
| `y` | number | Y coordinate (required) |
| `dx` | number | Horizontal delta |
| `dy` | number | Vertical delta |

Returns `{ "unsupported": true }` on Android.

### mouse_move

Move the mouse cursor without clicking.

| Param | Type | Description |
|-------|------|-------------|
| `x` | number | X coordinate (required) |
| `y` | number | Y coordinate (required) |

### double_click

Double-click at coordinates. On Android, performs two rapid taps.

| Param | Type | Description |
|-------|------|-------------|
| `x` | number | X coordinate (required) |
| `y` | number | Y coordinate (required) |

### hotkey

Press a key combination atomically (e.g., Ctrl+C, Alt+Tab). All keys are pressed in order then released in reverse.

| Param | Type | Description |
|-------|------|-------------|
| `keys` | string[] | Array of key names (required). Same key names as `press_key`. |

**Example:** `{ "keys": ["ctrl", "shift", "s"] }` for Save As.

---

## Window Management

### get_screen_size

Get the primary display dimensions. On Android, returns screen resolution and density.

| Param | Type | Default | Description |
|-------|------|---------|-------------|
| `max_width` | integer | 0 | If set, return scaled dimensions matching screenshot space |
| `max_height` | integer | 0 | If set, return scaled dimensions matching screenshot space |

**Returns (no scaling):** `{ "width": 3840, "height": 2160 }`

**Returns (with max_width: 1280):** `{ "width": 1280, "height": 720, "original_width": 3840, "original_height": 2160, "scaled": true }`

Android also returns `"density"`.

### list_windows

List all visible windows with titles and positions. On Android, uses AccessibilityService to list app windows.

| Param | Type | Default | Description |
|-------|------|---------|-------------|
| `max_width` | integer | 0 | Scale returned coordinates to match screenshot space |
| `max_height` | integer | 0 | Scale returned coordinates to match screenshot space |

**Returns:**
```json
{
  "windows": [
    { "index": 0, "title": "My Document", "x": 0, "y": 0, "width": 1920, "height": 1080 }
  ]
}
```

Mac also returns `"app"` (owning application name). Windows also returns `"minimized"` and `"maximized"`.

### focus_window

Bring a window to the foreground by title substring or index from `list_windows`. On Android, launches the app by name.

| Param | Type | Description |
|-------|------|-------------|
| `title` | string | Window title substring (case-insensitive) |
| `index` | integer | Window index from `list_windows` |

Provide either `title` or `index`.

**Returns:** `{ "focused": "window title" }`

### active_window

Get information about the currently focused window. Supported on all platforms including Android.

| Param | Type | Default | Description |
|-------|------|---------|-------------|
| `max_width` | integer | 0 | Scale returned coordinates to match screenshot space |
| `max_height` | integer | 0 | Scale returned coordinates to match screenshot space |

**Returns:** `{ "title": "Window Title", "x": 0, "y": 0, "width": 1920, "height": 1080 }`

### screenshot_window

Capture a specific window by title or index, without needing to focus it first (Windows). Mac and Linux may focus the window briefly.

| Param | Type | Default | Description |
|-------|------|---------|-------------|
| `title` | string | — | Window title substring |
| `index` | integer | — | Window index from `list_windows` |
| `max_width` | integer | — | Max width in pixels |
| `max_height` | integer | — | Max height in pixels |

**Returns:** `{ "image": "<base64 webp>", "title": "...", "width": ..., "height": ... }`

---

## System (Desktop Only)

### is_elevated

Check if the process is running with elevated/admin privileges.

No parameters.

**Returns:** `{ "elevated": true }` or `{ "elevated": false }`

### elevate

Request administrator/root privileges. Shows a confirmation dialog (Windows UAC, macOS password prompt, Linux pkexec). The process restarts with elevated privileges.

No parameters.

**Returns:** `{ "elevating": true }` or `{ "already_elevated": true }`

### screenshot_region

Capture a region of the screen for detailed inspection. Use small regions (e.g. 200x200) to zoom into specific areas. Coordinates are in screenshot space. Only scales down, never up.

| Param | Type | Default | Description |
|-------|------|---------|-------------|
| `min_x` | number | — | Left edge X coordinate (required) |
| `min_y` | number | — | Top edge Y coordinate (required) |
| `max_x` | number | — | Right edge X coordinate (required) |
| `max_y` | number | — | Bottom edge Y coordinate (required) |
| `quality` | integer | 100 | Image quality 1-100 |
| `output_max_width` | integer | — | Max output width (only scales down) |
| `output_max_height` | integer | — | Max output height (only scales down) |
| `max_width` | integer | 0 | Screenshot space width for coordinate scaling |
| `max_height` | integer | 0 | Screenshot space height for coordinate scaling |

**Returns:** `{ "image": "<base64 webp>" }`
