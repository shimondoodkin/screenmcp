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
        "hold_key" => handle_hold_key(params),
        "release_key" => handle_release_key(params),
        "press_key" => handle_press_key(params),
        "play_audio" => handle_play_audio(params),
        "mouse_move" => handle_mouse_move(params, config),
        "double_click" => handle_double_click(params, config),
        "hotkey" => handle_hotkey(params),
        "get_screen_size" => handle_get_screen_size(params, config),
        "list_windows" => handle_list_windows(params, config),
        "focus_window" => handle_focus_window(params),
        "active_window" => handle_active_window(params, config),
        "screenshot_window" => handle_screenshot_window(params, config),
        "elevate" => handle_elevate(),
        "is_elevated" => handle_is_elevated(),
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

    // Determine max dimensions from params, config, or built-in default
    let max_w = params
        .and_then(|p| p.get("max_width"))
        .and_then(|v| v.as_u64())
        .map(|v| v as u32)
        .or(config.max_screenshot_width)
        .or(Some(DEFAULT_SCALE_WIDTH as u32));
    let max_h = params
        .and_then(|p| p.get("max_height"))
        .and_then(|v| v.as_u64())
        .map(|v| v as u32)
        .or(config.max_screenshot_height)
        .or(Some(DEFAULT_SCALE_HEIGHT as u32));

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
    // image crate's WebP encoder is lossless-only; quality param is accepted
    // but not used (lossy would require libwebp). Lossless WebP is still smaller
    // than PNG for camera frames and the format matches the Android client.
    let _ = quality;
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

/// Get actual screen dimensions for coordinate scaling.
fn get_screen_dimensions() -> Result<(u32, u32), String> {
    let screens = screenshots::Screen::all().map_err(|e| format!("failed to list screens: {e}"))?;
    let screen = screens.first().ok_or("no screens found")?;
    Ok((screen.display_info.width, screen.display_info.height))
}

