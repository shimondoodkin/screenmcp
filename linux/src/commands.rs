use base64::Engine;
use enigo::{
    Button, Coordinate,
    Direction::{Click, Press, Release},
    Enigo, Key, Keyboard, Mouse, Settings,
};
use image::codecs::png::PngEncoder;
use image::ImageEncoder;
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
        "copy" => handle_copy(),
        "paste" => handle_paste(),
        "back" => handle_back(),
        "home" => handle_home(),
        "recents" => handle_recents(),
        "ui_tree" => handle_ui_tree(),
        "camera" => {
            return json!({
                "id": id,
                "status": "ok",
                "result": { "unsupported": true }
            });
        }
        "right_click" => handle_right_click(params),
        "middle_click" => handle_middle_click(params),
        "mouse_scroll" => handle_mouse_scroll(params),
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
    let screens = screenshots::Screen::all().map_err(|e| format!("failed to list screens: {e}"))?;
    let screen = screens
        .first()
        .ok_or_else(|| "no screens found".to_string())?;

    let capture = screen
        .capture()
        .map_err(|e| format!("screenshot failed: {e}"))?;

    let rgba_data = capture.rgba();
    let width = capture.width();
    let height = capture.height();

    let img = image::RgbaImage::from_raw(width, height, rgba_data.to_vec())
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
    enigo.key(Key::Control, Press).map_err(|e| format!("{e}"))?;
    enigo.key(Key::Unicode('a'), Click).map_err(|e| format!("{e}"))?;
    enigo.key(Key::Control, Release).map_err(|e| format!("{e}"))?;
    Ok(json!({}))
}

fn handle_copy() -> Result<Value, String> {
    let mut enigo = new_enigo()?;
    enigo.key(Key::Control, Press).map_err(|e| format!("{e}"))?;
    enigo.key(Key::Unicode('c'), Click).map_err(|e| format!("{e}"))?;
    enigo.key(Key::Control, Release).map_err(|e| format!("{e}"))?;
    Ok(json!({}))
}

fn handle_paste() -> Result<Value, String> {
    let mut enigo = new_enigo()?;
    enigo.key(Key::Control, Press).map_err(|e| format!("{e}"))?;
    enigo.key(Key::Unicode('v'), Click).map_err(|e| format!("{e}"))?;
    enigo.key(Key::Control, Release).map_err(|e| format!("{e}"))?;
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

/// Get list of windows with titles and positions using wmctrl.
/// Falls back to an empty list if wmctrl is not installed.
fn handle_ui_tree() -> Result<Value, String> {
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

                    windows.push(json!({
                        "title": title,
                        "x": x,
                        "y": y,
                        "width": width,
                        "height": height,
                        "windowId": win_id,
                    }));
                }
            }

            Ok(json!({ "tree": windows }))
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

                        windows.push(json!({
                            "title": title,
                            "x": x,
                            "y": y,
                            "width": width,
                            "height": height,
                            "windowId": win_id,
                        }));
                    }

                    Ok(json!({ "tree": windows }))
                }
                _ => {
                    Ok(json!({ "tree": [], "note": "install wmctrl or xdotool for window listing" }))
                }
            }
        }
    }
}
