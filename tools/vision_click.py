"""
Vision-guided clicking using ScreenMCP + Gemini vision.

Takes a screenshot via ScreenMCP local server, sends it to Gemini
with a description of what to click, gets back coordinates, clicks.

Usage:
  python vision_click.py "click the circle"
  python vision_click.py "click the Save button"
  python vision_click.py "click the X to close the window"

Requires:
  pip install google-genai requests Pillow

Environment:
  GEMINI_API_KEY     - Gemini API key
  SCREENMCP_URL      - ScreenMCP local server URL (default: http://127.0.0.1:6767)
  SCREENMCP_KEY      - ScreenMCP auth key
  SCREENMCP_WIDTH    - Screenshot width for scaling (default: 1280)
"""

import argparse
import base64
import io
import json
import os
import sys

import requests

SCREENMCP_URL = os.environ.get("SCREENMCP_URL", "http://127.0.0.1:6767")
SCREENMCP_KEY = os.environ.get("SCREENMCP_KEY", "")
SCREENSHOT_WIDTH = int(os.environ.get("SCREENMCP_WIDTH", "1280"))
GEMINI_API_KEY = os.environ.get("GEMINI_API_KEY", "")


_active_key = ""

def screenmcp_command(cmd: str, params: dict = None) -> dict:
    """Send a command to the ScreenMCP local server."""
    resp = requests.post(
        f"{SCREENMCP_URL}/command",
        headers={
            "Authorization": f"Bearer {_active_key}",
            "Content-Type": "application/json",
        },
        json={"cmd": cmd, "params": params or {}},
        timeout=30,
    )
    data = resp.json()
    if data.get("status") != "ok":
        raise RuntimeError(f"ScreenMCP error: {data.get('error', data)}")
    return data.get("result", {})


def take_screenshot() -> tuple[bytes, int, int]:
    """Take a screenshot, return (png_bytes, width, height)."""
    result = screenmcp_command("screenshot", {"max_width": SCREENSHOT_WIDTH})
    image_b64 = result["image"]
    webp_bytes = base64.b64decode(image_b64)

    # Convert WebP to PNG for Gemini
    from PIL import Image
    img = Image.open(io.BytesIO(webp_bytes))
    buf = io.BytesIO()
    img.save(buf, format="PNG")
    return buf.getvalue(), img.width, img.height


def ask_gemini_for_coordinates(png_bytes: bytes, width: int, height: int, instruction: str) -> list[dict]:
    """Send screenshot to Gemini and ask where to click.

    Returns list of {x, y, label} dicts in pixel coordinates.
    """
    from google import genai

    client = genai.Client(api_key=GEMINI_API_KEY)

    prompt = f"""You are a desktop automation assistant. The screenshot is {width}x{height} pixels.

The user wants to: {instruction}

Return the EXACT pixel coordinates (x, y) where to click to accomplish this.
If there are multiple targets, return all of them.

Respond with ONLY a JSON array, no markdown, no explanation:
[{{"x": 123, "y": 456, "label": "description of what this is"}}]

Be precise — estimate the CENTER of the target element."""

    image_part = genai.types.Part.from_bytes(data=png_bytes, mime_type="image/png")

    response = client.models.generate_content(
        model="gemini-2.0-flash",
        contents=[prompt, image_part],
    )

    text = response.text.strip()
    # Strip markdown fences if present
    if text.startswith("```"):
        text = text.split("\n", 1)[1]
        text = text.rsplit("```", 1)[0]

    return json.loads(text)


def click_at(x: int, y: int):
    """Click at coordinates using ScreenMCP with auto-scaling."""
    screenmcp_command("click", {"x": x, "y": y, "max_width": SCREENSHOT_WIDTH})


def main():
    parser = argparse.ArgumentParser(description="Vision-guided clicking with Gemini + ScreenMCP")
    parser.add_argument("instruction", help="What to click, e.g. 'click the circle'")
    parser.add_argument("--dry-run", action="store_true", help="Show coordinates but don't click")
    parser.add_argument("--all", action="store_true", help="Click all targets (default: first only)")
    parser.add_argument("--key", default=SCREENMCP_KEY, help="ScreenMCP auth key")
    parser.add_argument("--gemini-key", default=GEMINI_API_KEY, help="Gemini API key")
    args = parser.parse_args()

    global _active_key
    _active_key = args.key or SCREENMCP_KEY
    gemini_key = args.gemini_key or GEMINI_API_KEY

    if not _active_key:
        print("Error: set SCREENMCP_KEY env var or --key", file=sys.stderr)
        sys.exit(1)
    if not gemini_key:
        print("Error: set GEMINI_API_KEY env var or --gemini-key", file=sys.stderr)
        sys.exit(1)

    # Take screenshot
    print(f"Taking screenshot at {SCREENSHOT_WIDTH}px width...")
    png_bytes, w, h = take_screenshot()
    print(f"Screenshot: {w}x{h}")

    # Ask Gemini
    os.environ["GEMINI_API_KEY"] = gemini_key
    print(f"Asking Gemini: '{args.instruction}'...")
    targets = ask_gemini_for_coordinates(png_bytes, w, h, args.instruction)
    print(f"Found {len(targets)} target(s):")

    for i, t in enumerate(targets):
        x, y = int(t["x"]), int(t["y"])
        label = t.get("label", "")
        print(f"  [{i}] ({x}, {y}) — {label}")

    if args.dry_run:
        print("Dry run — not clicking.")
        return

    # Click
    to_click = targets if args.all else targets[:1]
    for t in to_click:
        x, y = int(t["x"]), int(t["y"])
        print(f"Clicking ({x}, {y})...")
        click_at(x, y)

    # Verify with screenshot
    print("Taking verification screenshot...")
    png_bytes, w, h = take_screenshot()
    with open("vision_click_result.png", "wb") as f:
        f.write(png_bytes)
    print("Saved verification to vision_click_result.png")


if __name__ == "__main__":
    main()
