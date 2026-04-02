# Windows Local Mode Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an embedded HTTP server to the Windows client that serves both a REST API (`POST /command`) and MCP Streamable HTTP (`/mcp`) for local AI assistant and script control without needing the worker/relay.

**Architecture:** The Windows tray app embeds an axum HTTP server on `127.0.0.1:6767` (configurable). It serves two interfaces: a plain REST endpoint for scripts and an MCP Streamable HTTP endpoint for AI assistants. New commands (hotkey, mouse_move, double_click, list_windows, focus_window, get_screen_size, elevate, is_elevated) are added to the command handler. A tray settings window manages the local mode key and port.

**Tech Stack:** Rust (axum, tokio, serde_json), existing enigo/screenshots/arboard/windows crates, MCP Streamable HTTP (hand-rolled JSON-RPC 2.0 over HTTP+SSE)

---

### Task 1: Add Config Fields

**Files:**
- Modify: `windows/src/config.rs`

- [ ] **Step 1: Add local mode fields to Config struct**

In `windows/src/config.rs`, add two fields to the `Config` struct after `opensource_api_url`:

```rust
    /// Local mode API key (empty = disabled)
    #[serde(default)]
    pub local_mode_key: String,

    /// Local mode HTTP port
    #[serde(default = "default_local_mode_port")]
    pub local_mode_port: u16,
```

Add the default function:

```rust
fn default_local_mode_port() -> u16 {
    6767
}
```

Add the fields to `Default` impl:

```rust
            local_mode_key: String::new(),
            local_mode_port: default_local_mode_port(),
```

- [ ] **Step 2: Build and verify**

Run: `cd windows && cargo build 2>&1 | tail -5`
Expected: Compiles successfully.

- [ ] **Step 3: Commit**

```bash
git add windows/src/config.rs
git commit -m "feat(windows): add local_mode_key and local_mode_port config fields"
```

---

### Task 2: Add New Commands — mouse_move, double_click, hotkey, get_screen_size

**Files:**
- Modify: `windows/src/commands.rs`

- [ ] **Step 1: Add command dispatch entries**

In `execute_command` match block, add before the `_ =>` arm:

```rust
        "mouse_move" => handle_mouse_move(params),
        "double_click" => handle_double_click(params),
        "hotkey" => handle_hotkey(params),
        "get_screen_size" => handle_get_screen_size(),
```

- [ ] **Step 2: Implement mouse_move**

Add after `handle_middle_click`:

```rust
fn handle_mouse_move(params: Option<&Value>) -> Result<Value, String> {
    let (x, y) = get_xy(params)?;
    let mut enigo = new_enigo()?;
    enigo
        .move_mouse(x, y, Coordinate::Abs)
        .map_err(|e| format!("move_mouse failed: {e}"))?;
    Ok(json!({}))
}
```

- [ ] **Step 3: Implement double_click**

```rust
fn handle_double_click(params: Option<&Value>) -> Result<Value, String> {
    let (x, y) = get_xy(params)?;
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
```

- [ ] **Step 4: Implement hotkey**

```rust
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
```

- [ ] **Step 5: Implement get_screen_size**

```rust
fn handle_get_screen_size() -> Result<Value, String> {
    let screens = screenshots::Screen::all().map_err(|e| format!("failed to list screens: {e}"))?;
    let screen = screens
        .first()
        .ok_or_else(|| "no screens found".to_string())?;
    let info = screen.display_info;
    Ok(json!({
        "width": info.width,
        "height": info.height,
        "x": info.x,
        "y": info.y,
    }))
}
```

- [ ] **Step 6: Build and verify**

Run: `cd windows && cargo build 2>&1 | tail -5`
Expected: Compiles successfully.

- [ ] **Step 7: Commit**

```bash
git add windows/src/commands.rs
git commit -m "feat(windows): add mouse_move, double_click, hotkey, get_screen_size commands"
```

---

### Task 3: Add New Commands — list_windows, focus_window

**Files:**
- Modify: `windows/src/commands.rs`

- [ ] **Step 1: Add command dispatch entries**

In `execute_command` match block, add:

```rust
        "list_windows" => handle_list_windows(),
        "focus_window" => handle_focus_window(params),
```

- [ ] **Step 2: Implement list_windows**

Add the `#[cfg(windows)]` implementation. This uses Win32 `EnumWindows` and applies the same filtering as `ui_tree` (offscreen check, viewport bounds):

```rust
#[cfg(windows)]
fn handle_list_windows() -> Result<Value, String> {
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

    let windows: Arc<std::sync::Mutex<Vec<WindowInfo>>> = Arc::new(std::sync::Mutex::new(Vec::new()));
    let windows_clone = windows.clone();
    let viewport = (vx, vy, vx + vw, vy + vh);

    unsafe extern "system" fn enum_callback(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let data = &*(lparam.0 as *const (Arc<std::sync::Mutex<Vec<WindowInfo>>>, (i32, i32, i32, i32)));
        let (windows, viewport) = data;

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
        let mut rect = std::mem::zeroed::<RECT>();
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
        if rect.right <= viewport.0 || rect.left >= viewport.2
            || rect.bottom <= viewport.1 || rect.top >= viewport.3
        {
            return TRUE;
        }

        let is_minimized = IsIconic(hwnd).as_bool();
        let is_maximized = IsZoomed(hwnd).as_bool();

        windows.lock().unwrap().push(WindowInfo {
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

    let data = (windows_clone, viewport);
    unsafe {
        let _ = EnumWindows(
            Some(enum_callback),
            LPARAM(&data as *const _ as isize),
        );
    }

    let windows = windows.lock().unwrap();
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
fn handle_list_windows() -> Result<Value, String> {
    Err("list_windows is only supported on Windows".to_string())
}
```

- [ ] **Step 3: Implement focus_window**

```rust
#[cfg(windows)]
fn handle_focus_window(params: Option<&Value>) -> Result<Value, String> {
    use windows::Win32::Foundation::{BOOL, HWND, LPARAM, TRUE};
    use windows::Win32::UI::WindowsAndMessaging::*;

    let p = params.ok_or("missing params")?;

    // Get target by title substring or index
    let target_title = p.get("title").and_then(|v| v.as_str());
    let target_index = p.get("index").and_then(|v| v.as_u64()).map(|v| v as usize);

    if target_title.is_none() && target_index.is_none() {
        return Err("provide either 'title' or 'index' parameter".to_string());
    }

    // Use list_windows logic to find the target
    let list_result = handle_list_windows()?;
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

    // Now find the HWND and bring it to front
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
                return BOOL(0); // Stop enumeration
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
        // Restore if minimized
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
```

