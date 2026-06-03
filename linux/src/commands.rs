use base64::Engine;
use enigo::{
    Button, Coordinate,
    Direction::{Click, Press, Release},
    Enigo, Key, Keyboard, Mouse, Settings,
};
use image::codecs::webp::WebPEncoder;
use image::ImageEncoder;
use nokhwa::pixel_format::RgbFormat;
use nokhwa::utils::{ApiBackend, CameraIndex, RequestedFormat, RequestedFormatType};
use nokhwa::Camera;
use serde_json::{json, Value};
use std::io::Cursor;
use std::thread;
use std::time::Duration;

use crate::config::Config;

/// Execute a command and return the JSON response value.
/// The response follows the phone protocol: {id, status, result?, error?}
pub fn execute_command(
    id: i64,
    cmd: &str,
    params: Option<&Value>,
    config: &Config,
) -> Value {
    let result = match cmd {
        "screenshot" => handle_screenshot(params, config),
        "click" => handle_click(params, config),
        "long_click" => handle_long_click(params, config),
        "drag" => handle_drag(params, config),
        "scroll" => handle_scroll(params, config),
        "type" => handle_type(params),
        "get_text" => handle_get_text(),
        "select_all" => handle_select_all(),
        "copy" => handle_copy(params),
        "paste" => handle_paste(params),
        "get_clipboard" => handle_get_clipboard(),
        "set_clipboard" => handle_set_clipboard(params),
        "back" => handle_back(),
        "home" => handle_home(),
        "recents" => handle_recents(),
        "ui_tree" => handle_ui_tree(params, config),
        "camera" => handle_camera(params),
        "list_cameras" => handle_list_cameras(),
        "right_click" => handle_right_click(params, config),
        "middle_click" => handle_middle_click(params, config),
        "mouse_scroll" => handle_mouse_scroll(params, config),
        "play_audio" => handle_play_audio(params),
        "hold_key" => handle_hold_key(params),
        "release_key" => handle_release_key(params),
        "press_key" => handle_press_key(params),
        "mouse_move" => handle_mouse_move(params, config),
        "double_click" => handle_double_click(params, config),
        "hotkey" => handle_hotkey(params),
        "get_screen_size" => handle_get_screen_size(params, config),
        "list_windows" => handle_list_windows(params, config),
        "focus_window" => handle_focus_window(params),
        "active_window" => handle_active_window(params, config),
        "screenshot_window" => handle_screenshot_window(params, config),
        "screenshot_region" => handle_screenshot_region(params, config),
        "is_elevated" => handle_is_elevated(),
        "elevate" => handle_elevate(),
        _ => {
            return json!({
                "id": id,
                "status": "error",
                "error": format!("unknown command: {cmd}")
            });
        }
    };

    match result {
        Ok(result_value) => json!({
            "id": id,
            "status": "ok",
            "result": result_value
        }),
        Err(e) => json!({
            "id": id,
            "status": "error",
            "error": e
        }),
    }
}

/// Current cursor position in native pixels via `xdotool getmouselocation`, or None.
/// On Wayland or without xdotool this returns None and the cursor overlay is skipped.
fn cursor_native_pos() -> Option<(f64, f64)> {
    let out = std::process::Command::new("xdotool")
        .args(["getmouselocation", "--shell"])
        .output()
        .ok()?;
    let s = String::from_utf8_lossy(&out.stdout);
    let mut x = None;
    let mut y = None;
    for line in s.lines() {
        if let Some(v) = line.strip_prefix("X=") { x = v.trim().parse::<f64>().ok(); }
        if let Some(v) = line.strip_prefix("Y=") { y = v.trim().parse::<f64>().ok(); }
    }
    Some((x?, y?))
}

fn handle_screenshot(
    params: Option<&Value>,
    config: &Config,
) -> Result<Value, String> {
    let screens = screenshots::Screen::all().map_err(|e| format!("failed to list screens: {e}"))?;
    let screen = screens
        .first()
        .ok_or_else(|| "no screens found".to_string())?;

    let capture = screen
        .capture()
        .map_err(|e| format!("screenshot failed: {e}"))?;

    let width = capture.width();
    let height = capture.height();
    let raw_pixels = capture.into_raw();
    let img = image::RgbaImage::from_raw(width, height, raw_pixels)
        .ok_or_else(|| "failed to create image from capture".to_string())?;

    // Determine max dimensions: explicit params > model default > config > legacy.
    let (mw_f, mh_f) = resolve_scale_dims(params, config);
    let max_w = if mw_f > 0.0 { Some(mw_f as u32) } else { None };
    let max_h = if mh_f > 0.0 { Some(mh_f as u32) } else { None };

    let img = if let (Some(mw), Some(mh)) = (max_w, max_h) {
        if width > mw || height > mh {
            image::DynamicImage::ImageRgba8(img)
                .resize(mw, mh, image::imageops::FilterType::Triangle)
                .to_rgba8()
        } else {
            img
        }
    } else if let Some(mw) = max_w {
        if width > mw {
            let ratio = mw as f64 / width as f64;
            let new_h = (height as f64 * ratio) as u32;
            image::DynamicImage::ImageRgba8(img)
                .resize_exact(mw, new_h, image::imageops::FilterType::Triangle)
                .to_rgba8()
        } else {
            img
        }
    } else if let Some(mh) = max_h {
        if height > mh {
            let ratio = mh as f64 / height as f64;
            let new_w = (width as f64 * ratio) as u32;
            image::DynamicImage::ImageRgba8(img)
                .resize_exact(new_w, mh, image::imageops::FilterType::Triangle)
                .to_rgba8()
        } else {
            img
        }
    } else {
        img
    };

    // Paint optional dot/cursor overlays. Output image == screenshot space for
    // full-screen capture, so dots map at identity (clipped to image bounds).
    let mut img = img;
    let (bw, bh) = (img.width(), img.height());
    let cursor_xy = cursor_native_pos()
        .map(|(nx, ny)| (nx * bw as f64 / width as f64, ny * bh as f64 / height as f64));
    crate::overlay::apply_overlays(&mut img, params, cursor_xy, move |x, y| {
        let (px, py) = (x.round() as i64, y.round() as i64);
        if px >= 0 && py >= 0 && (px as u32) < bw && (py as u32) < bh {
            Some((px, py))
        } else {
            None
        }
    });

    // Encode as WebP (smaller than PNG, matches Android client format)
    let quality = params
        .and_then(|p| p.get("quality"))
        .and_then(|v| v.as_u64())
        .unwrap_or(100) as u8;
    let mut buf = Cursor::new(Vec::new());
    // image crate's WebP encoder is lossless-only; quality param is accepted
    // but lossy encoding would require libwebp. Lossless WebP is still smaller
    // than PNG for screenshots and the format is consistent across all clients.
    let _ = quality;
    WebPEncoder::new_lossless(&mut buf)
        .write_image(
            img.as_raw(),
            img.width(),
            img.height(),
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|e| format!("WebP encode failed: {e}"))?;

    let b64 = base64::engine::general_purpose::STANDARD.encode(buf.into_inner());

    Ok(json!({ "image": b64 }))
}

