# Available Commands

## Global Options

| Option | Description |
|--------|-------------|
| `--api-key <key>` | API key (`pk_...` format) — required |
| `--api-url <url>` | API server URL |
| `--device-id <id>` | Target device ID (32 hex chars) |

---

## Commands

### screenshot

Take a screenshot and save as WebP.

```
screenmcp screenshot [outfile] [options]
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `outfile` | string | `screenshot_<timestamp>.webp` | Output filename |
| `--quality`, `-q` | integer | 100 | 1–99 lossy, 100+ lossless |
| `--max-width` | integer | — | Max width in pixels (aspect ratio preserved) |
| `--max-height` | integer | — | Max height in pixels (aspect ratio preserved) |

**Returns:** base64-encoded WebP image saved to file.

---

### click

Tap at screen coordinates.

```
screenmcp click <x> <y> [duration]
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `x` | number | — | X coordinate (required) |
| `y` | number | — | Y coordinate (required) |
| `duration` | integer | 100 | Press duration in milliseconds |

---

### long_click

Long-press at screen coordinates. *(shell only)*

```
long_click <x> <y>
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `x` | number | — | X coordinate (required) |
| `y` | number | — | Y coordinate (required) |

Uses 1000ms duration internally.

---

### drag

Drag gesture from one point to another. *(shell only)*

```
drag <startX> <startY> <endX> <endY>
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `startX` | number | — | Starting X coordinate (required) |
| `startY` | number | — | Starting Y coordinate (required) |
| `endX` | number | — | Ending X coordinate (required) |
| `endY` | number | — | Ending Y coordinate (required) |

Duration defaults to 300ms. Movement is interpolated over 20 steps.

---

### type

Type text into the currently focused input field.

```
screenmcp type <text>
```

| Parameter | Type | Description |
|-----------|------|-------------|
| `text` | string | Text to type (required). In shell mode, all words after `type` are joined. |

---

### get_text

Get text from the currently focused element. *(shell only)*

```
get_text
```

No parameters. Returns the text content of the focused field (phone) or clipboard contents (desktop).

---

### select_all

Select all text in the focused element. *(shell only)*

```
select_all
```

No parameters.

---

### copy

Copy selected text to clipboard. *(shell only)*

```
copy
```

No parameters.

---

### paste

Paste from clipboard. *(shell only)*

```
paste
```

No parameters.

---

### tree

Get the UI accessibility tree.

```
screenmcp tree
```

No parameters. Returns JSON.

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

### scroll

Scroll the screen in a direction.

```
screenmcp scroll <direction> [amount]
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `direction` | string | — | `up`, `down`, `left`, or `right` (required) |
| `amount` | integer | 300 | Scroll distance in pixels |

Scrolls from the center of a typical screen (540, 1200).

---

### camera

Capture a photo from the device camera.

```
screenmcp camera [facing] [options]
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `facing` | string | `rear` | `front` or `rear` |
| `--quality`, `-q` | integer | 80 | 1–99 lossy, 100 lossless |
| `--max-width` | integer | — | Max width in pixels (aspect ratio preserved) |
| `--max-height` | integer | — | Max height in pixels (aspect ratio preserved) |
| `--output`, `-o` | string | `camera_<timestamp>.webp` | Output filename |

**Note:** Returns "unsupported" on desktop clients.

---

### back

Press the Back button. *(shell only)*

```
back
```

Android: system Back. Desktop: Alt+Left (Win/Linux), Cmd+Left (Mac).

---

### home

Press the Home button. *(shell only)*

```
home
```

Android: system Home. Desktop: Win key (Windows), Cmd+F3 (Mac), Super (Linux).

---

### recents

Open the app switcher. *(shell only)*

```
recents
```

Android: recent apps. Desktop: Alt+Tab (Win/Linux), Cmd+Tab (Mac).

---

### shell

Interactive REPL mode. All commands above are available, plus desktop-only commands below.

```
screenmcp shell
```

Type `help` inside the shell to see all commands. Type `quit` or `exit` to close.

---

## Desktop-Only Commands *(shell only)*

### right_click

```
right_click <x> <y>
```

Right-click at coordinates. Returns "unsupported" on phone and Linux.

### middle_click

```
middle_click <x> <y>
```

Middle-click at coordinates. Returns "unsupported" on phone and Linux.

### mouse_scroll

```
mouse_scroll <x> <y> <dx> <dy>
```

Raw mouse scroll at coordinates with pixel deltas. Support varies by platform.

---

## Desktop-Only Keyboard Commands *(wire protocol)*

These commands are supported by desktop clients (Windows, Mac, Linux) but not exposed in the CLI example. Use `sendCommand()` from the SDK directly.

### hold_key

Hold a key down until `release_key` is called.

| Parameter | Type | Description |
|-----------|------|-------------|
| `key` | string | Key name (required) |

### release_key

Release a held key.

| Parameter | Type | Description |
|-----------|------|-------------|
| `key` | string | Key name (required) |

### press_key

Press and release a key.

| Parameter | Type | Description |
|-----------|------|-------------|
| `key` | string | Key name (required) |

**Supported key names:** `shift`, `ctrl`, `alt`, `meta`/`cmd`/`win`, `tab`, `enter`, `escape`, `space`, `backspace`, `delete`, `home`, `end`, `pageup`, `pagedown`, `up`, `down`, `left`, `right`, `f1`–`f12`, or any single character.

---

## Response Format

**Success:**
```json
{ "id": 7, "status": "ok", "result": { ... } }
```

**Error:**
```json
{ "id": 7, "status": "error", "error": "descriptive message" }
```
