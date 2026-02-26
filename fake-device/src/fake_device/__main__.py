"""CLI entry point for the fake ScreenMCP device."""

from __future__ import annotations

import asyncio
import logging
import signal
import sys

from .client import FakeDeviceClient
from .config import parse_args


def main() -> None:
    config = parse_args()

    # Configure logging
    level = logging.DEBUG if config.verbose else logging.INFO
    logging.basicConfig(
        level=level,
        format="%(asctime)s %(levelname)-8s [%(name)s] %(message)s",
        datefmt="%H:%M:%S",
    )

    # Summarize test modes
    modes = config.test_modes
    active_modes: list[str] = []
    if modes.screen_off:
        active_modes.append("screen-off")
    if modes.typing_fails:
        active_modes.append("typing-fails")
    if modes.slow_response > 0:
        active_modes.append(f"slow-response={modes.slow_response}s")
    if modes.disconnect_after > 0:
        active_modes.append(f"disconnect-after={modes.disconnect_after}")

    logger = logging.getLogger("fake_device")
    logger.info("Fake Device Client starting")
    logger.info("  Device ID:   %s", config.device_id)
    logger.info("  Device Name: %s", config.device_name)
    logger.info("  User ID:     %s", config.user_id)
    logger.info("  API URL:     %s", config.api_url)
    logger.info("  Mode:        %s", config.mode)
    if active_modes:
        logger.info("  Test modes:  %s", ", ".join(active_modes))

    client = FakeDeviceClient(config)

    async def run() -> None:
        loop = asyncio.get_running_loop()

        # Handle Ctrl+C gracefully
        stop_event = asyncio.Event()

        def _signal_handler() -> None:
            logger.info("Received shutdown signal")
            client.stop()
            stop_event.set()

        for sig in (signal.SIGINT, signal.SIGTERM):
            loop.add_signal_handler(sig, _signal_handler)

        try:
            task = asyncio.create_task(client.run())

            # Wait for either the task to finish or a shutdown signal
            done, pending = await asyncio.wait(
                [task, asyncio.create_task(stop_event.wait())],
                return_when=asyncio.FIRST_COMPLETED,
            )

            for t in pending:
                t.cancel()
                try:
                    await t
                except asyncio.CancelledError:
                    pass

        except asyncio.CancelledError:
            pass
        finally:
            # Unregister on shutdown
            logger.info("Unregistering device before exit...")
            await client.unregister_device()
            logger.info("Shutdown complete")

    try:
        asyncio.run(run())
    except KeyboardInterrupt:
        pass


if __name__ == "__main__":
    main()