fn handle_screenshot_region(params: Option<&Value>, config: &Config) -> Result<Value, String> {
    let p = params.ok_or("missing params")?;
    let min_x = p.get("min_x").and_then(|v| v.as_f64()).ok_or("missing min_x")?;
    let min_y = p.get("min_y").and_then(|v| v.as_f64()).ok_or("missing min_y")?;
    let max_x = p.get("max_x").and_then(|v| v.as_f64()).ok_or("missing max_x")?;
    let max_y = p.get("max_y").and_then(|v| v.as_f64()).ok_or("missing max_y")?;

    // Take full native screenshot
    let screens = screenshots::Screen::all().map_err(|e| format!("failed to list screens: {e}"))?;
    let screen = screens.first().ok_or("no screens found")?;
    let capture = screen.capture().map_err(|e| format!("screenshot failed: {e}"))?;
    let full_w = capture.width();
    let full_h = capture.height();
    let raw_pixels = capture.into_raw();
    let full_img = image::RgbaImage::from_raw(full_w, full_h, raw_pixels)
        .ok_or("failed to create image from capture")?;

    // Get scale factor from screenshot space to actual pixels
    let (mw, mh) = resolve_scale_dims(Some(p), config);

    let (scale_x, scale_y) = if mw > 0.0 && mh > 0.0 {
        (full_w as f64 / mw, full_h as f64 / mh)
    } else if mw > 0.0 {
        let s = full_w as f64 / mw; (s, s)
    } else if mh > 0.0 {
        let s = full_h as f64 / mh; (s, s)
    } else {
        (1.0, 1.0)
    };

    // Translate to actual pixel coordinates
    let px_min_x = ((min_x * scale_x) as u32).min(full_w.saturating_sub(1));
    let px_min_y = ((min_y * scale_y) as u32).min(full_h.saturating_sub(1));
    let px_max_x = ((max_x * scale_x) as u32).min(full_w);
    let px_max_y = ((max_y * scale_y) as u32).min(full_h);

    if px_max_x <= px_min_x || px_max_y <= px_min_y {
        return Err("region has zero or negative size".to_string());
    }

    let crop_w = px_max_x - px_min_x;
    let crop_h = px_max_y - px_min_y;

    // Crop
    let cropped = image::DynamicImage::ImageRgba8(full_img)
        .crop_imm(px_min_x, px_min_y, crop_w, crop_h)
        .to_rgba8();

    // Only scale DOWN if crop exceeds output max (never scale up)
    let out_max_w = p.get("output_max_width").and_then(|v| v.as_u64()).map(|v| v as u32);
    let out_max_h = p.get("output_max_height").and_then(|v| v.as_u64()).map(|v| v as u32);
    let img = if let (Some(omw), Some(omh)) = (out_max_w, out_max_h) {
        if cropped.width() > omw || cropped.height() > omh {
            image::DynamicImage::ImageRgba8(cropped)
                .resize(omw, omh, image::imageops::FilterType::Triangle)
                .to_rgba8()
        } else { cropped }
    } else if let Some(omw) = out_max_w {
        if cropped.width() > omw {
            let r = omw as f64 / cropped.width() as f64;
            let new_h = (cropped.height() as f64 * r) as u32;
            image::DynamicImage::ImageRgba8(cropped)
                .resize_exact(omw, new_h, image::imageops::FilterType::Triangle)
                .to_rgba8()
        } else { cropped }
    } else if let Some(omh) = out_max_h {
        if cropped.height() > omh {
            let r = omh as f64 / cropped.height() as f64;
            let new_w = (cropped.width() as f64 * r) as u32;
            image::DynamicImage::ImageRgba8(cropped)
                .resize_exact(new_w, omh, image::imageops::FilterType::Triangle)
                .to_rgba8()
        } else { cropped }
    } else { cropped };

    // Paint optional dot/cursor overlays. Map screenshot-space coords into the
    // cropped+scaled output: subtract region origin, apply output-per-screenshot scale.
    let mut img = img;
    let (iw, ih) = (img.width(), img.height());
    let out_per_ss_x = (iw as f64 / crop_w as f64) * scale_x;
    let out_per_ss_y = (ih as f64 / crop_h as f64) * scale_y;
    let cursor_xy = cursor_native_pos().map(|(nx, ny)| (nx / scale_x, ny / scale_y));
    crate::overlay::apply_overlays(&mut img, Some(p), cursor_xy, move |x, y| {
        let px = ((x - min_x) * out_per_ss_x).round() as i64;
        let py = ((y - min_y) * out_per_ss_y).round() as i64;
        if px >= 0 && py >= 0 && (px as u32) < iw && (py as u32) < ih {
            Some((px, py))
        } else {
            None
        }
    });

    let quality = p.get("quality").and_then(|v| v.as_u64()).unwrap_or(100) as u8;
    let _ = quality;
    let mut buf = Cursor::new(Vec::new());
    WebPEncoder::new_lossless(&mut buf)
        .write_image(img.as_raw(), img.width(), img.height(), image::ExtendedColorType::Rgba8)
        .map_err(|e| format!("WebP encode failed: {e}"))?;

    let b64 = base64::engine::general_purpose::STANDARD.encode(buf.into_inner());
    Ok(json!({ "image": b64 }))
}

fn handle_list_cameras() -> Result<Value, String> {
    let cameras = nokhwa::query(ApiBackend::Auto).unwrap_or_else(|_| vec![]);
    let list: Vec<Value> = cameras
        .iter()
        .map(|cam| {
            let id = match cam.index() {
                CameraIndex::Index(i) => i.to_string(),
                CameraIndex::String(s) => s.clone(),
            };
            json!({ "id": id, "facing": "external" })
        })
        .collect();
    Ok(json!({ "cameras": list }))
}

fn handle_camera(params: Option<&Value>) -> Result<Value, String> {
    let camera_id = params
        .and_then(|p| p.get("camera"))
        .and_then(|v| v.as_str())
        .unwrap_or("0");
    let quality = params
        .and_then(|p| p.get("quality"))
        .and_then(|v| v.as_u64())
        .unwrap_or(80) as u8;
    let max_w = params
        .and_then(|p| p.get("max_width"))
        .and_then(|v| v.as_u64())
        .map(|v| v as u32);
    let max_h = params
        .and_then(|p| p.get("max_height"))
        .and_then(|v| v.as_u64())
        .map(|v| v as u32);

    let idx: usize = camera_id
        .parse()
        .map_err(|_| format!("invalid camera id: {camera_id}"))?;
    let index = CameraIndex::Index(idx as u32);

    let requested =
        RequestedFormat::new::<RgbFormat>(RequestedFormatType::AbsoluteHighestFrameRate);
    let mut camera = Camera::new(index, requested)
        .map_err(|e| format!("failed to open camera {camera_id}: {e}"))?;

    camera
        .open_stream()
        .map_err(|e| format!("failed to start camera stream: {e}"))?;
    let frame = camera
        .frame()
        .map_err(|e| format!("failed to capture frame: {e}"))?;
    let _ = camera.stop_stream();

    let rgb_img = frame
        .decode_image::<RgbFormat>()
        .map_err(|e| format!("failed to decode frame: {e}"))?;

    let img = image::DynamicImage::ImageRgb8(rgb_img);

    // Apply max dimensions (same pattern as handle_screenshot)
    let width = img.width();
    let height = img.height();
    let img = if let (Some(mw), Some(mh)) = (max_w, max_h) {
        if width > mw || height > mh {
            img.resize(mw, mh, image::imageops::FilterType::Triangle)
        } else {
            img
        }
    } else if let Some(mw) = max_w {
        if width > mw {
            let ratio = mw as f64 / width as f64;
            let new_h = (height as f64 * ratio) as u32;
            img.resize_exact(mw, new_h, image::imageops::FilterType::Triangle)
        } else {
            img
        }
    } else if let Some(mh) = max_h {
        if height > mh {
            let ratio = mh as f64 / height as f64;
            let new_w = (width as f64 * ratio) as u32;
            img.resize_exact(new_w, mh, image::imageops::FilterType::Triangle)
        } else {
            img
        }
    } else {
        img
    };

    let rgba = img.to_rgba8();
    let mut buf = Cursor::new(Vec::new());
    let _ = quality; // image crate's WebP encoder is lossless-only
    WebPEncoder::new_lossless(&mut buf)
        .write_image(
            rgba.as_raw(),
            rgba.width(),
            rgba.height(),
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|e| format!("WebP encode failed: {e}"))?;

    let b64 = base64::engine::general_purpose::STANDARD.encode(buf.into_inner());
    Ok(json!({ "image": b64 }))
}

