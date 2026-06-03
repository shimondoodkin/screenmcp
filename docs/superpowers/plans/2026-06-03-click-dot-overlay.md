# Dot + Cursor Overlay Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add optional `dots`, `cursor`, and `dot_radius` overlay params to the existing `screenshot` and `screenshot_region` commands so a model can paint colored dots at estimated click positions (and optionally the real cursor) and visually verify before clicking.

**Architecture:** Overlays are drawn on the **final** image, just before WebP encode, in screenshot-space coordinates (the same space `click` uses). `screenshot` draws dots at identity coords; `screenshot_region` translates each dot by the region origin and output scale, clipping anything outside the crop. Each client gets a small dependency-free draw helper. Windows is the reference implementation (the only client buildable on this machine).

**Tech Stack:** Rust (`image` crate, already a dep) for desktop clients; Kotlin `Canvas` for Android; Python `PIL.ImageDraw` for python-cli; Zod (TS) and `serde_json` (Rust) for the two MCP tool schemas.

**Spec:** [docs/superpowers/specs/2026-06-03-click-dot-overlay-design.md](../specs/2026-06-03-click-dot-overlay-design.md)

**Branch:** `feature/click-dot-overlay` (already created)

---

## Shared Conventions (referenced by every client task)

**Param shape** (added to `screenshot` and `screenshot_region`):
- `dots`: array of objects `{ "x": number, "y": number, "color"?: string }`. Coords are screenshot-space.
- `cursor`: bool, default `false`.
- `dot_radius`: int, default `3`.

**Color parsing:** accept named colors `red, lime, green, cyan, yellow, magenta, white, black, blue, orange` or `#rrggbb`. Unknown/missing ⇒ `red`.

**Dot rendering:** filled circle of `radius` px in the dot color, then a 1px white ring and a 1px black ring just outside it (contrast on any background).

**Cursor rendering:** a crosshair (two 9px perpendicular strokes) centered on the cursor point, white core with black edge. Distinct from dots so it is never confused with an estimate.

**Coordinate mapping:**
- `screenshot`: returned image pixels == screenshot space ⇒ draw dots at `(x, y)` directly; clip if outside image bounds.
- `screenshot_region`: `x' = (x − min_x) × out_scale_x`, `y' = (y − min_y) × out_scale_y`, where `out_scale = output_dim / crop_dim`. Clip if `(x', y')` outside the output image.
- Cursor (native pixels from OS) → screenshot space = `native × (output_dim / full_dim)`, then through the same per-command mapping above.

---

## Task 1: Windows — overlay helper module (pure, TDD)

**Files:**
- Create: `windows/src/overlay.rs`
- Modify: `windows/src/main.rs` (add `mod overlay;`)
- Test: inline `#[cfg(test)]` in `windows/src/overlay.rs`

- [ ] **Step 1: Add the module declaration**

In `windows/src/main.rs`, add alongside the other `mod` lines:

```rust
mod overlay;
```

- [ ] **Step 2: Write the failing test**

Create `windows/src/overlay.rs` with ONLY the tests first:

```rust
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
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cd windows && cargo test overlay`
Expected: FAIL — `cannot find function parse_color` / `draw_dot`.

- [ ] **Step 4: Write the implementation (above the test module)**

Prepend to `windows/src/overlay.rs`:

