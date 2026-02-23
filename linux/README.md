# ScreenMCP Linux

A Linux system tray application that connects to the ScreenMCP worker as a "phone" device, allowing remote control of the Linux desktop (screenshots, mouse, keyboard, clipboard, window listing).

## Build

```bash
cd linux
cargo build --release
```

The binary will be at `target/release/screenmcp-linux`.

## Prerequisites

- **X11 or Wayland** display server
- **System tray support** (most desktop environments include this)
- Optional: **wmctrl** or **xdotool** for `ui_tree` window listing

Install optional dependencies:
```bash
# Debian/Ubuntu
sudo apt install wmctrl xdotool

# Fedora
sudo dnf install wmctrl xdotool

# Arch
sudo pacman -S wmctrl xdotool
```

## Configuration

The config file is located at:

```
~/.config/screenmcp/config.toml
```

A default config file is created automatically on first run. Edit it to add your API token:

```toml
api_url = "https://screenmcp.com"
token = "pk_your_api_key_here"
auto_connect = true
screenshot_quality = 80
```

### Config Options

| Option | Description | Default |
|--------|-------------|---------|
| `api_url` | API server URL for worker discovery | `https://screenmcp.com` |
| `worker_url` | Direct worker WebSocket URL (bypasses discovery) | none |
| `token` | Auth token (API key starting with `pk_` or Firebase ID token) | empty |
| `auto_connect` | Connect automatically on startup | `true` |
| `screenshot_quality` | Screenshot quality (1-100) | `80` |
| `max_screenshot_width` | Max screenshot width in pixels (resizes if larger) | none |
| `max_screenshot_height` | Max screenshot height in pixels (resizes if larger) | none |

You can also open the config file from the tray icon by clicking **Open Config File**.

## Usage

1. Run the application: `./target/release/screenmcp-linux`
2. A colored circle appears in the system tray:
   - **Red** = disconnected
   - **Green** = connected
3. Click the icon to access the menu:
   - **Connect** / **Disconnect** - manage the WebSocket connection
   - **Status** - shows current connection state
   - **Open Config File** - opens config.toml in your default editor (via xdg-open)
   - **Quit** - exit the application

## Supported Commands

| Command | Linux Behavior |
|---------|---------------|
| `screenshot` | Captures the primary screen |
| `click` | Left-click at (x, y) coordinates |
| `long_click` | Press-and-hold left button at (x, y) |
| `drag` | Drag from (startX, startY) to (endX, endY) |
| `scroll` | Scroll by direction or dx/dy values |
| `type` | Type text string |
| `get_text` | Read clipboard contents |
| `select_all` | Ctrl+A |
| `copy` | Ctrl+C |
| `paste` | Ctrl+V |
| `back` | Alt+Left (browser/navigation back) |
| `home` | Super key (show activities/desktop) |
| `recents` | Alt+Tab (window switcher) |
| `ui_tree` | List visible windows via wmctrl/xdotool |
| `right_click` | Right-click at (x, y) |
| `middle_click` | Middle-click at (x, y) |
| `mouse_scroll` | Scroll (alias for scroll) |
| `hold_key` | Press and hold a key |
| `release_key` | Release a held key |
| `press_key` | Press and release a key |
| `camera` | Not supported (returns unsupported) |

## Linux vs Windows/Mac Differences

- Keyboard shortcuts use **Ctrl** (same as Windows)
- `back` uses **Alt+Left** (same as Windows)
- `home` uses **Super** key (shows activities on GNOME, start menu on KDE)
- `recents` uses **Alt+Tab** (same as Windows)
- `ui_tree` uses **wmctrl** (preferred) or **xdotool** (fallback) for window listing
- Config is stored in `~/.config/screenmcp/` (XDG standard)

## Environment Variables

- `RUST_LOG=screenmcp_linux=debug` - enable debug logging
- `RUST_LOG=screenmcp_linux=trace` - enable trace logging