/// Get the inverse scale factors (screen→screenshot space) for output coordinates.
/// Returns (scale_x, scale_y) where output = actual * scale. Returns (1.0, 1.0) if no scaling.
fn get_output_scale(params: Option<&Value>, config: &Config) -> Result<(f64, f64), String> {
    let mw = params.and_then(|p| p.get("max_width")).and_then(|v| v.as_f64())
        .or(config.max_screenshot_width.map(|v| v as f64))
        .unwrap_or(DEFAULT_SCALE_WIDTH);
    let mh = params.and_then(|p| p.get("max_height")).and_then(|v| v.as_f64())
        .or(config.max_screenshot_height.map(|v| v as f64))
        .unwrap_or(DEFAULT_SCALE_HEIGHT);

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

/// Scale coordinates from screenshot space to actual screen space.
/// Uses max_width/max_height from params first, then falls back to config defaults.
/// If both are 0/absent, no scaling is applied.
/// Default screenshot dimensions for coordinate scaling (landscape).
const DEFAULT_SCALE_WIDTH: f64 = 1456.0;
const DEFAULT_SCALE_HEIGHT: f64 = 819.0;

fn scale_xy(x: f64, y: f64, params: Option<&Value>, config: &Config) -> Result<(i32, i32), String> {
    let mw = params.and_then(|p| p.get("max_width")).and_then(|v| v.as_f64())
        .or(config.max_screenshot_width.map(|v| v as f64))
        .unwrap_or(DEFAULT_SCALE_WIDTH);
    let mh = params.and_then(|p| p.get("max_height")).and_then(|v| v.as_f64())
        .or(config.max_screenshot_height.map(|v| v as f64))
        .unwrap_or(DEFAULT_SCALE_HEIGHT);

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
        params, config,
    )?;
    let (end_x, end_y) = scale_xy(
        p.get("endX").and_then(|v| v.as_f64()).ok_or("missing endX")?,
        p.get("endY").and_then(|v| v.as_f64()).ok_or("missing endY")?,
        params, config,
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
    #[cfg(target_os = "macos")]
    let modifier = Key::Meta;
    #[cfg(not(target_os = "macos"))]
    let modifier = Key::Control;

    enigo.key(modifier, Press).map_err(|e| format!("{e}"))?;
    enigo.key(Key::Unicode('a'), Click).map_err(|e| format!("{e}"))?;
    enigo.key(modifier, Release).map_err(|e| format!("{e}"))?;
    Ok(json!({}))
}

fn handle_copy(params: Option<&Value>) -> Result<Value, String> {
    let mut enigo = new_enigo()?;
    #[cfg(target_os = "macos")]
    let modifier = Key::Meta;
    #[cfg(not(target_os = "macos"))]
    let modifier = Key::Control;

    enigo.key(modifier, Press).map_err(|e| format!("{e}"))?;
    enigo.key(Key::Unicode('c'), Click).map_err(|e| format!("{e}"))?;
    enigo.key(modifier, Release).map_err(|e| format!("{e}"))?;

    let return_text = params
        .and_then(|p| p.get("return_text"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if return_text {
        // Small delay to let clipboard update
        thread::sleep(Duration::from_millis(50));
        let mut clipboard = arboard::Clipboard::new().map_err(|e| format!("clipboard init failed: {e}"))?;
        let text = clipboard.get_text().unwrap_or_default();
        Ok(json!({ "text": text }))
    } else {
        Ok(json!({}))
    }
}

fn handle_paste(params: Option<&Value>) -> Result<Value, String> {
    // If text param provided, set clipboard first
    if let Some(text) = params.and_then(|p| p.get("text")).and_then(|v| v.as_str()) {
        let mut clipboard = arboard::Clipboard::new().map_err(|e| format!("clipboard init failed: {e}"))?;
        clipboard.set_text(text).map_err(|e| format!("set clipboard failed: {e}"))?;
    }

    let mut enigo = new_enigo()?;
    #[cfg(target_os = "macos")]
    let modifier = Key::Meta;
    #[cfg(not(target_os = "macos"))]
    let modifier = Key::Control;

    enigo.key(modifier, Press).map_err(|e| format!("{e}"))?;
    enigo.key(Key::Unicode('v'), Click).map_err(|e| format!("{e}"))?;
    enigo.key(modifier, Release).map_err(|e| format!("{e}"))?;
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
    // Windows/Super key to show desktop/start menu
    #[cfg(target_os = "windows")]
    {
        enigo.key(Key::Meta, Click).map_err(|e| format!("{e}"))?;
    }
    #[cfg(target_os = "macos")]
    {
        // Cmd+F3 for Mission Control / show desktop
        enigo.key(Key::Meta, Press).map_err(|e| format!("{e}"))?;
        enigo.key(Key::F3, Click).map_err(|e| format!("{e}"))?;
        enigo.key(Key::Meta, Release).map_err(|e| format!("{e}"))?;
    }
    #[cfg(target_os = "linux")]
    {
        enigo.key(Key::Meta, Click).map_err(|e| format!("{e}"))?;
    }
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

    let mw = params.and_then(|p| p.get("max_width")).and_then(|v| v.as_f64())
        .or(config.max_screenshot_width.map(|v| v as f64))
        .unwrap_or(DEFAULT_SCALE_WIDTH);
    let mh = params.and_then(|p| p.get("max_height")).and_then(|v| v.as_f64())
        .or(config.max_screenshot_height.map(|v| v as f64))
        .unwrap_or(0.0);

    if mw > 0.0 || mh > 0.0 {
        // Compute scaled dimensions preserving aspect ratio (same logic as screenshot resize)
        let (sw, sh) = if mw > 0.0 && mh > 0.0 {
            let rw = mw / ow as f64;
            let rh = mh / oh as f64;
            let r = rw.min(rh).min(1.0);
            ((ow as f64 * r) as u32, (oh as f64 * r) as u32)
        } else if mw > 0.0 {
            let r = (mw / ow as f64).min(1.0);
            ((ow as f64 * r) as u32, (oh as f64 * r) as u32)
        } else {
            let r = (mh / oh as f64).min(1.0);
            ((ow as f64 * r) as u32, (oh as f64 * r) as u32)
        };
        Ok(json!({
            "width": sw,
            "height": sh,
            "original_width": ow,
            "original_height": oh,
            "scaled": true,
        }))
    } else {
        Ok(json!({
            "width": ow,
            "height": oh,
        }))
    }
}

#[cfg(windows)]
fn handle_list_windows_raw() -> Result<Value, String> {
    use std::sync::Arc;
    use windows::Win32::Foundation::{BOOL, HWND, LPARAM, TRUE};
    use windows::Win32::UI::WindowsAndMessaging::*;

    let vx = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) };
    let vy = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
    let vw = unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN) };
    let vh = unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN) };

    struct WindowInfo {
        title: String,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        is_minimized: bool,
        is_maximized: bool,
    }

    let windows_list: Arc<std::sync::Mutex<Vec<WindowInfo>>> =
        Arc::new(std::sync::Mutex::new(Vec::new()));

    // We need to pass data to the callback via LPARAM
    struct CallbackData {
        windows: Arc<std::sync::Mutex<Vec<WindowInfo>>>,
        viewport: (i32, i32, i32, i32),
    }

    let data = CallbackData {
        windows: windows_list.clone(),
        viewport: (vx, vy, vx + vw, vy + vh),
    };

    unsafe extern "system" fn enum_callback(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let data = &*(lparam.0 as *const CallbackData);

        // Skip invisible windows
        if !IsWindowVisible(hwnd).as_bool() {
            return TRUE;
        }

        // Skip windows with empty titles
        let mut title_buf = [0u16; 512];
        let len = GetWindowTextW(hwnd, &mut title_buf);
        if len == 0 {
            return TRUE;
        }
        let title = String::from_utf16_lossy(&title_buf[..len as usize]);

        // Get window rect
        let mut rect = std::mem::zeroed::<windows::Win32::Foundation::RECT>();
        if GetWindowRect(hwnd, &mut rect).is_err() {
            return TRUE;
        }

        let w = rect.right - rect.left;
        let h = rect.bottom - rect.top;

        // Skip zero-size windows
        if w <= 0 || h <= 0 {
            return TRUE;
        }

        // Skip windows entirely outside viewport (offscreen hidden windows)
        let vp = data.viewport;
        if rect.right <= vp.0 || rect.left >= vp.2 || rect.bottom <= vp.1 || rect.top >= vp.3 {
            return TRUE;
        }

        let is_minimized = IsIconic(hwnd).as_bool();
        let is_maximized = IsZoomed(hwnd).as_bool();

        data.windows.lock().unwrap().push(WindowInfo {
            title,
            x: rect.left,
            y: rect.top,
            width: w,
            height: h,
            is_minimized,
            is_maximized,
        });

        TRUE
    }

    unsafe {
        let _ = EnumWindows(Some(enum_callback), LPARAM(&data as *const _ as isize));
    }

    let windows = windows_list.lock().unwrap();
    let list: Vec<Value> = windows
        .iter()
        .enumerate()
        .map(|(i, w)| {
            json!({
                "index": i,
                "title": w.title,
                "x": w.x,
                "y": w.y,
                "width": w.width,
                "height": w.height,
                "minimized": w.is_minimized,
                "maximized": w.is_maximized,
            })
        })
        .collect();

    Ok(json!({ "windows": list }))
}

