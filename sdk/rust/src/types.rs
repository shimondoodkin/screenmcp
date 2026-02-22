use serde::{Deserialize, Serialize};

/// Options for creating a ScreenMCPClient.
#[derive(Debug, Clone)]
pub struct ClientOptions {
    /// API key (pk_... format) for authentication.
    pub api_key: String,
    /// Base URL of the ScreenMCP API server.
    /// Defaults to "https://server10.doodkin.com".
    pub api_url: Option<String>,
    /// Target device ID. If omitted, the server picks the first available device.
    pub device_id: Option<String>,
    /// Per-command timeout in milliseconds. Defaults to 30000.
    pub command_timeout_ms: Option<u64>,
    /// Automatically reconnect when the worker connection drops. Defaults to true.
    pub auto_reconnect: Option<bool>,
}

/// Scroll direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollDirection {
    Up,
    Down,
    Left,
    Right,
}

/// Result of a screenshot command.
#[derive(Debug, Clone, Deserialize)]
pub struct ScreenshotResult {
    /// Base64-encoded image (WebP).
    #[serde(default)]
    pub image: String,
}

/// Result of a get_text command.
#[derive(Debug, Clone, Deserialize)]
pub struct TextResult {
    /// Text content from the focused element.
    #[serde(default)]
    pub text: String,
}

/// Result of a ui_tree command.
#[derive(Debug, Clone, Deserialize)]
pub struct UiTreeResult {
    /// Accessibility tree nodes.
    #[serde(default)]
    pub tree: Vec<serde_json::Value>,
}

/// Result of a camera command.
#[derive(Debug, Clone, Deserialize)]
pub struct CameraResult {
    /// Base64-encoded image (WebP).
    #[serde(default)]
    pub image: String,
}

/// Result of a copy command.
#[derive(Debug, Clone, Deserialize)]
pub struct CopyResult {
    /// Copied text (only present when return_text was true).
    pub text: Option<String>,
}

/// Result of a clipboard read.
#[derive(Debug, Clone, Deserialize)]
pub struct ClipboardResult {
    /// Clipboard text contents.
    #[serde(default)]
    pub text: String,
}

/// Camera info from list_cameras.
#[derive(Debug, Clone, Deserialize)]
pub struct CameraInfo {
    pub id: String,
    pub facing: String,
}

/// Result of list_cameras command.
#[derive(Debug, Clone, Deserialize)]
pub struct ListCamerasResult {
    #[serde(default)]
    pub cameras: Vec<CameraInfo>,
}

/// Raw command response from the worker.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandResponse {
    pub id: i64,
    pub status: String,
    #[serde(default)]
    pub result: Option<serde_json::Value>,
    #[serde(default)]
    pub error: Option<String>,
}

// --- Wire protocol messages (internal) ---

/// Auth message sent by controller to worker.
#[derive(Debug, Serialize)]
pub(crate) struct AuthMsg {
    #[serde(rename = "type")]
    pub msg_type: &'static str,
    pub key: String,
    pub role: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_device_id: Option<String>,
    pub last_ack: i64,
}

/// Command sent by controller (no ID — worker assigns it).
#[derive(Debug, Serialize)]
pub(crate) struct ControllerCommand {
    pub cmd: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

/// Pong response to server ping.
#[derive(Debug, Serialize)]
pub(crate) struct PongMsg {
    #[serde(rename = "type")]
    pub msg_type: &'static str,
}

/// Discovery response from the API.
#[derive(Debug, Deserialize)]
pub(crate) struct DiscoverResponse {
    #[serde(rename = "wsUrl")]
    pub ws_url: String,
}

/// Parsed server message (from worker → controller).
#[derive(Debug)]
pub(crate) enum ServerMessage {
    AuthOk { phone_connected: bool },
    AuthFail { error: String },
    CmdAccepted { id: i64 },
    PhoneStatus { connected: bool },
    Ping,
    Error { error: String },
    CommandResponse(CommandResponse),
}

impl ServerMessage {
    pub fn parse(text: &str) -> Option<Self> {
        let v: serde_json::Value = serde_json::from_str(text).ok()?;

        if let Some(t) = v.get("type").and_then(|t| t.as_str()) {
            match t {
                "auth_ok" => {
                    let phone_connected = v
                        .get("phone_connected")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    return Some(Self::AuthOk { phone_connected });
                }
                "auth_fail" => {
                    let error = v
                        .get("error")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_string();
                    return Some(Self::AuthFail { error });
                }
                "cmd_accepted" => {
                    let id = v.get("id").and_then(|v| v.as_i64())?;
                    return Some(Self::CmdAccepted { id });
                }
                "phone_status" => {
                    let connected = v.get("connected").and_then(|v| v.as_bool())?;
                    return Some(Self::PhoneStatus { connected });
                }
                "ping" => return Some(Self::Ping),
                "error" => {
                    let error = v
                        .get("error")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_string();
                    return Some(Self::Error { error });
                }
                _ => {}
            }
        }

        // Command response: has id + status, no type
        if v.get("id").is_some() && v.get("status").is_some() && v.get("type").is_none() {
            let resp: CommandResponse = serde_json::from_value(v).ok()?;
            return Some(Self::CommandResponse(resp));
        }

        None
    }
}
