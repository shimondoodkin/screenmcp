use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::sync::{mpsc, oneshot, Mutex, Notify};
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, error, info, warn};

use crate::error::{Result, ScreenMCPError};
use crate::types::*;

const DEFAULT_API_URL: &str = "https://screenmcp.com";
const DEFAULT_COMMAND_TIMEOUT_MS: u64 = 30_000;

/// Internal command sent to the writer task.
enum WriterCmd {
    Send(String),
    Close,
}

/// Shared state between the client and the background recv task.
struct SharedState {
    /// Pending commands awaiting responses, keyed by server-assigned ID.
    pending: HashMap<i64, oneshot::Sender<CommandResponse>>,
    /// The most recent temp ID (for mapping cmd_accepted → pending entry).
    last_temp_id: i64,
    /// Whether the phone is connected to the worker.
    phone_connected: bool,
    /// Whether we are connected and authenticated.
    connected: bool,
}

/// ScreenMCP SDK client.
///
/// Connects to the ScreenMCP infrastructure (API server + worker relay) and
/// provides typed methods for every supported phone/desktop command.
///
/// ```no_run
/// use screenmcp::{ScreenMCPClient, ClientOptions};
///
/// # async fn example() -> screenmcp::Result<()> {
/// let mut phone = ScreenMCPClient::new(ClientOptions {
///     api_key: "pk_...".into(),
///     api_url: None,
///     device_id: None,
///     command_timeout_ms: None,
///     auto_reconnect: None,
/// });
/// phone.connect().await?;
/// let screenshot = phone.screenshot().await?;
/// phone.click(540, 1200).await?;
/// phone.disconnect().await?;
/// # Ok(())
/// # }
/// ```
pub struct ScreenMCPClient {
    api_key: String,
    api_url: String,
    device_id: Option<String>,
    command_timeout: Duration,
    auto_reconnect: bool,

    state: Arc<Mutex<SharedState>>,
    writer_tx: Option<mpsc::UnboundedSender<WriterCmd>>,
    recv_task: Option<tokio::task::JoinHandle<()>>,
    writer_task: Option<tokio::task::JoinHandle<()>>,
    http_client: reqwest::Client,

    /// Notified when auth completes (ok or fail).
    auth_notify: Arc<Notify>,
    /// Auth result from the recv task.
    auth_result: Arc<Mutex<Option<std::result::Result<bool, String>>>>,

    /// Worker URL currently in use.
    worker_url: Option<String>,

    /// Temp ID counter for pending command mapping.
    temp_id_counter: i64,
}

impl ScreenMCPClient {
    /// Create a new client with the given options.
    pub fn new(options: ClientOptions) -> Self {
        let api_url = options
            .api_url
            .unwrap_or_else(|| DEFAULT_API_URL.to_string())
            .trim_end_matches('/')
            .to_string();

        let command_timeout = Duration::from_millis(
            options.command_timeout_ms.unwrap_or(DEFAULT_COMMAND_TIMEOUT_MS),
        );

        Self {
            api_key: options.api_key,
            api_url,
            device_id: options.device_id,
            command_timeout,
            auto_reconnect: options.auto_reconnect.unwrap_or(true),
            state: Arc::new(Mutex::new(SharedState {
                pending: HashMap::new(),
                last_temp_id: 0,
                phone_connected: false,
                connected: false,
            })),
            writer_tx: None,
            recv_task: None,
            writer_task: None,
            http_client: reqwest::Client::new(),
            auth_notify: Arc::new(Notify::new()),
            auth_result: Arc::new(Mutex::new(None)),
            worker_url: None,
            temp_id_counter: 0,
        }
    }

    /// Discover a worker via the API, then connect to it via WebSocket.
    pub async fn connect(&mut self) -> Result<()> {
        let ws_url = self.discover().await?;
        self.worker_url = Some(ws_url.clone());
        self.connect_ws(&ws_url).await
    }

