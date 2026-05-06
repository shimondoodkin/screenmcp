"""Type definitions for ScreenMCP Python SDK."""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any, Literal, TypedDict


# ── Auth ─────────────────────────────────────────────────────────────────

@dataclass
class AuthMessage:
    """Sent by the controller to the worker on WebSocket open."""

    type: Literal["auth"] = "auth"
    key: str = ""
    role: Literal["controller"] = "controller"
    target_device_id: str = ""
    last_ack: int = 0

    def to_dict(self) -> dict[str, Any]:
        return {
            "type": self.type,
            "key": self.key,
            "role": self.role,
            "target_device_id": self.target_device_id,
            "last_ack": self.last_ack,
        }


# ── Server → Controller messages ────────────────────────────────────────

@dataclass
class AuthOkMessage:
    type: Literal["auth_ok"] = "auth_ok"
    resume_from: int = 0
    phone_connected: bool = False


@dataclass
class AuthFailMessage:
    type: Literal["auth_fail"] = "auth_fail"
    error: str = ""


@dataclass
class CmdAcceptedMessage:
    type: Literal["cmd_accepted"] = "cmd_accepted"
    id: int = 0


@dataclass
class PhoneStatusMessage:
    type: Literal["phone_status"] = "phone_status"
    connected: bool = False


@dataclass
class PingMessage:
    type: Literal["ping"] = "ping"


@dataclass
class ErrorMessage:
    type: Literal["error"] = "error"
    error: str = ""


# ── Command response from phone (relayed by worker) ─────────────────────

@dataclass
class CommandResponse:
    id: int = 0
    status: str = ""
    result: dict[str, Any] = field(default_factory=dict)
    error: str | None = None
    unsupported: bool = False


# ── Controller → Worker command ──────────────────────────────────────────

@dataclass
class ControllerCommand:
    cmd: str = ""
    params: dict[str, Any] | None = None

    def to_dict(self) -> dict[str, Any]:
        d: dict[str, Any] = {"cmd": self.cmd}
        if self.params:
            d["params"] = self.params
        return d


# ── Scroll direction literal ────────────────────────────────────────────

ScrollDirection = Literal["up", "down", "left", "right"]

# Direction → (dx, dy) multiplier mapping.  The *amount* is applied to the
# relevant axis; signs match Android scroll semantics (positive dy = scroll
# content up = "down" swipe gesture).
SCROLL_VECTORS: dict[ScrollDirection, tuple[int, int]] = {
    "up": (0, -1),
    "down": (0, 1),
    "left": (-1, 0),
    "right": (1, 0),
}


# ── UI Tree (ui_tree command) ───────────────────────────────────────────

class UiTreeRegion(TypedDict):
    """Bounding region for ui_tree region filtering."""

    min_x: int
    min_y: int
    max_x: int
    max_y: int


class UiTreeFlatNode(TypedDict, total=False):
    """Flat-mode UI tree node with optional fields."""

    controlType: str
    text: str
    value: str
    className: str
    resourceId: str
    contentDescription: str
    bounds: dict[str, Any]
    cx: int
    cy: int
    enabled: bool
    clickable: bool
    editable: bool
    scrollable: bool
    checked: bool
    focused: bool
    hwnd: int
    path: str