- [ ] **Step 4: Build and verify**

Run: `cd windows && cargo build 2>&1 | tail -5`
Expected: Compiles successfully.

- [ ] **Step 5: Commit**

```bash
git add windows/src/commands.rs
git commit -m "feat(windows): add list_windows and focus_window commands"
```

---

### Task 4: Add New Commands — elevate, is_elevated

**Files:**
- Modify: `windows/src/commands.rs`

- [ ] **Step 1: Add command dispatch entries**

In `execute_command` match block, add:

```rust
        "elevate" => handle_elevate(),
        "is_elevated" => handle_is_elevated(),
```

- [ ] **Step 2: Implement is_elevated**

```rust
#[cfg(windows)]
fn is_process_elevated() -> bool {
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
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

fn handle_is_elevated() -> Result<Value, String> {
    Ok(json!({ "elevated": is_process_elevated() }))
}
```

- [ ] **Step 3: Implement elevate**

The `elevate` command needs to show a native Windows message box for user confirmation, then relaunch itself as admin via `ShellExecuteW` with `runas` verb. Since `execute_command` runs in a blocking thread, the message box call is fine here.

```rust
#[cfg(windows)]
fn handle_elevate() -> Result<Value, String> {
    if is_process_elevated() {
        return Ok(json!({ "already_elevated": true }));
    }

    use windows::Win32::UI::WindowsAndMessaging::*;
    use windows::core::w;

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
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::core::PCWSTR;

    let exe_path = std::env::current_exe()
        .map_err(|e| format!("failed to get exe path: {e}"))?;
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

    // ShellExecuteW returns > 32 on success
    if result.0 as usize > 32 {
        // Exit current (non-elevated) process after a short delay
        // to allow the response to be sent back
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
```

- [ ] **Step 4: Add required Windows API features to Cargo.toml**

In `windows/Cargo.toml`, update the windows dependency features to include the additional APIs:

```toml
[target.'cfg(windows)'.dependencies]
windows = { version = "0.58", features = [
    "Win32_Foundation",
    "Win32_UI_WindowsAndMessaging",
    "Win32_UI_Accessibility",
    "Win32_UI_Shell",
    "Win32_Graphics_Gdi",
    "Win32_System_Com",
    "Win32_System_Threading",
    "Win32_Security",
] }
```

- [ ] **Step 5: Build and verify**

Run: `cd windows && cargo build 2>&1 | tail -5`
Expected: Compiles successfully.

- [ ] **Step 6: Commit**

```bash
git add windows/src/commands.rs windows/Cargo.toml
git commit -m "feat(windows): add elevate and is_elevated commands"
```

---

### Task 5: Add axum Dependency and Local HTTP Server Module

**Files:**
- Modify: `windows/Cargo.toml`
- Create: `windows/src/local_server.rs`
- Modify: `windows/src/main.rs`

- [ ] **Step 1: Add axum dependency**

In `windows/Cargo.toml`, add to `[dependencies]`:

```toml
axum = "0.8"
```

- [ ] **Step 2: Create local_server.rs with REST endpoint**

Create `windows/src/local_server.rs`:

```rust
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

use crate::commands;
use crate::config::Config;

#[derive(Clone)]
pub struct AppState {
    config: Arc<RwLock<Config>>,
}

fn verify_auth(headers: &HeaderMap, expected_key: &str) -> Result<(), StatusCode> {
    let auth = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if let Some(token) = auth.strip_prefix("Bearer ") {
        if token == expected_key {
            return Ok(());
        }
    }

    Err(StatusCode::UNAUTHORIZED)
}

async fn handle_command(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    let config = state.config.read().await;
    verify_auth(&headers, &config.local_mode_key).map_err(|s| {
        (s, Json(json!({"status": "error", "error": "unauthorized"})))
    })?;

    let cmd = body
        .get("cmd")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let params = body.get("params").cloned();

    if cmd.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"status": "error", "error": "missing cmd field"})),
        ));
    }

    let config_clone = config.clone();
    drop(config);

    info!("local: received command: {cmd}");

    let response = tokio::task::spawn_blocking(move || {
        commands::execute_command(0, &cmd, params.as_ref(), &config_clone)
    })
    .await
    .unwrap_or_else(|e| {
        json!({
            "status": "error",
            "error": format!("command panicked: {e}")
        })
    });

    // Strip the id field from the response (local mode doesn't use ids)
    let mut resp = response;
    if let Some(obj) = resp.as_object_mut() {
        obj.remove("id");
    }

    Ok(Json(resp))
}

async fn handle_health() -> impl IntoResponse {
    Json(json!({"status": "ok", "service": "screenmcp-local"}))
}

pub async fn run_local_server(config: Config) {
    if config.local_mode_key.is_empty() {
        info!("local mode disabled (no key configured)");
        return;
    }

    let port = config.local_mode_port;
    let state = AppState {
        config: Arc::new(RwLock::new(config)),
    };

    let app = Router::new()
        .route("/command", post(handle_command))
        .route("/health", get(handle_health))
        .with_state(state);

    let addr = format!("127.0.0.1:{port}");
    info!("local mode: starting HTTP server on {addr}");

    let listener = match tokio::net::TcpListener::bind(&addr).await {
        Ok(l) => l,
        Err(e) => {
            error!("local mode: failed to bind {addr}: {e}");
            return;
        }
    };

    if let Err(e) = axum::serve(listener, app).await {
        error!("local mode: server error: {e}");
    }
}
```

- [ ] **Step 3: Wire up local_server in main.rs**

In `windows/src/main.rs`, add the module declaration:

```rust
mod local_server;
```

Inside the `rt.block_on(async { ... })` block, before the `ws::run_ws_manager` call, add:

```rust
            // Start local mode HTTP server if key is configured
            if !config_clone.local_mode_key.is_empty() {
                let local_config = config_clone.clone();
                tokio::spawn(async move {
                    local_server::run_local_server(local_config).await;
                });
            }
```

- [ ] **Step 4: Build and verify**

Run: `cd windows && cargo build 2>&1 | tail -5`
Expected: Compiles successfully.

- [ ] **Step 5: Commit**

```bash
git add windows/src/local_server.rs windows/src/main.rs windows/Cargo.toml
git commit -m "feat(windows): add local mode HTTP server with POST /command endpoint"
```

---

### Task 6: Add MCP Streamable HTTP Endpoint

**Files:**
- Modify: `windows/src/local_server.rs`

- [ ] **Step 1: Add MCP handler functions**

