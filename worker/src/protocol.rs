use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Version & compatibility
// ---------------------------------------------------------------------------

/// Version info sent by clients in their auth message.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ClientVersion {
    pub major: u32,
    pub minor: u32,
    pub component: String, // "android", "windows", "linux", "mac", "remote", "sdk-ts", "sdk-py", "sdk-rust"
}

impl std::fmt::Display for ClientVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} v{}.{}", self.component, self.major, self.minor)
    }
}

/// Compatibility matrix: which major versions of each component are supported.
/// Format: (component_name, min_major_inclusive, max_major_inclusive)
pub const COMPATIBILITY: &[(&str, u32, u32)] = &[
    ("android", 1, 1),
    ("windows", 1, 1),
    ("linux", 1, 1),
    ("mac", 1, 1),
    ("remote", 1, 1),
    ("sdk-ts", 1, 1),
    ("sdk-py", 1, 1),
    ("sdk-rust", 1, 1),
    ("worker", 1, 1),
];

/// The worker's own version.
pub const WORKER_VERSION: ClientVersion = ClientVersion {
    major: 1,
    minor: 0,
    component: String::new(), // "worker" — set at compile time via Display, but const requires empty
};

/// Check whether a client version is compatible with the current worker.
/// Returns Ok(()) if compatible, or Err(message) with a human-readable explanation.
pub fn check_version_compatible(version: &ClientVersion) -> Result<(), String> {
    for &(component, min_major, max_major) in COMPATIBILITY {
        if component == version.component {
            if version.major < min_major {
                return Err(format!(
                    "Your {} (v{}.{}) is outdated. Please update to version {}.x or later.",
                    version.component, version.major, version.minor, min_major
                ));
            }
            if version.major > max_major {
                return Err(format!(
                    "Your {} (v{}.{}) is too new for this worker. Maximum supported: {}.x.",
                    version.component, version.major, version.minor, max_major
                ));
            }
            return Ok(());
        }
    }
    // Unknown component — allow by default (forward compatible)
    Ok(())
}

/// Version error codes for the error message sent to clients.
pub mod version_error {
    pub const OUTDATED_CLIENT: &str = "outdated_client";
    pub const OUTDATED_REMOTE: &str = "outdated_remote";
    pub const BOTH_OUTDATED: &str = "both_outdated";
}

// ---------------------------------------------------------------------------
// Messages
// ---------------------------------------------------------------------------

/// Messages from phone → server
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum PhoneMessage {
    Auth(AuthMessage),
    Ack(AckMessage),
    Response(CommandResponse),
    Pong(PongMessage),
}

#[derive(Debug, Deserialize)]
pub struct AuthMessage {
    #[serde(rename = "type")]
    pub msg_type: String, // "auth"
    #[serde(default)]
    pub user_id: Option<String>, // phone: Firebase ID token
    #[serde(default)]
    pub key: Option<String>, // controller: API key (pk_...)
    #[serde(default)]
    pub last_ack: i64,
    #[serde(default = "default_role")]
    pub role: String, // "phone" or "controller"
    #[serde(default)]
    pub device_id: Option<String>, // phone: client-generated crypto ID for routing
    #[serde(default)]
    pub target_device_id: Option<String>, // controller: which phone to control
    #[serde(default)]
    pub version: Option<ClientVersion>, // client version info for compatibility checking
}

fn default_role() -> String {
    "phone".into()
}

