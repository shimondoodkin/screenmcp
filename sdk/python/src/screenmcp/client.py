"""ScreenMCP Python client — async SDK for controlling Android phones."""

from __future__ import annotations

import asyncio
import json
import logging
import time
from typing import Any

import httpx
import websockets
from websockets.asyncio.client import ClientConnection

from .selector import ElementHandle, FoundElement, find_elements
from .types import (
    AuthMessage,
    CommandResponse,
    ControllerCommand,
    ScrollDirection,
    SCROLL_VECTORS,
)

logger = logging.getLogger("screenmcp")

_DEFAULT_API_URL = "https://screenmcp.com"
_DEFAULT_COMMAND_TIMEOUT = 30.0  # seconds


class ScreenMCPError(Exception):
    """Base exception for ScreenMCP errors."""


class AuthError(ScreenMCPError):
    """Raised when authentication fails."""


class ConnectionError(ScreenMCPError):  # noqa: A001  (shadows builtin intentionally)
    """Raised when the WebSocket connection is lost or unavailable."""


class CommandError(ScreenMCPError):
    """Raised when a phone command returns an error status."""


class ScreenMCPClient:
    """Async client for the ScreenMCP phone-control platform.

    Usage::

        async with ScreenMCPClient(api_key="pk_...") as phone:
            img = await phone.screenshot()
            await phone.click(540, 960)

    Parameters
    ----------
    api_key:
        A ``pk_``-prefixed API key issued from the ScreenMCP dashboard.
    api_url:
        Base URL of the ScreenMCP web API.
    device_id:
        Target device UUID.  If ``None`` the server will pick the first
        device registered to the authenticated user.
    command_timeout:
        Seconds to wait for a command response before raising a timeout.
    auto_reconnect:
        Whether to attempt transparent reconnection on WebSocket close.
    """

    def __init__(
        self,
        api_key: str,
        api_url: str = _DEFAULT_API_URL,
        device_id: str | None = None,
        command_timeout: float = _DEFAULT_COMMAND_TIMEOUT,
        auto_reconnect: bool = True,
    ) -> None:
        self._api_key = api_key
        self._api_url = api_url.rstrip("/")
        self._device_id = device_id or ""
        self._command_timeout = command_timeout
        self._auto_reconnect = auto_reconnect

        # Internal state
        self._ws: ClientConnection | None = None
        self._worker_url: str | None = None
        self._phone_connected: bool = False
        self._connected: bool = False

        # Pending command tracking.  Key is the server-assigned command id
        # (set after cmd_accepted) or a temporary negative id before that.
        self._pending: dict[int, asyncio.Future[CommandResponse]] = {}
        self._last_temp_id: int = 0
        self._recv_task: asyncio.Task[None] | None = None

    # ── Properties ───────────────────────────────────────────────────────

    @property
    def phone_connected(self) -> bool:
        """Whether the target phone is currently online."""
        return self._phone_connected

    @property
    def worker_url(self) -> str | None:
        """WebSocket URL of the connected worker."""
        return self._worker_url

    @property
    def connected(self) -> bool:
        """Whether the WebSocket connection is established and authenticated."""
        return self._connected

    # ── Lifecycle ────────────────────────────────────────────────────────

    async def connect(self) -> None:
        """Discover a worker and open an authenticated WebSocket connection."""
        self._worker_url = await self._discover()
        await self._connect_ws(self._worker_url)

    async def disconnect(self) -> None:
        """Close the WebSocket connection gracefully."""
        self._auto_reconnect = False
        await self._close_ws()

    async def __aenter__(self) -> "ScreenMCPClient":
        await self.connect()
        return self

    async def __aexit__(self, *exc: object) -> None:
        await self.disconnect()

    # ── High-level phone commands ────────────────────────────────────────

    async def screenshot(
        self,
        *,
        quality: int | None = None,
        max_width: int | None = None,
        max_height: int | None = None,
    ) -> dict[str, Any]:
        """Take a screenshot.  Returns dict with ``image`` (base64 JPEG)."""
        params: dict[str, Any] = {}
        if quality is not None:
            params["quality"] = quality
        if max_width is not None:
            params["max_width"] = max_width
        if max_height is not None:
            params["max_height"] = max_height
        resp = await self.send_command("screenshot", params or None)
        return resp.result

    async def click(self, x: int, y: int) -> dict[str, Any]:
        """Tap at (x, y)."""
        resp = await self.send_command("click", {"x": x, "y": y})
        return resp.result

    async def long_click(self, x: int, y: int) -> dict[str, Any]:
        """Long-press at (x, y)."""
        resp = await self.send_command("long_click", {"x": x, "y": y})
        return resp.result

    async def drag(
        self,
        start_x: int,
        start_y: int,
        end_x: int,
        end_y: int,
        duration: int = 300,
    ) -> dict[str, Any]:
        """Drag from (start_x, start_y) to (end_x, end_y)."""
        resp = await self.send_command(
            "drag",
            {
                "startX": start_x,
                "startY": start_y,
                "endX": end_x,
                "endY": end_y,
                "duration": duration,
            },
        )
        return resp.result

    async def scroll(
        self,
        direction: ScrollDirection,
        amount: int = 500,
    ) -> dict[str, Any]:
        """Scroll the screen in the given direction.

        Parameters
        ----------
        direction:
            One of ``"up"``, ``"down"``, ``"left"``, ``"right"``.
        amount:
            Scroll distance in pixels (default 500).
        """
        dx_mul, dy_mul = SCROLL_VECTORS[direction]
        # Use center of a typical 1080x1920 screen as the scroll origin.
        resp = await self.send_command(
            "scroll",
            {"x": 540, "y": 960, "dx": dx_mul * amount, "dy": dy_mul * amount},
        )
        return resp.result

    async def type_text(self, text: str) -> dict[str, Any]:
        """Type text into the currently focused input field.

        Named ``type_text`` because ``type`` is a Python builtin.
        """
        resp = await self.send_command("type", {"text": text})
        return resp.result

    async def get_text(self) -> dict[str, Any]:
        """Get text from the currently focused element.  Returns dict with ``text``."""
        resp = await self.send_command("get_text")
        return resp.result

    async def select_all(self) -> dict[str, Any]:
        """Select all text in the focused input."""
        resp = await self.send_command("select_all")
        return resp.result

    async def copy(self, *, return_text: bool = False) -> dict[str, Any]:
        """Copy the current selection to the clipboard.

        Parameters
        ----------
        return_text:
            If ``True``, the response includes ``text`` with the copied content.
        """
        params: dict[str, Any] = {}
        if return_text:
            params["return_text"] = True
        resp = await self.send_command("copy", params or None)
        return resp.result

    async def paste(self, text: str | None = None) -> dict[str, Any]:
        """Paste into the focused field.

        Parameters
        ----------
        text:
            If provided, sets clipboard to this text before pasting.
        """
        params: dict[str, Any] = {}
        if text is not None:
            params["text"] = text
        resp = await self.send_command("paste", params or None)
        return resp.result

    async def get_clipboard(self) -> dict[str, Any]:
        """Get the current clipboard text contents.  Returns dict with ``text``."""
        resp = await self.send_command("get_clipboard")
        return resp.result

    async def set_clipboard(self, text: str) -> dict[str, Any]:
        """Set the clipboard to the given text."""
        resp = await self.send_command("set_clipboard", {"text": text})
        return resp.result

    async def back(self) -> dict[str, Any]:
        """Press the Back button."""
        resp = await self.send_command("back")
        return resp.result

    async def home(self) -> dict[str, Any]:
        """Press the Home button."""
        resp = await self.send_command("home")
        return resp.result

    async def recents(self) -> dict[str, Any]:
        """Open the Recents / app-switcher view."""
        resp = await self.send_command("recents")
        return resp.result

    async def ui_tree(self) -> dict[str, Any]:
        """Get the UI accessibility tree.  Returns dict with ``tree``."""
        resp = await self.send_command("ui_tree")
        return resp.result

    async def list_cameras(self) -> dict[str, Any]:
        """List available cameras on the device.

        Returns dict with ``cameras`` — a list of ``{id, facing}`` objects.
        Desktop clients return an empty list.
        """
        resp = await self.send_command("list_cameras")
        return resp.result

    async def camera(
        self,
        camera_id: str = "0",
        *,
        quality: int | None = None,
        max_width: int | None = None,
        max_height: int | None = None,
    ) -> dict[str, Any]:
        """Take a photo with the device camera.  Returns dict with ``image`` (base64).

        Parameters
        ----------
        camera_id:
            Camera ID string (use ``list_cameras()`` to discover).
            Default ``"0"``.
        """
        params: dict[str, Any] = {"camera": camera_id}
        if quality is not None:
            params["quality"] = quality
        if max_width is not None:
            params["max_width"] = max_width
        if max_height is not None:
            params["max_height"] = max_height
        resp = await self.send_command("camera", params)
        return resp.result

    async def play_audio(
        self, audio_base64: str, volume: float | None = None
    ) -> None:
        """Play audio on the device.

        Parameters
        ----------
        audio_base64:
            Base64-encoded audio data.
        volume:
            Optional playback volume (0.0 to 1.0).
        """
        params: dict[str, Any] = {"audio_data": audio_base64}
        if volume is not None:
            params["volume"] = volume
        await self.send_command("play_audio", params)

    # ── Keyboard commands (desktop only) ──────────────────────────────

    async def hold_key(self, key: str) -> dict[str, Any]:
        """Press and hold a key (desktop only).  Use with ``release_key``."""
        resp = await self.send_command("hold_key", {"key": key})
        return resp.result

    async def release_key(self, key: str) -> dict[str, Any]:
        """Release a held key (desktop only)."""
        resp = await self.send_command("release_key", {"key": key})
        return resp.result

    async def press_key(self, key: str) -> dict[str, Any]:
        """Press and release a key in one action (desktop only)."""
        resp = await self.send_command("press_key", {"key": key})
        return resp.result

    # ── Selector-based element interaction ─────────────────────────────

    def find(self, selector: str, *, timeout: float = 3.0) -> ElementHandle:
        """Find element by selector. Returns fluent ElementHandle."""
        return ElementHandle(self, selector, timeout)

    async def find_all(
        self, selector: str, *, timeout: float = 3.0
    ) -> list[FoundElement]:
        """Find all matching elements."""
        deadline = time.monotonic() + timeout
        while True:
            result = await self.ui_tree()
            tree = result.get("tree", [])
            found = find_elements(tree, selector)
            if found:
                return found
            if time.monotonic() >= deadline:
                return []
            await asyncio.sleep(0.5)

    async def exists(self, selector: str, *, timeout: float = 2.0) -> bool:
        """Check if element exists."""
        deadline = time.monotonic() + timeout
        while True:
            result = await self.ui_tree()
            tree = result.get("tree", [])
            if find_elements(tree, selector):
                return True
            if time.monotonic() >= deadline:
                return False
            await asyncio.sleep(0.5)

    async def wait_for(
        self, selector: str, *, timeout: float = 10.0
    ) -> FoundElement:
        """Wait for element to appear."""
        return await self.find(selector, timeout=timeout).element()

    async def wait_for_gone(
        self, selector: str, *, timeout: float = 10.0
    ) -> None:
        """Wait for element to disappear."""
        deadline = time.monotonic() + timeout
        while True:
            result = await self.ui_tree()
            tree = result.get("tree", [])
            if not find_elements(tree, selector):
                return
            if time.monotonic() >= deadline:
                raise ScreenMCPError(f"Element still present: {selector}")
            await asyncio.sleep(0.5)

    # ── Generic command ──────────────────────────────────────────────────

    async def send_command(
        self,
        cmd: str,
        params: dict[str, Any] | None = None,
    ) -> CommandResponse:
        """Send an arbitrary command to the phone and await its response.

        Parameters
        ----------
        cmd:
            The command name (e.g. ``"screenshot"``, ``"click"``).
        params:
            Optional parameters dict.

        Returns
        -------
        CommandResponse
            The response from the phone.

        Raises
        ------
        ConnectionError
            If the WebSocket is not connected.
        CommandError
            If the phone returns a non-ok status.
        asyncio.TimeoutError
            If the command does not complete within *command_timeout*.
        """
        if self._ws is None or not self._connected:
            raise ConnectionError("not connected — call connect() first")

        command = ControllerCommand(cmd=cmd, params=params)
        temp_id = -(int(time.time() * 1000) + id(command)) & 0x7FFFFFFFFFFFFFFF
        temp_id = -temp_id  # ensure negative
        self._last_temp_id = temp_id

        loop = asyncio.get_running_loop()
        future: asyncio.Future[CommandResponse] = loop.create_future()
        self._pending[temp_id] = future

        await self._ws.send(json.dumps(command.to_dict()))

        try:
            return await asyncio.wait_for(future, timeout=self._command_timeout)
        except asyncio.TimeoutError:
            self._pending.pop(temp_id, None)
            raise

    # ── Discovery ────────────────────────────────────────────────────────

    async def _discover(self) -> str:
        """Call ``POST /api/discover`` and return the worker WebSocket URL."""
        async with httpx.AsyncClient() as http:
            resp = await http.post(
                f"{self._api_url}/api/discover",
                headers={
                    "Authorization": f"Bearer {self._api_key}",
                    "Content-Type": "application/json",
                },
            )
        if resp.status_code != 200:
            raise ScreenMCPError(
                f"discovery failed ({resp.status_code}): {resp.text}"
            )
        data = resp.json()
        ws_url = data.get("wsUrl")
        if not ws_url:
            raise ScreenMCPError("discovery returned no wsUrl")
        return ws_url

    # ── WebSocket ────────────────────────────────────────────────────────

    async def _connect_ws(self, worker_url: str) -> None:
        """Open WebSocket, authenticate, and start the receive loop."""
        self._ws = await websockets.connect(worker_url)

        # Send auth message
        auth = AuthMessage(
            key=self._api_key,
            target_device_id=self._device_id,
            last_ack=0,
        )
        await self._ws.send(json.dumps(auth.to_dict()))

        # Wait for auth_ok or auth_fail
        raw = await self._ws.recv()
        msg = json.loads(raw)

        if msg.get("type") == "auth_fail":
            await self._ws.close()
            self._ws = None
            raise AuthError(msg.get("error", "authentication failed"))

        if msg.get("type") == "auth_ok":
            self._phone_connected = msg.get("phone_connected", False)
            self._connected = True
        else:
            raise ScreenMCPError(f"unexpected auth response: {msg}")

        # Start background receive loop
        self._recv_task = asyncio.create_task(self._recv_loop())

    async def _recv_loop(self) -> None:
        """Background task that reads messages from the WebSocket."""
        assert self._ws is not None
        try:
            async for raw in self._ws:
                msg = json.loads(raw)
                self._handle_message(msg)
        except websockets.exceptions.ConnectionClosed:
            pass
        finally:
            self._connected = False
            # Reject all pending futures
            for fut in self._pending.values():
                if not fut.done():
                    fut.set_exception(ConnectionError("connection closed"))
            self._pending.clear()

            if self._auto_reconnect:
                asyncio.create_task(self._reconnect())

    def _handle_message(self, msg: dict[str, Any]) -> None:
        """Dispatch an incoming JSON message."""
        msg_type = msg.get("type")

        if msg_type == "cmd_accepted":
            server_id = msg["id"]
            temp_id = self._last_temp_id
            if temp_id in self._pending:
                self._pending[server_id] = self._pending.pop(temp_id)
            return

        if msg_type == "phone_status":
            self._phone_connected = msg.get("connected", False)
            logger.info("phone_status: connected=%s", self._phone_connected)
            return

        if msg_type == "ping":
            if self._ws is not None:
                asyncio.create_task(self._ws.send(json.dumps({"type": "pong"})))
            return

        if msg_type == "error":
            logger.error("server error: %s", msg.get("error"))
            return

        # Command response: has id + status, no type
        if "id" in msg and "status" in msg and "type" not in msg:
            cmd_id = msg["id"]
            future = self._pending.pop(cmd_id, None)
            if future is not None and not future.done():
                resp = CommandResponse(
                    id=msg["id"],
                    status=msg["status"],
                    result=msg.get("result", {}),
                    error=msg.get("error"),
                    unsupported=msg.get("unsupported", False),
                )
                if resp.status == "ok":
                    future.set_result(resp)
                else:
                    future.set_exception(
                        CommandError(resp.error or f"command failed: {resp.status}")
                    )

    async def _reconnect(self) -> None:
        """Attempt to reconnect with exponential back-off."""
        delays = [1.0, 2.0, 4.0, 8.0, 16.0, 30.0]
        for attempt, delay in enumerate(delays, 1):
            await asyncio.sleep(delay)
            try:
                self._worker_url = await self._discover()
                await self._connect_ws(self._worker_url)
                logger.info("reconnected (attempt %d)", attempt)
                return
            except Exception:
                logger.warning("reconnect attempt %d failed", attempt, exc_info=True)
        logger.error("reconnect exhausted after %d attempts", len(delays))

    async def _close_ws(self) -> None:
        """Close WebSocket and cancel the receive task."""
        if self._recv_task is not None:
            self._recv_task.cancel()
            try:
                await self._recv_task
            except asyncio.CancelledError:
                pass
            self._recv_task = None

        if self._ws is not None:
            await self._ws.close()
            self._ws = None

        self._connected = False
