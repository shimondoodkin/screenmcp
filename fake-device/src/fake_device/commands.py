"""Hardcoded command handlers that return fake but realistic responses."""

from __future__ import annotations

import base64
import io
import struct
import zlib
from typing import Any

from .config import TestModes
from . import test_modes as tm


def _make_png(width: int, height: int, r: int, g: int, b: int) -> str:
    """Generate a minimal solid-color PNG and return it as base64.

    Uses raw PNG encoding (no PIL dependency).
    """

    def _chunk(chunk_type: bytes, data: bytes) -> bytes:
        c = chunk_type + data
        return struct.pack(">I", len(data)) + c + struct.pack(">I", zlib.crc32(c) & 0xFFFFFFFF)

    # IHDR
    ihdr_data = struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)
    ihdr = _chunk(b"IHDR", ihdr_data)

    # IDAT: raw pixel rows, each prefixed with filter byte 0 (None)
    raw_rows = b""
    row = bytes([r, g, b]) * width
    for _ in range(height):
        raw_rows += b"\x00" + row
    idat = _chunk(b"IDAT", zlib.compress(raw_rows))

    # IEND
    iend = _chunk(b"IEND", b"")

    png = b"\x89PNG\r\n\x1a\n" + ihdr + idat + iend
    return base64.b64encode(png).decode("ascii")


def _make_jpeg_stub() -> str:
    """Generate a minimal valid JPEG (tiny 1x1 pixel) as base64.

    This is a pre-built minimal JPEG byte sequence.
    """
    # Minimal 1x1 white JPEG
    jpeg_bytes = bytes([
        0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, 0x49, 0x46, 0x00, 0x01,
        0x01, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0xFF, 0xDB, 0x00, 0x43,
        0x00, 0x08, 0x06, 0x06, 0x07, 0x06, 0x05, 0x08, 0x07, 0x07, 0x07, 0x09,
        0x09, 0x08, 0x0A, 0x0C, 0x14, 0x0D, 0x0C, 0x0B, 0x0B, 0x0C, 0x19, 0x12,
        0x13, 0x0F, 0x14, 0x1D, 0x1A, 0x1F, 0x1E, 0x1D, 0x1A, 0x1C, 0x1C, 0x20,
        0x24, 0x2E, 0x27, 0x20, 0x22, 0x2C, 0x23, 0x1C, 0x1C, 0x28, 0x37, 0x29,
        0x2C, 0x30, 0x31, 0x34, 0x34, 0x34, 0x1F, 0x27, 0x39, 0x3D, 0x38, 0x32,
        0x3C, 0x2E, 0x33, 0x34, 0x32, 0xFF, 0xC0, 0x00, 0x0B, 0x08, 0x00, 0x01,
        0x00, 0x01, 0x01, 0x01, 0x11, 0x00, 0xFF, 0xC4, 0x00, 0x1F, 0x00, 0x00,
        0x01, 0x05, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08,
        0x09, 0x0A, 0x0B, 0xFF, 0xC4, 0x00, 0xB5, 0x10, 0x00, 0x02, 0x01, 0x03,
        0x03, 0x02, 0x04, 0x03, 0x05, 0x05, 0x04, 0x04, 0x00, 0x00, 0x01, 0x7D,
        0x01, 0x02, 0x03, 0x00, 0x04, 0x11, 0x05, 0x12, 0x21, 0x31, 0x41, 0x06,
        0x13, 0x51, 0x61, 0x07, 0x22, 0x71, 0x14, 0x32, 0x81, 0x91, 0xA1, 0x08,
        0x23, 0x42, 0xB1, 0xC1, 0x15, 0x52, 0xD1, 0xF0, 0x24, 0x33, 0x62, 0x72,
        0x82, 0x09, 0x0A, 0x16, 0x17, 0x18, 0x19, 0x1A, 0x25, 0x26, 0x27, 0x28,
        0x29, 0x2A, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3A, 0x43, 0x44, 0x45,
        0x46, 0x47, 0x48, 0x49, 0x4A, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59,
        0x5A, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68, 0x69, 0x6A, 0x73, 0x74, 0x75,
        0x76, 0x77, 0x78, 0x79, 0x7A, 0x83, 0x84, 0x85, 0x86, 0x87, 0x88, 0x89,
        0x8A, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x98, 0x99, 0x9A, 0xA2, 0xA3,
        0xA4, 0xA5, 0xA6, 0xA7, 0xA8, 0xA9, 0xAA, 0xB2, 0xB3, 0xB4, 0xB5, 0xB6,
        0xB7, 0xB8, 0xB9, 0xBA, 0xC2, 0xC3, 0xC4, 0xC5, 0xC6, 0xC7, 0xC8, 0xC9,
        0xCA, 0xD2, 0xD3, 0xD4, 0xD5, 0xD6, 0xD7, 0xD8, 0xD9, 0xDA, 0xE1, 0xE2,
        0xE3, 0xE4, 0xE5, 0xE6, 0xE7, 0xE8, 0xE9, 0xEA, 0xF1, 0xF2, 0xF3, 0xF4,
        0xF5, 0xF6, 0xF7, 0xF8, 0xF9, 0xFA, 0xFF, 0xDA, 0x00, 0x08, 0x01, 0x01,
        0x00, 0x00, 0x3F, 0x00, 0x7B, 0x94, 0x11, 0x00, 0x00, 0x00, 0x00, 0x00,
        0xFF, 0xD9,
    ])
    return base64.b64encode(jpeg_bytes).decode("ascii")


