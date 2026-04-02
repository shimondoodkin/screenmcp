# Windows Desktop Guide

How to navigate and operate a Windows desktop using ScreenMCP tools.

## Desktop Layout

### Taskbar (Bottom of Screen)
- **Start button** — far left. Opens Start menu. Equivalent: `hotkey ["win"]` or `home`.
- **Search** — next to Start. Click to search apps and files. Or: `hotkey ["win", "s"]`.
- **Pinned apps** — icons in the middle of taskbar. Click to launch or switch to an app.
- **Running apps** — shown with an underline indicator. Click to switch.
- **System tray** — far right. Clock, network, volume, battery, notification icons.
- **Show desktop** — very far right edge. Or: `hotkey ["win", "d"]`.

### Window Anatomy
- **Title bar** — top of window. Shows app name and document title. Drag to move window.
- **Window controls** — top-right corner:
  - **Minimize** (—) — hides window to taskbar
  - **Maximize/Restore** (square icon) — toggles fullscreen. Or: `hotkey ["win", "up"]`
  - **Close** (X) — closes window. Or: `hotkey ["alt", "F4"]`
- **Menu bar** — below title bar (File, Edit, View, Help...)
- **Scrollbars** — right edge and bottom edge for scrollable content
- **Resize handles** — drag any window edge or corner to resize

## Keyboard Shortcuts (use with `hotkey`)

### Essential
| Keys | Action |
|------|--------|
| `["ctrl", "c"]` | Copy |
| `["ctrl", "v"]` | Paste |
| `["ctrl", "x"]` | Cut |
| `["ctrl", "a"]` | Select all |
| `["ctrl", "z"]` | Undo |
| `["ctrl", "y"]` | Redo |
| `["ctrl", "s"]` | Save |
| `["ctrl", "f"]` | Find |
| `["ctrl", "w"]` | Close tab |
| `["ctrl", "t"]` | New tab (browsers) |
| `["ctrl", "n"]` | New window |
| `["ctrl", "p"]` | Print |

### Window Management
| Keys | Action |
|------|--------|
| `["alt", "tab"]` | Switch between windows |
| `["alt", "F4"]` | Close current window |
| `["win", "d"]` | Show desktop / restore all windows |
| `["win", "e"]` | Open File Explorer |
| `["win", "r"]` | Open Run dialog |
| `["win", "l"]` | Lock screen |
| `["win", "i"]` | Open Settings |
| `["win", "up"]` | Maximize window |
| `["win", "down"]` | Restore / minimize window |
| `["win", "left"]` | Snap window to left half |
| `["win", "right"]` | Snap window to right half |
| `["win", "shift", "s"]` | Screenshot snip tool |
| `["ctrl", "shift", "esc"]` | Open Task Manager |

### Navigation
| Keys | Action |
|------|--------|
| `["alt", "left"]` | Back (browsers, File Explorer) |
| `["alt", "right"]` | Forward |
| `["tab"]` | Next field / element |
| `["shift", "tab"]` | Previous field / element |
| `["enter"]` | Activate focused button / confirm dialog |
| `["escape"]` | Cancel / close dialog / close menu |
| `["F2"]` | Rename selected file |
| `["F5"]` | Refresh |
| `["F11"]` | Toggle fullscreen (browsers) |

### Text Editing
| Keys | Action |
|------|--------|
| `["ctrl", "left"]` | Move cursor one word left |
| `["ctrl", "right"]` | Move cursor one word right |
| `["ctrl", "shift", "left"]` | Select word left |
| `["ctrl", "shift", "right"]` | Select word right |
| `["home"]` | Go to beginning of line |
| `["end"]` | Go to end of line |
| `["ctrl", "home"]` | Go to beginning of document |
| `["ctrl", "end"]` | Go to end of document |
| `["shift", "home"]` | Select to beginning of line |
| `["shift", "end"]` | Select to end of line |

## Common UI Patterns

### Dialog Boxes
- **OK / Cancel dialogs** — buttons usually at bottom-right. `enter` confirms, `escape` cancels.
- **Save dialogs** — have a file name field and a "Save" button. Navigate folders in the left pane.
- **File Open dialogs** — same structure. Double-click files or type path in address bar.

### Context Menus
- **Right-click** any element to open context menu.
- Context menus have items like Cut, Copy, Paste, Delete, Properties, etc.
- Click outside the menu or press `escape` to dismiss.

