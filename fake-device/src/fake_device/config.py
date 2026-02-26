"""Configuration from CLI arguments and environment variables."""

from __future__ import annotations

import argparse
import os
from dataclasses import dataclass, field
from typing import Literal


@dataclass
class TestModes:
    """Flags that alter fake device behavior for testing edge cases."""

    screen_off: bool = False
    typing_fails: bool = False
    slow_response: float = 0.0  # seconds of added delay; 0 = no delay
    disconnect_after: int = 0  # disconnect after N commands; 0 = never


@dataclass
class Config:
    """All runtime configuration for the fake device."""

    api_url: str = "http://localhost:3000"
    device_id: str = "fake-device-001"
    device_name: str = "Fake Test Device"
    user_id: str = "local-user"
    mode: Literal["opensource", "cloud"] = "opensource"
    test_modes: TestModes = field(default_factory=TestModes)
    verbose: bool = False


def parse_args(argv: list[str] | None = None) -> Config:
    """Parse CLI arguments and env vars into a Config."""
    parser = argparse.ArgumentParser(
        prog="fake-device",
        description="Fake ScreenMCP device for automated testing",
    )
    parser.add_argument(
        "--api-url",
        default=os.environ.get("FAKE_DEVICE_API_URL", "http://localhost:3000"),
        help="MCP server API URL (default: http://localhost:3000)",
    )
    parser.add_argument(
        "--device-id",
        default=os.environ.get("FAKE_DEVICE_ID", "fake-device-001"),
        help="Device ID to identify as (default: fake-device-001)",
    )
    parser.add_argument(
        "--device-name",
        default=os.environ.get("FAKE_DEVICE_NAME", "Fake Test Device"),
        help="Human-readable device name for registration",
    )
    parser.add_argument(
        "--user-id",
        default=os.environ.get("FAKE_DEVICE_USER_ID", "local-user"),
        help="User ID / auth token (default: local-user)",
    )
    parser.add_argument(
        "--mode",
        choices=["opensource", "cloud"],
        default=os.environ.get("FAKE_DEVICE_MODE", "opensource"),
        help="Login mode (default: opensource)",
    )
    parser.add_argument(
        "-v", "--verbose",
        action="store_true",
        default=os.environ.get("FAKE_DEVICE_VERBOSE", "").lower() in ("1", "true", "yes"),
        help="Enable verbose logging",
    )

    # Test mode flags
    test_group = parser.add_argument_group("test modes")
    test_group.add_argument(
        "--screen-off",
        action="store_true",
        help="Screenshot returns screen-off error",
    )
    test_group.add_argument(
        "--typing-fails",
        action="store_true",
        help="Type command returns no-focused-element error",
    )
    test_group.add_argument(
        "--slow-response",
        type=float,
        default=float(os.environ.get("FAKE_DEVICE_SLOW_RESPONSE", "0")),
        metavar="SECONDS",
        help="Add delay before every response (seconds)",
    )
    test_group.add_argument(
        "--disconnect-after",
        type=int,
        default=int(os.environ.get("FAKE_DEVICE_DISCONNECT_AFTER", "0")),
        metavar="N",
        help="Disconnect after N commands (0 = never)",
    )

    args = parser.parse_args(argv)

    return Config(
        api_url=args.api_url,
        device_id=args.device_id,
        device_name=args.device_name,
        user_id=args.user_id,
        mode=args.mode,
        verbose=args.verbose,
        test_modes=TestModes(
            screen_off=args.screen_off,
            typing_fails=args.typing_fails,
            slow_response=args.slow_response,
            disconnect_after=args.disconnect_after,
        ),
    )
