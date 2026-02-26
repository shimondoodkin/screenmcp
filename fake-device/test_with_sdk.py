"""End-to-end test: Python SDK -> MCP Server -> Worker -> Fake Device.

This script tests the ScreenMCP Python SDK against the full open-source stack
with the fake device acting as the phone.

Prerequisites:
  1. Worker running on ws://localhost:8080
  2. MCP server running on http://localhost:3000
  3. Fake device running and connected

Setup:
  # Terminal 1 — ensure config exists
  mkdir -p ~/.screenmcp
  cat > ~/.screenmcp/worker.toml << 'EOF'
  [user]
  id = "local-user"

  [auth]
  api_keys = ["pk_test123"]

  [devices]
  allowed = []

  [server]
  port = 3000
  worker_url = "ws://localhost:8080"
  EOF

  # Terminal 2 — start worker
  cd worker && cargo run

  # Terminal 3 — start MCP server
  cd mcp-server && npm run start

  # Terminal 4 — start fake device
  cd fake-device && pip install -e . && python -m fake_device \
      --api-url http://localhost:3000 \
      --device-id test-device-001 \
      --user-id local-user

  # Terminal 5 — run this test
  cd fake-device && pip install -e ../sdk/python && python test_with_sdk.py

Usage:
  python test_with_sdk.py [--api-url URL] [--api-key KEY] [--device-id ID]
"""

from __future__ import annotations

import argparse
import asyncio
import base64
import json
import logging
import sys
import traceback
from typing import Any

# ---------------------------------------------------------------------------
# Try importing the Python SDK.  If unavailable, fall back to raw HTTP/WS.
# ---------------------------------------------------------------------------
try:
    from screenmcp import (
        CommandError,
        ScreenMCPClient,
        ScreenMCPError,
        find_elements,
    )

    HAS_SDK = True
except ImportError:
    HAS_SDK = False

try:
    import httpx
    import websockets

    HAS_RAW_DEPS = True
except ImportError:
    HAS_RAW_DEPS = False


logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s %(levelname)-8s %(message)s",
    datefmt="%H:%M:%S",
)
log = logging.getLogger("test")


# ---------------------------------------------------------------------------
# Test result tracking
# ---------------------------------------------------------------------------
class TestResults:
    def __init__(self) -> None:
        self.passed: list[str] = []
        self.failed: list[tuple[str, str]] = []
        self.skipped: list[tuple[str, str]] = []

    def ok(self, name: str) -> None:
        log.info("  PASS  %s", name)
        self.passed.append(name)

    def fail(self, name: str, reason: str) -> None:
        log.error("  FAIL  %s: %s", name, reason)
        self.failed.append((name, reason))

    def skip(self, name: str, reason: str) -> None:
        log.warning("  SKIP  %s: %s", name, reason)
        self.skipped.append((name, reason))

    def summary(self) -> None:
        total = len(self.passed) + len(self.failed) + len(self.skipped)
        print("\n" + "=" * 60)
        print(f"Test Results: {len(self.passed)}/{total} passed", end="")
        if self.failed:
            print(f", {len(self.failed)} FAILED", end="")
        if self.skipped:
            print(f", {len(self.skipped)} skipped", end="")
        print()

        if self.failed:
            print("\nFailures:")
            for name, reason in self.failed:
                print(f"  - {name}: {reason}")

        if self.skipped:
            print("\nSkipped:")
            for name, reason in self.skipped:
                print(f"  - {name}: {reason}")

        print("=" * 60)

    @property
    def exit_code(self) -> int:
        return 1 if self.failed else 0