#[derive(Debug, Deserialize)]
pub struct AckMessage {
    pub ack: i64,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct CommandResponse {
    pub id: i64,
    pub status: String,
    #[serde(default)]
    pub result: Option<serde_json::Value>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PongMessage {
    #[serde(rename = "type")]
    pub msg_type: String, // "pong"
}

/// Messages from controller → server
#[derive(Debug, Deserialize)]
pub struct ControllerCommand {
    pub cmd: String,
    #[serde(default)]
    pub params: Option<serde_json::Value>,
}

/// Messages from server → phone
#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum ServerMessage {
    AuthOk(AuthOkMessage),
    AuthFail(AuthFailMessage),
    Command(Command),
    Ping(PingMessage),
    Error(ErrorMessage),
    VersionError(VersionErrorMessage),
    CommandAccepted(CommandAcceptedMessage),
    PhoneStatus(PhoneStatusMessage),
}

#[derive(Debug, Serialize)]
pub struct VersionErrorMessage {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub code: String,
    pub message: String,
    pub update_url: String,
}

#[derive(Debug, Serialize)]
pub struct AuthOkMessage {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub resume_from: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phone_connected: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct AuthFailMessage {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Command {
    pub id: i64,
    pub cmd: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub struct PingMessage {
    #[serde(rename = "type")]
    pub msg_type: String,
}

#[derive(Debug, Serialize)]
pub struct ErrorMessage {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub error: String,
}

#[derive(Debug, Serialize)]
pub struct CommandAcceptedMessage {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub id: i64,
}

#[derive(Debug, Serialize)]
pub struct PhoneStatusMessage {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub connected: bool,
}

impl ServerMessage {
    pub fn auth_ok(resume_from: i64) -> Self {
        Self::AuthOk(AuthOkMessage {
            msg_type: "auth_ok".into(),
            resume_from,
            phone_connected: None,
        })
    }

    pub fn auth_ok_controller(phone_connected: bool) -> Self {
        Self::AuthOk(AuthOkMessage {
            msg_type: "auth_ok".into(),
            resume_from: 0,
            phone_connected: Some(phone_connected),
        })
    }

    pub fn auth_fail(error: impl Into<String>) -> Self {
        Self::AuthFail(AuthFailMessage {
            msg_type: "auth_fail".into(),
            error: error.into(),
        })
    }

    pub fn ping() -> Self {
        Self::Ping(PingMessage {
            msg_type: "ping".into(),
        })
    }

    pub fn error(error: impl Into<String>) -> Self {
        Self::Error(ErrorMessage {
            msg_type: "error".into(),
            error: error.into(),
        })
    }

    pub fn cmd_accepted(id: i64) -> Self {
        Self::CommandAccepted(CommandAcceptedMessage {
            msg_type: "cmd_accepted".into(),
            id,
        })
    }

    pub fn phone_status(connected: bool) -> Self {
        Self::PhoneStatus(PhoneStatusMessage {
            msg_type: "phone_status".into(),
            connected,
        })
    }

    pub fn version_error(code: &str, message: impl Into<String>) -> Self {
        Self::VersionError(VersionErrorMessage {
            msg_type: "error".into(),
            code: code.into(),
            message: message.into(),
            update_url: "https://screenmcp.com/download".into(),
        })
    }
}

/// Try to figure out what kind of message the phone sent
pub fn parse_phone_message(text: &str) -> Result<PhoneMessage, serde_json::Error> {
    // Try to parse as a JSON value first to route by shape
    let v: serde_json::Value = serde_json::from_str(text)?;

    if let Some(t) = v.get("type").and_then(|t| t.as_str()) {
        match t {
            "auth" => {
                let msg: AuthMessage = serde_json::from_value(v)?;
                return Ok(PhoneMessage::Auth(msg));
            }
            "pong" => {
                let msg: PongMessage = serde_json::from_value(v)?;
                return Ok(PhoneMessage::Pong(msg));
            }
            _ => {}
        }
    }

    if v.get("ack").is_some() {
        let msg: AckMessage = serde_json::from_value(v)?;
        return Ok(PhoneMessage::Ack(msg));
    }

    if v.get("id").is_some() && v.get("status").is_some() {
        let msg: CommandResponse = serde_json::from_value(v)?;
        return Ok(PhoneMessage::Response(msg));
    }

    // Fallback: try untagged
    serde_json::from_value(v)
}

/// Parse a controller command message
pub fn parse_controller_message(text: &str) -> Result<ControllerCommand, serde_json::Error> {
    serde_json::from_str(text)
}