#[cfg(not(windows))]
fn handle_list_windows_raw() -> Result<Value, String> {
    Err("list_windows is only supported on Windows".to_string())
}

#[cfg(windows)]
fn handle_focus_window(params: Option<&Value>) -> Result<Value, String> {
    use windows::Win32::Foundation::{BOOL, HWND, LPARAM, TRUE};
    use windows::Win32::UI::WindowsAndMessaging::*;

    let p = params.ok_or("missing params")?;

    let target_title = p.get("title").and_then(|v| v.as_str());
    let target_index = p.get("index").and_then(|v| v.as_u64()).map(|v| v as usize);

    if target_title.is_none() && target_index.is_none() {
        return Err("provide either 'title' or 'index' parameter".to_string());
    }

    // Use list_windows to find the target title
    let list_result = handle_list_windows_raw()?;
    let windows = list_result
        .get("windows")
        .and_then(|v| v.as_array())
        .ok_or("failed to list windows")?;

    let target_window_title = if let Some(index) = target_index {
        windows
            .get(index)
            .and_then(|w| w.get("title"))
            .and_then(|t| t.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| format!("no window at index {index}"))?
    } else if let Some(title_substr) = target_title {
        let lower = title_substr.to_lowercase();
        windows
            .iter()
            .find_map(|w| {
                let t = w.get("title")?.as_str()?;
                if t.to_lowercase().contains(&lower) {
                    Some(t.to_string())
                } else {
                    None
                }
            })
            .ok_or_else(|| format!("no window matching '{title_substr}'"))?
    } else {
        return Err("provide either 'title' or 'index' parameter".to_string());
    };

    // Find the HWND by title and bring to front
    struct FindData {
        target: String,
        found: Option<isize>,
    }

    let mut find_data = FindData {
        target: target_window_title.clone(),
        found: None,
    };

    unsafe extern "system" fn find_callback(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let data = &mut *(lparam.0 as *mut FindData);
        let mut title_buf = [0u16; 512];
        let len = GetWindowTextW(hwnd, &mut title_buf);
        if len > 0 {
            let title = String::from_utf16_lossy(&title_buf[..len as usize]);
            if title == data.target {
                data.found = Some(hwnd.0 as isize);
                return BOOL(0);
            }
        }
        TRUE
    }

    unsafe {
        let _ = EnumWindows(
            Some(find_callback),
            LPARAM(&mut find_data as *mut _ as isize),
        );
    }

    let hwnd_val = find_data.found.ok_or("window not found")?;
    let hwnd = HWND(hwnd_val as *mut _);

    unsafe {
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        let _ = SetForegroundWindow(hwnd);
    }

    Ok(json!({ "focused": target_window_title }))
}

#[cfg(not(windows))]
fn handle_focus_window(params: Option<&Value>) -> Result<Value, String> {
    let _ = params;
    Err("focus_window is only supported on Windows".to_string())
}

