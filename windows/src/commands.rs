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
        "click" => handle_click(params),
        "long_click" => handle_long_click(params),
        "drag" => handle_drag(params),
        "scroll" => handle_scroll(params),
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
        "ui_tree" => handle_ui_tree(),
        "camera" => handle_camera(params),
        "list_cameras" => handle_list_cameras(),
        "right_click" => handle_right_click(params),
        "middle_click" => handle_middle_click(params),
        "mouse_scroll" => handle_mouse_scroll(params),
        "hold_key" => handle_hold_key(params),
        "release_key" => handle_release_key(params),
        "press_key" => handle_press_key(params),
        "play_audio" => handle_play_audio(params),
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

fn get_xy(params: Option<&Value>) -> Result<(i32, i32), String> {
    let p = params.ok_or("missing params")?;
    let x = p
        .get("x")
        .and_then(|v| v.as_f64())
        .ok_or("missing x")? as i32;
    let y = p
        .get("y")
        .and_then(|v| v.as_f64())
        .ok_or("missing y")? as i32;
    Ok((x, y))
}

fn new_enigo() -> Result<Enigo, String> {
    Enigo::new(&Settings::default()).map_err(|e| format!("failed to init enigo: {e}"))
}

fn handle_click(params: Option<&Value>) -> Result<Value, String> {
    let (x, y) = get_xy(params)?;
    let mut enigo = new_enigo()?;
    enigo
        .move_mouse(x, y, Coordinate::Abs)
        .map_err(|e| format!("move_mouse failed: {e}"))?;
    enigo
        .button(Button::Left, Click)
        .map_err(|e| format!("click failed: {e}"))?;
    Ok(json!({}))
}

fn handle_long_click(params: Option<&Value>) -> Result<Value, String> {
    let (x, y) = get_xy(params)?;
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

fn handle_drag(params: Option<&Value>) -> Result<Value, String> {
    let p = params.ok_or("missing params")?;
    let start_x = p.get("startX").and_then(|v| v.as_f64()).ok_or("missing startX")? as i32;
    let start_y = p.get("startY").and_then(|v| v.as_f64()).ok_or("missing startY")? as i32;
    let end_x = p.get("endX").and_then(|v| v.as_f64()).ok_or("missing endX")? as i32;
    let end_y = p.get("endY").and_then(|v| v.as_f64()).ok_or("missing endY")? as i32;
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

fn handle_scroll(params: Option<&Value>) -> Result<Value, String> {
    let p = params.ok_or("missing params")?;

    // Support both direction-based (Android style) and dx/dy based scroll
    let mut enigo = new_enigo()?;

    // If x,y provided, move mouse there first
    if let (Some(x), Some(y)) = (
        p.get("x").and_then(|v| v.as_f64()),
        p.get("y").and_then(|v| v.as_f64()),
    ) {
        enigo
            .move_mouse(x as i32, y as i32, Coordinate::Abs)
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

fn handle_right_click(params: Option<&Value>) -> Result<Value, String> {
    let (x, y) = get_xy(params)?;
    let mut enigo = new_enigo()?;
    enigo
        .move_mouse(x, y, Coordinate::Abs)
        .map_err(|e| format!("move_mouse failed: {e}"))?;
    enigo
        .button(Button::Right, Click)
        .map_err(|e| format!("right click failed: {e}"))?;
    Ok(json!({}))
}

fn handle_middle_click(params: Option<&Value>) -> Result<Value, String> {
    let (x, y) = get_xy(params)?;
    let mut enigo = new_enigo()?;
    enigo
        .move_mouse(x, y, Coordinate::Abs)
        .map_err(|e| format!("move_mouse failed: {e}"))?;
    enigo
        .button(Button::Middle, Click)
        .map_err(|e| format!("middle click failed: {e}"))?;
    Ok(json!({}))
}

fn handle_mouse_scroll(params: Option<&Value>) -> Result<Value, String> {
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
fn handle_ui_tree() -> Result<Value, String> {
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
fn handle_ui_tree() -> Result<Value, String> {
    Err("ui_tree is not supported on this platform".to_string())
}
