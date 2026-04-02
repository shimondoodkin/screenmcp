"""
Draw a visible dot at given coordinates on an image.

Usage:
  python mark_point.py input.png 400 300
  python mark_point.py input.png 400 300 -o marked.png
  python mark_point.py input.png 400 300 --radius 10 --color red
"""

import argparse
from PIL import Image, ImageDraw

COLOR_MAP = {
    "red": (255, 0, 0),
    "green": (0, 200, 0),
    "blue": (0, 80, 255),
    "yellow": (255, 255, 0),
    "magenta": (255, 0, 255),
    "cyan": (0, 255, 255),
    "white": (255, 255, 255),
    "black": (0, 0, 0),
}


def mark_point(input_path, x, y, output_path=None, radius=8, color="red"):
    img = Image.open(input_path).convert("RGB")
    draw = ImageDraw.Draw(img)

    fill = COLOR_MAP.get(color, COLOR_MAP["red"])

    # Filled circle
    draw.ellipse([x - radius, y - radius, x + radius, y + radius], fill=fill)
    # Black outline for contrast
    draw.ellipse([x - radius, y - radius, x + radius, y + radius], outline=(0, 0, 0), width=2)
    # Crosshair lines extending beyond the dot
    gap = radius + 3
    line_len = radius + 12
    draw.line([x - line_len, y, x - gap, y], fill=(0, 0, 0), width=1)
    draw.line([x + gap, y, x + line_len, y], fill=(0, 0, 0), width=1)
    draw.line([x, y - line_len, x, y - gap], fill=(0, 0, 0), width=1)
    draw.line([x, y + gap, x, y + line_len], fill=(0, 0, 0), width=1)

    if output_path is None:
        output_path = "output.png"

    img.save(output_path)
    print(f"Marked ({x}, {y}) → {output_path}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Mark a point on an image")
    parser.add_argument("input", help="Input image path")
    parser.add_argument("x", type=int, help="X coordinate")
    parser.add_argument("y", type=int, help="Y coordinate")
    parser.add_argument("-o", "--output", default="output.png", help="Output path (default: output.png)")
    parser.add_argument("--radius", type=int, default=8, help="Dot radius (default: 8)")
    parser.add_argument("--color", default="red", choices=COLOR_MAP.keys(), help="Dot color (default: red)")
    args = parser.parse_args()
    mark_point(args.input, args.x, args.y, args.output, args.radius, args.color)
