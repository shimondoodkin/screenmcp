# ScreenMCP

**Give your AI assistant eyes and hands on your phone and desktop.**

ScreenMCP connects AI assistants to real device screens via the [Model Context Protocol (MCP)](https://modelcontextprotocol.io). Your AI can take screenshots, tap, swipe, type, scroll, capture from the camera, play audio, and interact with any app — just like a human would.

Works with Claude Desktop, Cursor, Claude Code, OpenClaw, and any MCP-compatible client.

---

## How it works

```
AI Assistant (Claude, Cursor, etc.)
        ↕ MCP
   MCP Server (Node.js)
        ↕ WebSocket
      Worker (Rust)
        ↕ WebSocket
  Phone / Desktop App
        ↕ Accessibility APIs
       Device Screen
```

1. Install the app on your phone or desktop
2. Configure the MCP server in your AI client
3. Your AI can now see and control the device

---

## Supported platforms

| Platform | Status | Notes |
|----------|--------|-------|
| Android  | ✅ Stable | Full accessibility service, all commands |
| Windows  | ✅ Stable | Win32 UI tree, system tray |
| Linux    | 🧪 Beta | wmctrl/xdotool, X11 |
| macOS    | 🧪 Beta | CoreGraphics, menu bar |
| iOS      | ❌ Not available | Apple restrictions |

---

## Quick Start (Self-hosted / Open Source)

### 1. Start the worker and MCP server

```bash
# Clone the repo
git clone https://github.com/shimondoodkin/screenmcp.git
cd screenmcp

# Start with Docker
docker compose up

# Or manually:
# Worker (Rust, port 8080)
cd worker && cargo run

# MCP Server (Node.js, port 3000)
cd mcp-server && npm install && npm run build && npm start
```

### 2. Configure auth (`~/.screenmcp/worker.toml`)

```toml
[user]
id = "your-secret-token"

[auth]
api_keys = ["pk_your_api_key"]

[server]
port = 3000
worker_url = "ws://localhost:8080"
```

### 3. Install the app

- **Android:** Download the APK from [Releases](https://github.com/shimondoodkin/screenmcp/releases)
  - Enable Accessibility Service in Android Settings
  - In the app: enable "Open Source Server", enter your server URL and `user.id` token

- **Windows:** Download the `.exe` from [Releases](https://github.com/shimondoodkin/screenmcp/releases)
  - In the app settings: enable "Open Source Server", enter your server URL and token

- **Linux:** Download the `.deb` from [Releases](https://github.com/shimondoodkin/screenmcp/releases)
  ```bash
  sudo dpkg -i screenmcp-linux_0.1.0_amd64.deb
  ```
  - Launch ScreenMCP from your app launcher or run `screenmcp-linux`
  - In the app settings: enable "Open Source Server", enter your server URL and token

- **macOS:** Download the `.dmg` from [Releases](https://github.com/shimondoodkin/screenmcp/releases)
  - Drag ScreenMCP to `/Applications`
  - Remove quarantine: `xattr -cr /Applications/ScreenMCP.app`
  - Grant Screen Recording and Accessibility permissions in System Settings
  - In the app settings: enable "Open Source Server", enter your server URL and token

### 4. Add to your AI client

**Claude Desktop** (`~/Library/Application Support/Claude/claude_desktop_config.json`):
```json
{
  "mcpServers": {
    "screenmcp": {
      "command": "npx",
      "args": ["-y", "screenmcp-client"],
      "env": {
        "SCREENMCP_URL": "http://localhost:3000",
        "SCREENMCP_API_KEY": "pk_your_api_key"
      }
    }
  }
}
```

Or use the hosted version at `https://mcp.screenmcp.com/mcp` — see [screenmcp.com](https://screenmcp.com) for cloud setup.

---

## Available commands

| Command | Description |
|---------|-------------|
| `screenshot` | Take a screenshot (WebP) |
| `click` | Tap at coordinates |
| `long_click` | Long press |
| `drag` | Drag gesture |
| `scroll` | Scroll the screen |
| `type` | Type text |
| `get_text` | Get selected text |
| `select_all` | Select all text |
| `copy` / `paste` | Clipboard operations |
| `back` / `home` / `recents` | Navigation buttons |
| `ui_tree` | Get accessibility UI tree |
| `camera` | Capture from front/rear camera |
| `play_audio` | Play audio on the device |

---

## Architecture

See [architecture.md](architecture.md) for the full technical overview including auth flow, WebSocket protocol, and config file format.

---

## Cloud version

[screenmcp.com](https://screenmcp.com) offers a hosted version with:
- No self-hosting required
- Firebase authentication
- Multi-device management dashboard
- Free tier available (100 commands/day, 2 devices)

---

## Contributing

Pull requests welcome! Please open an issue first for major changes.

- [Open an issue](https://github.com/shimondoodkin/screenmcp/issues)
- See [adding-new-command.md](adding-new-command.md) for how to add new device commands
- See [architecture.md](architecture.md) for system design

---

## License

MIT — see [LICENSE](LICENSE)