Add the MCP Streamable HTTP implementation to `local_server.rs`. This handles `initialize`, `tools/list`, and `tools/call` JSON-RPC methods:

```rust
use axum::http::header;
use std::sync::atomic::{AtomicI64, Ordering};

static SESSION_COUNTER: AtomicI64 = AtomicI64::new(0);

fn mcp_tool_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "screenshot",
            "description": "Take a screenshot of the screen. Returns base64 WebP image.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "quality": {"type": "integer", "description": "Image quality 1-100 (default: 100)"},
                    "max_width": {"type": "integer", "description": "Max width for scaling"},
                    "max_height": {"type": "integer", "description": "Max height for scaling"}
                }
            }
        }),
        json!({
            "name": "ui_tree",
            "description": "Get the accessibility tree of the current screen. Returns UI nodes with bounds, text, clickable state.",
            "inputSchema": {"type": "object", "properties": {}}
        }),
        json!({
            "name": "click",
            "description": "Click at screen coordinates",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "x": {"type": "integer", "description": "X coordinate"},
                    "y": {"type": "integer", "description": "Y coordinate"},
                    "duration": {"type": "integer", "description": "Press duration in ms (default: 100)"}
                },
                "required": ["x", "y"]
            }
        }),
        json!({
            "name": "right_click",
            "description": "Right-click at screen coordinates",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "x": {"type": "integer", "description": "X coordinate"},
                    "y": {"type": "integer", "description": "Y coordinate"}
                },
                "required": ["x", "y"]
            }
        }),
        json!({
            "name": "double_click",
            "description": "Double-click at screen coordinates",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "x": {"type": "integer", "description": "X coordinate"},
                    "y": {"type": "integer", "description": "Y coordinate"}
                },
                "required": ["x", "y"]
            }
        }),
        json!({
            "name": "middle_click",
            "description": "Middle-click at screen coordinates",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "x": {"type": "integer", "description": "X coordinate"},
                    "y": {"type": "integer", "description": "Y coordinate"}
                },
                "required": ["x", "y"]
            }
        }),
        json!({
            "name": "long_click",
            "description": "Long press at coordinates (default 1000ms)",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "x": {"type": "integer", "description": "X coordinate"},
                    "y": {"type": "integer", "description": "Y coordinate"},
                    "duration": {"type": "integer", "description": "Press duration in ms (default: 1000)"}
                },
                "required": ["x", "y"]
            }
        }),
        json!({
            "name": "drag",
            "description": "Drag from one point to another",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "startX": {"type": "integer"},
                    "startY": {"type": "integer"},
                    "endX": {"type": "integer"},
                    "endY": {"type": "integer"},
                    "duration": {"type": "integer", "description": "Duration in ms (default: 300)"}
                },
                "required": ["startX", "startY", "endX", "endY"]
            }
        }),
        json!({
            "name": "scroll",
            "description": "Scroll the screen at coordinates",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "x": {"type": "integer", "description": "X coordinate"},
                    "y": {"type": "integer", "description": "Y coordinate"},
                    "dx": {"type": "integer", "description": "Horizontal delta"},
                    "dy": {"type": "integer", "description": "Vertical delta (negative = scroll content up)"},
                    "direction": {"type": "string", "description": "Alternative: up/down/left/right"},
                    "amount": {"type": "integer", "description": "Scroll amount (used with direction, default: 3)"}
                }
            }
        }),
        json!({
            "name": "mouse_move",
            "description": "Move mouse cursor to coordinates without clicking",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "x": {"type": "integer", "description": "X coordinate"},
                    "y": {"type": "integer", "description": "Y coordinate"}
                },
                "required": ["x", "y"]
            }
        }),
        json!({
            "name": "mouse_scroll",
            "description": "Mouse wheel scroll at coordinates",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "x": {"type": "integer"},
                    "y": {"type": "integer"},
                    "dx": {"type": "integer"},
                    "dy": {"type": "integer"}
                }
            }
        }),
        json!({
            "name": "type",
            "description": "Type text into the currently focused input field",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "text": {"type": "string", "description": "Text to type"}
                },
                "required": ["text"]
            }
        }),
        json!({
            "name": "press_key",
            "description": "Press and release a single key",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "key": {"type": "string", "description": "Key name: shift, ctrl, alt, meta/win, tab, enter, escape, space, backspace, delete, home, end, pageup, pagedown, up, down, left, right, f1-f12, or a single character"}
                },
                "required": ["key"]
            }
        }),
        json!({
            "name": "hold_key",
            "description": "Press and hold a key down. Use with release_key for manual key sequences.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "key": {"type": "string", "description": "Key name to hold"}
                },
                "required": ["key"]
            }
        }),
        json!({
            "name": "release_key",
            "description": "Release a held key",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "key": {"type": "string", "description": "Key name to release"}
                },
                "required": ["key"]
            }
        }),
        json!({
            "name": "hotkey",
            "description": "Press a key combination atomically. Example: [\"ctrl\", \"c\"] for copy, [\"alt\", \"tab\"] to switch windows, [\"win\"] for start menu.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "keys": {
                        "type": "array",
                        "items": {"type": "string"},
                        "description": "Array of key names to press simultaneously"
                    }
                },
                "required": ["keys"]
            }
        }),
        json!({
            "name": "get_text",
            "description": "Get text from the currently focused input field (reads clipboard)",
            "inputSchema": {"type": "object", "properties": {}}
        }),
        json!({
            "name": "select_all",
            "description": "Select all text in the focused field (Ctrl+A)",
            "inputSchema": {"type": "object", "properties": {}}
        }),
        json!({
            "name": "copy",
            "description": "Copy selected text (Ctrl+C). Optionally return the copied text.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "return_text": {"type": "boolean", "description": "If true, return the copied text (default: false)"}
                }
            }
        }),
        json!({
            "name": "paste",
            "description": "Paste into focused field (Ctrl+V). Optionally set clipboard first.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "text": {"type": "string", "description": "Text to set in clipboard before pasting. If omitted, pastes current clipboard."}
                }
            }
        }),
        json!({
            "name": "get_clipboard",
            "description": "Get the current clipboard text contents",
            "inputSchema": {"type": "object", "properties": {}}
        }),
        json!({
            "name": "set_clipboard",
            "description": "Set the clipboard to the given text",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "text": {"type": "string", "description": "Text to put in clipboard"}
                },
                "required": ["text"]
            }
        }),
        json!({
            "name": "get_screen_size",
            "description": "Get the primary screen dimensions",
            "inputSchema": {"type": "object", "properties": {}}
        }),
        json!({
            "name": "list_windows",
            "description": "List visible on-screen windows with title, position, size, and state",
            "inputSchema": {"type": "object", "properties": {}}
        }),
        json!({
            "name": "focus_window",
            "description": "Bring a window to the foreground by title substring or index from list_windows",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "title": {"type": "string", "description": "Window title substring to match"},
                    "index": {"type": "integer", "description": "Window index from list_windows"}
                }
            }
        }),
        json!({
            "name": "back",
            "description": "Press back (Alt+Left)",
            "inputSchema": {"type": "object", "properties": {}}
        }),
        json!({
            "name": "home",
            "description": "Press Windows key (Start menu)",
            "inputSchema": {"type": "object", "properties": {}}
        }),
        json!({
            "name": "recents",
            "description": "Show recent windows (Alt+Tab)",
            "inputSchema": {"type": "object", "properties": {}}
        }),
        json!({
            "name": "camera",
            "description": "Capture a photo from a connected camera",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "camera": {"type": "string", "description": "Camera ID (use list_cameras). Default: 0"},
                    "quality": {"type": "integer", "description": "Image quality 1-100 (default: 80)"},
                    "max_width": {"type": "integer"},
                    "max_height": {"type": "integer"}
                }
            }
        }),
        json!({
            "name": "list_cameras",
            "description": "List available cameras on the device",
            "inputSchema": {"type": "object", "properties": {}}
        }),
        json!({
            "name": "elevate",
            "description": "Request administrator privileges. Shows confirmation dialog to user. If approved, app relaunches elevated.",
            "inputSchema": {"type": "object", "properties": {}}
        }),
        json!({
            "name": "is_elevated",
            "description": "Check if the app is running with administrator privileges",
            "inputSchema": {"type": "object", "properties": {}}
        }),
        json!({
            "name": "play_audio",
            "description": "Play audio from base64-encoded WAV or MP3 data",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "audio_data": {"type": "string", "description": "Base64-encoded audio data (WAV or MP3)"},
                    "volume": {"type": "number", "description": "Volume 0.0-1.0 (default: 1.0)"}
                },
                "required": ["audio_data"]
            }
        }),
    ]
}

async fn handle_mcp_post(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    let config = state.config.read().await;
    if let Err(s) = verify_auth(&headers, &config.local_mode_key) {
        return (
            s,
            [(header::CONTENT_TYPE, "application/json")],
            Json(json!({
                "jsonrpc": "2.0",
                "error": {"code": -32000, "message": "unauthorized"},
                "id": body.get("id").cloned().unwrap_or(Value::Null)
            })),
        )
            .into_response();
    }

    let id = body.get("id").cloned().unwrap_or(Value::Null);
    let method = body
        .get("method")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let response = match method {
        "initialize" => {
            json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "protocolVersion": "2025-03-26",
                    "capabilities": {
                        "tools": {}
                    },
                    "serverInfo": {
                        "name": "screenmcp-local",
                        "version": env!("CARGO_PKG_VERSION")
                    }
                }
            })
        }
        "notifications/initialized" => {
            // Client acknowledgment, no response needed for notifications
            return (StatusCode::ACCEPTED, Json(json!({}))).into_response();
        }
        "tools/list" => {
            json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "tools": mcp_tool_definitions()
                }
            })
        }
        "tools/call" => {
            let params = body.get("params").cloned().unwrap_or(json!({}));
            let tool_name = params
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let tool_args = params
                .get("arguments")
                .cloned()
                .unwrap_or(json!({}));

            let config_clone = config.clone();
            drop(config);

            let result = tokio::task::spawn_blocking(move || {
                commands::execute_command(0, &tool_name, Some(&tool_args), &config_clone)
            })
            .await
            .unwrap_or_else(|e| json!({"status": "error", "error": format!("panicked: {e}")}));

            let status = result.get("status").and_then(|v| v.as_str()).unwrap_or("error");
            let is_error = status == "error";

            // For screenshot/camera, return image as embedded resource
            let content = if let Some(image_b64) = result.get("result").and_then(|r| r.get("image")).and_then(|v| v.as_str()) {
                json!([
                    {
                        "type": "image",
                        "data": image_b64,
                        "mimeType": "image/webp"
                    }
                ])
            } else if is_error {
                let error_msg = result.get("error").and_then(|v| v.as_str()).unwrap_or("unknown error");
                json!([{"type": "text", "text": error_msg}])
            } else {
                let result_value = result.get("result").cloned().unwrap_or(json!({}));
                json!([{"type": "text", "text": serde_json::to_string(&result_value).unwrap_or_default()}])
            };

            json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "content": content,
                    "isError": is_error
                }
            })
        }
        _ => {
            json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {
                    "code": -32601,
                    "message": format!("method not found: {method}")
                }
            })
        }
    };

    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/json")],
        Json(response),
    )
        .into_response()
}
```