fn get_screen_dimensions() -> Result<(u32, u32), String> {
    let screens = screenshots::Screen::all().map_err(|e| format!("failed to list screens: {e}"))?;
    let screen = screens.first().ok_or("no screens found")?;
    Ok((screen.display_info.width, screen.display_info.height))
}

/// Default screenshot dimensions for coordinate scaling (landscape).
const DEFAULT_SCALE_WIDTH: f64 = 1456.0;
const DEFAULT_SCALE_HEIGHT: f64 = 819.0;

/// Effective (max_width, max_height) for scaling. Precedence:
/// explicit params > model-based provider default > config > legacy constant.
/// Returns f64 with the existing "<= 0 disables" convention so both the screenshot
/// output sizing and the coordinate scaling resolve to the SAME dimensions.
fn resolve_scale_dims(params: Option<&Value>, config: &Config) -> (f64, f64) {
    let pw = params.and_then(|p| p.get("max_width")).and_then(|v| v.as_f64());
    let ph = params.and_then(|p| p.get("max_height")).and_then(|v| v.as_f64());
    if pw.is_some() || ph.is_some() {
        return (
            pw.or(config.max_screenshot_width.map(|v| v as f64)).unwrap_or(DEFAULT_SCALE_WIDTH),
            ph.or(config.max_screenshot_height.map(|v| v as f64)).unwrap_or(DEFAULT_SCALE_HEIGHT),
        );
    }
    if let Some(model) = params.and_then(|p| p.get("model")).and_then(|v| v.as_str()) {
        if let Ok((sw, sh)) = get_screen_dimensions() {
            if let Some((mw, mh)) = crate::provider_sizing::provider_default_size(model, sw, sh) {
                return (mw as f64, mh as f64);
            }
        }
    }
    (
        config.max_screenshot_width.map(|v| v as f64).unwrap_or(DEFAULT_SCALE_WIDTH),
        config.max_screenshot_height.map(|v| v as f64).unwrap_or(DEFAULT_SCALE_HEIGHT),
    )
}

fn scale_xy(x: f64, y: f64, params: Option<&Value>, config: &Config) -> Result<(i32, i32), String> {
    let (mw, mh) = resolve_scale_dims(params, config);

    if mw > 0.0 || mh > 0.0 {
        let (sw, sh) = get_screen_dimensions()?;
        let (sw, sh) = (sw as f64, sh as f64);
        let (scale_w, scale_h) = match (mw > 0.0, mh > 0.0) {
            (true, true) => (sw / mw, sh / mh),
            (true, false) => { let s = sw / mw; (s, s) }
            (false, true) => { let s = sh / mh; (s, s) }
            _ => (1.0, 1.0),
        };
        Ok(((x * scale_w) as i32, (y * scale_h) as i32))
    } else {
        Ok((x as i32, y as i32))
    }
}

fn get_xy(params: Option<&Value>, config: &Config) -> Result<(i32, i32), String> {
    let p = params.ok_or("missing params")?;
    let x = p.get("x").and_then(|v| v.as_f64()).ok_or("missing x")?;
    let y = p.get("y").and_then(|v| v.as_f64()).ok_or("missing y")?;
    scale_xy(x, y, params, config)
}

fn new_enigo() -> Result<Enigo, String> {
    Enigo::new(&Settings::default()).map_err(|e| format!("failed to init enigo: {e}"))
}

fn handle_click(params: Option<&Value>, config: &Config) -> Result<Value, String> {
    let (x, y) = get_xy(params, config)?;
    let mut enigo = new_enigo()?;
    enigo
        .move_mouse(x, y, Coordinate::Abs)
        .map_err(|e| format!("move_mouse failed: {e}"))?;
    enigo
        .button(Button::Left, Click)
        .map_err(|e| format!("click failed: {e}"))?;
    Ok(json!({}))
}

fn handle_long_click(params: Option<&Value>, config: &Config) -> Result<Value, String> {
    let (x, y) = get_xy(params, config)?;
    let duration_ms = params
        .and_then(|p| p.get("duration"))
        .and_then(|v| v.as_u64())
        .unwrap_or(1000);

    let mut enigo = new_enigo()?;
    enigo
        .move_mouse(x, y, Coordinate::Abs)
        .map_err(|e| format!("move_mouse failed: {e}"))?;
    enigo
        .button(Button::Left, Press)
        .map_err(|e| format!("mouse down failed: {e}"))?;
    thread::sleep(Duration::from_millis(duration_ms));
    enigo
        .button(Button::Left, Release)
        .map_err(|e| format!("mouse up failed: {e}"))?;
    Ok(json!({}))
}

fn handle_drag(params: Option<&Value>, config: &Config) -> Result<Value, String> {
    let p = params.ok_or("missing params")?;
    let (start_x, start_y) = scale_xy(
        p.get("startX").and_then(|v| v.as_f64()).ok_or("missing startX")?,
        p.get("startY").and_then(|v| v.as_f64()).ok_or("missing startY")?,
        params,
        config,
    )?;
    let (end_x, end_y) = scale_xy(
        p.get("endX").and_then(|v| v.as_f64()).ok_or("missing endX")?,
        p.get("endY").and_then(|v| v.as_f64()).ok_or("missing endY")?,
        params,
        config,
    )?;
    let duration_ms = p.get("duration").and_then(|v| v.as_u64()).unwrap_or(300);

    let mut enigo = new_enigo()?;
    enigo
        .move_mouse(start_x, start_y, Coordinate::Abs)
        .map_err(|e| format!("move start failed: {e}"))?;
    enigo
        .button(Button::Left, Press)
        .map_err(|e| format!("mouse down failed: {e}"))?;

    // Interpolate movement over duration
    let steps = 20u32;
    let step_delay = Duration::from_millis(duration_ms / steps as u64);
    for i in 1..=steps {
        let t = i as f64 / steps as f64;
        let cx = start_x as f64 + (end_x - start_x) as f64 * t;
        let cy = start_y as f64 + (end_y - start_y) as f64 * t;
        enigo
            .move_mouse(cx as i32, cy as i32, Coordinate::Abs)
            .map_err(|e| format!("move step failed: {e}"))?;
        thread::sleep(step_delay);
    }

    enigo
        .button(Button::Left, Release)
        .map_err(|e| format!("mouse up failed: {e}"))?;
    Ok(json!({}))
}

