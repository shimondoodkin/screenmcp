# Design: Dot + Cursor Overlays on `screenshot` / `screenshot_region`

**Date:** 2026-06-03
**Status:** Approved (brainstorming)

## Purpose

Give a vision model a way to verify an estimated click position before committing to
a `click`. The model supplies the coordinates it *thinks* it should click; the capture
command paints a colored dot at exactly that point and returns the annotated image. The
model looks at the dot, confirms it is on target (or adjusts), and only then clicks.

Secondary capability: optionally render the real mouse cursor into the screenshot, since
the OS screen capture (`screenshots` crate / Win32) does not include the cursor.

## Approach

Extend the two existing capture commands — `screenshot` and `screenshot_region` — rather
than add a new command. Rationale:

- The dot lands in the **exact coordinate space the model already uses for `click`**
  (screenshot space), so there is no separate coordinate convention to reason about.
- Reuses the existing capture → scale → encode pipeline instead of duplicating it across
  every client.
- The cursor is just another overlay element on the same image.

A separate `paint_dot` command was rejected because it would re-implement capture/scale
logic in every client for no added clarity.

## New Optional Parameters (both commands)

| Param        | Type                              | Default | Notes |
|--------------|-----------------------------------|---------|-------|
| `dots`       | array of `{ x, y, color? }`       | omitted | Markers in **screenshot-space** coords (same space as `click`). One element = the common "verify my estimate" case; an array lets you compare several candidate points in one shot. |
| `cursor`     | bool                              | `false` | Draw the real mouse cursor position as a distinct marker. Ignored on Android (no cursor). |
| `dot_radius` | int (screenshot-space px)         | `3`     | Filled-circle radius for every dot in this call. |

When `dots` is omitted/empty and `cursor` is false, output is **byte-identical** to today
(no overlay code runs). This preserves backward compatibility and existing tests.

### Color

- Accepts a named color (`red`, `lime`, `cyan`, `yellow`, `magenta`, `white`, `black`, …)
  or `#rrggbb`.
- Default `red`.
- Each dot is drawn as a **filled circle of the chosen color plus a thin white-then-black
  outline ring**, so it stays visible on any background.
- The cursor marker uses a **distinct shape (crosshair)** so it is never confused with an
  estimate dot, regardless of dot color.

## Coordinate Handling

All input coordinates (`dots[].x/y`) are in screenshot space — the same scaled space the
model sees and that `click` consumes. Overlays are drawn **after** the final resize/crop,
so screenshot-space pixels map 1:1 onto the output image.

- **`screenshot`**: image is captured native then resized down to `max_width`/`max_height`.
  Draw each dot directly at `(x, y)` on the final resized image.
- **`screenshot_region`**: the crop is taken in native pixels from the region (itself given
  in screenshot space) then optionally scaled to `output_max_*`. Map each dot:
  - `x' = (x - region.min_x) * output_scale_x`
  - `y' = (y - region.min_y) * output_scale_y`
  - Skip (clip) any dot whose mapped center falls outside the cropped output image.
- **Cursor**: query the OS cursor position in native pixels, convert to screenshot space
  using the same scale factor the command already computes, then (for region) apply the
  region translation above. Skip if outside the captured area.

## Drawing Helper

A small, dependency-free routine per client:

1. Filled circle (Bresenham/scanline fill) of radius `dot_radius` in the dot color.
2. A 1px white ring then a 1px black ring just outside the fill for contrast.
3. Cursor: two short perpendicular strokes (crosshair) centered on the cursor point, with
   the same white/black contrast edges.

No new third-party dependencies:
- **Rust clients** (`windows`, `mac`, `linux`): manual pixel writes on the `RgbaImage`
  already in hand (the `image` crate is already a dependency). No `imageproc`.
- **Android**: `android.graphics.Canvas.drawCircle` / `drawLine` on the bitmap before encode.
- **Python CLI**: `PIL.ImageDraw` (Pillow already used for imaging there).

## Files to Change

Follows [adding-new-command.md](../../adding-new-command.md), but lighter than a brand-new
command because `screenshot` / `screenshot_region` already exist end-to-end. The work is
"add optional params + overlay drawing" rather than "wire a new command name".

### Device clients (capture + draw)
- `windows/src/commands.rs` — parse `dots`/`cursor`/`dot_radius` in `handle_screenshot` and
  `handle_screenshot_region`; add `draw_overlays()` helper; query cursor via Win32
  `GetCursorPos`.
- `mac/src/commands.rs` — same; cursor via CoreGraphics.
- `linux/src/commands.rs` — same; cursor via X (xdotool/query) where available, else skip.
- `android/app/.../ScreenMcpService.kt` — draw dots on the screenshot bitmap (cursor ignored).

### Standalone / server
- `python-cli/screenmcp_cli.py` — overlay params on the local `screenshot`/`screenshot_region`
  handlers; update `python-cli/tests/test_registry.py` if schema completeness is asserted.
- `mcp-server/src/mcp.ts` — add `dots`, `cursor`, `dot_radius` (zod) to the existing
  `screenshot` and `screenshot_region` tool input schemas + descriptions.
- `screenmcp-cloud/mcp-server/src/tools.rs` — add the same params to the existing two
  `ToolDef` JSON schemas.

### SDKs (optional params on existing methods)
- `sdk/typescript/src/client.ts`, `sdk/python/src/screenmcp/client.py`,
  `sdk/rust/src/client.rs` — extend the existing `screenshot`/`screenshot_region` methods to
  accept the new optional params.

### UI / docs / tests
- `screenmcp-cloud/web/src/app/playground/page.tsx` — optional dot/cursor inputs for the two
  capture commands.
- `fake-device/src/fake_device/commands.py` — screenshot is already handled; confirm the new
  params are accepted (ignored) without error.
- Docs: `commands.md` (param spec), `wire-protocol.md` (example with `dots`),
  `implementations.md` (note cursor unsupported on Android).
- SDK tests (`fake-device/test_with_sdk.py`, `sdk/typescript/examples/cli/test_fake_device.ts`,
  `sdk/rust/examples/test_fake_device.rs`) — add a call passing `dots` to verify the param
  round-trips.

## Testing

- **Unit (per client where practical):** feed a known image + a dot at a known screenshot-space
  point, assert the center pixel is the dot color and an off-dot pixel is unchanged.
- **Region mapping:** dot inside region appears at translated position; dot outside region is
  clipped (no panic, no out-of-bounds write).
- **Backward compat:** omitting overlay params yields byte-identical output to pre-change.
- **Windows is the only client buildable on this machine** — verify there end to end; Mac/Linux/
  Android changes are reviewed for parity and built via the release/CI pipeline.

## Out of Scope (YAGNI)

- Text labels on dots (would pull a font dependency) — distinguish multiple dots by color.
- Lossy/quality control of the overlay — overlays inherit the existing lossless WebP encode.
- Drawing on `screenshot_window` — can be added later with the same helper if needed.
