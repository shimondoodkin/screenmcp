"""WebSocket client and SSE listener for the fake device."""

from __future__ import annotations

import asyncio
import json
import logging
import time
from typing import Any

import httpx
import websockets
from websockets.asyncio.client import ClientConnection

from .commands import handle_command
from .config import Config
from .test_modes import apply_delay, should_disconnect

logger = logging.getLogger("fake_device")


class FakeDeviceClient:
    """Fake device that connects to a ScreenMCP worker and responds to commands.

    Supports two connection modes:
      - ``opensource``: Listen on SSE for connect events, then connect via WS
      - ``cloud``: Register via API, then poll/manual trigger for connect
    """

    def __init__(self, config: Config) -> None:
        self.config = config
        self._ws: ClientConnection | None = None
        self._command_count: int = 0
        self._running: bool = False
        self._last_ack: int = 0

    # -- Lifecycle ------------------------------------------------------------

    async def run(self) -> None:
        """Main entry point. Run the fake device until cancelled."""
        self._running = True
        logger.info(
            "Starting fake device: id=%s mode=%s api=%s",
            self.config.device_id,
            self.config.mode,
            self.config.api_url,
        )

        if self.config.mode == "opensource":
            await self._run_opensource()
        else:
            await self._run_cloud()

    async def _run_opensource(self) -> None:
        """Open-source mode: register, then listen on SSE for connect events."""
        await self._register_device()

        while self._running:
            try:
                await self._listen_sse()
            except asyncio.CancelledError:
                raise
            except Exception:
                logger.exception("SSE connection failed, retrying in 3s")
                await asyncio.sleep(3)

    async def _run_cloud(self) -> None:
        """Cloud mode: register, then wait for manual trigger.

        In real cloud mode, the device would receive an FCM push.
        Here we register and then poll /api/discover ourselves as a stand-in.
        For testing, we just register and wait on SSE like opensource mode
        since the MCP server broadcasts events to all SSE listeners.
        """
        await self._register_device()

        while self._running:
            try:
                await self._listen_sse()
            except asyncio.CancelledError:
                raise
            except Exception:
                logger.exception("SSE connection failed, retrying in 3s")
                await asyncio.sleep(3)

    # -- Device registration --------------------------------------------------

    async def _register_device(self) -> None:
        """Register this device with the MCP server."""
        url = f"{self.config.api_url}/api/devices/register"
        headers = {
            "Authorization": f"Bearer {self.config.user_id}",
            "Content-Type": "application/json",
        }
        body = {
            "deviceId": self.config.device_id,
            "deviceName": self.config.device_name,
        }

        async with httpx.AsyncClient() as http:
            try:
                resp = await http.post(url, headers=headers, json=body)
                if resp.status_code == 200:
                    data = resp.json()
                    logger.info(
                        "Registered device: id=%s number=%s",
                        self.config.device_id,
                        data.get("device_number"),
                    )
                else:
                    logger.error(
                        "Registration failed: %d %s", resp.status_code, resp.text
                    )
            except Exception:
                logger.exception("Failed to register device")

    async def unregister_device(self) -> None:
        """Unregister this device from the MCP server."""
        url = f"{self.config.api_url}/api/devices/delete"
        headers = {
            "Authorization": f"Bearer {self.config.user_id}",
            "Content-Type": "application/json",
        }
        body = {"deviceId": self.config.device_id}

        async with httpx.AsyncClient() as http:
            try:
                resp = await http.post(url, headers=headers, json=body)
                if resp.status_code == 200:
                    logger.info("Unregistered device: id=%s", self.config.device_id)
                else:
                    logger.warning(
                        "Unregister returned: %d %s", resp.status_code, resp.text
                    )
            except Exception:
                logger.exception("Failed to unregister device")

    # -- SSE Listener ---------------------------------------------------------

    async def _listen_sse(self) -> None:
        """Connect to the SSE endpoint and wait for connect events."""
        url = f"{self.config.api_url}/api/events"
        headers = {"Authorization": f"Bearer {self.config.user_id}"}

        logger.info("Connecting to SSE: %s", url)

        async with httpx.AsyncClient(timeout=httpx.Timeout(None)) as http:
            async with http.stream("GET", url, headers=headers) as resp:
                if resp.status_code != 200:
                    body = await resp.aread()
                    logger.error("SSE connect failed: %d %s", resp.status_code, body.decode())
                    return

                logger.info("SSE connected, waiting for events...")

                buffer = ""
                async for chunk in resp.aiter_text():
                    buffer += chunk
                    # Parse SSE frames: lines starting with "data: " followed by blank line
                    while "\n\n" in buffer:
                        frame, buffer = buffer.split("\n\n", 1)
                        await self._handle_sse_frame(frame)

    async def _handle_sse_frame(self, frame: str) -> None:
        """Parse and handle a single SSE frame."""
        for line in frame.split("\n"):
            # Skip comments (heartbeats)
            if line.startswith(":"):
                logger.debug("SSE heartbeat")
                continue

            if line.startswith("data: "):
                data_str = line[6:]
                try:
                    event = json.loads(data_str)
                except json.JSONDecodeError:
                    logger.warning("SSE: invalid JSON: %s", data_str)
                    continue

                event_type = event.get("type")
                logger.info("SSE event: type=%s", event_type)

                if event_type == "connected":
                    logger.info("SSE: connected confirmation received")

                elif event_type == "connect":
                    target_id = event.get("target_device_id", "")
                    ws_url = event.get("wsUrl", "")

                    # In the open-source MCP server, events are broadcast to all
                    # SSE clients. We only act on events targeting our device_id.
                    # Server normalizes IDs (strips hyphens), so compare normalized.
                    norm = lambda s: s.replace("-", "")
                    if target_id and norm(target_id) != norm(self.config.device_id):
                        logger.debug(
                            "SSE: ignoring connect for device %s (we are %s)",
                            target_id,
                            self.config.device_id,
                        )
                        return

                    if ws_url:
                        logger.info(
                            "SSE: connect event, wsUrl=%s target=%s",
                            ws_url,
                            target_id,
                        )
                        # Handle the WS connection in a separate task so we keep
                        # listening on SSE
                        asyncio.create_task(self._connect_and_serve(ws_url))
                    else:
                        logger.warning("SSE: connect event but no wsUrl")

    # -- WebSocket client -----------------------------------------------------

    async def _connect_and_serve(self, ws_url: str) -> None:
        """Connect to the worker WebSocket and handle commands."""
        self._command_count = 0

        try:
            logger.info("Connecting to worker: %s", ws_url)
            self._ws = await websockets.connect(ws_url)

            # Authenticate as a phone/device
            auth_msg = {
                "type": "auth",
                "user_id": self.config.user_id,
                "role": "phone",
                "device_id": self.config.device_id,
                "last_ack": self._last_ack,
            }
            await self._ws.send(json.dumps(auth_msg))
            logger.info("Sent auth message (role=phone, device_id=%s)", self.config.device_id)

            # Wait for auth response
            raw = await self._ws.recv()
            msg = json.loads(raw)

            if msg.get("type") == "auth_fail":
                logger.error("Auth failed: %s", msg.get("error"))
                await self._ws.close()
                self._ws = None
                return

            if msg.get("type") == "auth_ok":
                logger.info("Authenticated with worker successfully")
            else:
                logger.warning("Unexpected auth response: %s", msg)

            # Serve commands
            await self._serve_commands()

        except websockets.exceptions.ConnectionClosed as e:
            logger.info("WebSocket closed: %s", e)
        except Exception:
            logger.exception("WebSocket error")
        finally:
            if self._ws is not None:
                try:
                    await self._ws.close()
                except Exception:
                    pass
                self._ws = None

    async def _serve_commands(self) -> None:
        """Read commands from the WebSocket and send back responses."""
        assert self._ws is not None

        async for raw in self._ws:
            msg = json.loads(raw)
            msg_type = msg.get("type")

            # Handle ping
            if msg_type == "ping":
                await self._ws.send(json.dumps({"type": "pong"}))
                logger.debug("Responded to ping")
                continue

            # Handle server messages we can ignore
            if msg_type in ("phone_status", "error", "auth_ok"):
                logger.debug("Server message: %s", msg_type)
                continue

            # It is a command if it has "id" and "cmd"
            cmd_id = msg.get("id")
            cmd = msg.get("cmd")

            if cmd_id is not None and cmd is not None:
                params = msg.get("params")
                self._command_count += 1
                logger.info(
                    "Command #%d: id=%s cmd=%s params=%s",
                    self._command_count,
                    cmd_id,
                    cmd,
                    params,
                )

                # Apply slow-response delay
                await apply_delay(self.config.test_modes)

                # Handle the command
                response = handle_command(cmd, params, self.config.test_modes)
                response["id"] = cmd_id
                await self._ws.send(json.dumps(response))

                # Track ack
                self._last_ack = cmd_id

                logger.info(
                    "Response: id=%s status=%s",
                    cmd_id,
                    response.get("status"),
                )

                # Check disconnect-after test mode
                if should_disconnect(
                    self.config.test_modes, self._command_count
                ):
                    logger.info(
                        "disconnect-after triggered at command #%d, closing",
                        self._command_count,
                    )
                    await self._ws.close()
                    return
            else:
                logger.debug("Unhandled message: %s", msg)

    # -- Shutdown -------------------------------------------------------------

    def stop(self) -> None:
        """Signal the client to stop."""
        self._running = False