fn handle_scroll(params: Option<&Value>, config: &Config) -> Result<Value, String> {
    let p = params.ok_or("missing params")?;

    // Support both direction-based (Android style) and dx/dy based scroll
    let mut enigo = new_enigo()?;

    // If x,y provided, move mouse there first (with optional scaling)
    if let (Some(x), Some(y)) = (
        p.get("x").and_then(|v| v.as_f64()),
        p.get("y").and_then(|v| v.as_f64()),
    ) {
        let (sx, sy) = scale_xy(x, y, params, config)?;
        enigo
            .move_mouse(sx, sy, Coordinate::Abs)
            .map_err(|e| format!("move_mouse failed: {e}"))?;
    }

    if let Some(direction) = p.get("direction").and_then(|v| v.as_str()) {
        let amount = p.get("amount").and_then(|v| v.as_i64()).unwrap_or(3) as i32;
        match direction {
            "up" => enigo
                .scroll(amount, enigo::Axis::Vertical)
                .map_err(|e| format!("scroll failed: {e}"))?,
            "down" => enigo
                .scroll(-amount, enigo::Axis::Vertical)
                .map_err(|e| format!("scroll failed: {e}"))?,
            "left" => enigo
                .scroll(-amount, enigo::Axis::Horizontal)
                .map_err(|e| format!("scroll failed: {e}"))?,
            "right" => enigo
                .scroll(amount, enigo::Axis::Horizontal)
                .map_err(|e| format!("scroll failed: {e}"))?,
            _ => return Err(format!("unknown scroll direction: {direction}")),
        }
    } else {
        // dx/dy based
        let dy = p.get("dy").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
        let dx = p.get("dx").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
        if dy != 0 {
            enigo
                .scroll(-dy, enigo::Axis::Vertical)
                .map_err(|e| format!("scroll failed: {e}"))?;
        }
        if dx != 0 {
            enigo
                .scroll(dx, enigo::Axis::Horizontal)
                .map_err(|e| format!("scroll failed: {e}"))?;
        }
    }

    Ok(json!({}))
}

fn handle_type(params: Option<&Value>) -> Result<Value, String> {
    let p = params.ok_or("missing params")?;
    let text = p
        .get("text")
        .and_then(|v| v.as_str())
        .ok_or("missing text")?;
    let mut enigo = new_enigo()?;
    enigo
        .text(text)
        .map_err(|e| format!("type failed: {e}"))?;
    Ok(json!({}))
}

fn handle_get_text() -> Result<Value, String> {
    let mut clipboard =
        arboard::Clipboard::new().map_err(|e| format!("clipboard init failed: {e}"))?;
    let text = clipboard
        .get_text()
        .map_err(|e| format!("get clipboard failed: {e}"))?;
    Ok(json!({ "text": text }))
}

fn handle_select_all() -> Result<Value, String> {
    let mut enigo = new_enigo()?;
    enigo.key(Key::Control, Press).map_err(|e| format!("{e}"))?;
    enigo.key(Key::Unicode('a'), Click).map_err(|e| format!("{e}"))?;
    enigo.key(Key::Control, Release).map_err(|e| format!("{e}"))?;
    Ok(json!({}))
}

