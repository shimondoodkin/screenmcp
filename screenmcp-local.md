# ScreenMCP Local Mode

**Direct desktop control without the worker/relay.** The ScreenMCP Windows app embeds an HTTP server that AI assistants and scripts can connect to directly on localhost.

```
AI Assistant (Claude Code, Cursor, etc.)
        |
        |  MCP Streamable HTTP (POST /mcp)
        |  or REST API (POST /command)
        |
   ScreenMCP Windows App
   (127.0.0.1:6767)
        |
   Desktop Screen
```

No worker, no MCP server process, no cloud. Just the Windows app and your AI assistant.

---

## Setup

### 1. Enable Local Mode

Right-click the ScreenMCP tray icon > **Local Mode Settings...**

- Enter an API key (or click **Generate** to create one)
- Port defaults to `6767`
- Click **Save** and restart the app

Or edit `~/.screenmcp/config.toml` directly:

```toml
local_mode_key = "your-api-key-here"
local_mode_port = 6767
```

### 2. Configure Your AI Client

#### Claude Code

Add to your Claude Code MCP settings:

```json
{
  "mcpServers": {
    "screenmcp": {
      "type": "url",
      "url": "http://127.0.0.1:6767/mcp",
      "headers": {
        "Authorization": "Bearer your-api-key-here"
      }
    }
  }
}
```

#### Cursor / Other MCP Clients

Use the same MCP Streamable HTTP URL: `http://127.0.0.1:6767/mcp` with Bearer token auth.

---

## API Endpoints

### `GET /health`

Health check. Returns `{"status": "ok", "service": "screenmcp-local"}`.

### `POST /command` — REST API

Direct command execution for scripts and simple integrations.

```bash
curl -X POST http://127.0.0.1:6767/command \
  -H "Authorization: Bearer your-api-key" \
  -H "Content-Type: application/json" \
  -d '{"cmd": "screenshot", "params": {"max_width": 1920}}'
```

**Response:**
```json
{"status": "ok", "result": {"image": "base64...", "width": 1920, "height": 1080}}
```

### `POST /mcp` — MCP Streamable HTTP

Full MCP protocol for AI assistants. Handles JSON-RPC 2.0 methods: `initialize`, `tools/list`, `tools/call`.

---

## Available Commands

### Vision
| Command | Parameters | Description |
|---------|-----------|-------------|
| `screenshot` | `quality?`, `max_width?`, `max_height?` | Take a screenshot, returns base64 WebP |
| `screenshot_window` | `title?`, `index?`, `max_width?`, `max_height?` | Capture a specific window without focusing it |
| `ui_tree` | | Get accessibility tree with element bounds, text, and state |

### Mouse
| Command | Parameters | Description |
|---------|-----------|-------------|
| `click` | `x`, `y`, `duration?` | Left-click at coordinates |
| `right_click` | `x`, `y` | Right-click at coordinates |
| `double_click` | `x`, `y` | Double-click at coordinates |
| `long_click` | `x`, `y`, `duration?` | Long press (default 1000ms) |
| `middle_click` | `x`, `y` | Middle-click at coordinates |
| `mouse_move` | `x`, `y` | Move cursor without clicking |
| `drag` | `startX`, `startY`, `endX`, `endY`, `duration?` | Drag between points |
| `scroll` | `x?`, `y?`, `dx?`, `dy?`, `direction?`, `amount?` | Scroll at coordinates |
| `mouse_scroll` | `x`, `y`, `dx`, `dy` | Mouse wheel scroll |

### Keyboard
| Command | Parameters | Description |
|---------|-----------|-------------|
| `type` | `text` | Type text into the focused field |
| `press_key` | `key` | Press and release a key |
| `hold_key` | `key` | Hold a key down |
| `release_key` | `key` | Release a held key |
| `hotkey` | `keys` (array) | Press key combination atomically, e.g. `["ctrl", "c"]` |

**Key names:** `shift`, `ctrl`, `alt`, `meta`/`win`, `tab`, `enter`, `escape`, `space`, `backspace`, `delete`, `home`, `end`, `pageup`, `pagedown`, `up`, `down`, `left`, `right`, `f1`-`f12`, or any single character.

### Text & Clipboard
| Command | Parameters | Description |
|---------|-----------|-------------|
| `get_text` | | Get text from focused field |
| `select_all` | | Select all text (Ctrl+A) |
| `copy` | `return_text?` | Copy selection (Ctrl+C) |
| `paste` | `text?` | Paste (Ctrl+V), optionally set clipboard first |
| `get_clipboard` | | Read clipboard contents |
| `set_clipboard` | `text` | Set clipboard text |

### Window Management
| Command | Parameters | Description |
|---------|-----------|-------------|
| `list_windows` | | List visible on-screen windows |
| `focus_window` | `title?`, `index?` | Bring window to foreground |
| `active_window` | | Get the currently active window's title, position, size |
| `get_screen_size` | | Get primary screen dimensions |

### Navigation
| Command | Parameters | Description |
|---------|-----------|-------------|
| `back` | | Browser back (Alt+Left) |
| `home` | | Windows key (Start menu) |
| `recents` | | Alt+Tab |

### System
| Command | Parameters | Description |
|---------|-----------|-------------|
| `elevate` | | Request admin privileges (shows user confirmation dialog) |
| `is_elevated` | | Check if running elevated |
| `camera` | `camera?`, `quality?`, `max_width?`, `max_height?` | Capture from camera |
| `list_cameras` | | List available cameras |
| `play_audio` | `audio_data`, `volume?` | Play base64 WAV/MP3 |

---

## Python Usage

```python
import requests, base64

URL = "http://127.0.0.1:6767"
KEY = "your-api-key"
HEADERS = {"Authorization": f"Bearer {KEY}", "Content-Type": "application/json"}

def cmd(name, params=None):
    body = {"cmd": name}
    if params:
        body["params"] = params
    r = requests.post(f"{URL}/command", json=body, headers=HEADERS)
    r.raise_for_status()
    d = r.json()
    if d.get("status") == "error":
        raise RuntimeError(d["error"])
    return d.get("result", {})

# Take screenshot
result = cmd("screenshot", {"max_width": 1920})
with open("screen.webp", "wb") as f:
    f.write(base64.b64decode(result["image"]))

# Click and type
cmd("click", {"x": 500, "y": 300})
cmd("type", {"text": "Hello!"})

# Keyboard shortcut
cmd("hotkey", {"keys": ["ctrl", "s"]})

# List and focus windows
windows = cmd("list_windows")["windows"]
cmd("focus_window", {"title": "Notepad"})

# Get active window
active = cmd("active_window")
print(f"Active: {active['title']}")
```

---

## Security

- The server only listens on `127.0.0.1` (localhost) — not accessible from the network
- All requests require the API key via `Authorization: Bearer <key>` header
- Empty key = local mode disabled entirely

---

## Skills

Skill files for AI assistants are in `screenmcp-local/skills/`:

- **`skill.md`** — Main tool reference and usage patterns
- **`windows.md`** — Comprehensive Windows desktop navigation guide with keyboard shortcuts
- **`python.md`** — Python scripting examples

Configure these as custom instructions/skills in your AI client to give it desktop navigation knowledge.
