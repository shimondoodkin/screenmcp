"""
Add pixel-perfect rulers around an image.

Generates horizontal (top) and vertical (left) rulers with:
- Major ticks every 100px with numeric labels
- Medium ticks every 50px
- Minor ticks every 10px

Usage:
  python add_rulers.py input.png                    # writes input_rulers.png
  python add_rulers.py input.png -o output.png      # custom output path
  python add_rulers.py input.png --major 200        # major ticks every 200px
"""

import argparse
import os
from PIL import Image, ImageDraw, ImageFont

RULER_THICKNESS = 30      # px width of ruler band
CORNER_SIZE = RULER_THICKNESS  # square in top-left corner
BG_COLOR = (245, 245, 245)
TICK_COLOR = (0, 0, 0)
LABEL_COLOR = (60, 60, 60)
FONT_SIZE = 10


def get_font(size=FONT_SIZE):
    """Get a small monospace-ish font."""
    for name in [
        "consola.ttf", "cour.ttf", "arial.ttf",  # Windows
        "DejaVuSansMono.ttf", "LiberationMono-Regular.ttf",  # Linux
        "Menlo.ttc", "Monaco.ttf",  # macOS
    ]:
        try:
            return ImageFont.truetype(name, size)
        except (OSError, IOError):
            continue
    return ImageFont.load_default()


def draw_horizontal_ruler(draw, img_width, ruler_y, offset_x, major, font):
    """Draw the top horizontal ruler."""
    # Background
    draw.rectangle([offset_x, ruler_y, offset_x + img_width, ruler_y + RULER_THICKNESS - 1], fill=BG_COLOR)
    # Bottom edge line
    draw.line([offset_x, ruler_y + RULER_THICKNESS - 1, offset_x + img_width, ruler_y + RULER_THICKNESS - 1], fill=TICK_COLOR)

    for px in range(0, img_width + 1):
        x = offset_x + px
        if px % major == 0:
            # Major tick
            tick_len = 12
            draw.line([x, ruler_y + RULER_THICKNESS - 1, x, ruler_y + RULER_THICKNESS - 1 - tick_len], fill=TICK_COLOR)
            label = str(px)
            bbox = font.getbbox(label)
            tw = bbox[2] - bbox[0]
            lx = x - tw // 2
            draw.text((lx, ruler_y + 2), label, fill=LABEL_COLOR, font=font)
        elif px % (major // 2) == 0:
            # Medium tick (half of major)
            tick_len = 8
            draw.line([x, ruler_y + RULER_THICKNESS - 1, x, ruler_y + RULER_THICKNESS - 1 - tick_len], fill=TICK_COLOR)
        elif px % 10 == 0:
            # Minor tick every 10px
            tick_len = 4
            draw.line([x, ruler_y + RULER_THICKNESS - 1, x, ruler_y + RULER_THICKNESS - 1 - tick_len], fill=TICK_COLOR)


def draw_vertical_ruler(draw, img_height, ruler_x, offset_y, major, font):
    """Draw the left vertical ruler."""
    # Background
    draw.rectangle([ruler_x, offset_y, ruler_x + RULER_THICKNESS - 1, offset_y + img_height], fill=BG_COLOR)
    # Right edge line
    draw.line([ruler_x + RULER_THICKNESS - 1, offset_y, ruler_x + RULER_THICKNESS - 1, offset_y + img_height], fill=TICK_COLOR)

    for px in range(0, img_height + 1):
        y = offset_y + px
        if px % major == 0:
            # Major tick
            tick_len = 12
            draw.line([ruler_x + RULER_THICKNESS - 1, y, ruler_x + RULER_THICKNESS - 1 - tick_len, y], fill=TICK_COLOR)
            label = str(px)
            bbox = font.getbbox(label)
            tw = bbox[2] - bbox[0]
            th = bbox[3] - bbox[1]
            # Draw rotated label
            lx = ruler_x + 2
            ly = y - th // 2
            draw.text((lx, ly), label, fill=LABEL_COLOR, font=font)
        elif px % (major // 2) == 0:
            # Medium tick
            tick_len = 8
            draw.line([ruler_x + RULER_THICKNESS - 1, y, ruler_x + RULER_THICKNESS - 1 - tick_len, y], fill=TICK_COLOR)
        elif px % 10 == 0:
            # Minor tick every 10px
            tick_len = 4
            draw.line([ruler_x + RULER_THICKNESS - 1, y, ruler_x + RULER_THICKNESS - 1 - tick_len, y], fill=TICK_COLOR)


def add_rulers(input_path, output_path=None, major=100):
    """Add rulers to an image and save."""
    img = Image.open(input_path).convert("RGB")
    w, h = img.size

    # New canvas: ruler bands + original image
    new_w = RULER_THICKNESS + w
    new_h = RULER_THICKNESS + h
    canvas = Image.new("RGB", (new_w, new_h), BG_COLOR)

    # Paste original image offset by ruler thickness
    canvas.paste(img, (RULER_THICKNESS, RULER_THICKNESS))

    draw = ImageDraw.Draw(canvas)
    font = get_font()

    # Draw corner square
    draw.rectangle([0, 0, RULER_THICKNESS - 1, RULER_THICKNESS - 1], fill=BG_COLOR)
    draw.line([0, RULER_THICKNESS - 1, RULER_THICKNESS - 1, RULER_THICKNESS - 1], fill=TICK_COLOR)
    draw.line([RULER_THICKNESS - 1, 0, RULER_THICKNESS - 1, RULER_THICKNESS - 1], fill=TICK_COLOR)

    # Draw rulers
    draw_horizontal_ruler(draw, w, 0, RULER_THICKNESS, major, font)
    draw_vertical_ruler(draw, h, 0, RULER_THICKNESS, major, font)

    if output_path is None:
        base, ext = os.path.splitext(input_path)
        output_path = f"{base}_rulers{ext}"

    canvas.save(output_path)
    print(f"Saved: {output_path} ({new_w}x{new_h})")
    return output_path


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Add pixel rulers around an image")
    parser.add_argument("input", help="Input image path")
    parser.add_argument("-o", "--output", help="Output image path")
    parser.add_argument("--major", type=int, default=100, help="Major tick interval (default: 100)")
    args = parser.parse_args()
    add_rulers(args.input, args.output, args.major)