fn handle_copy(params: Option<&Value>) -> Result<Value, String> {
    let mut enigo = new_enigo()?;
    enigo.key(Key::Control, Press).map_err(|e| format!("{e}"))?;
    enigo.key(Key::Unicode('c'), Click).map_err(|e| format!("{e}"))?;
    enigo.key(Key::Control, Release).map_err(|e| format!("{e}"))?;

    let return_text = params
        .and_then(|p| p.get("return_text"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if return_text {
        thread::sleep(Duration::from_millis(50));
        let mut clipboard = arboard::Clipboard::new().map_err(|e| format!("clipboard init failed: {e}"))?;
        let text = clipboard.get_text().unwrap_or_default();
        Ok(json!({ "text": text }))
    } else {
        Ok(json!({}))
    }
}

fn handle_paste(params: Option<&Value>) -> Result<Value, String> {
    if let Some(text) = params.and_then(|p| p.get("text")).and_then(|v| v.as_str()) {
        let mut clipboard = arboard::Clipboard::new().map_err(|e| format!("clipboard init failed: {e}"))?;
        clipboard.set_text(text).map_err(|e| format!("set clipboard failed: {e}"))?;
    }

    let mut enigo = new_enigo()?;
    enigo.key(Key::Control, Press).map_err(|e| format!("{e}"))?;
    enigo.key(Key::Unicode('v'), Click).map_err(|e| format!("{e}"))?;
    enigo.key(Key::Control, Release).map_err(|e| format!("{e}"))?;
    Ok(json!({}))
}

fn handle_get_clipboard() -> Result<Value, String> {
    let mut clipboard = arboard::Clipboard::new().map_err(|e| format!("clipboard init failed: {e}"))?;
    let text = clipboard.get_text().unwrap_or_default();
    Ok(json!({ "text": text }))
}

fn handle_set_clipboard(params: Option<&Value>) -> Result<Value, String> {
    let p = params.ok_or("missing params")?;
    let text = p.get("text").and_then(|v| v.as_str()).ok_or("missing text")?;
    let mut clipboard = arboard::Clipboard::new().map_err(|e| format!("clipboard init failed: {e}"))?;
    clipboard.set_text(text).map_err(|e| format!("set clipboard failed: {e}"))?;
    Ok(json!({}))
}

fn handle_back() -> Result<Value, String> {
    let mut enigo = new_enigo()?;
    // Alt+Left arrow (browser back / general back)
    enigo.key(Key::Alt, Press).map_err(|e| format!("{e}"))?;
    enigo
        .key(Key::LeftArrow, Click)
        .map_err(|e| format!("{e}"))?;
    enigo.key(Key::Alt, Release).map_err(|e| format!("{e}"))?;
    Ok(json!({}))
}

fn handle_home() -> Result<Value, String> {
    let mut enigo = new_enigo()?;
    // Super key to show activities/desktop (works on GNOME, KDE, etc.)
    enigo.key(Key::Meta, Click).map_err(|e| format!("{e}"))?;
    Ok(json!({}))
}

fn handle_recents() -> Result<Value, String> {
    let mut enigo = new_enigo()?;
    // Alt+Tab to show recent windows
    enigo.key(Key::Alt, Press).map_err(|e| format!("{e}"))?;
    enigo.key(Key::Tab, Click).map_err(|e| format!("{e}"))?;
    enigo.key(Key::Alt, Release).map_err(|e| format!("{e}"))?;
    Ok(json!({}))
}

fn handle_right_click(params: Option<&Value>, config: &Config) -> Result<Value, String> {
    let (x, y) = get_xy(params, config)?;
    let mut enigo = new_enigo()?;
    enigo
        .move_mouse(x, y, Coordinate::Abs)
        .map_err(|e| format!("move_mouse failed: {e}"))?;
    enigo
        .button(Button::Right, Click)
        .map_err(|e| format!("right click failed: {e}"))?;
    Ok(json!({}))
}

fn handle_middle_click(params: Option<&Value>, config: &Config) -> Result<Value, String> {
    let (x, y) = get_xy(params, config)?;
    let mut enigo = new_enigo()?;
    enigo
        .move_mouse(x, y, Coordinate::Abs)
        .map_err(|e| format!("move_mouse failed: {e}"))?;
    enigo
        .button(Button::Middle, Click)
        .map_err(|e| format!("middle click failed: {e}"))?;
    Ok(json!({}))
}

fn handle_mouse_scroll(params: Option<&Value>, config: &Config) -> Result<Value, String> {
    handle_scroll(params, config)
}

fn parse_key(key_name: &str) -> Result<Key, String> {
    match key_name.to_lowercase().as_str() {
        "shift" => Ok(Key::Shift),
        "ctrl" | "control" => Ok(Key::Control),
        "alt" => Ok(Key::Alt),
        "meta" | "cmd" | "win" | "command" | "super" => Ok(Key::Meta),
        "tab" => Ok(Key::Tab),
        "enter" | "return" => Ok(Key::Return),
        "escape" | "esc" => Ok(Key::Escape),
        "space" => Ok(Key::Space),
        "backspace" => Ok(Key::Backspace),
        "delete" | "del" => Ok(Key::Delete),
        "home" => Ok(Key::Home),
        "end" => Ok(Key::End),
        "pageup" => Ok(Key::PageUp),
        "pagedown" => Ok(Key::PageDown),
        "up" => Ok(Key::UpArrow),
        "down" => Ok(Key::DownArrow),
        "left" => Ok(Key::LeftArrow),
        "right" => Ok(Key::RightArrow),
        "f1" => Ok(Key::F1),
        "f2" => Ok(Key::F2),
        "f3" => Ok(Key::F3),
        "f4" => Ok(Key::F4),
        "f5" => Ok(Key::F5),
        "f6" => Ok(Key::F6),
        "f7" => Ok(Key::F7),
        "f8" => Ok(Key::F8),
        "f9" => Ok(Key::F9),
        "f10" => Ok(Key::F10),
        "f11" => Ok(Key::F11),
        "f12" => Ok(Key::F12),
        s if s.len() == 1 => Ok(Key::Unicode(s.chars().next().unwrap())),
        _ => Err(format!("unknown key: {key_name}")),
    }
}

fn handle_hold_key(params: Option<&Value>) -> Result<Value, String> {
    let p = params.ok_or("missing params")?;
    let key_name = p.get("key").and_then(|v| v.as_str()).ok_or("missing key")?;
    let key = parse_key(key_name)?;
    let mut enigo = new_enigo()?;
    enigo.key(key, Press).map_err(|e| format!("hold_key failed: {e}"))?;
    Ok(json!({}))
}

fn handle_release_key(params: Option<&Value>) -> Result<Value, String> {
    let p = params.ok_or("missing params")?;
    let key_name = p.get("key").and_then(|v| v.as_str()).ok_or("missing key")?;
    let key = parse_key(key_name)?;
    let mut enigo = new_enigo()?;
    enigo.key(key, Release).map_err(|e| format!("release_key failed: {e}"))?;
    Ok(json!({}))
}

fn handle_press_key(params: Option<&Value>) -> Result<Value, String> {
    let p = params.ok_or("missing params")?;
    let key_name = p.get("key").and_then(|v| v.as_str()).ok_or("missing key")?;
    let key = parse_key(key_name)?;
    let mut enigo = new_enigo()?;
    enigo.key(key, Click).map_err(|e| format!("press_key failed: {e}"))?;
    Ok(json!({}))
}

fn handle_play_audio(params: Option<&Value>) -> Result<Value, String> {
    let p = params.ok_or("missing params")?;
    let audio_data_b64 = p
        .get("audio_data")
        .and_then(|v| v.as_str())
        .ok_or("missing audio_data")?;
    let volume = p
        .get("volume")
        .and_then(|v| v.as_f64())
        .unwrap_or(1.0) as f32;

    // Decode base64 audio data
    let audio_bytes = base64::engine::general_purpose::STANDARD
        .decode(audio_data_b64)
        .map_err(|e| format!("base64 decode failed: {e}"))?;

    if audio_bytes.len() < 4 {
        return Err("audio data too short to detect format".to_string());
    }

    // Detect format from magic bytes: WAV starts with "RIFF", MP3 with 0xFF 0xFB or "ID3"
    let extension = if audio_bytes.starts_with(b"RIFF") {
        "wav"
    } else if audio_bytes.starts_with(b"ID3")
        || (audio_bytes[0] == 0xFF && audio_bytes[1] == 0xFB)
    {
        "mp3"
    } else {
        return Err("unsupported audio format: expected WAV (RIFF) or MP3 (ID3/0xFFFB)".to_string());
    };

    // Write to temp file
    let temp_path = std::env::temp_dir().join(format!("screenmcp_audio.{extension}"));
    std::fs::write(&temp_path, &audio_bytes)
        .map_err(|e| format!("failed to write temp audio file: {e}"))?;

    // Play audio using rodio
    let play_result = (|| -> Result<(), String> {
        let (_stream, stream_handle) = rodio::OutputStream::try_default()
            .map_err(|e| format!("failed to open audio output: {e}"))?;

        let file = std::fs::File::open(&temp_path)
            .map_err(|e| format!("failed to open temp audio file: {e}"))?;
        let buf_reader = std::io::BufReader::new(file);

        let source = rodio::Decoder::new(buf_reader)
            .map_err(|e| format!("failed to decode audio: {e}"))?;

        let sink = rodio::Sink::try_new(&stream_handle)
            .map_err(|e| format!("failed to create audio sink: {e}"))?;

        sink.set_volume(volume.clamp(0.0, 1.0));
        sink.append(source);
        sink.sleep_until_end();

        Ok(())
    })();

    // Clean up temp file regardless of playback outcome
    let _ = std::fs::remove_file(&temp_path);

    play_result?;
    Ok(json!({}))
}

fn handle_mouse_move(params: Option<&Value>, config: &Config) -> Result<Value, String> {
    let (x, y) = get_xy(params, config)?;
    let mut enigo = new_enigo()?;
    enigo
        .move_mouse(x, y, Coordinate::Abs)
        .map_err(|e| format!("move_mouse failed: {e}"))?;
    Ok(json!({}))
}

fn handle_double_click(params: Option<&Value>, config: &Config) -> Result<Value, String> {
    let (x, y) = get_xy(params, config)?;
    let mut enigo = new_enigo()?;
    enigo
        .move_mouse(x, y, Coordinate::Abs)
        .map_err(|e| format!("move_mouse failed: {e}"))?;
    enigo
        .button(Button::Left, Click)
        .map_err(|e| format!("click failed: {e}"))?;
    enigo
        .button(Button::Left, Click)
        .map_err(|e| format!("click failed: {e}"))?;
    Ok(json!({}))
}

fn handle_hotkey(params: Option<&Value>) -> Result<Value, String> {
    let p = params.ok_or("missing params")?;
    let keys_arr = p
        .get("keys")
        .and_then(|v| v.as_array())
        .ok_or("missing keys array")?;

    if keys_arr.is_empty() {
        return Err("keys array is empty".to_string());
    }

    let keys: Vec<Key> = keys_arr
        .iter()
        .map(|v| {
            let name = v.as_str().ok_or("key must be a string")?;
            parse_key(name)
        })
        .collect::<Result<Vec<_>, _>>()?;

    let mut enigo = new_enigo()?;

    // Press all keys in order
    for key in &keys {
        enigo.key(*key, Press).map_err(|e| format!("hotkey press failed: {e}"))?;
    }

    // Release all keys in reverse order
    for key in keys.iter().rev() {
        enigo.key(*key, Release).map_err(|e| format!("hotkey release failed: {e}"))?;
    }

    Ok(json!({}))
}

fn handle_get_screen_size(params: Option<&Value>, config: &Config) -> Result<Value, String> {
    let screens = screenshots::Screen::all().map_err(|e| format!("failed to list screens: {e}"))?;
    let screen = screens
        .first()
        .ok_or_else(|| "no screens found".to_string())?;
    let info = screen.display_info;
    let (ow, oh) = (info.width, info.height);

    // Model-based default (when injected by the server and no explicit size) keeps the
    // reported screen size consistent with the model-sized screenshots.
    let model_dims = if params.and_then(|p| p.get("max_width")).is_none()
        && params.and_then(|p| p.get("max_height")).is_none()
    {
        params.and_then(|p| p.get("model")).and_then(|v| v.as_str())
            .and_then(|m| crate::provider_sizing::provider_default_size(m, ow, oh))
    } else {
        None
    };
    let (mw, mh) = if let Some((dw, dh)) = model_dims {
        (dw as f64, dh as f64)
    } else {
        let mw = params.and_then(|p| p.get("max_width")).and_then(|v| v.as_f64()).or(config.max_screenshot_width.map(|v| v as f64)).unwrap_or(DEFAULT_SCALE_WIDTH);
        let mh = params.and_then(|p| p.get("max_height")).and_then(|v| v.as_f64()).or(config.max_screenshot_height.map(|v| v as f64)).unwrap_or(DEFAULT_SCALE_HEIGHT);
        (mw, mh)
    };

    if mw > 0.0 || mh > 0.0 {
        let r = if mw > 0.0 && mh > 0.0 {
            (mw / ow as f64).min(mh / oh as f64).min(1.0)
        } else if mw > 0.0 {
            (mw / ow as f64).min(1.0)
        } else {
            (mh / oh as f64).min(1.0)
        };
        Ok(json!({
            "width": (ow as f64 * r * 10.0).round() / 10.0,
            "height": (oh as f64 * r * 10.0).round() / 10.0,
            "original_width": ow,
            "original_height": oh,
            "scaled": true,
        }))
    } else {
        Ok(json!({ "width": ow as f64, "height": oh as f64 }))
    }
}

fn handle_list_windows_raw() -> Result<Value, String> {
    // Try wmctrl -lG for window list with geometry
    let output = std::process::Command::new("wmctrl")
        .args(["-lG"])
        .output();

    match output {
        Ok(out) if out.status.success() => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let mut windows: Vec<Value> = Vec::new();

            for (i, line) in stdout.lines().enumerate() {
                let parts: Vec<&str> = line.splitn(8, char::is_whitespace).collect();
                let parts: Vec<&str> = parts.into_iter().filter(|s| !s.is_empty()).collect();
                if parts.len() >= 8 {
                    let x: i32 = parts[2].parse().unwrap_or(0);
                    let y: i32 = parts[3].parse().unwrap_or(0);
                    let width: i32 = parts[4].parse().unwrap_or(0);
                    let height: i32 = parts[5].parse().unwrap_or(0);
                    let title = parts[7];

                    if parts[1] == "-1" {
                        continue;
                    }

                    windows.push(json!({
                        "index": i,
                        "title": title,
                        "x": x,
                        "y": y,
                        "width": width,
                        "height": height,
                    }));
                }
            }

            Ok(json!({ "windows": windows }))
        }
        _ => {
            // Fallback: xdotool
            let output = std::process::Command::new("xdotool")
                .args(["search", "--onlyvisible", "--name", ""])
                .output()
                .map_err(|_| "install wmctrl or xdotool for window listing".to_string())?;

            if !output.status.success() {
                return Err("xdotool search failed".to_string());
            }

            let stdout = String::from_utf8_lossy(&output.stdout);
            let mut windows: Vec<Value> = Vec::new();

            for (i, win_id) in stdout.lines().enumerate() {
                let win_id = win_id.trim();
                if win_id.is_empty() {
                    continue;
                }

                let name_out = std::process::Command::new("xdotool")
                    .args(["getwindowname", win_id])
                    .output();
                let title = name_out
                    .ok()
                    .filter(|o| o.status.success())
                    .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                    .unwrap_or_default();

                let geo_out = std::process::Command::new("xdotool")
                    .args(["getwindowgeometry", "--shell", win_id])
                    .output();

                let (mut x, mut y, mut width, mut height) = (0i32, 0i32, 0i32, 0i32);
                if let Ok(geo) = geo_out {
                    if geo.status.success() {
                        let geo_str = String::from_utf8_lossy(&geo.stdout);
                        for line in geo_str.lines() {
                            if let Some(val) = line.strip_prefix("X=") {
                                x = val.parse().unwrap_or(0);
                            } else if let Some(val) = line.strip_prefix("Y=") {
                                y = val.parse().unwrap_or(0);
                            } else if let Some(val) = line.strip_prefix("WIDTH=") {
                                width = val.parse().unwrap_or(0);
                            } else if let Some(val) = line.strip_prefix("HEIGHT=") {
                                height = val.parse().unwrap_or(0);
                            }
                        }
                    }
                }

                windows.push(json!({
                    "index": i,
                    "title": title,
                    "x": x,
                    "y": y,
                    "width": width,
                    "height": height,
                }));
            }

            Ok(json!({ "windows": windows }))
        }
    }
}

fn handle_focus_window(params: Option<&Value>) -> Result<Value, String> {
    let p = params.ok_or("missing params")?;
    let target_title = p.get("title").and_then(|v| v.as_str());
    let target_index = p.get("index").and_then(|v| v.as_u64()).map(|v| v as usize);

    if target_title.is_none() && target_index.is_none() {
        return Err("provide either 'title' or 'index' parameter".to_string());
    }

    // If by title, try wmctrl -a first
    if let Some(title) = target_title {
        let result = std::process::Command::new("wmctrl")
            .args(["-a", title])
            .output();
        if let Ok(out) = result {
            if out.status.success() {
                return Ok(json!({ "focused": title }));
            }
        }
        // Fallback: xdotool
        let result = std::process::Command::new("xdotool")
            .args(["search", "--name", title, "windowactivate"])
            .output();
        if let Ok(out) = result {
            if out.status.success() {
                return Ok(json!({ "focused": title }));
            }
        }
        return Err(format!("no window matching '{title}'"));
    }

    // By index: get window list and activate by index
    if let Some(index) = target_index {
        let list_result = handle_list_windows_raw()?;
        let windows = list_result
            .get("windows")
            .and_then(|v| v.as_array())
            .ok_or("failed to list windows")?;

        let title = windows
            .get(index)
            .and_then(|w| w.get("title"))
            .and_then(|t| t.as_str())
            .ok_or_else(|| format!("no window at index {index}"))?;

        let result = std::process::Command::new("wmctrl")
            .args(["-a", title])
            .output();
        if let Ok(out) = result {
            if out.status.success() {
                return Ok(json!({ "focused": title }));
            }
        }
        return Err(format!("failed to focus window at index {index}"));
    }

    Err("provide either 'title' or 'index' parameter".to_string())
}

fn handle_active_window(params: Option<&Value>, config: &Config) -> Result<Value, String> {
    // Try xdotool getactivewindow
    let output = std::process::Command::new("xdotool")
        .args(["getactivewindow"])
        .output()
        .map_err(|_| "xdotool not available".to_string())?;

    if !output.status.success() {
        return Ok(json!({ "title": null, "active": false }));
    }

    let win_id = String::from_utf8_lossy(&output.stdout).trim().to_string();

    let name_out = std::process::Command::new("xdotool")
        .args(["getwindowname", &win_id])
        .output();
    let title = name_out
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();

    let geo_out = std::process::Command::new("xdotool")
        .args(["getwindowgeometry", "--shell", &win_id])
        .output();

    let (mut raw_x, mut raw_y, mut raw_w, mut raw_h) = (0i32, 0i32, 0i32, 0i32);
    if let Ok(geo) = geo_out {
        if geo.status.success() {
            let geo_str = String::from_utf8_lossy(&geo.stdout);
            for line in geo_str.lines() {
                if let Some(val) = line.strip_prefix("X=") {
                    raw_x = val.parse().unwrap_or(0);
                } else if let Some(val) = line.strip_prefix("Y=") {
                    raw_y = val.parse().unwrap_or(0);
                } else if let Some(val) = line.strip_prefix("WIDTH=") {
                    raw_w = val.parse().unwrap_or(0);
                } else if let Some(val) = line.strip_prefix("HEIGHT=") {
                    raw_h = val.parse().unwrap_or(0);
                }
            }
        }
    }

    let (sx, sy) = get_output_scale(params, config)?;
    let x = (raw_x as f64 * sx * 10.0).round() / 10.0;
    let y = (raw_y as f64 * sy * 10.0).round() / 10.0;
    let width = (raw_w as f64 * sx * 10.0).round() / 10.0;
    let height = (raw_h as f64 * sy * 10.0).round() / 10.0;

    Ok(json!({
        "title": title,
        "x": x,
        "y": y,
        "width": width,
        "height": height,
    }))
}

fn handle_screenshot_window(params: Option<&Value>, config: &Config) -> Result<Value, String> {
    let p = params.ok_or("missing params")?;
    let target_title = p.get("title").and_then(|v| v.as_str());
    let target_index = p.get("index").and_then(|v| v.as_u64()).map(|v| v as usize);

    if target_title.is_none() && target_index.is_none() {
        return Err("provide either 'title' or 'index' parameter".to_string());
    }

    // Find window ID via xdotool
    let win_id = if let Some(title) = target_title {
        let output = std::process::Command::new("xdotool")
            .args(["search", "--name", title])
            .output()
            .map_err(|_| "xdotool not available".to_string())?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        stdout
            .lines()
            .next()
            .map(|s| s.trim().to_string())
            .ok_or_else(|| format!("no window matching '{title}'"))?
    } else if let Some(index) = target_index {
        let list_result = handle_list_windows_raw()?;
        let windows = list_result
            .get("windows")
            .and_then(|v| v.as_array())
            .ok_or("failed to list windows")?;
        let title = windows
            .get(index)
            .and_then(|w| w.get("title"))
            .and_then(|t| t.as_str())
            .ok_or_else(|| format!("no window at index {index}"))?;
        let output = std::process::Command::new("xdotool")
            .args(["search", "--name", title])
            .output()
            .map_err(|_| "xdotool not available".to_string())?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        stdout
            .lines()
            .next()
            .map(|s| s.trim().to_string())
            .ok_or_else(|| format!("no window matching '{title}'"))?
    } else {
        return Err("provide either 'title' or 'index' parameter".to_string());
    };

    // Use import (ImageMagick) to capture window by ID
    let temp_path = std::env::temp_dir().join("screenmcp_window.png");
    let result = std::process::Command::new("import")
        .args(["-window", &win_id, temp_path.to_str().unwrap()])
        .output();

    let png_bytes = match result {
        Ok(out) if out.status.success() => {
            std::fs::read(&temp_path).map_err(|e| format!("failed to read capture: {e}"))?
        }
        _ => {
            // Fallback: xdotool activate + scrot
            return Err("install imagemagick (import) for window screenshots".to_string());
        }
    };
    let _ = std::fs::remove_file(&temp_path);

    // Load PNG and convert to WebP
    let img = image::load_from_memory(&png_bytes)
        .map_err(|e| format!("failed to decode capture: {e}"))?;

    // Model-based default (no explicit size) computed from the captured WINDOW dimensions.
    let model_dims = if p.get("max_width").is_none() && p.get("max_height").is_none() {
        p.get("model").and_then(|v| v.as_str())
            .and_then(|m| crate::provider_sizing::provider_default_size(m, img.width(), img.height()))
    } else {
        None
    };
    let max_w = model_dims.map(|(w, _)| w)
        .or(p.get("max_width").and_then(|v| v.as_u64()).map(|v| v as u32))
        .or(config.max_screenshot_width).or(Some(DEFAULT_SCALE_WIDTH as u32));
    let max_h = model_dims.map(|(_, h)| h)
        .or(p.get("max_height").and_then(|v| v.as_u64()).map(|v| v as u32))
        .or(config.max_screenshot_height).or(Some(DEFAULT_SCALE_HEIGHT as u32));

    let img = if let (Some(mw), Some(mh)) = (max_w, max_h) {
        if img.width() > mw || img.height() > mh {
            img.resize(mw, mh, image::imageops::FilterType::Triangle)
        } else {
            img
        }
    } else if let Some(mw) = max_w {
        if img.width() > mw {
            let ratio = mw as f64 / img.width() as f64;
            let new_h = (img.height() as f64 * ratio) as u32;
            img.resize_exact(mw, new_h, image::imageops::FilterType::Triangle)
        } else {
            img
        }
    } else if let Some(mh) = max_h {
        if img.height() > mh {
            let ratio = mh as f64 / img.height() as f64;
            let new_w = (img.width() as f64 * ratio) as u32;
            img.resize_exact(new_w, mh, image::imageops::FilterType::Triangle)
        } else {
            img
        }
    } else {
        img
    };

    let rgba = img.to_rgba8();
    let mut buf = Cursor::new(Vec::new());
    WebPEncoder::new_lossless(&mut buf)
        .write_image(
            rgba.as_raw(),
            rgba.width(),
            rgba.height(),
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|e| format!("WebP encode failed: {e}"))?;

    let b64 = base64::engine::general_purpose::STANDARD.encode(buf.into_inner());

    // Get window title for response
    let name_out = std::process::Command::new("xdotool")
        .args(["getwindowname", &win_id])
        .output();
    let title = name_out
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();

    Ok(json!({
        "image": b64,
        "title": title,
        "width": rgba.width(),
        "height": rgba.height(),
    }))
}

fn handle_is_elevated() -> Result<Value, String> {
    let elevated = unsafe { libc::geteuid() == 0 };
    Ok(json!({ "elevated": elevated }))
}

fn handle_elevate() -> Result<Value, String> {
    if unsafe { libc::geteuid() == 0 } {
        return Ok(json!({ "already_elevated": true }));
    }

    let exe_path = std::env::current_exe()
        .map_err(|e| format!("failed to get exe path: {e}"))?;

    // Try pkexec first, then gksudo
    let result = std::process::Command::new("pkexec")
        .arg(exe_path.to_str().unwrap_or("screenmcp"))
        .spawn();

    match result {
        Ok(_) => {
            std::thread::spawn(|| {
                std::thread::sleep(std::time::Duration::from_millis(500));
                std::process::exit(0);
            });
            Ok(json!({ "elevating": true }))
        }
        Err(_) => Err("pkexec not available — install policykit to use elevate".to_string()),
    }
}

fn get_output_scale(params: Option<&Value>, config: &Config) -> Result<(f64, f64), String> {
    let (mw, mh) = resolve_scale_dims(params, config);
    if mw > 0.0 || mh > 0.0 {
        let (sw, sh) = get_screen_dimensions()?;
        let (sw, sh) = (sw as f64, sh as f64);
        Ok(match (mw > 0.0, mh > 0.0) {
            (true, true) => (mw / sw, mh / sh),
            (true, false) => { let s = mw / sw; (s, s) }
            (false, true) => { let s = mh / sh; (s, s) }
            _ => (1.0, 1.0),
        })
    } else {
        Ok((1.0, 1.0))
    }
}

fn scale_bounds_in_value(val: &Value, sx: f64, sy: f64) -> Value {
    match val {
        Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (k, v) in map {
                let scaled = match k.as_str() {
                    "left" | "right" | "x" | "width" if v.is_number() =>
                        json!((v.as_f64().unwrap() * sx * 10.0).round() / 10.0),
                    "top" | "bottom" | "y" | "height" if v.is_number() =>
                        json!((v.as_f64().unwrap() * sy * 10.0).round() / 10.0),
                    _ => scale_bounds_in_value(v, sx, sy),
                };
                out.insert(k.clone(), scaled);
            }
            Value::Object(out)
        }
        Value::Array(arr) => Value::Array(arr.iter().map(|v| scale_bounds_in_value(v, sx, sy)).collect()),
        _ => val.clone(),
    }
}