#[cfg(windows)]
fn handle_active_window(params: Option<&Value>, config: &Config) -> Result<Value, String> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::*;

    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd == HWND::default() {
        return Ok(json!({ "title": null, "active": false }));
    }

    let mut title_buf = [0u16; 512];
    let len = unsafe { GetWindowTextW(hwnd, &mut title_buf) };
    let title = if len > 0 {
        String::from_utf16_lossy(&title_buf[..len as usize])
    } else {
        String::new()
    };

    let mut rect = unsafe { std::mem::zeroed::<windows::Win32::Foundation::RECT>() };
    let _ = unsafe { GetWindowRect(hwnd, &mut rect) };

    let (sx, sy) = get_output_scale(params, config)?;
    let x = (rect.left as f64 * sx).round() as i64;
    let y = (rect.top as f64 * sy).round() as i64;
    let width = ((rect.right - rect.left) as f64 * sx).round() as i64;
    let height = ((rect.bottom - rect.top) as f64 * sy).round() as i64;

    Ok(json!({
        "title": title,
        "x": x,
        "y": y,
        "width": width,
        "height": height,
        "minimized": unsafe { IsIconic(hwnd).as_bool() },
        "maximized": unsafe { IsZoomed(hwnd).as_bool() },
    }))
}

#[cfg(not(windows))]
fn handle_active_window(params: Option<&Value>, config: &Config) -> Result<Value, String> {
    Err("active_window is only supported on Windows".to_string())
}

#[cfg(windows)]
fn handle_screenshot_window(params: Option<&Value>, config: &Config) -> Result<Value, String> {
    use windows::Win32::Foundation::{BOOL, HWND, LPARAM, TRUE};
    use windows::Win32::Graphics::Gdi::*;
    use windows::Win32::Storage::Xps::{PrintWindow, PRINT_WINDOW_FLAGS};
    use windows::Win32::UI::WindowsAndMessaging::*;

    let p = params.ok_or("missing params")?;
    let target_title = p.get("title").and_then(|v| v.as_str());
    let target_index = p.get("index").and_then(|v| v.as_u64()).map(|v| v as usize);

    if target_title.is_none() && target_index.is_none() {
        return Err("provide either 'title' or 'index' parameter".to_string());
    }

    // Find the target window
    let list_result = handle_list_windows_raw()?;
    let windows = list_result
        .get("windows")
        .and_then(|v| v.as_array())
        .ok_or("failed to list windows")?;

    let target_window_title = if let Some(index) = target_index {
        windows
            .get(index)
            .and_then(|w| w.get("title"))
            .and_then(|t| t.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| format!("no window at index {index}"))?
    } else if let Some(title_substr) = target_title {
        let lower = title_substr.to_lowercase();
        windows
            .iter()
            .find_map(|w| {
                let t = w.get("title")?.as_str()?;
                if t.to_lowercase().contains(&lower) {
                    Some(t.to_string())
                } else {
                    None
                }
            })
            .ok_or_else(|| format!("no window matching '{title_substr}'"))?
    } else {
        return Err("provide either 'title' or 'index' parameter".to_string());
    };

    // Find the HWND
    struct FindData {
        target: String,
        found: Option<isize>,
    }

    let mut find_data = FindData {
        target: target_window_title.clone(),
        found: None,
    };

    unsafe extern "system" fn find_callback(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let data = &mut *(lparam.0 as *mut FindData);
        let mut title_buf = [0u16; 512];
        let len = GetWindowTextW(hwnd, &mut title_buf);
        if len > 0 {
            let title = String::from_utf16_lossy(&title_buf[..len as usize]);
            if title == data.target {
                data.found = Some(hwnd.0 as isize);
                return BOOL(0);
            }
        }
        TRUE
    }

    unsafe {
        let _ = EnumWindows(
            Some(find_callback),
            LPARAM(&mut find_data as *mut _ as isize),
        );
    }

    let hwnd_val = find_data.found.ok_or("window not found")?;
    let hwnd = HWND(hwnd_val as *mut _);

    // Get window dimensions
    let mut rect = unsafe { std::mem::zeroed::<windows::Win32::Foundation::RECT>() };
    unsafe { GetWindowRect(hwnd, &mut rect) }
        .map_err(|e| format!("GetWindowRect failed: {e}"))?;

    let width = (rect.right - rect.left) as u32;
    let height = (rect.bottom - rect.top) as u32;

    if width == 0 || height == 0 {
        return Err("window has zero size".to_string());
    }

    // Create a compatible DC and bitmap
    let hdc_window = unsafe { GetDC(hwnd) };
    let hdc_mem = unsafe { CreateCompatibleDC(hdc_window) };
    let hbm = unsafe { CreateCompatibleBitmap(hdc_window, width as i32, height as i32) };
    let old_bm = unsafe { SelectObject(hdc_mem, hbm) };

    // Use PrintWindow to capture even if partially occluded
    // PW_RENDERFULLCONTENT (0x2) captures the full window content
    let pw_result = unsafe {
        PrintWindow(hwnd, hdc_mem, PRINT_WINDOW_FLAGS(0x2))
    };

    if !pw_result.as_bool() {
        // Fallback to BitBlt from window DC
        unsafe {
            let _ = BitBlt(hdc_mem, 0, 0, width as i32, height as i32, hdc_window, 0, 0, SRCCOPY);
        }
    }

    // Read pixels from the bitmap
    let mut bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width as i32,
            biHeight: -(height as i32), // top-down
            biPlanes: 1,
            biBitCount: 32,
            biCompression: 0, // BI_RGB
            ..Default::default()
        },
        ..Default::default()
    };

    let mut pixels = vec![0u8; (width * height * 4) as usize];
    unsafe {
        GetDIBits(
            hdc_mem,
            hbm,
            0,
            height,
            Some(pixels.as_mut_ptr() as *mut _),
            &mut bmi,
            DIB_RGB_COLORS,
        );
    }

    // Clean up GDI objects
    unsafe {
        SelectObject(hdc_mem, old_bm);
        let _ = DeleteObject(hbm);
        DeleteDC(hdc_mem);
        ReleaseDC(hwnd, hdc_window);
    }

    // Convert BGRA to RGBA
    for chunk in pixels.chunks_exact_mut(4) {
        chunk.swap(0, 2); // B <-> R
    }

    let img = image::RgbaImage::from_raw(width, height, pixels)
        .ok_or_else(|| "failed to create image from window capture".to_string())?;

    // Apply max dimensions
    let max_w = p
        .get("max_width")
        .and_then(|v| v.as_u64())
        .map(|v| v as u32)
        .or(config.max_screenshot_width)
        .or(Some(DEFAULT_SCALE_WIDTH as u32));
    let max_h = p
        .get("max_height")
        .and_then(|v| v.as_u64())
        .map(|v| v as u32)
        .or(config.max_screenshot_height)
        .or(Some(DEFAULT_SCALE_HEIGHT as u32));

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

    // Encode as WebP
    let mut buf = Cursor::new(Vec::new());
    WebPEncoder::new_lossless(&mut buf)
        .write_image(
            img.as_raw(),
            img.width(),
            img.height(),
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|e| format!("WebP encode failed: {e}"))?;

    let b64 = base64::engine::general_purpose::STANDARD.encode(buf.into_inner());

    Ok(json!({
        "image": b64,
        "title": target_window_title,
        "width": img.width(),
        "height": img.height(),
    }))
}