```rust
use image::{Rgba, RgbaImage};
use serde_json::Value;

/// Parse a named color or `#rrggbb` into RGBA. Defaults to red.
pub fn parse_color(s: Option<&str>) -> Rgba<u8> {
    let red = Rgba([255, 0, 0, 255]);
    let s = match s { Some(s) => s.trim(), None => return red };
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
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cd windows && cargo test overlay`
Expected: PASS (3 tests).

- [ ] **Step 6: Commit**

```bash
git add windows/src/overlay.rs windows/src/main.rs
git commit -m "feat(windows): add overlay draw helper (dot/cursor/color)"
```

---

## Task 2: Windows — wire overlays into `screenshot`

**Files:**
- Modify: `windows/src/commands.rs` (`handle_screenshot`, ~lines 87-165; cursor helper)

- [ ] **Step 1: Add a Win32 cursor-position helper**

Add near `get_screen_dimensions` in `windows/src/commands.rs`:

```rust
/// Current cursor position in native screen pixels, or None on failure.
fn cursor_native_pos() -> Option<(f64, f64)> {
    #[cfg(windows)]
    unsafe {
        let mut pt = windows_sys::Win32::Foundation::POINT { x: 0, y: 0 };
        if windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut pt) != 0 {
            return Some((pt.x as f64, pt.y as f64));
        }
        None
    }
    #[cfg(not(windows))]
    { None }
}
```

If `windows-sys` is not already a dependency, prefer the cursor crate already used by `enigo`, or add `windows-sys = { version = "0.59", features = ["Win32_UI_WindowsAndMessaging", "Win32_Foundation"] }` to `windows/Cargo.toml`. (Check first: `grep -n "windows-sys\|winapi" windows/Cargo.toml`. If `enigo` exposes mouse location, e.g. `Enigo::location()`, use that instead and skip the extra dependency.)

- [ ] **Step 2: Insert the overlay call before WebP encode in `handle_screenshot`**

In `handle_screenshot`, after the `let img = ...` resize block resolves the final `img` (currently around line 141, just before the "Encode as WebP" comment), make `img` mutable and add:

```rust
    let mut img = img;
    // Output image == screenshot space for full-screen capture.
    let out_w = img.width() as f64;
    let out_h = img.height() as f64;
    let cursor_xy = cursor_native_pos().map(|(nx, ny)| {
        (nx * out_w / width as f64, ny * out_h / height as f64)
    });
    crate::overlay::apply_overlays(&mut img, params, cursor_xy, |x, y| {
        let (px, py) = (x.round() as i64, y.round() as i64);
        if px >= 0 && py >= 0 && (px as u32) < img.width() && (py as u32) < img.height() {
            Some((px, py))
        } else {
            None
        }
    });
```

Note: `width`/`height` are the native capture dims captured at the top of the function (lines 100-101). The closure borrows `img` immutably for bounds while `apply_overlays` holds `&mut img` — to avoid a borrow conflict, capture the bounds first:

```rust
    let (bw, bh) = (img.width(), img.height());
    crate::overlay::apply_overlays(&mut img, params, cursor_xy, move |x, y| {
        let (px, py) = (x.round() as i64, y.round() as i64);
        if px >= 0 && py >= 0 && (px as u32) < bw && (py as u32) < bh { Some((px, py)) } else { None }
    });
```

- [ ] **Step 3: Type-check**

Run: `cd windows && cargo build`
Expected: compiles clean.

- [ ] **Step 4: Manual smoke test**

Run the Windows client, then from an MCP/SDK call: `screenshot` with `{ "dots": [{"x": 100, "y": 100, "color": "lime"}], "cursor": true }`. Open the returned image; confirm a green ringed dot at (100,100) and a crosshair at the live cursor.

- [ ] **Step 5: Commit**

```bash
git add windows/src/commands.rs windows/Cargo.toml
git commit -m "feat(windows): paint dot/cursor overlays in screenshot"
```

---

## Task 3: Windows — wire overlays into `screenshot_region`

**Files:**
- Modify: `windows/src/commands.rs` (`handle_screenshot_region`, ~lines 167-251)

- [ ] **Step 1: Insert overlay call before WebP encode**

In `handle_screenshot_region`, after the final `let img = ...` output-scale block (around line 240, before `let quality = ...`), add. The region origin is `min_x/min_y` (screenshot space); `scale_x/scale_y` (lines 187-195) convert screenshot space → native; the crop is `crop_w × crop_h` native and the output is `img.width() × img.height()`.

```rust
    let mut img = img;
    let (bw, bh) = (img.width() as f64, img.height() as f64);
    // output px per screenshot-space unit = (output_dim / crop_native) * (native per screenshot unit)
    let out_per_ss_x = (bw / crop_w as f64) * scale_x;
    let out_per_ss_y = (bh / crop_h as f64) * scale_y;
    let cursor_xy = cursor_native_pos().map(|(nx, ny)| (nx / scale_x, ny / scale_y)); // native → screenshot space
    let (iw, ih) = (img.width(), img.height());
    crate::overlay::apply_overlays(&mut img, Some(p), cursor_xy, move |x, y| {
        let px = ((x - min_x) * out_per_ss_x).round() as i64;
        let py = ((y - min_y) * out_per_ss_y).round() as i64;
        if px >= 0 && py >= 0 && (px as u32) < iw && (py as u32) < ih { Some((px, py)) } else { None }
    });
