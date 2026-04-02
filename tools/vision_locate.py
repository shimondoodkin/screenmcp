"""
Vision element locator — sends a screenshot to Gemini and returns coordinates.

Takes a base64 WebP image (from ScreenMCP screenshot) and a description,
returns JSON coordinates. Designed to be called by AI assistants that
handle the clicking themselves.

Usage:
  # From file
  python vision_locate.py --image screen.webp "find the circles"

  # From stdin (pipe base64 image)
  echo "<base64>" | python vision_locate.py --base64 "find the circles"

  # From ScreenMCP directly
  python vision_locate.py --screenmcp "find the circles"

Output (stdout JSON):
  [{"x": 123, "y": 456, "label": "large circle"}]

Requires:
  pip install google-genai Pillow requests

Environment:
  GEMINI_API_KEY     - Gemini API key
  SCREENMCP_URL      - ScreenMCP local server URL (default: http://127.0.0.1:6767)
  SCREENMCP_KEY      - ScreenMCP auth key
  SCREENMCP_WIDTH    - Screenshot width (default: 1280)
"""

import argparse
import base64
import io
import json
import os
import sys

SCREENMCP_URL = os.environ.get("SCREENMCP_URL", "http://127.0.0.1:6767")
SCREENMCP_KEY = os.environ.get("SCREENMCP_KEY", "")
SCREENSHOT_WIDTH = int(os.environ.get("SCREENMCP_WIDTH", "1280"))


def take_screenshot(key: str, max_width: int = 1280) -> tuple[bytes, int, int]:
    """Take screenshot via ScreenMCP, return (png_bytes, width, height)."""
    import requests
    from PIL import Image

    resp = requests.post(
        f"{SCREENMCP_URL}/command",
        headers={"Authorization": f"Bearer {key}", "Content-Type": "application/json"},
        json={"cmd": "screenshot", "params": {"max_width": max_width}},
        timeout=30,
    )
    data = resp.json()
    if data.get("status") != "ok":
        raise RuntimeError(f"ScreenMCP error: {data.get('error', data)}")

    webp_bytes = base64.b64decode(data["result"]["image"])
    img = Image.open(io.BytesIO(webp_bytes))
    buf = io.BytesIO()
    img.save(buf, format="PNG")
    return buf.getvalue(), img.width, img.height


def load_image(path: str) -> tuple[bytes, int, int]:
    """Load image from file, return (png_bytes, width, height)."""
    from PIL import Image

    img = Image.open(path)
    buf = io.BytesIO()
    img.save(buf, format="PNG")
    return buf.getvalue(), img.width, img.height


def locate(png_bytes: bytes, width: int, height: int, instruction: str) -> list[dict]:
    """Ask Gemini to find elements in the image. Returns [{x, y, label}]."""
    from google import genai

    client = genai.Client(api_key=os.environ.get("GEMINI_API_KEY") or os.environ.get("GOOGLE_API_KEY"))

    prompt = f"""You are a desktop automation vision assistant. The screenshot is {width}x{height} pixels.

Task: {instruction}

Return the EXACT pixel coordinates (x, y) of each matching element's CENTER.

Respond with ONLY a JSON array, no markdown, no explanation:
[{{"x": 123, "y": 456, "label": "description"}}]"""

    image_part = genai.types.Part.from_bytes(data=png_bytes, mime_type="image/png")
    response = client.models.generate_content(
        model="gemini-2.5-flash",
        contents=[prompt, image_part],
    )

    text = (response.text or "").strip()
    if text.startswith("```"):
        text = text.split("\n", 1)[1]
        text = text.rsplit("```", 1)[0]

    return json.loads(text)


def main():
    parser = argparse.ArgumentParser(description="Locate elements in a screenshot using Gemini vision")
    parser.add_argument("instruction", help="What to find, e.g. 'find all buttons'")
    parser.add_argument("--image", help="Path to image file")
    parser.add_argument("--base64", action="store_true", help="Read base64 image from stdin")
    parser.add_argument("--screenmcp", action="store_true", help="Take screenshot from ScreenMCP")
    parser.add_argument("--key", default="", help="ScreenMCP auth key")
    parser.add_argument("--width", type=int, default=SCREENSHOT_WIDTH, help="Screenshot width")
    args = parser.parse_args()

    sw = args.width

    if args.screenmcp:
        key = args.key or SCREENMCP_KEY
        if not key:
            print("Error: set SCREENMCP_KEY or --key", file=sys.stderr)
            sys.exit(1)
        png_bytes, w, h = take_screenshot(key, sw)
    elif args.image:
        png_bytes, w, h = load_image(args.image)
    elif args.base64:
        from PIL import Image
        raw = base64.b64decode(sys.stdin.read().strip())
        img = Image.open(io.BytesIO(raw))
        buf = io.BytesIO()
        img.save(buf, format="PNG")
        png_bytes, w, h = buf.getvalue(), img.width, img.height
    else:
        print("Error: provide --image, --base64, or --screenmcp", file=sys.stderr)
        sys.exit(1)

    targets = locate(png_bytes, w, h, args.instruction)
    print(json.dumps(targets, indent=2))


if __name__ == "__main__":
    main()
