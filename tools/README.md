# ScreenMCP Timing Tool

CLI tool that measures latency at every step of the ScreenMCP connection and command pipeline.

## Setup

```bash
cd tools
npm install
```

## Usage

```bash
node timing.mjs <api-url> <api-key> <device-id> [--loop N]
```

**Arguments:**

| Arg | Description |
|-----|-------------|
| `api-url` | MCP server URL (e.g. `http://localhost:3000` or `https://screenmcp.com`) |
| `api-key` | API key or user ID for auth (from `worker.toml` or cloud dashboard) |
| `device-id` | Target device hex ID (shown in the Android app or `~/.screenmcp/worker.toml`) |
| `--loop N` | Send N screenshot commands to compare first vs subsequent (default: 1) |

## Examples

```bash
# Single screenshot, local server
node timing.mjs http://localhost:3000 pk_abc123 a1b2c3d4

# 5 screenshots, cloud server
node timing.mjs https://screenmcp.com pk_abc123 a1b2c3d4 --loop 5
```

## Output

Each step prints a timestamped log line with elapsed time:

```
[12:34:56.789] === ScreenMCP Timing Client ===
[12:34:56.790] DISCOVER: POST http://localhost:3000/api/discover
[12:34:56.850] DISCOVER: got wsUrl=ws://localhost:8080 (60ms)

[12:34:56.851] WS_CONNECT: opening ws://localhost:8080
[12:34:56.870] WS_OPEN: connected (19ms)
[12:34:56.871] AUTH: sending auth message
[12:34:56.880] AUTH_OK: phone_connected=true (29ms)

[12:34:56.881] CMD_SEND: screenshot (quality=80, max_width=720)
[12:34:56.890] CMD_ACCEPTED: id=1 (9ms)
[12:34:57.200] CMD_RESPONSE: status=ok (319ms, 85KB)

[12:34:57.201] === Summary ===
[12:34:57.201] Total time: 411ms
[12:34:57.201] Screenshot time: 319ms
```

With `--loop`, a summary shows avg/min/max across iterations.

## What it measures

| Step | What happens |
|------|-------------|
| `DISCOVER` | `POST /api/discover` — server looks up worker URL, sends SSE event to phone |
| `WS_CONNECT` | Opens raw WebSocket to the worker relay |
| `AUTH` | Sends auth message, worker verifies token |
| `WAIT_PHONE` | If phone wasn't connected, waits for it to come online (up to 30s) |
| `CMD_SEND` → `CMD_ACCEPTED` | Worker queues command for the phone |
| `CMD_ACCEPTED` → `CMD_RESPONSE` | Phone captures screenshot, compresses, sends back |