```

(`min_x`, `min_y`, `scale_x`, `scale_y`, `crop_w`, `crop_h` are all in scope from earlier in the function.)

- [ ] **Step 2: Type-check**

Run: `cd windows && cargo build`
Expected: compiles clean.

- [ ] **Step 3: Manual smoke test**

`screenshot_region` with a region of `{min_x:0,min_y:0,max_x:400,max_y:400}` and `dots:[{x:200,y:200,color:"cyan"}]`. Confirm the dot lands at the center of the returned 400×400-ish crop. Add a dot at `{x:900,y:900}` and confirm it is clipped (absent), no panic.

- [ ] **Step 4: Commit**

```bash
git add windows/src/commands.rs
git commit -m "feat(windows): paint dot/cursor overlays in screenshot_region"
```

---

## Task 4: MCP Server (TS) — add params to the two tool schemas

**Files:**
- Modify: `mcp-server/src/mcp.ts` (`screenshot` tool ~lines 29-42, `screenshot_region` tool ~lines 433-451)

- [ ] **Step 1: Define a shared overlay schema fragment**

Near `scalingParams` (line 21) add:

```ts
const overlayParams = {
  dots: z.array(z.object({
    x: z.number().describe('X in screenshot space (same coords as click)'),
    y: z.number().describe('Y in screenshot space'),
    color: z.string().optional().describe('Named color (red, lime, cyan, yellow, magenta, white, ...) or #rrggbb. Default red.'),
  })).optional().describe('Paint dots at estimated click positions to verify before clicking.'),
  cursor: z.boolean().optional().describe('Draw the real mouse cursor as a crosshair (desktop only; ignored on Android).'),
  dot_radius: z.number().int().optional().describe('Dot radius in pixels (default 3).'),
};
```

- [ ] **Step 2: Spread it into both tool input schemas**

Add `...overlayParams,` to the `inputSchema` of the `screenshot` tool and the `screenshot_region` tool. Also append to each tool's `description`: `' Optionally paint estimated-click dots and the cursor.'`

- [ ] **Step 3: Type-check**

Run: `cd mcp-server && npx tsc --noEmit`
Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add mcp-server/src/mcp.ts
git commit -m "feat(mcp-server): expose dots/cursor/dot_radius on screenshot tools"
```

---

## Task 5: Cloud MCP Server (Rust) — add params to the two ToolDefs

**Files:**
- Modify: `screenmcp-cloud/mcp-server/src/tools.rs` (the `screenshot` and `screenshot_region` `ToolDef`s in `all_tools()`)

- [ ] **Step 1: Locate the two ToolDefs**

Run: `grep -n '"screenshot"\|"screenshot_region"' screenmcp-cloud/mcp-server/src/tools.rs`

- [ ] **Step 2: Add overlay properties to each `input_schema`**

In both `json!({...})` schemas, add to `properties`:

```rust
"dots": {
    "type": "array",
    "description": "Paint dots at estimated click positions to verify before clicking.",
    "items": {
        "type": "object",
        "properties": {
            "x": {"type": "number", "description": "X in screenshot space"},
            "y": {"type": "number", "description": "Y in screenshot space"},
            "color": {"type": "string", "description": "Named color or #rrggbb (default red)"}
        },
        "required": ["x", "y"]
    }
},
"cursor": {"type": "boolean", "description": "Draw the real cursor as a crosshair (desktop only)"},
"dot_radius": {"type": "integer", "description": "Dot radius px (default 3)"}
```

Do NOT add them to any `required` array (all optional).

- [ ] **Step 3: Type-check**

Run: `cd screenmcp-cloud/mcp-server && cargo build` (or `cargo check`)
Expected: compiles. (If not buildable on this machine, verify by inspection and rely on CI.)

