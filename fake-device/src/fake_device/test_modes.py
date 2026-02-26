"""Test mode behaviors that modify command responses."""

from __future__ import annotations

import asyncio
from typing import Any

from .config import TestModes


async def apply_delay(modes: TestModes) -> None:
    """Sleep if slow-response mode is enabled."""
    if modes.slow_response > 0:
        await asyncio.sleep(modes.slow_response)


def should_disconnect(modes: TestModes, command_count: int) -> bool:
    """Return True if the device should disconnect after this command."""
    if modes.disconnect_after > 0 and command_count >= modes.disconnect_after:
        return True
    return False


def override_screenshot(modes: TestModes) -> dict[str, Any] | None:
    """Return an error response if screen-off mode is active, else None."""
    if modes.screen_off:
        return {"status": "error", "error": "Screen is off"}
    return None


def override_type(modes: TestModes) -> dict[str, Any] | None:
    """Return an error response if typing-fails mode is active, else None."""
    if modes.typing_fails:
        return {"status": "error", "error": "No focused text element"}
    return None