# Cache the generated assets so we only build them once.
_SCREENSHOT_PNG: str | None = None
_CAMERA_IMAGE: str | None = None


def _get_screenshot_png() -> str:
    global _SCREENSHOT_PNG
    if _SCREENSHOT_PNG is None:
        _SCREENSHOT_PNG = _make_png(100, 100, 66, 133, 244)  # Google blue
    return _SCREENSHOT_PNG


def _get_camera_image() -> str:
    global _CAMERA_IMAGE
    if _CAMERA_IMAGE is None:
        _CAMERA_IMAGE = _make_png(80, 60, 76, 175, 80)  # green-ish photo
    return _CAMERA_IMAGE


# Hardcoded Android-like UI tree
_UI_TREE = [
    {
        "className": "FrameLayout",
        "resourceId": "",
        "text": "",
        "contentDescription": "",
        "bounds": {"left": 0, "top": 0, "right": 1080, "bottom": 1920},
        "clickable": False,
        "editable": False,
        "focused": False,
        "scrollable": False,
        "checkable": False,
        "checked": False,
        "children": [
            {
                "className": "LinearLayout",
                "resourceId": "com.android.launcher:id/workspace",
                "text": "",
                "contentDescription": "",
                "bounds": {"left": 0, "top": 48, "right": 1080, "bottom": 1776},
                "clickable": False,
                "editable": False,
                "focused": False,
                "scrollable": True,
                "checkable": False,
                "checked": False,
                "children": [
                    {
                        "className": "TextView",
                        "resourceId": "com.android.launcher:id/app_label",
                        "text": "Settings",
                        "contentDescription": "Settings",
                        "bounds": {"left": 60, "top": 200, "right": 240, "bottom": 280},
                        "clickable": True,
                        "editable": False,
                        "focused": False,
                        "scrollable": False,
                        "checkable": False,
                        "checked": False,
                        "children": [],
                    },
                    {
                        "className": "TextView",
                        "resourceId": "com.android.launcher:id/app_label",
                        "text": "Chrome",
                        "contentDescription": "Chrome",
                        "bounds": {"left": 300, "top": 200, "right": 480, "bottom": 280},
                        "clickable": True,
                        "editable": False,
                        "focused": False,
                        "scrollable": False,
                        "checkable": False,
                        "checked": False,
                        "children": [],
                    },
                    {
                        "className": "EditText",
                        "resourceId": "com.android.chrome:id/search_box",
                        "text": "Search or type URL",
                        "contentDescription": "Search",
                        "bounds": {"left": 60, "top": 400, "right": 1020, "bottom": 460},
                        "clickable": True,
                        "editable": True,
                        "focused": False,
                        "scrollable": False,
                        "checkable": False,
                        "checked": False,
                        "children": [],
                    },
                ],
            },
            {
                "className": "FrameLayout",
                "resourceId": "com.android.systemui:id/navigation_bar",
                "text": "",
                "contentDescription": "",
                "bounds": {"left": 0, "top": 1776, "right": 1080, "bottom": 1920},
                "clickable": False,
                "editable": False,
                "focused": False,
                "scrollable": False,
                "checkable": False,
                "checked": False,
                "children": [
                    {
                        "className": "ImageButton",
                        "resourceId": "com.android.systemui:id/back",
                        "text": "",
                        "contentDescription": "Back",
                        "bounds": {"left": 60, "top": 1800, "right": 180, "bottom": 1896},
                        "clickable": True,
                        "editable": False,
                        "focused": False,
                        "scrollable": False,
                        "checkable": False,
                        "checked": False,
                        "children": [],
                    },
                    {
                        "className": "ImageButton",
                        "resourceId": "com.android.systemui:id/home",
                        "text": "",
                        "contentDescription": "Home",
                        "bounds": {"left": 420, "top": 1800, "right": 660, "bottom": 1896},
                        "clickable": True,
                        "editable": False,
                        "focused": False,
                        "scrollable": False,
                        "checkable": False,
                        "checked": False,
                        "children": [],
                    },
                    {
                        "className": "ImageButton",
                        "resourceId": "com.android.systemui:id/recent_apps",
                        "text": "",
                        "contentDescription": "Recent apps",
                        "bounds": {"left": 900, "top": 1800, "right": 1020, "bottom": 1896},
                        "clickable": True,
                        "editable": False,
                        "focused": False,
                        "scrollable": False,
                        "checkable": False,
                        "checked": False,
                        "children": [],
                    },
                ],
            },
        ],
    }
]