- [ ] **Step 4: Commit**

```bash
git add screenmcp-cloud/mcp-server/src/tools.rs
git commit -m "feat(cloud-mcp): expose dots/cursor/dot_radius on screenshot tools"
```

---

## Task 6: Mac client — overlay helper + wire into both handlers

**Files:**
- Create: `mac/src/overlay.rs` (identical contents to `windows/src/overlay.rs`)
- Modify: `mac/src/main.rs` (`mod overlay;`)
- Modify: `mac/src/commands.rs` (`handle_screenshot`, `handle_screenshot_region`, cursor helper)

- [ ] **Step 1: Copy the overlay module**

Create `mac/src/overlay.rs` with the **exact** contents from Task 1 Step 4 (the `parse_color` / `put` / `draw_dot` / `draw_cursor` / `apply_overlays` block) plus the `#[cfg(test)]` tests from Task 1 Step 2. Add `mod overlay;` to `mac/src/main.rs`.

- [ ] **Step 2: Add a mac cursor helper in `commands.rs`**

```rust
/// Cursor position in native pixels via CoreGraphics, or None.
fn cursor_native_pos() -> Option<(f64, f64)> {
    use core_graphics::event::CGEvent;
    use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
    let src = CGEventSource::new(CGEventSourceStateID::CombinedSessionState).ok()?;
    let p = CGEvent::new(src).ok()?.location();
    Some((p.x, p.y))
}
```

If `core-graphics` is already a dependency (it is — used for `ui_tree`), no Cargo change needed. Verify: `grep -n core-graphics mac/Cargo.toml`.

- [ ] **Step 3: Wire into `handle_screenshot` and `handle_screenshot_region`**

Apply the SAME two insertions as Windows Task 2 Step 2 and Task 3 Step 1 (the `apply_overlays` blocks), at the equivalent point — immediately before the WebP encode — in `mac/src/commands.rs`. The variable names (`width`, `height`, `min_x`, `scale_x`, `crop_w`, etc.) mirror the Windows handlers; confirm by reading the mac handlers first and matching the locals.

- [ ] **Step 4: Verify (cannot build on this Windows machine)**

Inspect for parity with Windows; build via CI / release pipeline. Note in the commit message that mac is unbuilt locally.

- [ ] **Step 5: Commit**

```bash
git add mac/src/overlay.rs mac/src/main.rs mac/src/commands.rs
git commit -m "feat(mac): paint dot/cursor overlays in screenshot/screenshot_region"
```

---

## Task 7: Linux client — overlay helper + wire into both handlers

**Files:**
- Create: `linux/src/overlay.rs` (identical to Task 1)
- Modify: `linux/src/main.rs` (`mod overlay;`)
- Modify: `linux/src/commands.rs` (handlers + cursor helper)

- [ ] **Step 1: Copy the overlay module**

Create `linux/src/overlay.rs` with the exact contents from Task 1 (impl + tests). Add `mod overlay;` to `linux/src/main.rs`.

- [ ] **Step 2: Add a linux cursor helper**

Use the same query path the client already uses for pointer/window state (xdotool/X). If `xdotool` is shelled out elsewhere in `linux/src`, mirror it:

```rust
/// Cursor position in native pixels via `xdotool getmouselocation`, or None.
fn cursor_native_pos() -> Option<(f64, f64)> {
    let out = std::process::Command::new("xdotool")
        .args(["getmouselocation", "--shell"]).output().ok()?;
    let s = String::from_utf8_lossy(&out.stdout);
    let mut x = None; let mut y = None;
    for line in s.lines() {
        if let Some(v) = line.strip_prefix("X=") { x = v.trim().parse::<f64>().ok(); }
        if let Some(v) = line.strip_prefix("Y=") { y = v.trim().parse::<f64>().ok(); }
    }
    Some((x?, y?))
}
```

(If the client targets Wayland or has no xdotool, returning `None` simply skips the cursor — dots still work.)

- [ ] **Step 3: Wire into both handlers**

Same two `apply_overlays` insertions as Windows Tasks 2 & 3, matched to the linux handler locals.

- [ ] **Step 4: Verify (cannot build locally)**

Inspect for parity; build via CI.