#[cfg(not(windows))]
fn handle_screenshot_window(params: Option<&Value>, _config: &Config) -> Result<Value, String> {
    let _ = params;
    Err("screenshot_window is only supported on Windows".to_string())
}

#[cfg(windows)]
fn is_process_elevated() -> bool {
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::Security::{
        GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
    };
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    unsafe {
        let mut token = HANDLE::default();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).is_err() {
            return false;
        }

        let mut elevation = TOKEN_ELEVATION::default();
        let mut size = 0u32;
        let result = GetTokenInformation(
            token,
            TokenElevation,
            Some(&mut elevation as *mut _ as *mut _),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut size,
        );
        let _ = CloseHandle(token);

        result.is_ok() && elevation.TokenIsElevated != 0
    }
}

#[cfg(not(windows))]
fn is_process_elevated() -> bool {
    false
}

/// Public wrapper for tray.rs to check elevation status.
pub fn is_process_elevated_check() -> bool {
    is_process_elevated()
}

fn handle_is_elevated() -> Result<Value, String> {
    Ok(json!({ "elevated": is_process_elevated() }))
}

#[cfg(windows)]
fn handle_elevate() -> Result<Value, String> {
    if is_process_elevated() {
        return Ok(json!({ "already_elevated": true }));
    }

    use windows::core::w;
    use windows::Win32::UI::WindowsAndMessaging::*;

    // Show confirmation dialog
    let result = unsafe {
        MessageBoxW(
            None,
            w!("The AI assistant is requesting administrator privileges.\n\nAllow?"),
            w!("ScreenMCP - Elevation Request"),
            MB_OKCANCEL | MB_ICONQUESTION | MB_SETFOREGROUND,
        )
    };

    if result == IDCANCEL {
        return Err("User denied elevation request".to_string());
    }

    // Relaunch as admin
    use windows::core::PCWSTR;
    use windows::Win32::UI::Shell::ShellExecuteW;

    let exe_path =
        std::env::current_exe().map_err(|e| format!("failed to get exe path: {e}"))?;
    let exe_wide: Vec<u16> = exe_path
        .to_string_lossy()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();

    let result = unsafe {
        ShellExecuteW(
            None,
            w!("runas"),
            PCWSTR(exe_wide.as_ptr()),
            None,
            None,
            SW_SHOWNORMAL,
        )
    };

    if result.0 as usize > 32 {
        // Exit current process after response is sent
        std::thread::spawn(|| {
            std::thread::sleep(std::time::Duration::from_millis(500));
            std::process::exit(0);
        });
        Ok(json!({ "elevating": true }))
    } else {
        Err("Failed to launch elevated process (UAC was likely cancelled)".to_string())
    }
}

