"""
Draw multiple visible dots on an image.

Usage:
  python mark_points.py input.png -o output.png -p 100,200,red 300,400,blue 500,600,green
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


def mark_points(input_path, points, output_path="output.png", radius=8):
    img = Image.open(input_path).convert("RGB")
    draw = ImageDraw.Draw(img)

    for x, y, color in points:
        fill = COLOR_MAP.get(color, COLOR_MAP["red"])
        draw.ellipse([x - radius, y - radius, x + radius, y + radius], fill=fill)
        draw.ellipse([x - radius, y - radius, x + radius, y + radius], outline=(0, 0, 0), width=2)
        gap = radius + 3
        line_len = radius + 12
        draw.line([x - line_len, y, x - gap, y], fill=(0, 0, 0), width=1)
        draw.line([x + gap, y, x + line_len, y], fill=(0, 0, 0), width=1)
        draw.line([x, y - line_len, x, y - gap], fill=(0, 0, 0), width=1)
        draw.line([x, y + gap, x, y + line_len], fill=(0, 0, 0), width=1)

    img.save(output_path)
    print(f"Marked {len(points)} points -> {output_path}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Mark multiple points on an image")
    parser.add_argument("input", help="Input image path")
    parser.add_argument("-o", "--output", default="output.png")
    parser.add_argument("-r", "--radius", type=int, default=8)
    parser.add_argument("-p", "--points", nargs="+", required=True,
                        help="Points as x,y,color (e.g. 100,200,red 300,400,blue)")
    args = parser.parse_args()
    points = []
    for p in args.points:
        parts = p.split(",")
        points.append((int(parts[0]), int(parts[1]), parts[2] if len(parts) > 2 else "red"))
    mark_points(args.input, points, args.output, args.radius)