- [ ] **Step 5: Commit**

```bash
git add linux/src/overlay.rs linux/src/main.rs linux/src/commands.rs
git commit -m "feat(linux): paint dot/cursor overlays in screenshot/screenshot_region"
```

---

## Task 8: Android — paint dots on the screenshot bitmap

**Files:**
- Modify: `android/app/src/main/java/.../ScreenMcpService.kt` (screenshot method)
- Modify: `android/app/src/main/java/.../WebSocketClient.kt` (pass `dots`/`dot_radius` params through; ignore `cursor`)

- [ ] **Step 1: Read the existing screenshot method**

Run: `grep -n "fun .*screenshot\|Bitmap\|WebP\|compress" android/app/src/main/java/**/ScreenMcpService.kt` to find where the final scaled `Bitmap` is produced before compress.

- [ ] **Step 2: Add an overlay helper**

In `ScreenMcpService.kt`:

```kotlin
private fun parseColor(s: String?): Int {
    if (s == null) return android.graphics.Color.RED
    if (s.startsWith("#") && s.length == 7) {
        return try { android.graphics.Color.parseColor(s) } catch (e: Exception) { android.graphics.Color.RED }
    }
    return when (s.lowercase()) {
        "red" -> android.graphics.Color.RED
        "lime", "green" -> android.graphics.Color.GREEN
        "blue" -> android.graphics.Color.BLUE
        "cyan" -> android.graphics.Color.CYAN
        "yellow" -> android.graphics.Color.YELLOW
        "magenta" -> android.graphics.Color.MAGENTA
        "white" -> android.graphics.Color.WHITE
        "black" -> android.graphics.Color.BLACK
        else -> android.graphics.Color.RED
    }
}

/** Draw dots (screenshot-space == bitmap pixels here) onto a mutable copy. */
private fun paintDots(src: Bitmap, dots: org.json.JSONArray?, radius: Int): Bitmap {
    if (dots == null || dots.length() == 0) return src
    val out = src.copy(Bitmap.Config.ARGB_8888, true)
    val canvas = android.graphics.Canvas(out)
    val r = radius.coerceIn(1, 100).toFloat()
    for (i in 0 until dots.length()) {
        val d = dots.getJSONObject(i)
        val x = d.getDouble("x").toFloat()
        val y = d.getDouble("y").toFloat()
        val fill = android.graphics.Paint().apply { isAntiAlias = true; color = parseColor(d.optString("color", null)) }
        val ringW = android.graphics.Paint().apply { isAntiAlias = true; style = android.graphics.Paint.Style.STROKE; strokeWidth = 1f; color = android.graphics.Color.WHITE }
        val ringB = android.graphics.Paint().apply { isAntiAlias = true; style = android.graphics.Paint.Style.STROKE; strokeWidth = 1f; color = android.graphics.Color.BLACK }
        canvas.drawCircle(x, y, r, fill)
        canvas.drawCircle(x, y, r + 1f, ringW)
        canvas.drawCircle(x, y, r + 2f, ringB)
    }
    return out
}
```

- [ ] **Step 3: Call it before compress**

Where the final scaled bitmap `bmp` is compressed to WebP, replace `bmp` with `paintDots(bmp, dots, dotRadius)`. Read `dots`/`dot_radius` from the command params in `WebSocketClient.kt`'s screenshot dispatch and pass them into the service method (extend its signature). `cursor` is ignored on Android.

- [ ] **Step 4: Build**

Note: Android cannot build on this machine. Verify by inspection; build via release pipeline (`cd android && ./gradlew assembleDebug` on a capable host).

- [ ] **Step 5: Commit**

```bash
git add android/app/src/main/java
git commit -m "feat(android): paint estimated-click dots in screenshot"
```

---

## Task 9: Python CLI — overlay via PIL + registry test

**Files:**
- Modify: `python-cli/screenmcp_cli.py` (`cmd_screenshot` ~line 170, `cmd_screenshot_region` ~line 179)
- Modify: `python-cli/tests/test_registry.py` (schema completeness)

- [ ] **Step 1: Add a PIL overlay helper**

Near the imaging helpers (around `_image_result`, line 155):

