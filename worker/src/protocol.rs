use serde::{Deserialize, Serialize};

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
    pub token: String,
    #[serde(default)]
    pub last_ack: i64,
    #[serde(default = "default_role")]
    pub role: String, // "phone" or "controller"
    #[serde(default)]
    pub target_device_id: Option<String>, // for controllers: which phone to control
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
    CommandAccepted(CommandAcceptedMessage),
    PhoneStatus(PhoneStatusMessage),
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
