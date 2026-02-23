use base64::Engine;
use enigo::{
    Button, Coordinate,
    Direction::{Click, Press, Release},
    Enigo, Key, Keyboard, Mouse, Settings,
};
use image::codecs::png::PngEncoder;
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
        "play_audio" => return json!({"status": "ok", "unsupported": true}),
        "hold_key" => handle_hold_key(params),
        "release_key" => handle_release_key(params),
        "press_key" => handle_press_key(params),
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

    // Encode as PNG
    let mut buf = Cursor::new(Vec::new());
    PngEncoder::new(&mut buf)
        .write_image(
            img.as_raw(),
            img.width(),
            img.height(),
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|e| format!("PNG encode failed: {e}"))?;

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
    // NOTE: On macOS, enigo requires Accessibility permission in
    // System Preferences > Privacy & Security > Accessibility.
    Enigo::new(&Settings::default()).map_err(|e| format!("failed to init enigo (ensure Accessibility permission is granted): {e}"))
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

    // Support both direction-based (Android style) and dx/dy based scroll.
    // NOTE: macOS uses natural scrolling by default, so scroll direction
    // may feel inverted compared to Windows/Linux. The enigo crate handles
    // this at the OS level, so we use the same logic as the PC version.
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

/// Get a list of visible windows with titles and positions using the macOS Accessibility API.
/// Requires Accessibility permission in System Preferences > Privacy & Security > Accessibility.
#[cfg(target_os = "macos")]
fn handle_ui_tree() -> Result<Value, String> {
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

        windows.push(json!({
            "title": title,
            "app": owner_name,
            "windowName": window_name,
            "x": x,
            "y": y,
            "width": width,
            "height": height,
            "windowId": window_id,
        }));
    }

    // Release the window list
    unsafe {
        core_foundation::base::CFRelease(window_list as _);
    }

    Ok(json!({ "tree": windows }))
}

#[cfg(not(target_os = "macos"))]
fn handle_ui_tree() -> Result<Value, String> {
    // Fallback for non-macOS builds (e.g., cross-compilation testing)
    Ok(json!({ "tree": [], "note": "ui_tree requires macOS" }))
}