```python
from PIL import ImageDraw  # add alongside the existing `from PIL import Image`

_NAMED = {
    "red": (255, 0, 0), "lime": (0, 255, 0), "green": (0, 255, 0), "blue": (0, 0, 255),
    "cyan": (0, 255, 255), "yellow": (255, 255, 0), "magenta": (255, 0, 255),
    "orange": (255, 136, 0), "white": (255, 255, 255), "black": (0, 0, 0),
}

def _parse_color(s):
    if not s:
        return (255, 0, 0)
    if s.startswith("#") and len(s) == 7:
        try:
            return tuple(int(s[i:i+2], 16) for i in (1, 3, 5))
        except ValueError:
            return (255, 0, 0)
    return _NAMED.get(s.lower(), (255, 0, 0))

def _draw_overlays(img, params, map_xy):
    """img: PIL.Image (RGB/RGBA); map_xy: (x,y)->(px,py) or None to clip."""
    dots = (params or {}).get("dots")
    if not dots:
        return img
    radius = int((params or {}).get("dot_radius", 3))
    radius = max(1, min(100, radius))
    img = img.convert("RGBA")
    draw = ImageDraw.Draw(img)
    for d in dots:
        pt = map_xy(float(d["x"]), float(d["y"]))
        if pt is None:
            continue
        px, py = pt
        col = _parse_color(d.get("color"))
        draw.ellipse([px-radius, py-radius, px+radius, py+radius], fill=col)
        draw.ellipse([px-radius-1, py-radius-1, px+radius+1, py+radius+1], outline=(255,255,255))
        draw.ellipse([px-radius-2, py-radius-2, px+radius+2, py+radius+2], outline=(0,0,0))
    return img
```

(Cursor on the python-cli local capture is optional; if `pyautogui`/`mss` cursor position is readily available, mirror the desktop crosshair, else skip — out of scope to add a new dep.)

- [ ] **Step 2: Call it before encoding**

In `cmd_screenshot`, after the final scaled `img` is produced and before encoding to bytes, add `img = _draw_overlays(img, args, lambda x, y: (round(x), round(y)) if 0 <= x < img.width and 0 <= y < img.height else None)`. In `cmd_screenshot_region`, use the region-translating map (subtract `min_x/min_y`, multiply by output scale), matching the existing crop/scale math in that handler.

- [ ] **Step 3: Update the registry completeness test**

In `python-cli/tests/test_registry.py`, if it asserts the input schema of `screenshot`/`screenshot_region`, add `dots`, `cursor`, `dot_radius` to the expected set (or relax to allow optional extras). Update the `TOOLS` entry inputSchema in `screenmcp_cli.py` for both commands to include the three optional params.

- [ ] **Step 4: Run tests**

