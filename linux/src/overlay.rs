use image::{Rgba, RgbaImage};
use serde_json::Value;

/// Parse a named color or `#rrggbb` into RGBA. Defaults to red.
pub fn parse_color(s: Option<&str>) -> Rgba<u8> {
    let red = Rgba([255, 0, 0, 255]);
    let s = match s {
        Some(s) => s.trim(),
        None => return red,
    };
    if let Some(hex) = s.strip_prefix('#') {
        if hex.len() == 6 {
            if let Ok(n) = u32::from_str_radix(hex, 16) {
                return Rgba([(n >> 16) as u8, (n >> 8) as u8, n as u8, 255]);
            }
        }
        return red;
    }
    match s.to_ascii_lowercase().as_str() {
        "red" => red,
        "lime" | "green" => Rgba([0, 255, 0, 255]),
        "blue" => Rgba([0, 0, 255, 255]),
        "cyan" => Rgba([0, 255, 255, 255]),
        "yellow" => Rgba([255, 255, 0, 255]),
        "magenta" => Rgba([255, 0, 255, 255]),
        "orange" => Rgba([255, 136, 0, 255]),
        "white" => Rgba([255, 255, 255, 255]),
        "black" => Rgba([0, 0, 0, 255]),
        _ => red,
    }
}

#[inline]
fn put(img: &mut RgbaImage, x: i64, y: i64, c: Rgba<u8>) {
    if x >= 0 && y >= 0 && (x as u32) < img.width() && (y as u32) < img.height() {
        img.put_pixel(x as u32, y as u32, c);
    }
}

/// Filled circle of `radius` at (cx,cy) in `color`, ringed white then black for contrast.
pub fn draw_dot(img: &mut RgbaImage, cx: i64, cy: i64, radius: i64, color: Rgba<u8>) {
    let r = radius.max(1);
    let white = Rgba([255, 255, 255, 255]);
    let black = Rgba([0, 0, 0, 255]);
    for dy in -(r + 2)..=(r + 2) {
        for dx in -(r + 2)..=(r + 2) {
            let d2 = dx * dx + dy * dy;
            let (px, py) = (cx + dx, cy + dy);
            if d2 <= r * r {
                put(img, px, py, color);
            } else if d2 <= (r + 1) * (r + 1) {
                put(img, px, py, white);
            } else if d2 <= (r + 2) * (r + 2) {
                put(img, px, py, black);
            }
        }
    }
}

/// Crosshair cursor marker centered at (cx,cy): white core, black edge.
pub fn draw_cursor(img: &mut RgbaImage, cx: i64, cy: i64) {
    let white = Rgba([255, 255, 255, 255]);
    let black = Rgba([0, 0, 0, 255]);
    for d in -4..=4 {
        // black outline (thicker), then white core
        put(img, cx + d, cy - 1, black);
        put(img, cx + d, cy + 1, black);
        put(img, cx - 1, cy + d, black);
        put(img, cx + 1, cy + d, black);
        put(img, cx + d, cy, white);
        put(img, cx, cy + d, white);
    }
}

/// Apply `dots` (and pre-mapped `cursor_xy`) from params onto the final image.
/// `map` converts a screenshot-space (x,y) to output-image pixels, or None if clipped.
pub fn apply_overlays(
    img: &mut RgbaImage,
    params: Option<&Value>,
    cursor_xy: Option<(f64, f64)>,
    map: impl Fn(f64, f64) -> Option<(i64, i64)>,
) {
    let radius = params
        .and_then(|p| p.get("dot_radius"))
        .and_then(|v| v.as_i64())
        .unwrap_or(3)
        .clamp(1, 100);

    if let Some(dots) = params.and_then(|p| p.get("dots")).and_then(|v| v.as_array()) {
        for d in dots {
            let x = d.get("x").and_then(|v| v.as_f64());
            let y = d.get("y").and_then(|v| v.as_f64());
            if let (Some(x), Some(y)) = (x, y) {
                if let Some((px, py)) = map(x, y) {
                    let color = parse_color(d.get("color").and_then(|v| v.as_str()));
                    draw_dot(img, px, py, radius, color);
                }
            }
        }
    }

    let want_cursor = params
        .and_then(|p| p.get("cursor"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if want_cursor {
        if let Some((cx, cy)) = cursor_xy {
            if let Some((px, py)) = map(cx, cy) {
                draw_cursor(img, px, py);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};

    #[test]
    fn parse_color_named_and_hex() {
        assert_eq!(parse_color(Some("lime")), Rgba([0, 255, 0, 255]));
        assert_eq!(parse_color(Some("#ff8800")), Rgba([255, 136, 0, 255]));
        assert_eq!(parse_color(None), Rgba([255, 0, 0, 255])); // default red
        assert_eq!(parse_color(Some("bogus")), Rgba([255, 0, 0, 255]));
    }

    #[test]
    fn draw_dot_paints_center_and_leaves_far_pixels() {
        let mut img = RgbaImage::from_pixel(40, 40, Rgba([0, 0, 0, 255]));
        draw_dot(&mut img, 20, 20, 3, Rgba([0, 255, 0, 255]));
        assert_eq!(img.get_pixel(20, 20), &Rgba([0, 255, 0, 255]));
        assert_eq!(img.get_pixel(0, 0), &Rgba([0, 0, 0, 255]));
    }

    #[test]
    fn draw_dot_clips_off_image_without_panic() {
        let mut img = RgbaImage::from_pixel(10, 10, Rgba([0, 0, 0, 255]));
        draw_dot(&mut img, -5, -5, 3, Rgba([255, 0, 0, 255]));
        draw_dot(&mut img, 100, 100, 3, Rgba([255, 0, 0, 255]));
        // No panic == pass.
    }
}
