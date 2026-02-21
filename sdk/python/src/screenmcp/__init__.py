"""ScreenMCP Python SDK — control Android phones programmatically."""

from .client import (
    AuthError,
    CommandError,
    ConnectionError,
    ScreenMCPClient,
    ScreenMCPError,
)
from .types import CommandResponse, ScrollDirection

__all__ = [
    "AuthError",
    "CommandError",
    "CommandResponse",
    "ConnectionError",
    "ScreenMCPClient",
    "ScreenMCPError",
    "ScrollDirection",
]

__version__ = "0.1.0"