### Dropdown / Combo Boxes
- Click to open the dropdown list.
- Click an item to select it.
- Or: focus the dropdown, then use `up`/`down` arrow keys.

### Checkboxes and Radio Buttons
- Click to toggle. Or: focus and press `space`.
- Checkboxes: multiple can be selected.
- Radio buttons: only one in a group can be selected.

### Tabs
- Click a tab to switch to it.
- Browsers: `["ctrl", "tab"]` for next tab, `["ctrl", "shift", "tab"]` for previous.

### Tree Views (File Explorer, Settings)
- Click arrow to expand/collapse.
- Or: focus and use `left`/`right` arrow keys.
- `right` expands, `left` collapses or goes to parent.

### Ribbons (Office apps)
- Click a ribbon tab (Home, Insert, etc.) to show its tools.
- Tools are organized in groups with labeled buttons.

## Common Operations

### Opening an Application
1. `hotkey ["win"]` to open Start menu
2. `type` the app name
3. `press_key "enter"` to launch

Or use `list_windows` + `focus_window` if the app is already running.

### Switching Between Windows
- **Quick toggle:** `hotkey ["alt", "tab"]`
- **Programmatic:** `list_windows` to find by title, then `focus_window`
- **Taskbar:** Click the app icon on the taskbar

### Entering Text in a Field
1. `click` on the text field
2. `select_all` to select existing text (if replacing)
3. `type` the new text

### Copying Text from Screen
1. `click` at the start of the text
2. Use `drag` to select, or `select_all` for the whole field
3. `copy` with `return_text: true` to get the text

### Saving a File
1. `hotkey ["ctrl", "s"]`
2. If it's a new file, a Save As dialog appears
3. `type` the filename in the name field
4. `click` the Save button or `press_key "enter"`

### Scrolling Through Content
- **Smooth scroll:** `scroll` with `dy` (negative = scroll content up, positive = scroll content down)
- **Page scroll:** `press_key "pagedown"` or `press_key "pageup"`
- **To top/bottom:** `hotkey ["ctrl", "home"]` or `hotkey ["ctrl", "end"]`

### Managing Files in Explorer
- **Open Explorer:** `hotkey ["win", "e"]`
- **Navigate:** Click folders or type path in address bar
- **Select file:** Click it. Multi-select: hold Ctrl and click.
- **Rename:** Select file, `press_key "F2"`, `type` new name, `press_key "enter"`
- **Delete:** Select file, `press_key "delete"`
- **Copy/Paste files:** Select, `hotkey ["ctrl", "c"]`, navigate, `hotkey ["ctrl", "v"]`

### Working with the System Tray
- Click the **^** arrow to show hidden tray icons
- Right-click a tray icon for its menu
- ScreenMCP's own tray icon is in the system tray

## Elevation (Admin Privileges)

Some actions require administrator privileges:
- Installing software
- Modifying system settings
- Interacting with elevated windows (Task Manager when opened by admin, UAC prompts)

When UI elements don't respond to clicks or `ui_tree` returns empty for a window, it may be running elevated. Use `is_elevated` to check current status, and `elevate` to request admin privileges if needed.

## Multi-Monitor

- `get_screen_size` returns the primary monitor dimensions.
- Secondary monitors may be to the left (negative x coordinates) or above (negative y coordinates).
- `ui_tree` and `screenshot` cover the primary monitor by default.
- Use coordinates from `list_windows` to understand window placement across monitors.

## Precise Clicking with screenshot_region

The default screenshot is 1456x819 but the actual screen may be 3840x2160 or higher. When clicking small targets (icons, checkboxes, small buttons), your coordinate estimates from the full screenshot may be off by several pixels.

**Use `screenshot_region` to zoom in and click precisely:**

1. Take a full `screenshot` to locate the general area
2. Call `screenshot_region(min_x, min_y, max_x, max_y)` with a tight box (100-200 units wide) around the target
3. The returned image is at native resolution — much more detail than the full screenshot
4. Find the target pixel `(px, py)` in the cropped image
5. Convert back to screenshot coordinates:
   ```
   screen_x = min_x + (px / image_width)  * (max_x - min_x)
   screen_y = min_y + (py / image_height) * (max_y - min_y)
   ```
6. `focus_window` then `click(screen_x, screen_y)`

**Example:** A 120x90 region in screenshot space becomes ~316x238 native pixels on a 4K display. Each pixel you estimate maps to a smaller real area, so clicks land 2-3x closer to the target.