    /// Gracefully close the connection. Disables auto-reconnect.
    pub async fn disconnect(&mut self) -> Result<()> {
        self.auto_reconnect = false;
        if let Some(tx) = self.writer_tx.take() {
            let _ = tx.send(WriterCmd::Close);
        }
        if let Some(h) = self.recv_task.take() {
            let _ = h.await;
        }
        if let Some(h) = self.writer_task.take() {
            let _ = h.await;
        }
        let mut st = self.state.lock().await;
        st.connected = false;
        st.pending.clear();
        Ok(())
    }

    /// Whether the client is connected to the worker.
    pub async fn connected(&self) -> bool {
        self.state.lock().await.connected
    }

    /// Whether the target phone is currently connected to the worker.
    pub async fn phone_connected(&self) -> bool {
        self.state.lock().await.phone_connected
    }

    /// The worker WebSocket URL currently in use.
    pub fn worker_url(&self) -> Option<&str> {
        self.worker_url.as_deref()
    }

    // -----------------------------------------------------------------------
    // Phone commands
    // -----------------------------------------------------------------------

    /// Take a screenshot. Returns the base64-encoded WebP image.
    pub async fn screenshot(&mut self) -> Result<ScreenshotResult> {
        let resp = self.send_command("screenshot", None).await?;
        let result: ScreenshotResult = resp
            .result
            .map(|v| serde_json::from_value(v).unwrap_or(ScreenshotResult { image: String::new() }))
            .unwrap_or(ScreenshotResult { image: String::new() });
        Ok(result)
    }

    /// Tap at the given screen coordinates.
    pub async fn click(&mut self, x: i32, y: i32) -> Result<()> {
        self.send_command("click", Some(serde_json::json!({ "x": x, "y": y })))
            .await?;
        Ok(())
    }

    /// Long-press at the given screen coordinates.
    pub async fn long_click(&mut self, x: i32, y: i32) -> Result<()> {
        self.send_command("long_click", Some(serde_json::json!({ "x": x, "y": y })))
            .await?;
        Ok(())
    }

    /// Drag from (start_x, start_y) to (end_x, end_y).
    pub async fn drag(
        &mut self,
        start_x: i32,
        start_y: i32,
        end_x: i32,
        end_y: i32,
    ) -> Result<()> {
        self.send_command(
            "drag",
            Some(serde_json::json!({
                "startX": start_x,
                "startY": start_y,
                "endX": end_x,
                "endY": end_y,
            })),
        )
        .await?;
        Ok(())
    }

    /// Scroll the screen.
    pub async fn scroll(&mut self, direction: ScrollDirection, amount: Option<i32>) -> Result<()> {
        let dist = amount.unwrap_or(300);
        let center_x = 540;
        let center_y = 1200;
        let (dx, dy) = match direction {
            ScrollDirection::Up => (0, -dist),
            ScrollDirection::Down => (0, dist),
            ScrollDirection::Left => (-dist, 0),
            ScrollDirection::Right => (dist, 0),
        };
        self.send_command(
            "scroll",
            Some(serde_json::json!({ "x": center_x, "y": center_y, "dx": dx, "dy": dy })),
        )
        .await?;
        Ok(())
    }

    /// Type text into the currently focused input field.
    pub async fn type_text(&mut self, text: &str) -> Result<()> {
        self.send_command("type", Some(serde_json::json!({ "text": text })))
            .await?;
        Ok(())
    }

    /// Get text from the currently focused element.
    pub async fn get_text(&mut self) -> Result<TextResult> {
        let resp = self.send_command("get_text", None).await?;
        let result: TextResult = resp
            .result
            .map(|v| serde_json::from_value(v).unwrap_or(TextResult { text: String::new() }))
            .unwrap_or(TextResult { text: String::new() });
        Ok(result)
    }

    /// Select all text in the focused element.
    pub async fn select_all(&mut self) -> Result<()> {
        self.send_command("select_all", None).await?;
        Ok(())
    }

    /// Copy selected text to clipboard.
    pub async fn copy(&mut self) -> Result<CopyResult> {
        let resp = self.send_command("copy", None).await?;
        let result: CopyResult = resp
            .result
            .map(|v| serde_json::from_value(v).unwrap_or(CopyResult { text: None }))
            .unwrap_or(CopyResult { text: None });
        Ok(result)
    }