# ---------------------------------------------------------------------------
# SDK-based tests
# ---------------------------------------------------------------------------
async def test_with_python_sdk(
    api_url: str, api_key: str, device_id: str, results: TestResults
) -> None:
    """Run tests using the ScreenMCP Python SDK."""
    log.info("Testing with Python SDK (screenmcp)")
    log.info("  api_url=%s, device_id=%s", api_url, device_id)

    client = ScreenMCPClient(
        api_key=api_key,
        api_url=api_url,
        command_timeout=10.0,
        auto_reconnect=False,
    )

    # ── Test: list_devices ────────────────────────────────────────────
    try:
        devices = await client.list_devices()
        if isinstance(devices, list):
            results.ok(f"list_devices() -> {len(devices)} devices")
        else:
            results.fail("list_devices", f"Expected list, got {type(devices)}")
    except Exception as e:
        results.fail("list_devices", str(e))

    try:
        phone = await client.connect(device_id=device_id)
        log.info("  Connected to worker: %s", phone.worker_url)
    except Exception as e:
        results.fail("connect", f"Failed to connect: {e}")
        return

    # Wait for the fake device (phone) to connect via SSE → WS
    if not phone.phone_connected:
        log.info("  Waiting for phone to connect...")
        for _ in range(30):  # up to 15 seconds
            await asyncio.sleep(0.5)
            if phone.phone_connected:
                break
        if phone.phone_connected:
            log.info("  Phone connected!")
        else:
            results.fail("phone_connect", "Phone did not connect within 15s")
            await phone.disconnect()
            return

    # ── Test: screenshot ──────────────────────────────────────────────
    try:
        result = await phone.screenshot()
        image_b64 = result.get("image", "")
        if not image_b64:
            results.fail("screenshot", "No image data returned")
        else:
            raw = base64.b64decode(image_b64)
            # Check PNG signature
            if raw[:4] == b"\x89PNG":
                results.ok("screenshot (PNG image received)")
            else:
                results.ok(f"screenshot ({len(raw)} bytes, not PNG but valid base64)")
    except Exception as e:
        results.fail("screenshot", str(e))

    # ── Test: click ───────────────────────────────────────────────────
    try:
        result = await phone.click(540, 960)
        results.ok("click(540, 960)")
    except Exception as e:
        results.fail("click", str(e))

    # ── Test: type_text ───────────────────────────────────────────────
    try:
        result = await phone.type_text("hello world")
        results.ok("type_text('hello world')")
    except Exception as e:
        results.fail("type_text", str(e))

    # ── Test: ui_tree ─────────────────────────────────────────────────
    try:
        result = await phone.ui_tree()
        tree = result.get("tree", [])
        if not tree:
            results.fail("ui_tree", "Empty tree returned")
        else:
            # Validate structure
            first = tree[0] if isinstance(tree, list) else tree
            has_class = "className" in first
            has_bounds = "bounds" in first
            has_children = "children" in first
            if has_class and has_bounds and has_children:
                results.ok(f"ui_tree (root className={first.get('className')})")
            else:
                results.fail(
                    "ui_tree",
                    f"Unexpected structure: class={has_class}, bounds={has_bounds}, children={has_children}",
                )
    except Exception as e:
        results.fail("ui_tree", str(e))

    # ── Test: back ────────────────────────────────────────────────────
    try:
        result = await phone.back()
        results.ok("back()")
    except Exception as e:
        results.fail("back", str(e))

    # ── Test: home ────────────────────────────────────────────────────
    try:
        result = await phone.home()
        results.ok("home()")
    except Exception as e:
        results.fail("home", str(e))

    # ── Test: recents ─────────────────────────────────────────────────
    try:
        result = await phone.recents()
        results.ok("recents()")
    except Exception as e:
        results.fail("recents", str(e))

    # ── Test: long_click ──────────────────────────────────────────────
    try:
        result = await phone.long_click(100, 200)
        results.ok("long_click(100, 200)")
    except Exception as e:
        results.fail("long_click", str(e))

    # ── Test: scroll ──────────────────────────────────────────────────
    try:
        result = await phone.scroll("down", 500)
        results.ok("scroll('down', 500)")
    except Exception as e:
        results.fail("scroll", str(e))

    # ── Test: get_text ────────────────────────────────────────────────
    try:
        result = await phone.get_text()
        text = result.get("text", "")
        if text:
            results.ok(f"get_text() -> '{text}'")
        else:
            results.fail("get_text", "No text returned")
    except Exception as e:
        results.fail("get_text", str(e))

    # ── Test: copy ────────────────────────────────────────────────────
    try:
        result = await phone.copy(return_text=True)
        results.ok(f"copy(return_text=True) -> text={result.get('text', '(none)')}")
    except Exception as e:
        results.fail("copy", str(e))

    # ── Test: get_clipboard ───────────────────────────────────────────
    try:
        result = await phone.get_clipboard()
        results.ok(f"get_clipboard() -> '{result.get('text', '')}'")
    except Exception as e:
        results.fail("get_clipboard", str(e))

    # ── Test: set_clipboard ───────────────────────────────────────────
    try:
        result = await phone.set_clipboard("test clipboard content")
        results.ok("set_clipboard('test clipboard content')")
    except Exception as e:
        results.fail("set_clipboard", str(e))

    # ── Test: paste ───────────────────────────────────────────────────
    try:
        result = await phone.paste()
        results.ok("paste()")
    except Exception as e:
        results.fail("paste", str(e))

    # ── Test: select_all ──────────────────────────────────────────────
    try:
        result = await phone.select_all()
        results.ok("select_all()")
    except Exception as e:
        results.fail("select_all", str(e))

    # ── Test: list_cameras ────────────────────────────────────────────
    try:
        result = await phone.list_cameras()
        cameras = result.get("cameras", [])
        results.ok(f"list_cameras() -> {len(cameras)} cameras")
    except Exception as e:
        results.fail("list_cameras", str(e))

    # ── Test: camera ──────────────────────────────────────────────────
    try:
        result = await phone.camera("0")
        image_b64 = result.get("image", "")
        if image_b64:
            results.ok(f"camera('0') -> {len(image_b64)} base64 chars")
        else:
            results.fail("camera", "No image returned")
    except Exception as e:
        results.fail("camera", str(e))

    # ── Test: selector engine with ui_tree data ───────────────────────
    try:
        result = await phone.ui_tree()
        tree = result.get("tree", [])

        # Test text selector
        found = find_elements(tree, "text:Settings")
        if found and found[0].text == "Settings":
            results.ok(f"selector text:Settings -> ({found[0].x}, {found[0].y})")
        else:
            results.fail("selector text:Settings", f"Expected Settings, got {found}")

        # Test role selector
        found = find_elements(tree, "role:EditText")
        if found:
            results.ok(f"selector role:EditText -> {found[0].class_name}")
        else:
            results.fail("selector role:EditText", "No EditText found")

        # Test desc selector
        found = find_elements(tree, "desc:Home")
        if found:
            results.ok(f"selector desc:Home -> ({found[0].x}, {found[0].y})")
        else:
            results.fail("selector desc:Home", "No element with desc 'Home'")

        # Test id selector
        found = find_elements(tree, "id:com.android.systemui:id/back")
        if found:
            results.ok(f"selector id:...back -> ({found[0].x}, {found[0].y})")
        else:
            results.fail("selector id:...back", "No element found")

        # Test negation selector
        found = find_elements(tree, "!text:Settings&&role:TextView")
        chrome_found = any(e.text == "Chrome" for e in found)
        if chrome_found:
            results.ok("selector !text:Settings&&role:TextView -> found Chrome")
        else:
            results.fail("selector negation", f"Expected Chrome, got {[e.text for e in found]}")

    except Exception as e:
        results.fail("selector engine", str(e))

    # ── Test: find() fluent API ───────────────────────────────────────
    try:
        el = await phone.find("text:Chrome", timeout=2.0).element()
        if el.text == "Chrome":
            results.ok(f"find('text:Chrome').element() -> ({el.x}, {el.y})")
        else:
            results.fail("find fluent", f"Expected Chrome, got {el.text}")
    except Exception as e:
        results.fail("find fluent", str(e))

    # ── Test: exists() ────────────────────────────────────────────────
    try:
        exists = await phone.exists("text:Settings", timeout=1.0)
        if exists:
            results.ok("exists('text:Settings') -> True")
        else:
            results.fail("exists", "Expected True for Settings")
    except Exception as e:
        results.fail("exists", str(e))

    # ── Test: exists() for non-existent element ──────────────────────
    try:
        exists = await phone.exists("text:NonExistentElement", timeout=1.0)
        if not exists:
            results.ok("exists('text:NonExistentElement') -> False")
        else:
            results.fail("exists non-existent", "Expected False")
    except Exception as e:
        results.fail("exists non-existent", str(e))

    # ── Test: keyboard commands (desktop-style) ───────────────────────
    try:
        result = await phone.press_key("Enter")
        results.ok("press_key('Enter')")
    except Exception as e:
        results.fail("press_key", str(e))

    # ── Test: drag ────────────────────────────────────────────────────
    try:
        result = await phone.drag(100, 200, 500, 600)
        results.ok("drag(100, 200, 500, 600)")
    except Exception as e:
        results.fail("drag", str(e))

    # ── Test: unknown command returns error ────────────────────────────
    try:
        resp = await phone.send_command("totally_fake_command")
        results.fail("unknown_command", "Expected CommandError but got success")
    except CommandError as e:
        results.ok(f"unknown command raises CommandError: {e}")
    except Exception as e:
        results.fail("unknown_command", f"Unexpected error type: {type(e).__name__}: {e}")

    # Clean up
    await phone.disconnect()
    log.info("  Disconnected from worker")


