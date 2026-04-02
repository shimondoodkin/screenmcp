use axum::{
    extract::State,
    http::{header, HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info};

use crate::commands;
use crate::config::Config;

#[derive(Clone)]
struct AppState {
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

// ── REST API: POST /command ──

async fn handle_command(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    let config = state.config.read().await;
    if let Err(s) = verify_auth(&headers, &config.local_mode_key) {
        return (s, Json(json!({"status": "error", "error": "unauthorized"}))).into_response();
    }

    let cmd = body
        .get("cmd")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let params = body.get("params").cloned();

    if cmd.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"status": "error", "error": "missing cmd field"})),
        )
            .into_response();
    }

    let config_clone = config.clone();
    drop(config);

    info!("local: command: {cmd}");

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

    // Strip the id field (local mode doesn't use ids)
    let mut resp = response;
    if let Some(obj) = resp.as_object_mut() {
        obj.remove("id");
    }

    (StatusCode::OK, Json(resp)).into_response()
}

// ── Health check ──

async fn handle_health() -> impl IntoResponse {
    Json(json!({"status": "ok", "service": "screenmcp-local"}))
}

// ── MCP Streamable HTTP: POST /mcp ──

fn scaling_props() -> Value {
    json!({
        "max_width": {"type": "integer", "description": "Screenshot width for coordinate scaling (default: 1456). Set to 0 to disable.", "default": 1456},
        "max_height": {"type": "integer", "description": "Screenshot height for coordinate scaling (default: 819). Set to 0 to disable.", "default": 819}
    })
}

