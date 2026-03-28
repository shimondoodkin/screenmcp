# ScreenMCP Local — Desktop Control via MCP

You have access to ScreenMCP tools that let you see and control a Windows desktop. These tools connect to the ScreenMCP app running locally.

## Available Tools

### Vision
- **screenshot** — Take a screenshot. Returns base64 WebP image. Use `max_width`/`max_height` to control size.
- **ui_tree** — Get the accessibility tree. Returns structured data with element names, types, bounds (x,y,width,height), text content, and clickable/focusable state. Excellent for finding interactive elements without visual inspection.

### Mouse
- **click** — Left-click at (x, y). Optional `duration` in ms.
- **right_click** — Right-click at (x, y). Opens context menus.
- **double_click** — Double-click at (x, y). Opens files, selects words.
- **long_click** — Long press at (x, y). Default 1000ms hold.
- **middle_click** — Middle-click at (x, y).
- **mouse_move** — Move cursor to (x, y) without clicking. Useful for hover actions.
- **drag** — Drag from (startX, startY) to (endX, endY). Optional `duration`.
- **scroll** — Scroll at (x, y) with `dx`/`dy` deltas, or use `direction` (up/down/left/right) + `amount`.
- **mouse_scroll** — Raw mouse wheel scroll at coordinates.

### Keyboard
- **type** — Type text into the focused field. Handles special characters.
- **press_key** — Press and release a single key. Key names: shift, ctrl, alt, meta/win, tab, enter, escape, space, backspace, delete, home, end, pageup, pagedown, up, down, left, right, f1-f12, or a single character.
- **hold_key** / **release_key** — Hold and release keys manually. Use for complex sequences.
- **hotkey** — Press a key combination atomically. Pass an array of key names: `["ctrl", "c"]`, `["alt", "tab"]`, `["win", "d"]`. Preferred over hold_key/release_key for standard shortcuts.

### Text & Clipboard
- **get_text** — Get text from the focused field (reads clipboard).
- **select_all** — Select all text (Ctrl+A).
- **copy** — Copy selection (Ctrl+C). Set `return_text: true` to get the copied text back.
- **paste** — Paste (Ctrl+V). Optionally pass `text` to set clipboard before pasting.
- **get_clipboard** — Read clipboard contents.
- **set_clipboard** — Set clipboard to given text.

### Window Management
- **list_windows** — List all visible on-screen windows with title, position, size, minimized/maximized state, and index.
- **focus_window** — Bring a window to front by `title` (substring match) or `index` from list_windows.
- **get_screen_size** — Get primary screen dimensions (width, height, x, y).

### Navigation
- **back** — Browser back / general back (Alt+Left).
- **home** — Press Windows key (opens Start menu).
- **recents** — Show recent windows (Alt+Tab).

### System
- **elevate** — Request administrator privileges. Shows a confirmation dialog to the user. Needed for interacting with elevated windows.
- **is_elevated** — Check if running with admin privileges.
- **camera** / **list_cameras** — Capture from connected cameras.
- **play_audio** — Play base64-encoded WAV/MP3 audio.

## How to Use These Tools

### Understanding the Screen

Use both **screenshot** and **ui_tree** as needed:

- **screenshot** gives you a visual picture of what's on screen. Great for understanding layout, reading visual content (images, charts, web pages), and verifying actions worked.
- **ui_tree** gives you structured data about every UI element. Great for finding buttons, text fields, labels, and their exact coordinates. Elements include bounds, text, control type, and interaction state.

Use whichever fits the situation. Often you'll use both — ui_tree to find elements and their coordinates, screenshot to verify visual state.

### Taking Actions

1. Identify the target — use ui_tree to find elements or screenshot to see the screen
2. Act — click, type, hotkey, etc.
3. Verify — take a screenshot or check ui_tree to confirm the action worked

### Coordinates

All coordinates are in screen pixels. (0,0) is the top-left corner of the primary monitor. Multi-monitor setups may have negative coordinates for monitors to the left/above.

Use `get_screen_size` to know the screen bounds. Use element bounds from `ui_tree` for precise clicking — click the center of an element's bounding box.

### Tips

- **Prefer hotkey over hold_key/release_key** for standard shortcuts. It's atomic and more reliable.
- **For text input:** Click the target field first, then use `type`. To replace existing text: `select_all` then `type`.
- **For window switching:** Use `list_windows` + `focus_window` for programmatic control, or `hotkey ["alt", "tab"]` for quick toggle.
- **Screenshots can be large.** Use `max_width` and `max_height` to keep them manageable.
