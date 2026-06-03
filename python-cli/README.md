# ScreenMCP — Python CLI (Standalone Local stdio MCP)

A single-file MCP server that controls **this** computer's desktop directly over
stdio. No worker, no relay, no auth, no network. Windows, macOS, Linux.

## Install

    cd screenmcp/python-cli
    pip install -e .            # core
    pip install -e ".[camera]"  # + camera (opencv-python)
    pip install -e ".[audio]"   # + play_audio (simpleaudio)

Or just run the file with the deps available:

    pip install mss pynput Pillow pyperclip pygetwindow
    python screenmcp_cli.py

## Configure an MCP client

Claude Code:

    claude mcp add screenmcp-local -- python /abs/path/to/screenmcp_cli.py

Or JSON (Claude Desktop `mcpServers`):

    {
      "mcpServers": {
        "screenmcp-local": {
          "command": "python",
          "args": ["/abs/path/to/screenmcp_cli.py"]
        }
      }
    }

## Coordinates

Screenshots default to 1456x819. Click/drag/scroll coordinates are in that space
and auto-scale to your real screen. Override per call with `max_width`/`max_height`
(0 = native pixels).

## macOS permissions

The first screenshot triggers a **Screen Recording** prompt; the first click/keypress
triggers an **Accessibility** prompt. Both attach to the app that launched python
(Terminal/iTerm/your MCP client). Grant them in
**System Settings → Privacy & Security → Screen Recording / Accessibility**, then
restart the launching app. Denied permissions surface as an `isError` tool result
naming the missing permission.

## Linux notes

Window management uses `wmctrl` (and `xdotool` for active window). Install them:
`sudo apt install wmctrl xdotool`. Wayland is best-effort; some window ops may
report `unsupported`.

## Unsupported / stubbed

- `ui_tree` — not implemented (out of scope).
- `elevate` — returns `unsupported` (cannot re-launch a live stdio process elevated).
- `camera` / `play_audio` — require the optional `[camera]` / `[audio]` extras.

## Tests

    pip install pytest
    cd screenmcp/python-cli
    python -m pytest

Unit tests mock the input/screenshot/window backends, so they run without a
display. `tests/test_stdio.py` spawns the server as a subprocess and exercises the
real JSON-RPC protocol end to end (needs the runtime deps installed in the
interpreter that runs pytest).
