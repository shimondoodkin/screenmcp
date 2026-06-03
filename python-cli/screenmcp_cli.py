#!/usr/bin/env python3
# === IMPORTS & CONSTANTS ===
import sys
import json

VERSION = "0.1.0"
SERVER_NAME = "screenmcp-cli"
DEFAULT_W = 1456
DEFAULT_H = 819

# Populated in the REGISTRY section near the bottom of the file.
TOOLS = {}


# === COORDINATE SCALING ===
def scaled_space(maxw, maxh):
    """Resolve the screenshot-space dimensions. None -> default; 0 kept as 0 (native)."""
    w = DEFAULT_W if maxw is None else maxw
    h = DEFAULT_H if maxh is None else maxh
    return (w, h)


def _axis(coord, native_dim, scaled_dim):
    if scaled_dim == 0:           # 0 => native passthrough, no scaling
        return int(round(coord))
    return int(round(coord * native_dim / scaled_dim))


def to_native(x, y, native, maxw=None, maxh=None):
    """Map a coordinate from screenshot space to native screen pixels."""
    sw, sh = scaled_space(maxw, maxh)
    return (_axis(x, native[0], sw), _axis(y, native[1], sh))


# === JSON-RPC STDIO SERVER ===
def _result(rpc_id, result):
    return {"jsonrpc": "2.0", "id": rpc_id, "result": result}


def _error(rpc_id, code, message):
    return {"jsonrpc": "2.0", "id": rpc_id, "error": {"code": code, "message": message}}


def _handle(msg):
    """Return a response dict, or None for notifications/no-reply."""
    method = msg.get("method")
    rpc_id = msg.get("id")
    if method == "initialize":
        return _result(rpc_id, {
            "protocolVersion": "2024-11-05",
            "capabilities": {"tools": {}},
            "serverInfo": {"name": SERVER_NAME, "version": VERSION},
        })
    if method == "notifications/initialized":
        return None
    if method == "ping":
        return _result(rpc_id, {})
    if method == "tools/list":
        return _result(rpc_id, {"tools": _list_tools()})
    if method == "tools/call":
        return _call_tool(rpc_id, msg.get("params") or {})
    if rpc_id is None:
        return None
    return _error(rpc_id, -32601, f"Method not found: {method}")


def _list_tools():
    return [
        {"name": n, "description": t["description"], "inputSchema": t["inputSchema"]}
        for n, t in TOOLS.items()
    ]


def _call_tool(rpc_id, params):
    name = params.get("name")
    args = params.get("arguments") or {}
    tool = TOOLS.get(name)
    if tool is None:
        return _error(rpc_id, -32602, f"Unknown tool: {name}")
    try:
        return _result(rpc_id, tool["handler"](args))
    except Exception as exc:  # surfaced as an MCP tool error, not a transport error
        return _result(rpc_id, {
            "content": [{"type": "text", "text": f"{type(exc).__name__}: {exc}"}],
            "isError": True,
        })


def serve(stdin, stdout):
    for raw in stdin:
        raw = raw.strip()
        if not raw:
            continue
        try:
            msg = json.loads(raw)
        except json.JSONDecodeError:
            stdout.write(json.dumps(_error(None, -32700, "Parse error")) + "\n")
            stdout.flush()
            continue
        resp = _handle(msg)
        if resp is not None:
            stdout.write(json.dumps(resp) + "\n")
            stdout.flush()


# === MAIN ===
def run():
    serve(sys.stdin, sys.stdout)


if __name__ == "__main__":
    run()
