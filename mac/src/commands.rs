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
        "active_window" => handle_active_window(),
        "screenshot_window" => handle_screenshot_window(params, config),
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

fn handle_screenshot(
    params: Option<&Value>,
    config: &Config,
) -> Result<Value, String> {
    // NOTE: On macOS, this requires Screen Recording permission in
    // System Preferences > Privacy & Security > Screen Recording.
    // The first time this runs, macOS will prompt the user to grant permission.
    let screens = screenshots::Screen::all().map_err(|e| format!("failed to list screens: {e}"))?;
    let screen = screens
        .first()
        .ok_or_else(|| "no screens found".to_string())?;

    let capture = screen
        .capture()
        .map_err(|e| format!("screenshot failed (ensure Screen Recording permission is granted): {e}"))?;

    let width = capture.width();
    let height = capture.height();
    let raw_pixels = capture.into_raw();
    let img = image::RgbaImage::from_raw(width, height, raw_pixels)
        .ok_or_else(|| "failed to create image from capture".to_string())?;

    // Determine max dimensions from params or config
    let max_w = params
        .and_then(|p| p.get("max_width"))
        .and_then(|v| v.as_u64())
        .map(|v| v as u32)
        .or(config.max_screenshot_width);
    let max_h = params
        .and_then(|p| p.get("max_height"))
        .and_then(|v| v.as_u64())
        .map(|v| v as u32)
        .or(config.max_screenshot_height);

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

fn scale_xy(x: f64, y: f64, params: Option<&Value>, config: &Config) -> Result<(i32, i32), String> {
    let mw = params.and_then(|p| p.get("max_width")).and_then(|v| v.as_f64())
        .or(config.max_screenshot_width.map(|v| v as f64))
        .unwrap_or(0.0);
    let mh = params.and_then(|p| p.get("max_height")).and_then(|v| v.as_f64())
        .or(config.max_screenshot_height.map(|v| v as f64))
        .unwrap_or(0.0);

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
    // NOTE: On macOS, enigo requires Accessibility permission in
    // System Preferences > Privacy & Security > Accessibility.
    Enigo::new(&Settings::default()).map_err(|e| format!("failed to init enigo (ensure Accessibility permission is granted): {e}"))
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
    )?;
    let (end_x, end_y) = scale_xy(
        p.get("endX").and_then(|v| v.as_f64()).ok_or("missing endX")?,
        p.get("endY").and_then(|v| v.as_f64()).ok_or("missing endY")?,
        params,
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

    // Support both direction-based (Android style) and dx/dy based scroll.
    // NOTE: macOS uses natural scrolling by default, so scroll direction
    // may feel inverted compared to Windows/Linux. The enigo crate handles
    // this at the OS level, so we use the same logic as the PC version.
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
    // macOS uses Cmd (Meta) instead of Ctrl
    enigo.key(Key::Meta, Press).map_err(|e| format!("{e}"))?;
    enigo.key(Key::Unicode('a'), Click).map_err(|e| format!("{e}"))?;
    enigo.key(Key::Meta, Release).map_err(|e| format!("{e}"))?;
    Ok(json!({}))
}

fn handle_copy(params: Option<&Value>) -> Result<Value, String> {
    let mut enigo = new_enigo()?;
    // macOS uses Cmd+C
    enigo.key(Key::Meta, Press).map_err(|e| format!("{e}"))?;
    enigo.key(Key::Unicode('c'), Click).map_err(|e| format!("{e}"))?;
    enigo.key(Key::Meta, Release).map_err(|e| format!("{e}"))?;

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
    // macOS uses Cmd+V
    enigo.key(Key::Meta, Press).map_err(|e| format!("{e}"))?;
    enigo.key(Key::Unicode('v'), Click).map_err(|e| format!("{e}"))?;
    enigo.key(Key::Meta, Release).map_err(|e| format!("{e}"))?;
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
    // macOS: Cmd+Left arrow (browser back / general navigation back)
    // This differs from Windows/Linux which uses Alt+Left
    enigo.key(Key::Meta, Press).map_err(|e| format!("{e}"))?;
    enigo
        .key(Key::LeftArrow, Click)
        .map_err(|e| format!("{e}"))?;
    enigo.key(Key::Meta, Release).map_err(|e| format!("{e}"))?;
    Ok(json!({}))
}

fn handle_home() -> Result<Value, String> {
    let mut enigo = new_enigo()?;
    // macOS: Cmd+H hides the current application (closest to "home" behavior).
    // Alternatively, F11 or Cmd+F3 shows desktop via Mission Control.
    // We use Cmd+H as it is the standard macOS "minimize/hide" action.
    enigo.key(Key::Meta, Press).map_err(|e| format!("{e}"))?;
    enigo.key(Key::Unicode('h'), Click).map_err(|e| format!("{e}"))?;
    enigo.key(Key::Meta, Release).map_err(|e| format!("{e}"))?;
    Ok(json!({}))
}

fn handle_recents() -> Result<Value, String> {
    let mut enigo = new_enigo()?;
    // macOS: Cmd+Tab to show application switcher (equivalent of Alt+Tab on Windows)
    enigo.key(Key::Meta, Press).map_err(|e| format!("{e}"))?;
    enigo.key(Key::Tab, Click).map_err(|e| format!("{e}"))?;
    enigo.key(Key::Meta, Release).map_err(|e| format!("{e}"))?;
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
    handle_scroll(params)
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

    for key in &keys {
        enigo.key(*key, Press).map_err(|e| format!("hotkey press failed: {e}"))?;
    }

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

    let mw = params.and_then(|p| p.get("max_width")).and_then(|v| v.as_f64()).or(config.max_screenshot_width.map(|v| v as f64)).unwrap_or(0.0);
    let mh = params.and_then(|p| p.get("max_height")).and_then(|v| v.as_f64()).or(config.max_screenshot_height.map(|v| v as f64)).unwrap_or(0.0);

    if mw > 0.0 || mh > 0.0 {
        let r = if mw > 0.0 && mh > 0.0 {
            (mw / ow as f64).min(mh / oh as f64).min(1.0)
        } else if mw > 0.0 {
            (mw / ow as f64).min(1.0)
        } else {
            (mh / oh as f64).min(1.0)
        };
        Ok(json!({
            "width": (ow as f64 * r) as u32,
            "height": (oh as f64 * r) as u32,
            "original_width": ow,
            "original_height": oh,
            "scaled": true,
        }))
    } else {
        Ok(json!({ "width": ow, "height": oh }))
    }
}

fn handle_list_windows_raw() -> Result<Value, String> {
    // Use osascript to list windows via AppleScript
    let script = r#"
    set output to ""
    tell application "System Events"
        set procs to every process whose visible is true
        repeat with proc in procs
            set procName to name of proc
            try
                set wins to every window of proc
                repeat with w in wins
                    set winName to name of w
                    set {x, y} to position of w
                    set {width, height} to size of w
                    set output to output & procName & "|||" & winName & "|||" & x & "|||" & y & "|||" & width & "|||" & height & linefeed
                end repeat
            end try
        end repeat
    end tell
    return output
    "#;

    let output = std::process::Command::new("osascript")
        .args(["-e", script])
        .output()
        .map_err(|e| format!("osascript failed: {e}"))?;

    if !output.status.success() {
        return Err("failed to list windows — grant Accessibility permission".to_string());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut windows: Vec<Value> = Vec::new();

    for (i, line) in stdout.lines().enumerate() {
        let parts: Vec<&str> = line.split("|||").collect();
        if parts.len() >= 6 {
            let app = parts[0];
            let title = parts[1];
            let x: i32 = parts[2].trim().parse().unwrap_or(0);
            let y: i32 = parts[3].trim().parse().unwrap_or(0);
            let width: i32 = parts[4].trim().parse().unwrap_or(0);
            let height: i32 = parts[5].trim().parse().unwrap_or(0);

            let display_title = if title.is_empty() { app } else { title };
            windows.push(json!({
                "index": i,
                "title": display_title,
                "app": app,
                "x": x,
                "y": y,
                "width": width,
                "height": height,
            }));
        }
    }

    Ok(json!({ "windows": windows }))
}

fn handle_focus_window(params: Option<&Value>) -> Result<Value, String> {
    let p = params.ok_or("missing params")?;
    let target_title = p.get("title").and_then(|v| v.as_str());
    let target_index = p.get("index").and_then(|v| v.as_u64()).map(|v| v as usize);

    if target_title.is_none() && target_index.is_none() {
        return Err("provide either 'title' or 'index' parameter".to_string());
    }

    if let Some(index) = target_index {
        let list_result = handle_list_windows_raw()?;
        let windows = list_result
            .get("windows")
            .and_then(|v| v.as_array())
            .ok_or("failed to list windows")?;
        let app = windows
            .get(index)
            .and_then(|w| w.get("app"))
            .and_then(|t| t.as_str())
            .ok_or_else(|| format!("no window at index {index}"))?;
        let title = windows
            .get(index)
            .and_then(|w| w.get("title"))
            .and_then(|t| t.as_str())
            .unwrap_or(app);

        let script = format!(
            "tell application \"{}\" to activate",
            app.replace('"', "\\\"")
        );
        let _ = std::process::Command::new("osascript")
            .args(["-e", &script])
            .output();
        return Ok(json!({ "focused": title }));
    }

    if let Some(title) = target_title {
        // Try to find matching app in window list
        let list_result = handle_list_windows_raw()?;
        let windows = list_result
            .get("windows")
            .and_then(|v| v.as_array())
            .ok_or("failed to list windows")?;

        let lower = title.to_lowercase();
        let app = windows
            .iter()
            .find_map(|w| {
                let t = w.get("title")?.as_str()?;
                let a = w.get("app")?.as_str()?;
                if t.to_lowercase().contains(&lower) || a.to_lowercase().contains(&lower) {
                    Some(a.to_string())
                } else {
                    None
                }
            })
            .ok_or_else(|| format!("no window matching '{title}'"))?;

        let script = format!(
            "tell application \"{}\" to activate",
            app.replace('"', "\\\"")
        );
        let _ = std::process::Command::new("osascript")
            .args(["-e", &script])
            .output();
        return Ok(json!({ "focused": title }));
    }

    Err("provide either 'title' or 'index' parameter".to_string())
}

fn handle_active_window() -> Result<Value, String> {
    let script = r#"
    tell application "System Events"
        set frontApp to first application process whose frontmost is true
        set appName to name of frontApp
        try
            set frontWin to front window of frontApp
            set winName to name of frontWin
            set {x, y} to position of frontWin
            set {w, h} to size of frontWin
            return appName & "|||" & winName & "|||" & x & "|||" & y & "|||" & w & "|||" & h
        on error
            return appName & "||||||0|||0|||0|||0"
        end try
    end tell
    "#;

    let output = std::process::Command::new("osascript")
        .args(["-e", script])
        .output()
        .map_err(|e| format!("osascript failed: {e}"))?;

    if !output.status.success() {
        return Ok(json!({ "title": null, "active": false }));
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let parts: Vec<&str> = stdout.split("|||").collect();

    if parts.len() >= 6 {
        let app = parts[0];
        let title = if parts[1].is_empty() { app } else { parts[1] };
        let x: i32 = parts[2].parse().unwrap_or(0);
        let y: i32 = parts[3].parse().unwrap_or(0);
        let width: i32 = parts[4].parse().unwrap_or(0);
        let height: i32 = parts[5].parse().unwrap_or(0);

        Ok(json!({
            "title": title,
            "app": app,
            "x": x,
            "y": y,
            "width": width,
            "height": height,
        }))
    } else {
        Ok(json!({ "title": null, "active": false }))
    }
}

fn handle_screenshot_window(params: Option<&Value>, config: &Config) -> Result<Value, String> {
    let p = params.ok_or("missing params")?;
    let target_title = p.get("title").and_then(|v| v.as_str());
    let target_index = p.get("index").and_then(|v| v.as_u64()).map(|v| v as usize);

    if target_title.is_none() && target_index.is_none() {
        return Err("provide either 'title' or 'index' parameter".to_string());
    }

    // Find window title
    let list_result = handle_list_windows_raw()?;
    let windows = list_result
        .get("windows")
        .and_then(|v| v.as_array())
        .ok_or("failed to list windows")?;

    let (target_app, target_win_title) = if let Some(index) = target_index {
        let w = windows.get(index).ok_or_else(|| format!("no window at index {index}"))?;
        let app = w.get("app").and_then(|t| t.as_str()).unwrap_or("");
        let title = w.get("title").and_then(|t| t.as_str()).unwrap_or(app);
        (app.to_string(), title.to_string())
    } else if let Some(title_substr) = target_title {
        let lower = title_substr.to_lowercase();
        windows
            .iter()
            .find_map(|w| {
                let t = w.get("title")?.as_str()?;
                let a = w.get("app")?.as_str()?;
                if t.to_lowercase().contains(&lower) || a.to_lowercase().contains(&lower) {
                    Some((a.to_string(), t.to_string()))
                } else {
                    None
                }
            })
            .ok_or_else(|| format!("no window matching '{title_substr}'"))?
    } else {
        return Err("provide either 'title' or 'index' parameter".to_string());
    };

    // Use screencapture -l to capture specific window
    // First get window ID via CGWindowListCopyWindowInfo
    let temp_path = std::env::temp_dir().join("screenmcp_window.png");

    // Use screencapture with window owner name
    let result = std::process::Command::new("screencapture")
        .args(["-o", "-l", &format!("0"), "-x", temp_path.to_str().unwrap()])
        .output();

    // Fallback: activate window and take full screenshot, then crop
    // Activate the window first
    let script = format!(
        "tell application \"{}\" to activate",
        target_app.replace('"', "\\\"")
    );
    let _ = std::process::Command::new("osascript")
        .args(["-e", &script])
        .output();
    thread::sleep(Duration::from_millis(300));

    // Take screenshot
    let result = std::process::Command::new("screencapture")
        .args(["-x", "-o", temp_path.to_str().unwrap()])
        .output();

    if result.is_err() || !result.as_ref().unwrap().status.success() {
        return Err("screencapture failed".to_string());
    }

    let png_bytes = std::fs::read(&temp_path)
        .map_err(|e| format!("failed to read capture: {e}"))?;
    let _ = std::fs::remove_file(&temp_path);

    let img = image::load_from_memory(&png_bytes)
        .map_err(|e| format!("failed to decode capture: {e}"))?;

    let max_w = p.get("max_width").and_then(|v| v.as_u64()).map(|v| v as u32)
        .or(config.max_screenshot_width);
    let max_h = p.get("max_height").and_then(|v| v.as_u64()).map(|v| v as u32)
        .or(config.max_screenshot_height);

    let img = if let (Some(mw), Some(mh)) = (max_w, max_h) {
        if img.width() > mw || img.height() > mh {
            img.resize(mw, mh, image::imageops::FilterType::Triangle)
        } else { img }
    } else if let Some(mw) = max_w {
        if img.width() > mw {
            let ratio = mw as f64 / img.width() as f64;
            let new_h = (img.height() as f64 * ratio) as u32;
            img.resize_exact(mw, new_h, image::imageops::FilterType::Triangle)
        } else { img }
    } else if let Some(mh) = max_h {
        if img.height() > mh {
            let ratio = mh as f64 / img.height() as f64;
            let new_w = (img.width() as f64 * ratio) as u32;
            img.resize_exact(new_w, mh, image::imageops::FilterType::Triangle)
        } else { img }
    } else { img };

    let rgba = img.to_rgba8();
    let mut buf = Cursor::new(Vec::new());
    WebPEncoder::new_lossless(&mut buf)
        .write_image(rgba.as_raw(), rgba.width(), rgba.height(), image::ExtendedColorType::Rgba8)
        .map_err(|e| format!("WebP encode failed: {e}"))?;

    let b64 = base64::engine::general_purpose::STANDARD.encode(buf.into_inner());

    Ok(json!({
        "image": b64,
        "title": target_win_title,
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

    // Use osascript to request admin privileges
    let script = format!(
        "do shell script \"{}\" with administrator privileges",
        exe_path.to_str().unwrap_or("screenmcp-mac").replace('"', "\\\"")
    );

    let result = std::process::Command::new("osascript")
        .args(["-e", &script])
        .spawn();

    match result {
        Ok(_) => {
            std::thread::spawn(|| {
                std::thread::sleep(std::time::Duration::from_millis(500));
                std::process::exit(0);
            });
            Ok(json!({ "elevating": true }))
        }
        Err(_) => Err("failed to request elevation".to_string()),
    }
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

fn get_output_scale(params: Option<&Value>, config: &Config) -> Result<(f64, f64), String> {
    let mw = params.and_then(|p| p.get("max_width")).and_then(|v| v.as_f64())
        .or(config.max_screenshot_width.map(|v| v as f64))
        .unwrap_or(0.0);
    let mh = params.and_then(|p| p.get("max_height")).and_then(|v| v.as_f64())
        .or(config.max_screenshot_height.map(|v| v as f64))
        .unwrap_or(0.0);
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
                        json!((v.as_f64().unwrap() * sx).round() as i64),
                    "top" | "bottom" | "y" | "height" if v.is_number() =>
                        json!((v.as_f64().unwrap() * sy).round() as i64),
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

/// Get a list of visible windows with titles and positions using the macOS Accessibility API.
/// Requires Accessibility permission in System Preferences > Privacy & Security > Accessibility.
#[cfg(target_os = "macos")]
fn handle_ui_tree_raw() -> Result<Value, String> {
    // Use the macOS CGWindowListCopyWindowInfo API via core-graphics
    // to enumerate visible windows with their titles and bounds.
    use core_foundation::base::{CFType, TCFType};
    use core_foundation::boolean::CFBoolean;
    use core_foundation::dictionary::CFDictionaryRef;
    use core_foundation::number::CFNumber;
    use core_foundation::string::CFString;
    use core_graphics::display::{
        kCGNullWindowID, kCGWindowListOptionOnScreenOnly, CGWindowListCopyWindowInfo,
    };

    let window_list = unsafe {
        CGWindowListCopyWindowInfo(kCGWindowListOptionOnScreenOnly, kCGNullWindowID)
    };

    if window_list.is_null() {
        return Ok(json!({ "tree": [] }));
    }

    let count = unsafe { core_foundation::array::CFArrayGetCount(window_list as _) };
    let mut windows: Vec<Value> = Vec::new();

    for i in 0..count {
        let dict = unsafe {
            core_foundation::array::CFArrayGetValueAtIndex(window_list as _, i) as CFDictionaryRef
        };

        if dict.is_null() {
            continue;
        }

        // Helper to get a string value from the dictionary
        let get_string = |key: &str| -> Option<String> {
            let cf_key = CFString::new(key);
            let mut value: *const std::ffi::c_void = std::ptr::null();
            let found = unsafe {
                core_foundation::dictionary::CFDictionaryGetValueIfPresent(
                    dict,
                    cf_key.as_concrete_TypeRef() as _,
                    &mut value,
                )
            };
            if found != 0 && !value.is_null() {
                let cf_str = unsafe { CFString::wrap_under_get_rule(value as _) };
                Some(cf_str.to_string())
            } else {
                None
            }
        };

        // Helper to get a number value from the dictionary
        let get_number = |key: &str| -> Option<i64> {
            let cf_key = CFString::new(key);
            let mut value: *const std::ffi::c_void = std::ptr::null();
            let found = unsafe {
                core_foundation::dictionary::CFDictionaryGetValueIfPresent(
                    dict,
                    cf_key.as_concrete_TypeRef() as _,
                    &mut value,
                )
            };
            if found != 0 && !value.is_null() {
                let cf_num = unsafe { CFNumber::wrap_under_get_rule(value as _) };
                cf_num.to_i64()
            } else {
                None
            }
        };

        let owner_name = get_string("kCGWindowOwnerName").unwrap_or_default();
        let window_name = get_string("kCGWindowName").unwrap_or_default();
        let window_layer = get_number("kCGWindowLayer").unwrap_or(0);

        // Skip windows on layers other than 0 (desktop layer) to avoid menu bar items, etc.
        if window_layer != 0 {
            continue;
        }

        // Skip windows with no owner name
        if owner_name.is_empty() {
            continue;
        }

        // Get window bounds from the kCGWindowBounds dictionary
        let bounds_key = CFString::new("kCGWindowBounds");
        let mut bounds_value: *const std::ffi::c_void = std::ptr::null();
        let has_bounds = unsafe {
            core_foundation::dictionary::CFDictionaryGetValueIfPresent(
                dict,
                bounds_key.as_concrete_TypeRef() as _,
                &mut bounds_value,
            )
        };

        let (x, y, width, height) = if has_bounds != 0 && !bounds_value.is_null() {
            let bounds_dict = bounds_value as CFDictionaryRef;

            let get_bounds_num = |key: &str| -> f64 {
                let cf_key = CFString::new(key);
                let mut val: *const std::ffi::c_void = std::ptr::null();
                let found = unsafe {
                    core_foundation::dictionary::CFDictionaryGetValueIfPresent(
                        bounds_dict,
                        cf_key.as_concrete_TypeRef() as _,
                        &mut val,
                    )
                };
                if found != 0 && !val.is_null() {
                    let cf_num = unsafe { CFNumber::wrap_under_get_rule(val as _) };
                    cf_num.to_f64().unwrap_or(0.0)
                } else {
                    0.0
                }
            };

            (
                get_bounds_num("X") as i64,
                get_bounds_num("Y") as i64,
                get_bounds_num("Width") as i64,
                get_bounds_num("Height") as i64,
            )
        } else {
            (0, 0, 0, 0)
        };

        let window_id = get_number("kCGWindowNumber").unwrap_or(0);

        let title = if window_name.is_empty() {
            owner_name.clone()
        } else {
            format!("{owner_name} - {window_name}")
        };

        // Build sparse node: only include non-empty / non-default values
        let mut node = json!({});
        let m = node.as_object_mut().unwrap();

        if !title.is_empty() {
            m.insert("text".into(), json!(title));
        }
        if !window_name.is_empty() {
            m.insert("className".into(), json!(window_name));
        }

        // Bounds in standardized format
        if width != 0 || height != 0 || x != 0 || y != 0 {
            m.insert("bounds".into(), json!({
                "left": x,
                "top": y,
                "right": x + width,
                "bottom": y + height,
                "width": width,
                "height": height,
            }));
        }

        if window_id != 0 {
            m.insert("hWnd".into(), json!(window_id));
        }

        windows.push(node);
    }

    // Release the window list
    unsafe {
        core_foundation::base::CFRelease(window_list as _);
    }

    Ok(json!({ "os": "macos", "tree": windows }))
}

#[cfg(not(target_os = "macos"))]
fn handle_ui_tree_raw() -> Result<Value, String> {
    Err("ui_tree requires macOS".to_string())
}