Run: `cd python-cli && python -m pytest tests/test_registry.py -v`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add python-cli/screenmcp_cli.py python-cli/tests/test_registry.py
git commit -m "feat(python-cli): paint estimated-click dots in screenshot/region"
```

---

## Task 10: SDKs — optional overlay params on existing methods

**Files:**
- Modify: `sdk/typescript/src/client.ts:258` (`screenshot`), `:511` (`screenshotRegion`)
- Modify: `sdk/python/src/screenmcp/client.py:273` (`screenshot`), `:609` (`screenshot_region`)
- Modify: `sdk/rust/src/client.rs:267` (`screenshot`), `:563` (`screenshot_region`)

- [ ] **Step 1: TypeScript — extend opts**

Add to the `screenshot` opts type and `screenshotRegion` opts type: `dots?: { x: number; y: number; color?: string }[]; cursor?: boolean; dotRadius?: number;` and forward them in the params object as `dots`, `cursor`, `dot_radius`.

- [ ] **Step 2: Python — extend signatures**

Add `dots: list | None = None, cursor: bool = False, dot_radius: int = 3` to both methods; include them in `params` only when set (don't send `dots` if None, to keep byte-compat).

- [ ] **Step 3: Rust — extend signatures**

Add optional params (e.g. `dots: Option<serde_json::Value>, cursor: bool, dot_radius: Option<u32>`) and merge into the command JSON. Keep existing call sites compiling (add new args or provide a builder/overload as fits the existing style — read surrounding methods first).

- [ ] **Step 4: Type-check each SDK**

Run: `cd sdk/typescript && npx tsc --noEmit`; `cd sdk/rust && cargo build`; `cd sdk/python && python -c "import screenmcp"`.
Expected: all clean.

- [ ] **Step 5: Commit**

```bash
git add sdk/
git commit -m "feat(sdk): optional dots/cursor/dot_radius on screenshot methods"
```

---

## Task 11: Playground UI

**Files:**
- Modify: `screenmcp-cloud/web/src/app/playground/page.tsx`

- [ ] **Step 1: Add optional inputs for screenshot + screenshot_region**

For both commands' parameter UI add: a textarea/JSON input for `dots` (e.g. `[{"x":100,"y":100,"color":"red"}]`), a `cursor` checkbox, and a `dot_radius` number (default 3). Thread them through `buildParams()` only when non-empty.

- [ ] **Step 2: Build / type-check**

Run: `cd screenmcp-cloud/web && npx tsc --noEmit` (or the project's lint/build).
Expected: clean.

- [ ] **Step 3: Commit**

```bash
git add screenmcp-cloud/web/src/app/playground/page.tsx
git commit -m "feat(playground): dot/cursor inputs for screenshot commands"
```

---

## Task 12: Fake device + SDK round-trip tests

**Files:**
- Modify: `fake-device/src/fake_device/commands.py` (ensure new params accepted)
- Modify: `fake-device/test_with_sdk.py`, `sdk/typescript/examples/cli/test_fake_device.ts`, `sdk/rust/examples/test_fake_device.rs`

- [ ] **Step 1: Confirm fake-device tolerates the params**

`screenshot` already returns a canned image. Verify `handle_command` does not reject unknown params; if it validates keys, add `dots`/`cursor`/`dot_radius` to the allowed set. No drawing needed (canned response).

- [ ] **Step 2: Add round-trip test blocks**

In each test file, add a call that passes `dots: [{x:10,y:10,color:"lime"}]` to `screenshot` and asserts a successful image response (the fake device echoes success). This verifies the param threads through MCP/SDK without error.

- [ ] **Step 3: Run the fake-device tests**

Run the existing fake-device test entrypoint (see `fake-device/` README / `testing.md`).
Expected: new blocks PASS.

- [ ] **Step 4: Commit**

```bash
git add fake-device/ sdk/typescript/examples/cli/test_fake_device.ts sdk/rust/examples/test_fake_device.rs
git commit -m "test: round-trip dots param through fake device + SDKs"
```

---

## Task 13: Documentation

**Files:**
- Modify: `docs/commands.md`, `docs/wire-protocol.md`, `docs/implementations.md`

- [ ] **Step 1: commands.md**

Under `screenshot` and `screenshot_region`, document the three optional params (`dots`, `cursor`, `dot_radius`), their types, defaults (`dot_radius`=3, `cursor`=false), coordinate space (screenshot space, same as `click`), and that `cursor` is desktop-only.

- [ ] **Step 2: wire-protocol.md**

Add a wire example:

```json
{"cmd":"screenshot","params":{"dots":[{"x":100,"y":100,"color":"lime"}],"cursor":true,"dot_radius":3}}
```

- [ ] **Step 3: implementations.md**

Add/annotate a row noting dot overlays supported on all clients; cursor overlay supported on Windows/Mac/Linux, not Android.

- [ ] **Step 4: Commit**

```bash
git add docs/commands.md docs/wire-protocol.md docs/implementations.md
git commit -m "docs: document dots/cursor/dot_radius overlay params"
```

---

## Final verification

- [ ] `cd windows && cargo test && cargo build` — Windows reference passes end to end.
- [ ] `cd mcp-server && npx tsc --noEmit` — server schema compiles.
- [ ] `cd python-cli && python -m pytest -q` — registry test passes.
- [ ] Manual: `screenshot` and `screenshot_region` with dots + cursor produce visibly correct overlays on Windows.
- [ ] Omitting overlay params yields unchanged output (backward compat).
- [ ] Mac/Linux/Android/cloud changes reviewed for parity; built via CI.
