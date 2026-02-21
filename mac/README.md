# ScreenMCP Mac

A macOS menu bar application that connects to the ScreenMCP worker as a "phone" device, allowing remote control of the Mac desktop (screenshots, mouse, keyboard, clipboard, window listing).

## Build

```bash
cd mac
cargo build --release
```

The binary will be at `target/release/screenmcp-mac`.

## Required macOS Permissions

ScreenMCP Mac requires two system permissions to function properly. macOS will prompt you to grant these the first time the app tries to use the relevant functionality.

### Screen Recording

Required for the `screenshot` command.

1. Open **System Settings** (or System Preferences on older macOS)
2. Go to **Privacy & Security > Screen Recording**
3. Click the **+** button and add `screenmcp-mac` (or your terminal if running from terminal)
4. Toggle the switch to enable it
5. You may need to restart the app after granting permission

### Accessibility

Required for mouse control (`click`, `drag`, `scroll`) and keyboard input (`type`, `select_all`, `copy`, `paste`, `back`, `home`, `recents`).

1. Open **System Settings** (or System Preferences on older macOS)
2. Go to **Privacy & Security > Accessibility**
3. Click the **+** button and add `screenmcp-mac` (or your terminal if running from terminal)
4. Toggle the switch to enable it
5. You may need to restart the app after granting permission

## Configuration

The config file is located at:

```
~/Library/Application Support/screenmcp/config.toml
```

A default config file is created automatically on first run. Edit it to add your API token:

```toml
api_url = "https://server10.doodkin.com"
token = "pk_your_api_key_here"
auto_connect = true
screenshot_quality = 80
```

### Config Options

| Option | Description | Default |
|--------|-------------|---------|
| `api_url` | API server URL for worker discovery | `https://server10.doodkin.com` |
| `worker_url` | Direct worker WebSocket URL (bypasses discovery) | none |
| `token` | Auth token (API key starting with `pk_` or Firebase ID token) | empty |
| `auto_connect` | Connect automatically on startup | `true` |
| `screenshot_quality` | Screenshot quality (1-100) | `80` |
| `max_screenshot_width` | Max screenshot width in pixels (resizes if larger) | none |
| `max_screenshot_height` | Max screenshot height in pixels (resizes if larger) | none |

You can also open the config file from the menu bar icon by clicking **Open Config File**.

## Usage

1. Run the application: `./target/release/screenmcp-mac`
2. A colored circle appears in the macOS menu bar:
   - **Red** = disconnected
   - **Green** = connected
3. Click the icon to access the menu:
   - **Connect** / **Disconnect** - manage the WebSocket connection
   - **Status** - shows current connection state
   - **Open Config File** - opens config.toml in your default editor
   - **Quit** - exit the application

## Supported Commands

| Command | macOS Behavior |
|---------|---------------|
| `screenshot` | Captures the primary screen (requires Screen Recording permission) |
| `click` | Left-click at (x, y) coordinates |
| `long_click` | Press-and-hold left button at (x, y) |
| `drag` | Drag from (startX, startY) to (endX, endY) |
| `scroll` | Scroll by direction or dx/dy values |
| `type` | Type text string |
| `get_text` | Read clipboard contents |
| `select_all` | Cmd+A |
| `copy` | Cmd+C |
| `paste` | Cmd+V |
| `back` | Cmd+Left (browser/navigation back) |
| `home` | Cmd+H (hide current application) |
| `recents` | Cmd+Tab (application switcher) |
| `ui_tree` | List visible windows with titles and bounds |
| `right_click` | Right-click at (x, y) |
| `middle_click` | Middle-click at (x, y) |
| `mouse_scroll` | Scroll (alias for scroll) |
| `camera` | Not supported (returns unsupported) |

## macOS vs PC Differences

- Keyboard shortcuts use **Cmd** (Meta) instead of Ctrl
- `back` uses **Cmd+Left** instead of Alt+Left
- `home` uses **Cmd+H** (hide app) instead of Windows key
- `recents` uses **Cmd+Tab** instead of Alt+Tab
- `ui_tree` uses the macOS CGWindowListCopyWindowInfo API instead of Win32 EnumWindows
- Config is stored in `~/Library/Application Support/` (macOS standard) instead of `~/.config/`

## Environment Variables

- `RUST_LOG=screenmcp_mac=debug` - enable debug logging
- `RUST_LOG=screenmcp_mac=trace` - enable trace logging
