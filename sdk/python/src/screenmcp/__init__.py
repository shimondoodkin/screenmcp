"""ScreenMCP Python SDK — control Android phones programmatically."""

from .client import (
    AuthError,
    CommandError,
    ConnectionError,
    DeviceConnection,
    ScreenMCPClient,
    ScreenMCPError,
)
from .selector import ElementHandle, FoundElement, find_elements, parse_selector
from .types import CommandResponse, ScrollDirection

__all__ = [
    "AuthError",
    "CommandError",
    "CommandResponse",
    "ConnectionError",
    "DeviceConnection",
    "ElementHandle",
    "FoundElement",
    "ScreenMCPClient",
    "ScreenMCPError",
    "ScrollDirection",
    "find_elements",
    "parse_selector",
]

__version__ = "0.1.0"