# ---------------------------------------------------------------------------
# Raw WebSocket tests (fallback if SDK not available)
# ---------------------------------------------------------------------------
async def test_with_raw_ws(
    api_url: str, api_key: str, device_id: str, results: TestResults
) -> None:
    """Run tests using raw HTTP + WebSocket (no SDK dependency)."""
    log.info("Testing with raw HTTP/WebSocket (SDK not available)")

    # Discover worker URL
    async with httpx.AsyncClient() as http:
        resp = await http.post(
            f"{api_url}/api/discover",
            headers={
                "Authorization": f"Bearer {api_key}",
                "Content-Type": "application/json",
            },
            json={"device_id": device_id},
        )

    if resp.status_code != 200:
        results.fail("discover", f"HTTP {resp.status_code}: {resp.text}")
        return

    data = resp.json()
    ws_url = data.get("wsUrl", "")
    if not ws_url:
        results.fail("discover", "No wsUrl in response")
        return

    results.ok(f"discover -> {ws_url}")

    # Connect to worker via WebSocket
    try:
        ws = await websockets.connect(ws_url)
    except Exception as e:
        results.fail("ws_connect", str(e))
        return

    # Authenticate as controller
    auth_msg = {
        "type": "auth",
        "key": api_key,
        "role": "controller",
        "target_device_id": device_id,
        "last_ack": 0,
    }
    await ws.send(json.dumps(auth_msg))

    raw = await ws.recv()
    msg = json.loads(raw)
    if msg.get("type") == "auth_ok":
        results.ok(f"auth_ok (phone_connected={msg.get('phone_connected')})")
    elif msg.get("type") == "auth_fail":
        results.fail("auth", msg.get("error", "unknown"))
        await ws.close()
        return
    else:
        results.fail("auth", f"Unexpected response: {msg}")
        await ws.close()
        return

    cmd_id_counter = 0

    async def send_cmd(cmd: str, params: dict | None = None) -> dict:
        nonlocal cmd_id_counter
        msg_out: dict[str, Any] = {"cmd": cmd}
        if params:
            msg_out["params"] = params
        await ws.send(json.dumps(msg_out))

        # Read messages until we get a response with status
        while True:
            raw = await asyncio.wait_for(ws.recv(), timeout=10.0)
            reply = json.loads(raw)

            # Skip cmd_accepted, phone_status, ping messages
            if reply.get("type") in ("cmd_accepted", "phone_status", "ping"):
                if reply.get("type") == "ping":
                    await ws.send(json.dumps({"type": "pong"}))
                continue

            # Response has id + status
            if "status" in reply:
                return reply

    # Test commands
    commands_to_test = [
        ("screenshot", None),
        ("click", {"x": 540, "y": 960}),
        ("type", {"text": "hello"}),
        ("ui_tree", None),
        ("back", None),
        ("home", None),
        ("get_text", None),
        ("scroll", {"x": 540, "y": 960, "dx": 0, "dy": 500}),
    ]

    for cmd, params in commands_to_test:
        try:
            resp = await send_cmd(cmd, params)
            if resp.get("status") == "ok":
                result = resp.get("result", {})
                # Extra validation for specific commands
                if cmd == "screenshot" and not result.get("image"):
                    results.fail(f"raw_{cmd}", "No image in response")
                elif cmd == "ui_tree" and not result.get("tree"):
                    results.fail(f"raw_{cmd}", "No tree in response")
                elif cmd == "get_text" and "text" not in result:
                    results.fail(f"raw_{cmd}", "No text in response")
                else:
                    results.ok(f"raw_{cmd}")
            else:
                results.fail(f"raw_{cmd}", f"status={resp.get('status')}: {resp.get('error')}")
        except asyncio.TimeoutError:
            results.fail(f"raw_{cmd}", "Timed out waiting for response")
        except Exception as e:
            results.fail(f"raw_{cmd}", str(e))

    # Test unknown command returns error
    try:
        resp = await send_cmd("unknown_cmd_xyz")
        if resp.get("status") == "error":
            results.ok(f"raw_unknown_command -> error: {resp.get('error')}")
        else:
            results.fail("raw_unknown_command", f"Expected error, got {resp.get('status')}")
    except Exception as e:
        results.fail("raw_unknown_command", str(e))

    await ws.close()
    log.info("  WebSocket closed")