    /// Paste into the focused field. Optionally set clipboard before pasting.
    pub async fn paste(&mut self, text: Option<&str>) -> Result<()> {
        let params = text.map(|t| serde_json::json!({ "text": t }));
        self.send_command("paste", params).await?;
        Ok(())
    }

    /// Get clipboard text contents.
    pub async fn get_clipboard(&mut self) -> Result<ClipboardResult> {
        let resp = self.send_command("get_clipboard", None).await?;
        let result: ClipboardResult = resp
            .result
            .map(|v| serde_json::from_value(v).unwrap_or(ClipboardResult { text: String::new() }))
            .unwrap_or(ClipboardResult { text: String::new() });
        Ok(result)
    }

    /// Set clipboard to the given text.
    pub async fn set_clipboard(&mut self, text: &str) -> Result<()> {
        self.send_command("set_clipboard", Some(serde_json::json!({ "text": text })))
            .await?;
        Ok(())
    }

    /// Press the Back button.
    pub async fn back(&mut self) -> Result<()> {
        self.send_command("back", None).await?;
        Ok(())
    }

    /// Press the Home button.
    pub async fn home(&mut self) -> Result<()> {
        self.send_command("home", None).await?;
        Ok(())
    }

    /// Open the Recents / app switcher.
    pub async fn recents(&mut self) -> Result<()> {
        self.send_command("recents", None).await?;
        Ok(())
    }

    /// Get the UI accessibility tree.
    pub async fn ui_tree(&mut self) -> Result<UiTreeResult> {
        let resp = self.send_command("ui_tree", None).await?;
        let result: UiTreeResult = resp
            .result
            .map(|v| serde_json::from_value(v).unwrap_or(UiTreeResult { tree: vec![] }))
            .unwrap_or(UiTreeResult { tree: vec![] });
        Ok(result)
    }

    /// List available cameras on the device.
    pub async fn list_cameras(&mut self) -> Result<ListCamerasResult> {
        let resp = self.send_command("list_cameras", None).await?;
        let result: ListCamerasResult = resp
            .result
            .map(|v| {
                serde_json::from_value(v).unwrap_or(ListCamerasResult { cameras: vec![] })
            })
            .unwrap_or(ListCamerasResult { cameras: vec![] });
        Ok(result)
    }

    /// Take a photo with the device camera.
    pub async fn camera(&mut self, camera_id: Option<&str>) -> Result<CameraResult> {
        let params = camera_id.map(|id| serde_json::json!({ "camera": id }));
        let resp = self.send_command("camera", params).await?;
        let result: CameraResult = resp
            .result
            .map(|v| serde_json::from_value(v).unwrap_or(CameraResult { image: String::new() }))
            .unwrap_or(CameraResult { image: String::new() });
        Ok(result)
    }

    // -----------------------------------------------------------------------
    // Keyboard commands (desktop only)
    // -----------------------------------------------------------------------

    /// Press and hold a key (desktop only).
    pub async fn hold_key(&mut self, key: &str) -> Result<()> {
        self.send_command("hold_key", Some(serde_json::json!({ "key": key })))
            .await?;
        Ok(())
    }

    /// Release a held key (desktop only).
    pub async fn release_key(&mut self, key: &str) -> Result<()> {
        self.send_command("release_key", Some(serde_json::json!({ "key": key })))
            .await?;
        Ok(())
    }