- [ ] **Step 2: Add MCP route to the router**

In the `run_local_server` function, update the Router to include the MCP endpoint:

```rust
    let app = Router::new()
        .route("/command", post(handle_command))
        .route("/mcp", post(handle_mcp_post))
        .route("/health", get(handle_health))
        .with_state(state);
```

- [ ] **Step 3: Build and verify**

Run: `cd windows && cargo build 2>&1 | tail -5`
Expected: Compiles successfully.

- [ ] **Step 4: Commit**

```bash
git add windows/src/local_server.rs
git commit -m "feat(windows): add MCP Streamable HTTP endpoint at /mcp"
```

---

### Task 7: Add Local Mode Settings Window and Tray Menu Items

**Files:**
- Create: `windows/src/local_mode_window.rs`
- Modify: `windows/src/tray.rs`
- Modify: `windows/src/main.rs`

- [ ] **Step 1: Create local_mode_window.rs**

Create `windows/src/local_mode_window.rs`:

```rust
use eframe::egui;

use crate::config::Config;

pub struct LocalModeState {
    pub key: String,
    pub port_str: String,
    pub status: String,
    pub saved: bool,
}

impl LocalModeState {
    pub fn new() -> Self {
        let config = Config::load();
        Self {
            key: config.local_mode_key.clone(),
            port_str: config.local_mode_port.to_string(),
            status: String::new(),
            saved: false,
        }
    }

    /// Render the local mode settings UI. Returns true if the viewport should close.
    pub fn render(&mut self, ctx: &egui::Context) -> bool {
        let mut should_close = false;

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(20.0);
                ui.label(egui::RichText::new("Local Mode Settings").size(22.0).strong());
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new("Configure direct HTTP access for AI assistants and scripts")
                        .size(13.0)
                        .color(ui.visuals().weak_text_color()),
                );
                ui.add_space(20.0);
            });

            ui.set_min_width(320.0);

            // API Key field
            ui.horizontal(|ui| {
                ui.label("API Key:");
                ui.add(
                    egui::TextEdit::singleline(&mut self.key)
                        .desired_width(200.0)
                        .password(true)
                        .hint_text("Enter key or generate one"),
                );
            });

            ui.add_space(4.0);

            ui.horizontal(|ui| {
                ui.label("          "); // Alignment spacer
                if ui.button("Generate Key").clicked() {
                    let mut bytes = [0u8; 16];
                    getrandom::getrandom(&mut bytes).expect("random failed");
                    self.key = bytes.iter().map(|b| format!("{:02x}", b)).collect();
                }
                if ui.button("Show/Hide").clicked() {
                    // Toggle is handled by the password field - we'll add a visibility toggle
                }
            });

            ui.add_space(12.0);

            // Port field
            ui.horizontal(|ui| {
                ui.label("Port:       ");
                ui.add(
                    egui::TextEdit::singleline(&mut self.port_str)
                        .desired_width(80.0)
                        .hint_text("6767"),
                );
            });

            ui.add_space(20.0);

            ui.vertical_centered(|ui| {
                if ui
                    .add_sized([220.0, 38.0], egui::Button::new("Save"))
                    .clicked()
                {
                    let port: u16 = match self.port_str.trim().parse() {
                        Ok(p) if p > 0 => p,
                        _ => {
                            self.status = "Invalid port number".to_string();
                            return;
                        }
                    };

                    let mut config = Config::load();
                    config.local_mode_key = self.key.trim().to_string();
                    config.local_mode_port = port;
                    match config.save() {
                        Ok(()) => {
                            self.status = if config.local_mode_key.is_empty() {
                                "Saved. Local mode is disabled (no key).".to_string()
                            } else {
                                format!("Saved. Restart app to apply. Will listen on :{port}")
                            };
                            self.saved = true;
                        }
                        Err(e) => {
                            self.status = format!("Error: {e}");
                        }
                    }
                }

                ui.add_space(8.0);

                if ui
                    .add_sized([220.0, 38.0], egui::Button::new("Close"))
                    .clicked()
                {
                    should_close = true;
                }

                if !self.status.is_empty() {
                    ui.add_space(12.0);
                    ui.label(
                        egui::RichText::new(&self.status)
                            .size(13.0)
                            .color(ui.visuals().weak_text_color()),
                    );
                }

                // Show MCP config snippet when key is set
                if !self.key.trim().is_empty() {
                    ui.add_space(16.0);
                    ui.label(
                        egui::RichText::new("Claude Code MCP config:")
                            .size(13.0)
                            .strong(),
                    );
                    let port = self.port_str.trim();
                    let snippet = format!(
                        r#"{{"mcpServers":{{"screenmcp":{{"type":"url","url":"http://127.0.0.1:{port}/mcp","headers":{{"Authorization":"Bearer {}"}}}}}}}}"#,
                        self.key.trim()
                    );
                    let mut snippet_display = snippet.clone();
                    ui.add(
                        egui::TextEdit::multiline(&mut snippet_display)
                            .desired_width(380.0)
                            .desired_rows(3)
                            .font(egui::TextStyle::Monospace)
                            .interactive(true),
                    );
                }
            });
        });

        should_close
    }
}
```