fn handle_ui_tree(params: Option<&Value>, config: &Config) -> Result<Value, String> {
    let result = handle_ui_tree_raw()?;
    let (sx, sy) = get_output_scale(params, config)?;
    if sx == 1.0 && sy == 1.0 { return Ok(result); }
    Ok(scale_bounds_in_value(&result, sx, sy))
}

fn handle_list_windows(params: Option<&Value>, config: &Config) -> Result<Value, String> {
    let result = handle_list_windows_raw()?;
    let (sx, sy) = get_output_scale(params, config)?;
    if sx == 1.0 && sy == 1.0 { return Ok(result); }
    Ok(scale_bounds_in_value(&result, sx, sy))
}

/// Get list of windows with titles and positions using wmctrl.
/// Falls back to an error if wmctrl is not installed.
fn handle_ui_tree_raw() -> Result<Value, String> {
    // Try wmctrl -lG for window list with geometry
    let output = std::process::Command::new("wmctrl")
        .args(["-lG"])
        .output();

    match output {
        Ok(out) if out.status.success() => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let mut windows: Vec<Value> = Vec::new();

            for line in stdout.lines() {
                // wmctrl -lG format: <win_id> <desktop> <x> <y> <w> <h> <hostname> <title>
                let parts: Vec<&str> = line.splitn(8, char::is_whitespace).collect();
                let parts: Vec<&str> = parts.into_iter().filter(|s| !s.is_empty()).collect();
                if parts.len() >= 8 {
                    let win_id = parts[0];
                    let x: i64 = parts[2].parse().unwrap_or(0);
                    let y: i64 = parts[3].parse().unwrap_or(0);
                    let width: i64 = parts[4].parse().unwrap_or(0);
                    let height: i64 = parts[5].parse().unwrap_or(0);
                    let title = parts[7];

                    // Skip desktop window entries (desktop -1)
                    if parts[1] == "-1" {
                        continue;
                    }

                    // Sparse output: only include non-empty/non-default values
                    let mut node = json!({});
                    let m = node.as_object_mut().unwrap();
                    if !title.is_empty() {
                        m.insert("text".into(), json!(title));
                    }
                    m.insert("hWnd".into(), json!(win_id));
                    m.insert("bounds".into(), json!({
                        "left": x,
                        "top": y,
                        "right": x + width,
                        "bottom": y + height,
                        "width": width,
                        "height": height,
                    }));
                    windows.push(node);
                }
            }

            Ok(json!({ "os": "linux", "tree": windows }))
        }
        _ => {
            // wmctrl not available — try xdotool as fallback
            let output = std::process::Command::new("xdotool")
                .args(["search", "--onlyvisible", "--name", ""])
                .output();

            match output {
                Ok(out) if out.status.success() => {
                    let stdout = String::from_utf8_lossy(&out.stdout);
                    let mut windows: Vec<Value> = Vec::new();

                    for win_id in stdout.lines() {
                        let win_id = win_id.trim();
                        if win_id.is_empty() {
                            continue;
                        }

                        // Get window name
                        let name_out = std::process::Command::new("xdotool")
                            .args(["getwindowname", win_id])
                            .output();
                        let title = name_out
                            .ok()
                            .filter(|o| o.status.success())
                            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                            .unwrap_or_default();

                        // Get window geometry
                        let geo_out = std::process::Command::new("xdotool")
                            .args(["getwindowgeometry", "--shell", win_id])
                            .output();

                        let (mut x, mut y, mut width, mut height) = (0i64, 0i64, 0i64, 0i64);
                        if let Ok(geo) = geo_out {
                            if geo.status.success() {
                                let geo_str = String::from_utf8_lossy(&geo.stdout);
                                for line in geo_str.lines() {
                                    if let Some(val) = line.strip_prefix("X=") {
                                        x = val.parse().unwrap_or(0);
                                    } else if let Some(val) = line.strip_prefix("Y=") {
                                        y = val.parse().unwrap_or(0);
                                    } else if let Some(val) = line.strip_prefix("WIDTH=") {
                                        width = val.parse().unwrap_or(0);
                                    } else if let Some(val) = line.strip_prefix("HEIGHT=") {
                                        height = val.parse().unwrap_or(0);
                                    }
                                }
                            }
                        }

                        // Sparse output: only include non-empty/non-default values
                        let mut node = json!({});
                        let m = node.as_object_mut().unwrap();
                        if !title.is_empty() {
                            m.insert("text".into(), json!(title));
                        }
                        m.insert("hWnd".into(), json!(win_id));
                        m.insert("bounds".into(), json!({
                            "left": x,
                            "top": y,
                            "right": x + width,
                            "bottom": y + height,
                            "width": width,
                            "height": height,
                        }));
                        windows.push(node);
                    }

                    Ok(json!({ "os": "linux", "tree": windows }))
                }
                _ => {
                    Err("install wmctrl or xdotool for window listing".to_string())
                }
            }
        }
    }
}