#[cfg(not(windows))]
fn handle_elevate() -> Result<Value, String> {
    Err("elevate is only supported on Windows".to_string())
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

/// Full UIAutomation accessibility tree.
/// Walks the control view from the desktop root, extracting element properties
/// and interaction patterns to match the Android ui_tree output format.
#[cfg(windows)]
fn handle_ui_tree_raw() -> Result<Value, String> {
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED,
    };
    use windows::Win32::UI::Accessibility::*;

    // COM guard: initialize on entry, uninitialize on drop
    struct ComGuard;
    impl Drop for ComGuard {
        fn drop(&mut self) {
            unsafe { CoUninitialize(); }
        }
    }

    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED)
            .ok()
            .map_err(|e| format!("CoInitializeEx failed: {e}"))?;
    }
    let _com = ComGuard;

    let automation: IUIAutomation = unsafe {
        CoCreateInstance(&CUIAutomation, None, CLSCTX_ALL)
            .map_err(|e| format!("failed to create IUIAutomation: {e}"))?
    };

    let walker = unsafe {
        automation
            .ControlViewWalker()
            .map_err(|e| format!("ControlViewWalker failed: {e}"))?
    };

    let root = unsafe {
        automation
            .GetRootElement()
            .map_err(|e| format!("GetRootElement failed: {e}"))?
    };

    // Virtual screen bounds (covers all monitors)
    use windows::Win32::UI::WindowsAndMessaging::{
        GetSystemMetrics, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
        SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN,
    };
    let vx = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) };
    let vy = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
    let vw = unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN) };
    let vh = unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN) };
    let viewport = [vx, vy, vx + vw, vy + vh];

    // Walk top-level children of the desktop (each top-level window).
    // ControlViewWalker returns siblings front-to-back (z-order), so we
    // track rects of already-included siblings and skip any window whose
    // bounds are fully enclosed by one in front of it.
    let mut children = Vec::new();
    let max_depth: u32 = 10;
    let mut covered_rects: Vec<[i32; 4]> = Vec::new();

    let mut child = unsafe { walker.GetFirstChildElement(&root).ok() };
    while let Some(ref el) = child {
        if let Some(node) = walk_element(el, &walker, 1, max_depth, &mut covered_rects, &viewport) {
            children.push(node);
        }
        child = unsafe { walker.GetNextSiblingElement(el).ok() };
    }

    Ok(json!({ "tree": children, "os": "windows" }))
}

/// Check if rect `inner` is fully enclosed by rect `outer`.
/// Rects are [left, top, right, bottom].
#[cfg(windows)]
fn is_fully_enclosed(inner: &[i32; 4], outer: &[i32; 4]) -> bool {
    inner[0] >= outer[0] && inner[1] >= outer[1] && inner[2] <= outer[2] && inner[3] <= outer[3]
}