# ---------------------------------------------------------------------------
# Error mode tests (screen_off, typing_fails)
# ---------------------------------------------------------------------------
async def test_error_modes_with_sdk(
    api_url: str, api_key: str, device_id: str, results: TestResults
) -> None:
    """Test error modes (requires fake device started with error flags).

    These tests are skipped if the fake device is not running in error mode.
    We detect this by checking if screenshot/type commands fail or succeed.
    """
    log.info("Testing error modes (screen_off, typing_fails)")
    log.info("  NOTE: These tests only pass if the fake device was started with")
    log.info("        --screen-off and/or --typing-fails flags.")
    log.info("  If not, they will be detected as 'normal mode' and skipped.")

    client = ScreenMCPClient(
        api_key=api_key,
        api_url=api_url,
        command_timeout=10.0,
        auto_reconnect=False,
    )

    try:
        phone = await client.connect(device_id=device_id)
    except Exception as e:
        results.skip("error_modes", f"Cannot connect: {e}")
        return

    # Test screen_off mode
    try:
        result = await phone.screenshot()
        # If we got here, screen is NOT off
        results.skip("screen_off", "Fake device not in --screen-off mode (screenshot succeeded)")
    except CommandError as e:
        if "Screen is off" in str(e):
            results.ok("screen_off: screenshot returns 'Screen is off' error")
        else:
            results.fail("screen_off", f"Unexpected error: {e}")
    except Exception as e:
        results.fail("screen_off", f"Unexpected error type: {type(e).__name__}: {e}")

    # Test typing_fails mode
    try:
        result = await phone.type_text("test")
        # If we got here, typing is NOT failing
        results.skip("typing_fails", "Fake device not in --typing-fails mode (type succeeded)")
    except CommandError as e:
        if "No focused text element" in str(e):
            results.ok("typing_fails: type returns 'No focused text element' error")
        else:
            results.fail("typing_fails", f"Unexpected error: {e}")
    except Exception as e:
        results.fail("typing_fails", f"Unexpected error type: {type(e).__name__}: {e}")

    await phone.disconnect()


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------
def parse_test_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Test ScreenMCP SDKs against a fake device"
    )
    parser.add_argument(
        "--api-url",
        default="http://localhost:3000",
        help="MCP server URL (default: http://localhost:3000)",
    )
    parser.add_argument(
        "--api-key",
        default="pk_test123",
        help="API key for controller auth (default: pk_test123)",
    )
    parser.add_argument(
        "--device-id",
        default="test-device-001",
        help="Target device ID (default: test-device-001)",
    )
    parser.add_argument(
        "--test-error-modes",
        action="store_true",
        help="Also run error mode tests (screen_off, typing_fails)",
    )
    parser.add_argument(
        "--force-raw",
        action="store_true",
        help="Force raw WebSocket testing even if SDK is available",
    )
    return parser.parse_args()


async def main() -> int:
    args = parse_test_args()
    results = TestResults()

    print("=" * 60)
    print("ScreenMCP SDK Integration Test")
    print(f"  API URL:    {args.api_url}")
    print(f"  API Key:    {args.api_key}")
    print(f"  Device ID:  {args.device_id}")
    print(f"  SDK:        {'screenmcp (Python)' if HAS_SDK and not args.force_raw else 'raw HTTP/WS'}")
    print("=" * 60)

    if HAS_SDK and not args.force_raw:
        await test_with_python_sdk(args.api_url, args.api_key, args.device_id, results)
    elif HAS_RAW_DEPS:
        await test_with_raw_ws(args.api_url, args.api_key, args.device_id, results)
    else:
        print("ERROR: Neither the screenmcp SDK nor httpx/websockets are installed.")
        print("Install with: pip install -e ../sdk/python")
        print("  or:         pip install httpx websockets")
        return 1

    if args.test_error_modes and HAS_SDK and not args.force_raw:
        await test_error_modes_with_sdk(
            args.api_url, args.api_key, args.device_id, results
        )

    results.summary()
    return results.exit_code


if __name__ == "__main__":
    sys.exit(asyncio.run(main()))