    /// Press and release a key in one action (desktop only).
    pub async fn press_key(&mut self, key: &str) -> Result<()> {
        self.send_command("press_key", Some(serde_json::json!({ "key": key })))
            .await?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Generic command
    // -----------------------------------------------------------------------

    /// Send an arbitrary command to the phone.
    pub async fn send_command(
        &mut self,
        cmd: &str,
        params: Option<serde_json::Value>,
    ) -> Result<CommandResponse> {
        let tx = self
            .writer_tx
            .as_ref()
            .ok_or(ScreenMCPError::NotConnected)?;

        // Create the command message (no ID — worker assigns it)
        let msg = ControllerCommand {
            cmd: cmd.to_string(),
            params,
        };
        let json = serde_json::to_string(&msg)?;

        // Create oneshot channel for the response
        let (resp_tx, resp_rx) = oneshot::channel();

        // Use a negative temp ID to track this command until cmd_accepted maps it
        self.temp_id_counter -= 1;
        let temp_id = self.temp_id_counter;

        {
            let mut st = self.state.lock().await;
            st.last_temp_id = temp_id;
            st.pending.insert(temp_id, resp_tx);
        }

        // Send the command
        tx.send(WriterCmd::Send(json))
            .map_err(|_| ScreenMCPError::NotConnected)?;

        // Wait for response with timeout
        match timeout(self.command_timeout, resp_rx).await {
            Ok(Ok(resp)) => {
                if resp.status == "ok" {
                    Ok(resp)
                } else {
                    Err(ScreenMCPError::Command(
                        resp.error
                            .unwrap_or_else(|| format!("command failed: {}", resp.status)),
                    ))
                }
            }
            Ok(Err(_)) => {
                // Channel closed (connection dropped)
                let mut st = self.state.lock().await;
                st.pending.remove(&temp_id);
                Err(ScreenMCPError::Connection("connection closed".into()))
            }
            Err(_) => {
                // Timeout
                let mut st = self.state.lock().await;
                st.pending.remove(&temp_id);
                Err(ScreenMCPError::Timeout(cmd.to_string()))
            }
        }
    }

    // -----------------------------------------------------------------------
    // Internal: discovery & WebSocket
    // -----------------------------------------------------------------------

    async fn discover(&self) -> Result<String> {
        let url = format!("{}/api/discover", self.api_url);

        let mut body = serde_json::Map::new();
        if let Some(ref device_id) = self.device_id {
            body.insert("device_id".into(), serde_json::json!(device_id));
        }

        let resp = self
            .http_client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&body)
            .send()
            .await?;

        let status = resp.status().as_u16();
        if status != 200 {
            let body_text = resp.text().await.unwrap_or_default();
            return Err(ScreenMCPError::Discovery {
                status,
                body: body_text,
            });
        }

        let data: DiscoverResponse = resp.json().await?;
        if data.ws_url.is_empty() {
            return Err(ScreenMCPError::Discovery {
                status: 200,
                body: "discovery returned no wsUrl".into(),
            });
        }

        Ok(data.ws_url)
    }