/// Recursively walk a UIAutomation element and its children.
/// `sibling_rects` tracks bounding rects of previously included siblings
/// at the same level (front-to-back z-order) for occlusion culling.
/// `viewport` is [left, top, right, bottom] of the virtual screen.
#[cfg(windows)]
fn walk_element(
    el: &windows::Win32::UI::Accessibility::IUIAutomationElement,
    walker: &windows::Win32::UI::Accessibility::IUIAutomationTreeWalker,
    depth: u32,
    max_depth: u32,
    sibling_rects: &mut Vec<[i32; 4]>,
    viewport: &[i32; 4],
) -> Option<Value> {
    use windows::core::Interface;
    use windows::Win32::UI::Accessibility::*;

    // ── Phase 1: cheap filters (2-3 COM calls) ── skip entire subtrees early

    // Skip offscreen / invisible elements (and don't recurse into them)
    let is_offscreen = unsafe { el.CurrentIsOffscreen().ok() }
        .map(|b| b.as_bool())
        .unwrap_or(false);
    if is_offscreen {
        return None;
    }

    // Spatial filtering on bounding rect
    let bounds_raw = unsafe { el.CurrentBoundingRectangle().ok() };
    let has_real_bounds = bounds_raw.as_ref().map_or(false, |r| r.right > r.left && r.bottom > r.top);
    if has_real_bounds {
        let r = bounds_raw.as_ref().unwrap();
        let rect = [r.left, r.top, r.right, r.bottom];
        // Skip elements entirely outside the viewport
        if rect[2] <= viewport[0] || rect[0] >= viewport[2]
            || rect[3] <= viewport[1] || rect[1] >= viewport[3]
        {
            return None;
        }
        // Z-order occlusion: skip elements fully covered by a sibling in front
        if sibling_rects.iter().any(|sr| is_fully_enclosed(&rect, sr)) {
            return None;
        }
    }

    // ── Phase 2: minimal props for noise filter (2 COM calls) + recurse

    let name = unsafe { el.CurrentName().ok() }
        .map(|s| s.to_string())
        .unwrap_or_default();
    let automation_id = unsafe { el.CurrentAutomationId().ok() }
        .map(|s| s.to_string())
        .unwrap_or_default();

    // Recurse into children (fresh sibling_rects per level for occlusion)
    let mut child_nodes = Vec::new();
    if depth < max_depth {
        let mut child_sibling_rects: Vec<[i32; 4]> = Vec::new();
        let mut child = unsafe { walker.GetFirstChildElement(el).ok() };
        while let Some(ref c) = child {
            if let Some(node) = walk_element(c, walker, depth + 1, max_depth, &mut child_sibling_rects, viewport) {
                child_nodes.push(node);
            }
            child = unsafe { walker.GetNextSiblingElement(c).ok() };
        }
    }

    // Noise filter: skip leaf nodes with empty name AND empty automationId
    if child_nodes.is_empty() && name.is_empty() && automation_id.is_empty() {
        return None;
    }

    // Skip zero/no-bounds elements that have no visible children
    if !has_real_bounds && child_nodes.is_empty() {
        return None;
    }

    // ── Phase 3: full properties + patterns (only for nodes that survive) ──

    let class_name = unsafe { el.CurrentClassName().ok() }
        .map(|s| s.to_string())
        .unwrap_or_default();
    let help_text = unsafe { el.CurrentHelpText().ok() }
        .map(|s| s.to_string())
        .unwrap_or_default();
    let control_type_id = unsafe { el.CurrentControlType().unwrap_or_default() };
    let is_enabled = unsafe { el.CurrentIsEnabled().ok() }
        .map(|b| b.as_bool())
        .unwrap_or(true);
    let is_focusable = unsafe { el.CurrentIsKeyboardFocusable().ok() }
        .map(|b| b.as_bool())
        .unwrap_or(false);
    let has_focus = if is_focusable {
        unsafe { el.CurrentHasKeyboardFocus().ok() }
            .map(|b| b.as_bool())
            .unwrap_or(false)
    } else {
        false
    };
    let native_hwnd = unsafe { el.CurrentNativeWindowHandle().ok() }
        .map(|h| h.0 as u64)
        .unwrap_or(0);

    // Bounds as {left, top, right, bottom, width, height}
    let bounds_json = bounds_raw
        .as_ref()
        .map(|r| {
            let left = r.left as i32;
            let top = r.top as i32;
            let right = r.right as i32;
            let bottom = r.bottom as i32;
            let width = right - left;
            let height = bottom - top;
            json!({"left": left, "top": top, "right": right, "bottom": bottom, "width": width, "height": height})
        })
        .unwrap_or(json!({"left": 0, "top": 0, "right": 0, "bottom": 0, "width": 0, "height": 0}));

    let clickable = unsafe {
        el.GetCurrentPattern(UIA_InvokePatternId).is_ok()
    };
    let (editable, value) = unsafe {
        match el.GetCurrentPattern(UIA_ValuePatternId) {
            Ok(pat) => {
                let vp: Result<IUIAutomationValuePattern, _> = pat.cast();
                match vp {
                    Ok(vp) => {
                        let v = vp.CurrentValue().ok().map(|s| s.to_string()).unwrap_or_default();
                        (true, v)
                    }
                    Err(_) => (true, String::new()),
                }
            }
            Err(_) => (false, String::new()),
        }
    };
    let (checkable, checked) = unsafe {
        match el.GetCurrentPattern(UIA_TogglePatternId) {
            Ok(pat) => {
                let tp: Result<IUIAutomationTogglePattern, _> = pat.cast();
                match tp {
                    Ok(tp) => {
                        let state = tp.CurrentToggleState().unwrap_or(ToggleState_Off);
                        (true, state == ToggleState_On)
                    }
                    Err(_) => (true, false),
                }
            }
            Err(_) => (false, false),
        }
    };
    let scrollable = unsafe {
        el.GetCurrentPattern(UIA_ScrollPatternId).is_ok()
    };

    let ct_name = control_type_name(control_type_id);

    // Sparse JSON: only include non-default / non-empty values.
    // Key insertion order = output order (preserve_order feature).
    let mut node = json!({});
    let m = node.as_object_mut().unwrap();

    // 1. Text / identity
    if !name.is_empty() { m.insert("text".into(), json!(name)); }
    if editable && !value.is_empty() { m.insert("value".into(), json!(value)); }
    m.insert("controlType".into(), json!(ct_name));
    if !class_name.is_empty() { m.insert("className".into(), json!(class_name)); }
    if !automation_id.is_empty() { m.insert("resourceId".into(), json!(automation_id)); }
    if !help_text.is_empty() { m.insert("contentDescription".into(), json!(help_text)); }

    // 2. Bounds
    m.insert("bounds".into(), bounds_json);

    // 3. State & interaction flags (only non-defaults)
    if !is_enabled { m.insert("enabled".into(), json!(false)); }
    if clickable { m.insert("clickable".into(), json!(true)); }
    if editable { m.insert("editable".into(), json!(true)); }
    if scrollable { m.insert("scrollable".into(), json!(true)); }
    if checkable {
        m.insert("checked".into(), json!(checked));
    }
    if is_focusable {
        m.insert("focused".into(), json!(has_focus));
    }
    if native_hwnd != 0 { m.insert("hwnd".into(), json!(native_hwnd)); }

    // 4. Children last
    if !child_nodes.is_empty() {
        m.insert("children".into(), json!(child_nodes));
    }

    // Register this element's rect so later siblings can be culled if fully behind it
    if let Some(ref r) = bounds_raw {
        let rect = [r.left, r.top, r.right, r.bottom];
        if rect[2] > rect[0] && rect[3] > rect[1] {
            sibling_rects.push(rect);
        }
    }

    Some(node)
}