def handle_command(
    cmd: str,
    params: dict[str, Any] | None,
    modes: TestModes,
) -> dict[str, Any]:
    """Return a hardcoded response dict for the given command.

    The response does NOT include ``id`` -- the caller must add it.
    Returns a dict with at least ``status`` (and ``result`` or ``error``).
    """
    # Check test-mode overrides first
    if cmd == "screenshot":
        override = tm.override_screenshot(modes)
        if override is not None:
            return override
        return {"status": "ok", "result": {"image": _get_screenshot_png()}}

    if cmd == "type":
        override = tm.override_type(modes)
        if override is not None:
            return override
        return {"status": "ok", "result": {}}

    if cmd == "ui_tree":
        return {"status": "ok", "result": {"tree": _UI_TREE}}

    if cmd == "get_text":
        return {"status": "ok", "result": {"text": "Fake text content"}}

    if cmd == "camera":
        return {"status": "ok", "result": {"image": _get_camera_image()}}

    if cmd == "list_cameras":
        return {
            "status": "ok",
            "result": {
                "cameras": [
                    {"id": "0", "facing": "back"},
                    {"id": "1", "facing": "front"},
                ]
            },
        }

    if cmd == "copy":
        p = params or {}
        if p.get("return_text"):
            return {"status": "ok", "result": {"text": "Fake copied text"}}
        return {"status": "ok", "result": {}}

    if cmd == "get_clipboard":
        return {"status": "ok", "result": {"text": "Fake clipboard content"}}

    # Simple ok commands: click, long_click, drag, scroll, select_all, paste,
    # set_clipboard, back, home, recents, hold_key, release_key, press_key
    simple_commands = {
        "click", "long_click", "drag", "scroll",
        "select_all", "paste", "set_clipboard",
        "back", "home", "recents",
        "hold_key", "release_key", "press_key",
    }
    if cmd in simple_commands:
        return {"status": "ok", "result": {}}

    # Desktop-only mouse commands: return unsupported (we pretend to be Android)
    desktop_only = {"right_click", "middle_click", "mouse_scroll"}
    if cmd in desktop_only:
        return {"status": "ok", "unsupported": True, "result": {}}

    # Unknown command
    return {"status": "error", "error": f"unknown command: {cmd}"}
