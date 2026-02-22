# ScreenMCP Skill

Control Android phones and desktops from any AI coding agent (Claude Code, Cursor, Cline, etc.) via MCP.

## Quick Start

### Claude Code

```bash
# Clone and cd into the skill directory
git clone https://github.com/nicedoctor/screenmcp.git
cd screenmcp/skill

# Set your API key (get one at https://screenmcp.com)
export SCREENMCP_API_KEY=pk_your_key_here

# Claude Code auto-detects .mcp.json and SKILL.md
claude
> take a screenshot of my phone
```

Or add the MCP server manually:

```bash
claude mcp add screenmcp --transport streamable-http --url https://screenmcp.com/mcp --header "Authorization: Bearer pk_your_key"
```

### Other Agents (Cursor, Cline, etc.)

Add to your MCP config:

```json
{
  "mcpServers": {
    "screenmcp": {
      "type": "streamable-http",
      "url": "https://screenmcp.com/mcp",
      "headers": {
        "Authorization": "Bearer pk_your_key_here"
      }
    }
  }
}
```

## How It Works

```
AI Agent (Claude Code / Cursor)
    │
    │  MCP tools (screenshot, click, type, ui_tree, ...)
    ▼
ScreenMCP Server (screenmcp.com/mcp)
    │
    │  WebSocket relay
    ▼
Phone / Desktop (ScreenMCP app)
```

1. Agent calls MCP tools like `screenshot`, `click`, `ui_tree`
2. Server relays commands to your connected device via WebSocket
3. Device executes and returns results (screenshots, UI trees, etc.)

## Getting an API Key

1. Go to [screenmcp.com](https://screenmcp.com) and sign in with Google
2. Dashboard → API Keys → Create New Key
3. Copy the `pk_...` key

## Connecting a Phone

1. Download the ScreenMCP app from the Dashboard
2. Open it, enable "Open Source Server" mode
3. Enter your User ID and server URL
4. Grant Accessibility and Screen Capture permissions

## Available Tools

`list_devices`, `screenshot`, `ui_tree`, `click`, `long_click`, `scroll`, `drag`, `type`, `get_text`, `select_all`, `copy`, `paste`, `get_clipboard`, `set_clipboard`, `back`, `home`, `recents`, `camera`, `list_cameras`, `hold_key`, `release_key`, `press_key`, `right_click`, `middle_click`, `mouse_scroll`

See [SKILL.md](SKILL.md) for full documentation.