/// Map UIA control type ID to a human-readable string.
#[cfg(windows)]
#[allow(non_upper_case_globals)]
fn control_type_name(id: windows::Win32::UI::Accessibility::UIA_CONTROLTYPE_ID) -> &'static str {
    use windows::Win32::UI::Accessibility::*;
    match id {
        UIA_ButtonControlTypeId => "Button",
        UIA_CalendarControlTypeId => "Calendar",
        UIA_CheckBoxControlTypeId => "CheckBox",
        UIA_ComboBoxControlTypeId => "ComboBox",
        UIA_EditControlTypeId => "Edit",
        UIA_HyperlinkControlTypeId => "Hyperlink",
        UIA_ImageControlTypeId => "Image",
        UIA_ListItemControlTypeId => "ListItem",
        UIA_ListControlTypeId => "List",
        UIA_MenuControlTypeId => "Menu",
        UIA_MenuBarControlTypeId => "MenuBar",
        UIA_MenuItemControlTypeId => "MenuItem",
        UIA_ProgressBarControlTypeId => "ProgressBar",
        UIA_RadioButtonControlTypeId => "RadioButton",
        UIA_ScrollBarControlTypeId => "ScrollBar",
        UIA_SliderControlTypeId => "Slider",
        UIA_SpinnerControlTypeId => "Spinner",
        UIA_StatusBarControlTypeId => "StatusBar",
        UIA_TabControlTypeId => "Tab",
        UIA_TabItemControlTypeId => "TabItem",
        UIA_TextControlTypeId => "Text",
        UIA_ToolBarControlTypeId => "ToolBar",
        UIA_ToolTipControlTypeId => "ToolTip",
        UIA_TreeControlTypeId => "Tree",
        UIA_TreeItemControlTypeId => "TreeItem",
        UIA_CustomControlTypeId => "Custom",
        UIA_GroupControlTypeId => "Group",
        UIA_ThumbControlTypeId => "Thumb",
        UIA_DataGridControlTypeId => "DataGrid",
        UIA_DataItemControlTypeId => "DataItem",
        UIA_DocumentControlTypeId => "Document",
        UIA_SplitButtonControlTypeId => "SplitButton",
        UIA_WindowControlTypeId => "Window",
        UIA_PaneControlTypeId => "Pane",
        UIA_HeaderControlTypeId => "Header",
        UIA_HeaderItemControlTypeId => "HeaderItem",
        UIA_TableControlTypeId => "Table",
        UIA_TitleBarControlTypeId => "TitleBar",
        UIA_SeparatorControlTypeId => "Separator",
        UIA_SemanticZoomControlTypeId => "SemanticZoom",
        UIA_AppBarControlTypeId => "AppBar",
        _ => "Unknown",
    }
}

#[cfg(not(windows))]
fn handle_ui_tree_raw() -> Result<Value, String> {
    Err("ui_tree is not supported on this platform".to_string())
}

fn handle_ui_tree(params: Option<&Value>, config: &Config) -> Result<Value, String> {
    let result = handle_ui_tree_raw()?;
    let (sx, sy) = get_output_scale(params, config)?;
    if sx == 1.0 && sy == 1.0 {
        return Ok(result);
    }
    Ok(scale_bounds_in_value(&result, sx, sy))
}

fn handle_list_windows(params: Option<&Value>, config: &Config) -> Result<Value, String> {
    let result = handle_list_windows_raw()?;
    let (sx, sy) = get_output_scale(params, config)?;
    if sx == 1.0 && sy == 1.0 {
        return Ok(result);
    }
    Ok(scale_bounds_in_value(&result, sx, sy))
}

/// Recursively scale coordinate fields in a JSON value.
fn scale_bounds_in_value(val: &Value, sx: f64, sy: f64) -> Value {
    match val {
        Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (k, v) in map {
                let scaled = match k.as_str() {
                    "left" | "right" | "x" | "width" if v.is_number() => {
                        json!((v.as_f64().unwrap() * sx).round() as i64)
                    }
                    "top" | "bottom" | "y" | "height" if v.is_number() => {
                        json!((v.as_f64().unwrap() * sy).round() as i64)
                    }
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