fn mcp_tool_definitions() -> Vec<Value> {
    let sp = scaling_props();
    vec![
        json!({
            "name": "screenshot",
            "description": "Take a screenshot of the screen. Returns base64 WebP image. Default max_width=1456, max_height=819.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "quality": {"type": "integer", "description": "Image quality 1-100 (default: 100)"},
                    "max_width": sp["max_width"],
                    "max_height": sp["max_height"]
                }
            }
        }),
        json!({
            "name": "ui_tree",
            "description": "Get the accessibility tree of the current screen. Returns UI nodes with bounds scaled to screenshot coordinates.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "max_width": sp["max_width"],
                    "max_height": sp["max_height"]
                }
            }
        }),
        json!({
            "name": "click",
            "description": "Click at screen coordinates. Coordinates are in screenshot space and auto-scaled.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "x": {"type": "number", "description": "X coordinate"},
                    "y": {"type": "number", "description": "Y coordinate"},
                    "duration": {"type": "integer", "description": "Press duration in ms (default: 100)"},
                    "max_width": sp["max_width"],
                    "max_height": sp["max_height"]
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
                    "x": {"type": "number", "description": "X coordinate"},
                    "y": {"type": "number", "description": "Y coordinate"},
                    "max_width": sp["max_width"],
                    "max_height": sp["max_height"]
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
                    "x": {"type": "number", "description": "X coordinate"},
                    "y": {"type": "number", "description": "Y coordinate"},
                    "max_width": sp["max_width"],
                    "max_height": sp["max_height"]
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
                    "x": {"type": "number", "description": "X coordinate"},
                    "y": {"type": "number", "description": "Y coordinate"},
                    "max_width": sp["max_width"],
                    "max_height": sp["max_height"]
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
                    "x": {"type": "number", "description": "X coordinate"},
                    "y": {"type": "number", "description": "Y coordinate"},
                    "duration": {"type": "integer", "description": "Press duration in ms (default: 1000)"},
                    "max_width": sp["max_width"],
                    "max_height": sp["max_height"]
                },
                "required": ["x", "y"]
            }
        }),
        json!({
            "name": "drag",
            "description": "Drag from one point to another. Coordinates are in screenshot space.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "startX": {"type": "number"},
                    "startY": {"type": "number"},
                    "endX": {"type": "number"},
                    "endY": {"type": "number"},
                    "duration": {"type": "integer", "description": "Duration in ms (default: 300)"},
                    "max_width": sp["max_width"],
                    "max_height": sp["max_height"]
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
                    "x": {"type": "number", "description": "X coordinate"},
                    "y": {"type": "number", "description": "Y coordinate"},
                    "dx": {"type": "number", "description": "Horizontal delta"},
                    "dy": {"type": "number", "description": "Vertical delta (negative = scroll content up)"},
                    "direction": {"type": "string", "description": "Alternative: up/down/left/right"},
                    "amount": {"type": "integer", "description": "Scroll amount (used with direction, default: 3)"},
                    "max_width": sp["max_width"],
                    "max_height": sp["max_height"]
                }
            }
        }),
        json!({
            "name": "mouse_move",
            "description": "Move mouse cursor to coordinates without clicking",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "x": {"type": "number", "description": "X coordinate"},
                    "y": {"type": "number", "description": "Y coordinate"},
                    "max_width": sp["max_width"],
                    "max_height": sp["max_height"]
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
                    "x": {"type": "number"},
                    "y": {"type": "number"},
                    "dx": {"type": "number"},
                    "dy": {"type": "number"},
                    "max_width": sp["max_width"],
                    "max_height": sp["max_height"]
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
            "name": "screenshot_region",
            "description": "Capture a region of the screen at native resolution for precise inspection. Use tight regions (e.g. 120x90 in screenshot coords) to zoom into buttons, text, or small elements. To click precisely: take a region, find the target pixel in the cropped image, then compute screen_x = min_x + (pixel_x / image_width) * (max_x - min_x), screen_y = min_y + (pixel_y / image_height) * (max_y - min_y). Only scales down, never up.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "min_x": {"type": "number", "description": "Left edge X coordinate"},
                    "min_y": {"type": "number", "description": "Top edge Y coordinate"},
                    "max_x": {"type": "number", "description": "Right edge X coordinate"},
                    "max_y": {"type": "number", "description": "Bottom edge Y coordinate"},
                    "quality": {"type": "integer", "description": "Image quality 1-100 (default: 100)"},
                    "output_max_width": {"type": "integer", "description": "Max output width (only scales down)"},
                    "output_max_height": {"type": "integer", "description": "Max output height (only scales down)"},
                    "max_width": sp["max_width"],
                    "max_height": sp["max_height"]
                },
                "required": ["min_x", "min_y", "max_x", "max_y"]
            }
        }),
        json!({
            "name": "get_screen_size",
            "description": "Get screen dimensions. Returns scaled dimensions matching screenshot space by default.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "max_width": sp["max_width"],
                    "max_height": sp["max_height"]
                }
            }
        }),
        json!({
            "name": "list_windows",
            "description": "List visible on-screen windows with title, position, size, and state. Coordinates in screenshot space.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "max_width": sp["max_width"],
                    "max_height": sp["max_height"]
                }
            }
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
            "name": "active_window",
            "description": "Get the title, position, size, and state of the currently active (foreground) window. Coordinates in screenshot space.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "max_width": sp["max_width"],
                    "max_height": sp["max_height"]
                }
            }
        }),
        json!({
            "name": "screenshot_window",
            "description": "Capture a screenshot of a specific window by title or index, without needing to bring it to front",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "title": {"type": "string", "description": "Window title substring to match"},
                    "index": {"type": "integer", "description": "Window index from list_windows"},
                    "max_width": {"type": "integer", "description": "Max width for scaling"},
                    "max_height": {"type": "integer", "description": "Max height for scaling"}
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
            // Client acknowledgment — no response needed
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
            let tool_args = params.get("arguments").cloned().unwrap_or(json!({}));

            let config_clone = config.clone();
            drop(config);

            let result = tokio::task::spawn_blocking(move || {
                commands::execute_command(0, &tool_name, Some(&tool_args), &config_clone)
            })
            .await
            .unwrap_or_else(|e| {
                json!({"status": "error", "error": format!("panicked: {e}")})
            });

            let status = result
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("error");
            let is_error = status == "error";

            // For screenshot/camera, return image as embedded resource
            let content = if let Some(image_b64) = result
                .get("result")
                .and_then(|r| r.get("image"))
                .and_then(|v| v.as_str())
            {
                json!([
                    {
                        "type": "image",
                        "data": image_b64,
                        "mimeType": "image/webp"
                    }
                ])
            } else if is_error {
                let error_msg = result
                    .get("error")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown error");
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

// ── Server startup ──

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
        .route("/mcp", post(handle_mcp_post))
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
