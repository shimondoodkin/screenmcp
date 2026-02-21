#!/usr/bin/env python3
"""ScreenMCP AI Agent -- control an Android phone with natural language.

Usage:
    python screenmcp_agent.py "Open Settings and turn on Wi-Fi"

Environment variables:
    SCREENMCP_API_KEY   -- ScreenMCP API key (required, starts with pk_)
    ANTHROPIC_API_KEY  -- Anthropic API key (required)
    SCREENMCP_API_URL   -- ScreenMCP server URL (default: https://server10.doodkin.com)
    SCREENMCP_DEVICE_ID -- Target device UUID (optional)
    SCREENMCP_MAX_STEPS -- Maximum agent loop iterations (default: 15)
"""

from __future__ import annotations

import asyncio
import base64
import json
import os
import sys
import textwrap
from typing import Any

import anthropic
from screenmcp import ScreenMCPClient

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

SCREENMCP_API_KEY = os.environ.get("SCREENMCP_API_KEY", "")
SCREENMCP_API_URL = os.environ.get("SCREENMCP_API_URL", "https://server10.doodkin.com")
SCREENMCP_DEVICE_ID = os.environ.get("SCREENMCP_DEVICE_ID", "")
ANTHROPIC_API_KEY = os.environ.get("ANTHROPIC_API_KEY", "")
MAX_STEPS = int(os.environ.get("SCREENMCP_MAX_STEPS", "15"))
MODEL = "claude-sonnet-4-20250514"

# ---------------------------------------------------------------------------
# System prompt
# ---------------------------------------------------------------------------

SYSTEM_PROMPT = textwrap.dedent("""\
    You are an AI agent controlling an Android phone. You receive a screenshot
    of the phone's current screen and a task from the user. You must decide
    the next single action to take.

    Respond with a JSON object (and nothing else) with the following structure:

    {
        "thinking": "<brief reasoning about what you see and what to do next>",
        "action": "<action_name>",
        "params": { ... },
        "done": false
    }

    Available actions and their params:

    - click:        {"x": int, "y": int}
    - long_click:   {"x": int, "y": int}
    - drag:         {"start_x": int, "start_y": int, "end_x": int, "end_y": int}
    - scroll:       {"direction": "up"|"down"|"left"|"right", "amount": int}
    - type:         {"text": "string to type"}
    - get_text:     {}
    - select_all:   {}
    - copy:         {}
    - paste:        {}
    - back:         {}
    - home:         {}
    - recents:      {}
    - ui_tree:      {}
    - screenshot:   {}     (take another screenshot without acting)
    - wait:         {}     (wait briefly, e.g. for loading)

    Set "done": true when the task is complete. When done, set action to "none"
    and explain in "thinking" why the task is finished.

    Rules:
    - Always pick exactly ONE action per turn.
    - Coordinates are in pixels. Typical phone resolution is 1080x1920 or 1080x2400.
    - If the screen shows a loading state, use "wait".
    - If you are stuck after several attempts, set done to true and explain.
    - Be precise with tap coordinates -- aim for the center of buttons/elements.
""")

# ---------------------------------------------------------------------------
# Agent
# ---------------------------------------------------------------------------


def parse_agent_response(text: str) -> dict[str, Any]:
    """Extract the JSON object from the model response."""
    text = text.strip()
    # Strip markdown fences if present
    if text.startswith("```"):
        first_nl = text.index("\n")
        last_fence = text.rfind("```")
        text = text[first_nl + 1 : last_fence].strip()
    return json.loads(text)


async def execute_action(
    phone: ScreenMCPClient, action: str, params: dict[str, Any]
) -> str:
    """Execute a single action on the phone and return a status string."""
    try:
        if action == "click":
            await phone.click(params["x"], params["y"])
            return f"Clicked at ({params['x']}, {params['y']})"

        if action == "long_click":
            await phone.long_click(params["x"], params["y"])
            return f"Long-clicked at ({params['x']}, {params['y']})"

        if action == "drag":
            await phone.drag(
                params["start_x"],
                params["start_y"],
                params["end_x"],
                params["end_y"],
                params.get("duration", 300),
            )
            return "Drag completed"

        if action == "scroll":
            await phone.scroll(params["direction"], params.get("amount", 500))
            return f"Scrolled {params['direction']}"

        if action == "type":
            await phone.type_text(params["text"])
            return f"Typed: {params['text']}"

        if action == "get_text":
            result = await phone.get_text()
            return f"Text: {result.get('text', '')}"

        if action == "select_all":
            await phone.select_all()
            return "Selected all"

        if action == "copy":
            await phone.copy()
            return "Copied"

        if action == "paste":
            await phone.paste()
            return "Pasted"

        if action == "back":
            await phone.back()
            return "Pressed Back"

        if action == "home":
            await phone.home()
            return "Pressed Home"

        if action == "recents":
            await phone.recents()
            return "Opened Recents"

        if action == "ui_tree":
            result = await phone.ui_tree()
            tree_str = json.dumps(result, indent=2)
            # Truncate very large trees
            if len(tree_str) > 4000:
                tree_str = tree_str[:4000] + "\n... (truncated)"
            return f"UI Tree:\n{tree_str}"

        if action == "screenshot":
            return "Taking another screenshot"

        if action == "wait":
            await asyncio.sleep(2)
            return "Waited 2 seconds"

        if action == "none":
            return "No action (task complete)"

        return f"Unknown action: {action}"

    except Exception as exc:
        return f"Error executing {action}: {exc}"


