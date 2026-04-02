# ScreenMCP Local — Desktop Control via MCP

You have access to ScreenMCP tools that let you see and control a desktop. These tools connect to the ScreenMCP app running locally.

## Coordinate Scaling (Important)

All coordinates auto-scale between screenshot space and actual screen. Default: **1456x819**.

- Screenshots return a 1456x819 image by default
- Click/drag/scroll coordinates are in the same 1456x819 space — auto-scaled to actual screen
- ui_tree and list_windows return bounds in 1456x819 space
- You can override with `max_width`/`max_height` params on any command (set to 0 to disable)

**You do NOT need to know the actual screen resolution or DPI.** Just use pixel coordinates from the screenshot directly.

## Critical: Window Focus

**Always call `focus_window` before clicking on a specific app.** Clicks go to whatever window is currently focused. If the terminal or another app is in front, your click will land there instead of the target app.

```
1. focus_window(title: "Paint")   ← bring Paint to front
2. click(x: 400, y: 300)          ← now lands on Paint's canvas
```

## Available Tools

### Vision
- **screenshot** — Take a screenshot. Returns base64 WebP image. Default 1456x819.
- **screenshot_region** — Capture a region at native resolution for precise inspection. Pass `min_x, min_y, max_x, max_y` in screenshot coords. Use tight regions (e.g. 120x90) to zoom into small elements. See "Precision Clicking" below.
- **ui_tree** — Get the accessibility tree with element names, types, bounds, text, clickable/focusable state. Bounds are in screenshot coordinates.
- **screenshot_window** — Capture a specific window by `title` or `index` without focusing it.
- **active_window** — Get title, position, size of the currently focused window.

### Mouse
- **click** — Left-click at (x, y). Optional `duration` in ms.
- **right_click** — Right-click at (x, y). Opens context menus.
- **double_click** — Double-click at (x, y). Opens files, selects words.
- **long_click** — Long press at (x, y). Default 1000ms hold.
- **middle_click** — Middle-click at (x, y).
- **mouse_move** — Move cursor to (x, y) without clicking. Useful for hover.
- **drag** — Drag from (startX, startY) to (endX, endY). Use for drawing, moving, resizing.
- **scroll** — Scroll at (x, y) with `dx`/`dy` deltas, or use `direction` (up/down/left/right) + `amount`.
- **mouse_scroll** — Raw mouse wheel scroll at coordinates.

### Keyboard
- **type** — Type text into the focused field.
- **press_key** — Press and release a single key. Names: shift, ctrl, alt, meta/win, tab, enter, escape, space, backspace, delete, home, end, pageup, pagedown, up, down, left, right, f1-f12, or a single character.
- **hold_key** / **release_key** — Hold and release keys manually.
- **hotkey** — Press a key combination atomically: `["ctrl", "c"]`, `["alt", "tab"]`, `["win", "d"]`. Preferred over hold_key/release_key.

### Text & Clipboard
- **get_text** — Get text from the focused field.
- **select_all** — Select all text (Ctrl+A).
- **copy** — Copy selection (Ctrl+C). Set `return_text: true` to get the text back.
- **paste** — Paste (Ctrl+V). Optionally pass `text` to set clipboard first.
- **get_clipboard** — Read clipboard contents.
- **set_clipboard** — Set clipboard to given text.

### Window Management
- **list_windows** — List all visible windows with title, position, size, state, and index.
- **focus_window** — Bring a window to front by `title` (substring match) or `index`.
- **active_window** — Get the currently focused window info.
- **get_screen_size** — Get screen dimensions in screenshot space (default 1456x819) plus original resolution.

### Navigation
- **back** — Browser back / general back (Alt+Left).
- **home** — Press Windows key (opens Start menu).
- **recents** — Show recent windows (Alt+Tab).

### System
- **elevate** — Request administrator privileges (shows confirmation dialog).
- **is_elevated** — Check if running with admin privileges.
- **camera** / **list_cameras** — Capture from connected cameras.
- **play_audio** — Play base64-encoded WAV/MP3 audio.

## How to Use

### Workflow Pattern

```
1. focus_window(title: "target app")  ← ALWAYS focus first
2. screenshot()                        ← see the screen
3. click/type/hotkey                   ← take action
4. screenshot()                        ← verify result
```

### Finding Elements

Use both **screenshot** and **ui_tree**:

- **screenshot** — visual picture, good for layout, images, verifying actions
- **ui_tree** — structured data with exact coordinates, good for finding buttons, text fields, labels

Often use both: ui_tree to find coordinates, screenshot to verify.

### Coordinate Tips

- Coordinates from screenshot pixels map directly to click coordinates (auto-scaled)
- Use element bounds from `ui_tree` for precise clicking — click the center of the bounding box
- `get_screen_size` returns dimensions in the same coordinate space as screenshots
- For percentage-based estimation: think "this element is X% from left, Y% from top" then multiply by screenshot dimensions (1456, 819)

### Common Patterns

**Click a button found via ui_tree:**
```
tree = ui_tree()
# Find button with text "Save" in the tree
# Use its bounds center: x = (left + right) / 2, y = (top + bottom) / 2
click(x, y)
```

**Type into a field:**
```
focus_window(title: "Notepad")
click(x, y)           # click the text field
select_all()           # select existing text
type(text: "new text") # replace with new text
```

**Switch apps and interact:**
```
focus_window(title: "Chrome")
hotkey(keys: ["ctrl", "l"])  # focus address bar
type(text: "example.com")
press_key(key: "enter")
```

**Draw a line in Paint:**
```
focus_window(title: "Paint")
drag(startX: 100, startY: 200, endX: 400, endY: 200, duration: 500)
```

### Precision Clicking with screenshot_region

When you need to click a small target precisely (icons, small buttons, checkboxes), use `screenshot_region` to zoom in and compute exact coordinates:

```
1. screenshot()                                    ← see the full screen
2. Spot the target area, estimate rough bounds
3. screenshot_region(min_x=400, min_y=300,         ← zoom into a tight region
                     max_x=520, max_y=390)            (120x90 in screenshot space)
4. The returned image is at native resolution       ← much more detail
   (e.g. 316x238 pixels for a 120x90 region)
5. Find the target at pixel (px, py) in the crop
6. Convert back to screenshot coordinates:
     screen_x = min_x + (px / image_width)  * (max_x - min_x)
     screen_y = min_y + (py / image_height) * (max_y - min_y)
7. focus_window(title: "target app")
8. click(x: screen_x, y: screen_y)                ← precise click
```

**Why this works:** The full screenshot is 1456x819 but the actual screen might be 3840x2160. A 120x90 region in screenshot space maps to ~316x238 native pixels. You see 2-3x more detail, so your coordinate estimation is 2-3x more precise.

**Tip:** Use regions of about 100-200 units in screenshot space. Too large defeats the purpose, too small might miss the target.

### Tips

- **Always focus_window first** — clicks land on the focused window
- **Prefer hotkey over hold_key/release_key** — it's atomic and more reliable
- **For text input:** Click the target field first, then `type`
- **Verify after acting** — take a screenshot to confirm the action worked
- **If a click seems to do nothing** — check if the right window is focused with `active_window`
- **For precise clicks** — use `screenshot_region` workflow above instead of guessing from the full screenshot