- [ ] **Step 2: Add module declaration and tray menu items**

In `windows/src/main.rs`, add:

```rust
mod local_mode_window;
```

In `windows/src/tray.rs`, add to the `MenuItems` struct:

```rust
    local_mode_status: MenuItem,
    local_mode_settings: MenuItem,
    run_as_admin: MenuItem,
```

In the menu construction inside `TrayApp::new()`, add these items after the test_window section (before the quit separator):

```rust
        let config_for_local = Config::load();
        let local_status_text = if config_for_local.local_mode_key.is_empty() {
            "Local: Disabled".to_string()
        } else {
            format!("Local: Listening on :{}", config_for_local.local_mode_port)
        };
        let local_mode_status = MenuItem::new(&local_status_text, false, None);
        let local_mode_settings = MenuItem::new("Local Mode Settings...", true, None);

        let is_elevated = crate::commands::is_process_elevated_check();
        let admin_label = if is_elevated {
            "Running as Administrator"
        } else {
            "Run as Administrator"
        };
        let run_as_admin = MenuItem::new(admin_label, !is_elevated, None);
```

Add them to the menu:

```rust
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&local_mode_status);
        let _ = menu.append(&local_mode_settings);
        let _ = menu.append(&run_as_admin);
```

Add to MenuItems struct init:

```rust
            local_mode_status,
            local_mode_settings,
            run_as_admin,
```

- [ ] **Step 3: Add local mode viewport state to TrayApp**

Add to `TrayApp` struct fields:

```rust
    show_local_mode: Arc<AtomicBool>,
    focus_local_mode: Arc<AtomicBool>,
    local_mode_state: Arc<Mutex<crate::local_mode_window::LocalModeState>>,
```

Initialize in `TrayApp::new()`:

```rust
            show_local_mode: Arc::new(AtomicBool::new(false)),
            focus_local_mode: Arc::new(AtomicBool::new(false)),
            local_mode_state: Arc::new(Mutex::new(crate::local_mode_window::LocalModeState::new())),
```

- [ ] **Step 4: Handle menu events for new items**

In `handle_menu_events`, add handlers:

```rust
            } else if event.id() == items.local_mode_settings.id() {
                info!("menu: local mode settings clicked");
                *self.local_mode_state.lock().unwrap() = crate::local_mode_window::LocalModeState::new();
                self.show_local_mode.store(true, Ordering::SeqCst);
                self.focus_local_mode.store(true, Ordering::SeqCst);
            } else if event.id() == items.run_as_admin.id() {
                info!("menu: run as admin clicked");
                std::thread::spawn(|| {
                    let _ = crate::commands::execute_command(0, "elevate", None, &Config::load());
                });
```

- [ ] **Step 5: Add local mode viewport to eframe update**

In `TrayApp`'s `update` method, add the local mode viewport after the test viewport block:

```rust
        // ── Local Mode viewport ──
        if self.show_local_mode.load(Ordering::SeqCst) {
            let state = self.local_mode_state.clone();
            let show = self.show_local_mode.clone();
            let focus = self.focus_local_mode.clone();
            ctx.show_viewport_deferred(
                egui::ViewportId::from_hash_of("local_mode_window"),
                egui::ViewportBuilder::default()
                    .with_title("ScreenMCP Local Mode")
                    .with_inner_size([450.0, 520.0]),
                move |ctx, _class| {
                    if ctx.input(|i| i.viewport().close_requested()) {
                        show.store(false, Ordering::SeqCst);
                        return;
                    }
                    if focus.swap(false, Ordering::SeqCst) {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                    }
                    let mut s = state.lock().unwrap();
                    let should_close = s.render(ctx);
                    if should_close {
                        show.store(false, Ordering::SeqCst);
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                },
            );
        }
```

- [ ] **Step 6: Expose is_process_elevated for tray.rs**

In `windows/src/commands.rs`, add a public wrapper:

```rust
pub fn is_process_elevated_check() -> bool {
    is_process_elevated()
}
```

- [ ] **Step 7: Build and verify**

Run: `cd windows && cargo build 2>&1 | tail -5`
Expected: Compiles successfully.

- [ ] **Step 8: Commit**

```bash
git add windows/src/local_mode_window.rs windows/src/tray.rs windows/src/main.rs windows/src/commands.rs
git commit -m "feat(windows): add local mode settings window and tray menu items"
```

---

### Task 8: Create Skills Directory and Files

**Files:**
- Create: `screenmcp-local/skills/skill.md`
- Create: `screenmcp-local/skills/windows.md`
- Create: `screenmcp-local/skills/python.md`

- [ ] **Step 1: Create skill.md**

Create `screenmcp-local/skills/skill.md`:

```markdown
# ScreenMCP Local — Desktop Control via MCP

You have access to ScreenMCP tools that let you see and control a Windows desktop. These tools connect to the ScreenMCP app running locally.

## Available Tools

### Vision
- **screenshot** — Take a screenshot. Returns base64 WebP image. Use `max_width`/`max_height` to control size.
- **ui_tree** — Get the accessibility tree. Returns structured data with element names, types, bounds (x,y,width,height), text content, and clickable/focusable state. Excellent for finding interactive elements without visual inspection.

### Mouse
- **click** — Left-click at (x, y). Optional `duration` in ms.
- **right_click** — Right-click at (x, y). Opens context menus.
- **double_click** — Double-click at (x, y). Opens files, selects words.
- **long_click** — Long press at (x, y). Default 1000ms hold.
- **middle_click** — Middle-click at (x, y).
- **mouse_move** — Move cursor to (x, y) without clicking. Useful for hover actions.
- **drag** — Drag from (startX, startY) to (endX, endY). Optional `duration`.
- **scroll** — Scroll at (x, y) with `dx`/`dy` deltas, or use `direction` (up/down/left/right) + `amount`.
- **mouse_scroll** — Raw mouse wheel scroll at coordinates.

### Keyboard
- **type** — Type text into the focused field. Handles special characters.
- **press_key** — Press and release a single key. Key names: shift, ctrl, alt, meta/win, tab, enter, escape, space, backspace, delete, home, end, pageup, pagedown, up, down, left, right, f1-f12, or a single character.
- **hold_key** / **release_key** — Hold and release keys manually. Use for complex sequences.
- **hotkey** — Press a key combination atomically. Pass an array of key names: `["ctrl", "c"]`, `["alt", "tab"]`, `["win", "d"]`. Preferred over hold_key/release_key for standard shortcuts.

### Text & Clipboard
- **get_text** — Get text from the focused field (reads clipboard).
- **select_all** — Select all text (Ctrl+A).
- **copy** — Copy selection (Ctrl+C). Set `return_text: true` to get the copied text back.
- **paste** — Paste (Ctrl+V). Optionally pass `text` to set clipboard before pasting.
- **get_clipboard** — Read clipboard contents.
- **set_clipboard** — Set clipboard to given text.

### Window Management
- **list_windows** — List all visible on-screen windows with title, position, size, minimized/maximized state, and index.
- **focus_window** — Bring a window to front by `title` (substring match) or `index` from list_windows.
- **get_screen_size** — Get primary screen dimensions (width, height, x, y).

### Navigation
- **back** — Browser back / general back (Alt+Left).
- **home** — Press Windows key (opens Start menu).
- **recents** — Show recent windows (Alt+Tab).

### System
- **elevate** — Request administrator privileges. Shows a confirmation dialog to the user. Needed for interacting with elevated windows.
- **is_elevated** — Check if running with admin privileges.
- **camera** / **list_cameras** — Capture from connected cameras.
- **play_audio** — Play base64-encoded WAV/MP3 audio.

## How to Use These Tools

### Understanding the Screen

Use both **screenshot** and **ui_tree** as needed:

- **screenshot** gives you a visual picture of what's on screen. Great for understanding layout, reading visual content (images, charts, web pages), and verifying actions worked.
- **ui_tree** gives you structured data about every UI element. Great for finding buttons, text fields, labels, and their exact coordinates. Elements include bounds, text, control type, and interaction state.

Use whichever fits the situation. Often you'll use both — ui_tree to find elements and their coordinates, screenshot to verify visual state.

### Taking Actions

1. Identify the target — use ui_tree to find elements or screenshot to see the screen
2. Act — click, type, hotkey, etc.
3. Verify — take a screenshot or check ui_tree to confirm the action worked

### Coordinates

All coordinates are in screen pixels. (0,0) is the top-left corner of the primary monitor. Multi-monitor setups may have negative coordinates for monitors to the left/above.

Use `get_screen_size` to know the screen bounds. Use element bounds from `ui_tree` for precise clicking — click the center of an element's bounding box.

### Tips

- **Prefer hotkey over hold_key/release_key** for standard shortcuts. It's atomic and more reliable.
- **For text input:** Click the target field first, then use `type`. To replace existing text: `select_all` then `type`.
- **For window switching:** Use `list_windows` + `focus_window` for programmatic control, or `hotkey ["alt", "tab"]` for quick toggle.
- **Screenshots can be large.** Use `max_width` and `max_height` to keep them manageable.
```

- [ ] **Step 2: Create windows.md**

Create `screenmcp-local/skills/windows.md`:

