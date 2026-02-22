use thiserror::Error;

#[derive(Debug, Error)]
pub enum ScreenMCPError {
    #[error("authentication failed: {0}")]
    Auth(String),

    #[error("connection error: {0}")]
    Connection(String),

    #[error("command error: {0}")]
    Command(String),

    #[error("command timed out: {0}")]
    Timeout(String),

    #[error("not connected")]
    NotConnected,

    #[error("discovery failed ({status}): {body}")]
    Discovery { status: u16, body: String },

    #[error("websocket error: {0}")]
    WebSocket(#[from] tokio_tungstenite::tungstenite::Error),

    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, ScreenMCPError>;
