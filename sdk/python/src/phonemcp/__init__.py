"""PhoneMCP Python SDK — control Android phones programmatically."""

from .client import (
    AuthError,
    CommandError,
    ConnectionError,
    PhoneMCPClient,
    PhoneMCPError,
)
from .types import CommandResponse, ScrollDirection

__all__ = [
    "AuthError",
    "CommandError",
    "CommandResponse",
    "ConnectionError",
    "PhoneMCPClient",
    "PhoneMCPError",
    "ScrollDirection",
]

__version__ = "0.1.0"