```markdown
# Windows Desktop Guide

How to navigate and operate a Windows desktop using ScreenMCP tools.

## Desktop Layout

### Taskbar (Bottom of Screen)
- **Start button** — far left. Opens Start menu. Equivalent: `hotkey ["win"]` or `home`.
- **Search** — next to Start. Click to search apps and files. Or: `hotkey ["win", "s"]`.
- **Pinned apps** — icons in the middle of taskbar. Click to launch or switch to an app.
- **Running apps** — shown with an underline indicator. Click to switch.
- **System tray** — far right. Clock, network, volume, battery, notification icons.
- **Show desktop** — very far right edge. Or: `hotkey ["win", "d"]`.

### Window Anatomy
- **Title bar** — top of window. Shows app name and document title. Drag to move window.
- **Window controls** — top-right corner:
  - **Minimize** (—) — hides window to taskbar
  - **Maximize/Restore** (□) — toggles fullscreen. Or: `hotkey ["win", "up"]`
  - **Close** (X) — closes window. Or: `hotkey ["alt", "F4"]`
- **Menu bar** — below title bar (File, Edit, View, Help...)
- **Scrollbars** — right edge and bottom edge for scrollable content
- **Resize handles** — drag any window edge or corner to resize

## Keyboard Shortcuts (use with `hotkey`)

### Essential
| Keys | Action |
|------|--------|
| `["ctrl", "c"]` | Copy |
| `["ctrl", "v"]` | Paste |
| `["ctrl", "x"]` | Cut |
| `["ctrl", "a"]` | Select all |
| `["ctrl", "z"]` | Undo |
| `["ctrl", "y"]` | Redo |
| `["ctrl", "s"]` | Save |
| `["ctrl", "f"]` | Find |
| `["ctrl", "w"]` | Close tab |
| `["ctrl", "t"]` | New tab (browsers) |
| `["ctrl", "n"]` | New window |
| `["ctrl", "p"]` | Print |

### Window Management
| Keys | Action |
|------|--------|
| `["alt", "tab"]` | Switch between windows |
| `["alt", "F4"]` | Close current window |
| `["win", "d"]` | Show desktop / restore all windows |
| `["win", "e"]` | Open File Explorer |
| `["win", "r"]` | Open Run dialog |
| `["win", "l"]` | Lock screen |
| `["win", "i"]` | Open Settings |
| `["win", "up"]` | Maximize window |
| `["win", "down"]` | Restore / minimize window |
| `["win", "left"]` | Snap window to left half |
| `["win", "right"]` | Snap window to right half |
| `["win", "shift", "s"]` | Screenshot snip tool |
| `["ctrl", "shift", "esc"]` | Open Task Manager |

### Navigation
| Keys | Action |
|------|--------|
| `["alt", "left"]` | Back (browsers, File Explorer) |
| `["alt", "right"]` | Forward |
| `["tab"]` | Next field / element |
| `["shift", "tab"]` | Previous field / element |
| `["enter"]` | Activate focused button / confirm dialog |
| `["escape"]` | Cancel / close dialog / close menu |
| `["F2"]` | Rename selected file |
| `["F5"]` | Refresh |
| `["F11"]` | Toggle fullscreen (browsers) |

### Text Editing
| Keys | Action |
|------|--------|
| `["ctrl", "left"]` | Move cursor one word left |
| `["ctrl", "right"]` | Move cursor one word right |
| `["ctrl", "shift", "left"]` | Select word left |
| `["ctrl", "shift", "right"]` | Select word right |
| `["home"]` | Go to beginning of line |
| `["end"]` | Go to end of line |
| `["ctrl", "home"]` | Go to beginning of document |
| `["ctrl", "end"]` | Go to end of document |
| `["shift", "home"]` | Select to beginning of line |
| `["shift", "end"]` | Select to end of line |

## Common UI Patterns

### Dialog Boxes
- **OK / Cancel dialogs** — buttons usually at bottom-right. `enter` confirms, `escape` cancels.
- **Save dialogs** — have a file name field and a "Save" button. Navigate folders in the left pane.
- **File Open dialogs** — same structure. Double-click files or type path in address bar.

### Context Menus
- **Right-click** any element to open context menu.
- Context menus have items like Cut, Copy, Paste, Delete, Properties, etc.
- Click outside the menu or press `escape` to dismiss.

### Dropdown / Combo Boxes
- Click to open the dropdown list.
- Click an item to select it.
- Or: focus the dropdown, then use `up`/`down` arrow keys.

### Checkboxes and Radio Buttons
- Click to toggle. Or: focus and press `space`.
- Checkboxes: multiple can be selected.
- Radio buttons: only one in a group can be selected.

### Tabs
- Click a tab to switch to it.
- Browsers: `["ctrl", "tab"]` for next tab, `["ctrl", "shift", "tab"]` for previous.

### Tree Views (File Explorer, Settings)
- Click arrow to expand/collapse.
- Or: focus and use `left`/`right` arrow keys.
- `right` expands, `left` collapses or goes to parent.

### Ribbons (Office apps)
- Click a ribbon tab (Home, Insert, etc.) to show its tools.
- Tools are organized in groups with labeled buttons.

## Common Operations

### Opening an Application
1. `hotkey ["win"]` to open Start menu
2. `type` the app name
3. `press_key "enter"` to launch

Or use `list_windows` + `focus_window` if the app is already running.

### Switching Between Windows
- **Quick toggle:** `hotkey ["alt", "tab"]`
- **Programmatic:** `list_windows` → find by title → `focus_window`
- **Taskbar:** Click the app icon on the taskbar

### Entering Text in a Field
1. `click` on the text field
2. `select_all` to select existing text (if replacing)
3. `type` the new text

### Copying Text from Screen
1. `click` at the start of the text
2. Use `drag` to select, or `select_all` for the whole field
3. `copy` with `return_text: true` to get the text

### Saving a File
1. `hotkey ["ctrl", "s"]`
2. If it's a new file, a Save As dialog appears
3. `type` the filename in the name field
4. `click` the Save button or `press_key "enter"`

### Scrolling Through Content
- **Smooth scroll:** `scroll` with `dy` (negative = scroll content up, positive = scroll content down)
- **Page scroll:** `press_key "pagedown"` or `press_key "pageup"`
- **To top/bottom:** `hotkey ["ctrl", "home"]` or `hotkey ["ctrl", "end"]`

### Managing Files in Explorer
- **Open Explorer:** `hotkey ["win", "e"]`
- **Navigate:** Click folders or type path in address bar
- **Select file:** Click it. Multi-select: hold Ctrl and click.
- **Rename:** Select file, `press_key "F2"`, `type` new name, `press_key "enter"`
- **Delete:** Select file, `press_key "delete"`
- **Copy/Paste files:** Select → `hotkey ["ctrl", "c"]` → navigate → `hotkey ["ctrl", "v"]`

### Working with the System Tray
- Click the **^** arrow to show hidden tray icons
- Right-click a tray icon for its menu
- ScreenMCP's own tray icon is in the system tray

## Elevation (Admin Privileges)

Some actions require administrator privileges:
- Installing software
- Modifying system settings
- Interacting with elevated windows (Task Manager when opened by admin, UAC prompts)

When UI elements don't respond to clicks or `ui_tree` returns empty for a window, it may be running elevated. Use `is_elevated` to check current status, and `elevate` to request admin privileges if needed.

## Multi-Monitor

- `get_screen_size` returns the primary monitor dimensions.
- Secondary monitors may be to the left (negative x coordinates) or above (negative y coordinates).
- `ui_tree` and `screenshot` cover the primary monitor by default.
- Use coordinates from `list_windows` to understand window placement across monitors.
```

