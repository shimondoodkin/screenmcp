import json
import subprocess
import sys
import os

HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def test_end_to_end_initialize_list_and_call():
    reqs = [
        {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}},
        {"jsonrpc": "2.0", "method": "notifications/initialized"},
        {"jsonrpc": "2.0", "id": 2, "method": "tools/list"},
        {"jsonrpc": "2.0", "id": 3, "method": "tools/call",
         "params": {"name": "get_screen_size", "arguments": {}}},
    ]
    stdin = "".join(json.dumps(r) + "\n" for r in reqs)
    proc = subprocess.run([sys.executable, "screenmcp_cli.py"],
                          input=stdin, capture_output=True, text=True, cwd=HERE, timeout=30)
    responses = [json.loads(l) for l in proc.stdout.splitlines() if l.strip()]
    by_id = {r.get("id"): r for r in responses}
    assert by_id[1]["result"]["serverInfo"]["name"] == "screenmcp-cli"
    assert len(by_id[2]["result"]["tools"]) >= 30
    # get_screen_size returns a text content block with width/height
    assert "width" in by_id[3]["result"]["content"][0]["text"]