async def run_agent(instruction: str) -> None:
    """Main agent loop."""
    if not SCREENMCP_API_KEY:
        print("Error: SCREENMCP_API_KEY environment variable is required.")
        sys.exit(1)
    if not ANTHROPIC_API_KEY:
        print("Error: ANTHROPIC_API_KEY environment variable is required.")
        sys.exit(1)

    client = anthropic.Anthropic(api_key=ANTHROPIC_API_KEY)
    history: list[str] = []

    print(f"Task: {instruction}")
    print(f"Connecting to ScreenMCP ({SCREENMCP_API_URL})...")

    async with ScreenMCPClient(
        api_key=SCREENMCP_API_KEY,
        api_url=SCREENMCP_API_URL,
        device_id=SCREENMCP_DEVICE_ID or None,
        auto_reconnect=False,
    ) as phone:
        if not phone.phone_connected:
            print("Warning: phone is not currently connected. Proceeding anyway...")
        else:
            print("Phone connected. Starting agent loop.")

        for step in range(1, MAX_STEPS + 1):
            print(f"\n--- Step {step}/{MAX_STEPS} ---")

            # 1. Take screenshot
            print("Taking screenshot...")
            try:
                screenshot_result = await phone.screenshot(quality=50, max_width=1080)
            except Exception as exc:
                print(f"Screenshot failed: {exc}")
                break

            image_b64 = screenshot_result.get("image", "")
            if not image_b64:
                print("No image data received. Retrying...")
                await asyncio.sleep(1)
                continue

            # 2. Build message for Claude
            history_text = ""
            if history:
                history_text = "\n\nAction history:\n" + "\n".join(
                    f"  Step {i + 1}: {h}" for i, h in enumerate(history)
                )

            user_content = [
                {
                    "type": "image",
                    "source": {
                        "type": "base64",
                        "media_type": "image/jpeg",
                        "data": image_b64,
                    },
                },
                {
                    "type": "text",
                    "text": f"Task: {instruction}{history_text}\n\nWhat is the next action?",
                },
            ]

            # 3. Ask Claude
            try:
                response = client.messages.create(
                    model=MODEL,
                    max_tokens=1024,
                    system=SYSTEM_PROMPT,
                    messages=[{"role": "user", "content": user_content}],
                )
            except Exception as exc:
                print(f"Claude API error: {exc}")
                break

            raw_text = response.content[0].text
            print(f"Claude says: {raw_text[:300]}")

            # 4. Parse response
            try:
                decision = parse_agent_response(raw_text)
            except (json.JSONDecodeError, ValueError) as exc:
                print(f"Failed to parse Claude response: {exc}")
                history.append(f"[parse error] {raw_text[:100]}")
                continue

            thinking = decision.get("thinking", "")
            action = decision.get("action", "none")
            params = decision.get("params", {})
            done = decision.get("done", False)

            print(f"Thinking: {thinking}")
            print(f"Action: {action} {params}")

            if done:
                print(f"\nTask completed at step {step}.")
                print(f"Reason: {thinking}")
                break

            # 5. Execute action
            result_msg = await execute_action(phone, action, params)
            print(f"Result: {result_msg}")
            history.append(f"{action}({json.dumps(params)}) -> {result_msg}")

            # Brief pause to let the phone UI settle
            await asyncio.sleep(0.5)

        else:
            print(f"\nMax steps ({MAX_STEPS}) reached. Task may be incomplete.")

    print("\nAgent finished.")


# ---------------------------------------------------------------------------
# Entry point
# ---------------------------------------------------------------------------

def main() -> None:
    if len(sys.argv) < 2:
        print("Usage: python screenmcp_agent.py \"<instruction>\"")
        print("Example: python screenmcp_agent.py \"Open Settings and turn on Wi-Fi\"")
        sys.exit(1)

    instruction = " ".join(sys.argv[1:])
    asyncio.run(run_agent(instruction))


if __name__ == "__main__":
    main()