- [ ] **Step 3: Create python.md**

Create `screenmcp-local/skills/python.md`:

```markdown
# Controlling ScreenMCP from Python

Use the REST API at `POST http://127.0.0.1:6767/command` to control the desktop from Python scripts.

## Setup

```python
import requests
import base64
import json
from io import BytesIO

SCREENMCP_URL = "http://127.0.0.1:6767"
SCREENMCP_KEY = "your-api-key-here"

HEADERS = {
    "Authorization": f"Bearer {SCREENMCP_KEY}",
    "Content-Type": "application/json",
}

def cmd(name: str, params: dict = None) -> dict:
    """Send a command to ScreenMCP and return the result."""
    body = {"cmd": name}
    if params:
        body["params"] = params
    resp = requests.post(f"{SCREENMCP_URL}/command", json=body, headers=HEADERS)
    resp.raise_for_status()
    data = resp.json()
    if data.get("status") == "error":
        raise RuntimeError(data.get("error", "unknown error"))
    return data.get("result", {})
```

## Taking Screenshots

```python
# Take a screenshot and save to file
result = cmd("screenshot", {"max_width": 1920, "max_height": 1080})
image_bytes = base64.b64decode(result["image"])
with open("screenshot.webp", "wb") as f:
    f.write(image_bytes)

# Convert to PIL Image for processing
from PIL import Image
img = Image.open(BytesIO(image_bytes))
print(f"Screenshot: {img.width}x{img.height}")
```

## Mouse and Keyboard

```python
# Click
cmd("click", {"x": 500, "y": 300})

# Right-click
cmd("right_click", {"x": 500, "y": 300})

# Double-click
cmd("double_click", {"x": 500, "y": 300})

# Type text
cmd("type", {"text": "Hello, world!"})

# Press a key
cmd("press_key", {"key": "enter"})

# Keyboard shortcut
cmd("hotkey", {"keys": ["ctrl", "c"]})

# Scroll down
cmd("scroll", {"x": 500, "y": 500, "dy": -3})

# Drag
cmd("drag", {"startX": 100, "startY": 200, "endX": 400, "endY": 200})
```

## Reading the Screen

```python
# Get accessibility tree
result = cmd("ui_tree")
for node in result.get("tree", []):
    print(f"  {node.get('ct', '')} - {node.get('name', '')} @ {node.get('bounds', '')}")

# Get screen size
result = cmd("get_screen_size")
print(f"Screen: {result['width']}x{result['height']}")

# List windows
result = cmd("list_windows")
for w in result.get("windows", []):
    print(f"  [{w['index']}] {w['title']} ({w['width']}x{w['height']})")
```

## Clipboard

```python
# Copy and read
cmd("select_all")
result = cmd("copy", {"return_text": True})
print(f"Text: {result['text']}")

# Set clipboard and paste
cmd("set_clipboard", {"text": "Pasted from Python"})
cmd("paste")
```

## Window Management

```python
# Focus a window by title
cmd("focus_window", {"title": "Notepad"})

# Focus by index
result = cmd("list_windows")
cmd("focus_window", {"index": 0})  # Focus the first window
```

## Automation Example

```python
import time

def open_app(name: str):
    """Open an app via Start menu search."""
    cmd("hotkey", {"keys": ["win"]})
    time.sleep(0.5)
    cmd("type", {"text": name})
    time.sleep(1)
    cmd("press_key", {"key": "enter"})
    time.sleep(2)

def save_file(filename: str):
    """Save current document with a specific name."""
    cmd("hotkey", {"keys": ["ctrl", "s"]})
    time.sleep(0.5)
    cmd("select_all")
    cmd("type", {"text": filename})
    cmd("press_key", {"key": "enter"})

# Example: Open Notepad, type something, save
open_app("notepad")
cmd("type", {"text": "Hello from ScreenMCP Python!"})
save_file("test.txt")
```
```

- [ ] **Step 4: Commit**

```bash
git add screenmcp-local/skills/
git commit -m "docs: add skill files for local mode (main skill, windows guide, python guide)"
```

---

### Task 9: Integration Test — Manual Verification

- [ ] **Step 1: Build the Windows client**

Run: `cd windows && cargo build --release 2>&1 | tail -5`
Expected: Compiles successfully.

- [ ] **Step 2: Set a local mode key in config**

Add to `~/.screenmcp/config.toml`:
```toml
local_mode_key = "test-key-12345"
local_mode_port = 6767
```

- [ ] **Step 3: Start the app and verify HTTP server**

Start the app, then test:

```bash
curl -s http://127.0.0.1:6767/health
```
Expected: `{"status":"ok","service":"screenmcp-local"}`

```bash
curl -s -X POST http://127.0.0.1:6767/command \
  -H "Authorization: Bearer test-key-12345" \
  -H "Content-Type: application/json" \
  -d '{"cmd":"get_screen_size"}'
```
Expected: `{"status":"ok","result":{"width":...,"height":...}}`

- [ ] **Step 4: Test MCP endpoint**

```bash
curl -s -X POST http://127.0.0.1:6767/mcp \
  -H "Authorization: Bearer test-key-12345" \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}'
```
Expected: JSON-RPC response with protocolVersion and serverInfo.

```bash
curl -s -X POST http://127.0.0.1:6767/mcp \
  -H "Authorization: Bearer test-key-12345" \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":2,"method":"tools/list"}'
```
Expected: JSON-RPC response with tools array listing all commands.

- [ ] **Step 5: Test unauthorized access**

```bash
curl -s -X POST http://127.0.0.1:6767/command \
  -H "Authorization: Bearer wrong-key" \
  -H "Content-Type: application/json" \
  -d '{"cmd":"get_screen_size"}'
```
Expected: HTTP 401

- [ ] **Step 6: Commit final state**

```bash
git add -A
git commit -m "feat(windows): local mode complete with REST API, MCP, and skills"
```
