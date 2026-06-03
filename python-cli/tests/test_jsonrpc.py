import io
import json
import screenmcp_cli as app


def run_lines(lines):
    """Feed JSON-RPC request dicts, return list of response dicts."""
    stdin = io.StringIO("".join(json.dumps(l) + "\n" for l in lines))
    stdout = io.StringIO()
    app.serve(stdin, stdout)
    out = []
    for raw in stdout.getvalue().splitlines():
        if raw.strip():
            out.append(json.loads(raw))
    return out


def test_initialize_advertises_tools():
    resp = run_lines([{"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}}])
    assert resp[0]["id"] == 1
    assert resp[0]["result"]["capabilities"]["tools"] == {}
    assert resp[0]["result"]["serverInfo"]["name"] == "screenmcp-cli"


def test_notifications_initialized_produces_no_response():
    resp = run_lines([{"jsonrpc": "2.0", "method": "notifications/initialized"}])
    assert resp == []


def test_unknown_method_returns_method_not_found():
    resp = run_lines([{"jsonrpc": "2.0", "id": 7, "method": "bogus"}])
    assert resp[0]["error"]["code"] == -32601


def test_ping_returns_empty_result():
    resp = run_lines([{"jsonrpc": "2.0", "id": 3, "method": "ping"}])
    assert resp[0]["result"] == {}
