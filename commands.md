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

No parameters.

**Returns:** `{ "tree": [ ...nodes ] }`

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