    async fn connect_ws(&mut self, ws_url: &str) -> Result<()> {
        info!("connecting to worker: {}", ws_url);

        let (ws_stream, _) = tokio_tungstenite::connect_async(ws_url).await?;
        let (ws_write, ws_read) = ws_stream.split();

        // Channel for sending messages to the WebSocket
        let (writer_tx, mut writer_rx) = mpsc::unbounded_channel::<WriterCmd>();
        self.writer_tx = Some(writer_tx.clone());

        // Send auth message
        let auth = AuthMsg {
            msg_type: "auth",
            key: self.api_key.clone(),
            role: "controller",
            target_device_id: self.device_id.clone(),
            last_ack: 0,
        };
        let auth_json = serde_json::to_string(&auth)?;
        let _ = writer_tx.send(WriterCmd::Send(auth_json));

        // Reset auth state
        *self.auth_result.lock().await = None;

        let state = self.state.clone();
        let auth_notify = self.auth_notify.clone();
        let auth_result = self.auth_result.clone();

        // Writer task: reads from channel, writes to WebSocket
        let writer_task = tokio::spawn(async move {
            let mut ws_write = ws_write;
            while let Some(cmd) = writer_rx.recv().await {
                match cmd {
                    WriterCmd::Send(text) => {
                        if let Err(e) = ws_write.send(Message::Text(text.into())).await {
                            debug!("ws write error: {}", e);
                            break;
                        }
                    }
                    WriterCmd::Close => {
                        let _ = ws_write.close().await;
                        break;
                    }
                }
            }
        });

        // Receiver task: reads from WebSocket, dispatches to pending commands
        let recv_writer_tx = writer_tx.clone();
        let recv_task = tokio::spawn(async move {
            let mut ws_read = ws_read;
            while let Some(msg_result) = ws_read.next().await {
                let msg = match msg_result {
                    Ok(m) => m,
                    Err(e) => {
                        debug!("ws read error: {}", e);
                        break;
                    }
                };

                let text = match msg {
                    Message::Text(t) => t.to_string(),
                    Message::Close(_) => break,
                    _ => continue,
                };

                let server_msg = match ServerMessage::parse(&text) {
                    Some(m) => m,
                    None => {
                        debug!("unknown message: {}", text);
                        continue;
                    }
                };

                match server_msg {
                    ServerMessage::AuthOk { phone_connected } => {
                        let mut st = state.lock().await;
                        st.connected = true;
                        st.phone_connected = phone_connected;
                        *auth_result.lock().await = Some(Ok(phone_connected));
                        auth_notify.notify_one();
                        info!("authenticated, phone_connected={}", phone_connected);
                    }
                    ServerMessage::AuthFail { error } => {
                        *auth_result.lock().await = Some(Err(error.clone()));
                        auth_notify.notify_one();
                        error!("auth failed: {}", error);
                        break;
                    }
                    ServerMessage::CmdAccepted { id } => {
                        let mut st = state.lock().await;
                        let temp_id = st.last_temp_id;
                        if let Some(sender) = st.pending.remove(&temp_id) {
                            st.pending.insert(id, sender);
                        }
                    }
                    ServerMessage::CommandResponse(resp) => {
                        let mut st = state.lock().await;
                        if let Some(sender) = st.pending.remove(&resp.id) {
                            let _ = sender.send(resp);
                        }
                    }
                    ServerMessage::PhoneStatus { connected } => {
                        let mut st = state.lock().await;
                        st.phone_connected = connected;
                        debug!("phone_status: connected={}", connected);
                    }
                    ServerMessage::Ping => {
                        let pong = PongMsg { msg_type: "pong" };
                        if let Ok(json) = serde_json::to_string(&pong) {
                            let _ = recv_writer_tx.send(WriterCmd::Send(json));
                        }
                    }
                    ServerMessage::Error { error } => {
                        warn!("server error: {}", error);
                    }
                }
            }

            // Connection closed — reject all pending commands
            let mut st = state.lock().await;
            st.connected = false;
            for (_, sender) in st.pending.drain() {
                let _ = sender.send(CommandResponse {
                    id: 0,
                    status: "error".into(),
                    result: None,
                    error: Some("connection closed".into()),
                });
            }
        });

        self.recv_task = Some(recv_task);
        self.writer_task = Some(writer_task);

        // Wait for auth result
        self.auth_notify.notified().await;

        let result = self.auth_result.lock().await.take();
        match result {
            Some(Ok(_)) => Ok(()),
            Some(Err(e)) => Err(ScreenMCPError::Auth(e)),
            None => Err(ScreenMCPError::Auth("no auth response".into())),
        }
    }

    /// Reconnect with exponential backoff.
    pub async fn reconnect(&mut self) -> Result<()> {
        let delays = [1000u64, 2000, 4000, 8000, 16000, 30000];

        for delay_ms in &delays {
            tokio::time::sleep(Duration::from_millis(*delay_ms)).await;

            match self.discover().await {
                Ok(ws_url) => {
                    self.worker_url = Some(ws_url.clone());
                    match self.connect_ws(&ws_url).await {
                        Ok(()) => {
                            info!("reconnected to {}", ws_url);
                            return Ok(());
                        }
                        Err(e) => {
                            warn!("reconnect ws failed: {}", e);
                        }
                    }
                }
                Err(e) => {
                    warn!("reconnect discover failed: {}", e);
                }
            }
        }

        Err(ScreenMCPError::Connection(
            "reconnect failed after all attempts".into(),
        ))
    }
}

impl Drop for ScreenMCPClient {
    fn drop(&mut self) {
        if let Some(tx) = self.writer_tx.take() {
            let _ = tx.send(WriterCmd::Close);
        }
    }
}
