use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, watch};
use tokio::time::{interval, Instant};
use tokio_tungstenite::tungstenite::Message;
use tracing::{error, info, warn};

use crate::commands;
use crate::config::Config;

/// Connection status shared with the tray.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionStatus {
    Disconnected,
    Connecting,
    Connected,
    Reconnecting,
    Error(String),
}

impl std::fmt::Display for ConnectionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Disconnected => write!(f, "Disconnected"),
            Self::Connecting => write!(f, "Connecting..."),
            Self::Connected => write!(f, "Connected"),
            Self::Reconnecting => write!(f, "Reconnecting..."),
            Self::Error(e) => write!(f, "Error: {e}"),
        }
    }
}

/// Commands from tray to WS manager.
#[derive(Debug)]
pub enum WsCommand {
    Connect,
    /// Connect directly to a specific worker URL (used by SSE events).
    ConnectToWorker(String),
    Disconnect,
    UpdateConfig(Config),
    Shutdown,
}

/// Discover a worker URL from the API server.
async fn discover_worker(api_url: &str, token: &str) -> Result<String, String> {
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{api_url}/api/discover"))
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .send()
        .await
        .map_err(|e| format!("discovery request failed: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("discovery failed ({status}): {body}"));
    }

    let data: Value = resp
        .json()
        .await
        .map_err(|e| format!("discovery parse failed: {e}"))?;

    data.get("wsUrl")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| "discovery returned no wsUrl".to_string())
}

/// Convert an HTTP(S) URL to WS(S) if needed.
fn to_ws_url(url: &str) -> String {
    if url.starts_with("ws://") || url.starts_with("wss://") {
        return url.to_string();
    }
    if url.starts_with("https://") {
        return url.replacen("https://", "wss://", 1);
    }
    if url.starts_with("http://") {
        return url.replacen("http://", "ws://", 1);
    }
    format!("wss://{url}")
}

/// Run the WebSocket manager loop. This handles connect/disconnect/reconnect.
pub async fn run_ws_manager(
    mut cmd_rx: mpsc::Receiver<WsCommand>,
    status_tx: watch::Sender<ConnectionStatus>,
    initial_config: Config,
) {
    let config = Arc::new(tokio::sync::RwLock::new(initial_config.clone()));

    // Auto-connect on startup if configured (but not in opensource mode — SSE handles that)
    let should_auto_connect = initial_config.auto_connect
        && initial_config.is_ready()
        && !initial_config.opensource_server_enabled;

    let (internal_tx, mut internal_rx) = mpsc::channel::<WsCommand>(16);

    if should_auto_connect {
        let _ = internal_tx.send(WsCommand::Connect).await;
    }

    // Start SSE listener if opensource mode is enabled on startup
    let mut sse_task: Option<tokio::task::JoinHandle<()>> = None;
    if initial_config.opensource_server_enabled && initial_config.is_ready() {
        let cfg = initial_config.clone();
        let sse_tx = internal_tx.clone();
        sse_task = Some(tokio::spawn(async move {
            run_sse_listener(cfg, sse_tx).await;
        }));
    }

    let mut connection_task: Option<tokio::task::JoinHandle<()>> = None;
    let (disconnect_tx, _) = tokio::sync::broadcast::channel::<()>(1);

    loop {
        let cmd = tokio::select! {
            Some(cmd) = cmd_rx.recv() => cmd,
            Some(cmd) = internal_rx.recv() => cmd,
            else => break,
        };

        match cmd {
            WsCommand::Connect => {
                // Cancel any existing connection
                if let Some(handle) = connection_task.take() {
                    handle.abort();
                }

                let cfg = config.read().await.clone();
                if !cfg.is_ready() {
                    let _ = status_tx.send(ConnectionStatus::Error(
                        "No token configured. Edit config file.".to_string(),
                    ));
                    info!("config not ready, config path: {}", Config::config_path().display());
                    continue;
                }

                let status_tx2 = status_tx.clone();
                let disconnect_rx = disconnect_tx.subscribe();
                let internal_tx2 = internal_tx.clone();

                connection_task = Some(tokio::spawn(async move {
                    run_connection(cfg, status_tx2, disconnect_rx, internal_tx2).await;
                }));
            }
            WsCommand::ConnectToWorker(ws_url) => {
                // Cancel any existing connection
                if let Some(handle) = connection_task.take() {
                    handle.abort();
                }

                let cfg = config.read().await.clone();
                if !cfg.is_ready() {
                    let _ = status_tx.send(ConnectionStatus::Error(
                        "No token configured. Edit config file.".to_string(),
                    ));
                    continue;
                }

                let status_tx2 = status_tx.clone();
                let disconnect_rx = disconnect_tx.subscribe();
                let internal_tx2 = internal_tx.clone();

                info!("connecting directly to worker: {ws_url}");
                connection_task = Some(tokio::spawn(async move {
                    run_connection_to_worker(cfg, ws_url, status_tx2, disconnect_rx, internal_tx2).await;
                }));
            }
            WsCommand::Disconnect => {
                let _ = disconnect_tx.send(());
                if let Some(handle) = connection_task.take() {
                    handle.abort();
                }
                let _ = status_tx.send(ConnectionStatus::Disconnected);
                info!("disconnected by user");
            }
            WsCommand::UpdateConfig(new_config) => {
                // Check if opensource mode changed
                let old_config = config.read().await.clone();
                let oss_changed = old_config.opensource_server_enabled != new_config.opensource_server_enabled
                    || old_config.opensource_user_id != new_config.opensource_user_id
                    || old_config.opensource_api_url != new_config.opensource_api_url;

                *config.write().await = new_config.clone();
                info!("config updated");

                // Restart SSE listener if opensource settings changed
                if oss_changed {
                    if let Some(handle) = sse_task.take() {
                        handle.abort();
                        info!("stopped previous SSE listener");
                    }
                    if new_config.opensource_server_enabled && new_config.is_ready() {
                        let cfg = new_config.clone();
                        let sse_tx = internal_tx.clone();
                        sse_task = Some(tokio::spawn(async move {
                            run_sse_listener(cfg, sse_tx).await;
                        }));
                        info!("started SSE listener for opensource mode");
                    }
                }
            }
            WsCommand::Shutdown => {
                let _ = disconnect_tx.send(());
                if let Some(handle) = connection_task.take() {
                    handle.abort();
                }
                if let Some(handle) = sse_task.take() {
                    handle.abort();
                }
                break;
            }
        }
    }

    info!("ws manager shutting down");
}

/// Run a single connection attempt with auto-reconnect.
async fn run_connection(
    config: Config,
    status_tx: watch::Sender<ConnectionStatus>,
    mut disconnect_rx: tokio::sync::broadcast::Receiver<()>,
    reconnect_tx: mpsc::Sender<WsCommand>,
) {
    let _ = status_tx.send(ConnectionStatus::Connecting);

    // Discover worker URL
    let ws_url = if let Some(ref direct_url) = config.worker_url {
        to_ws_url(direct_url)
    } else {
        match discover_worker(config.effective_api_url(), config.effective_token()).await {
            Ok(url) => to_ws_url(&url),
            Err(e) => {
                error!("worker discovery failed: {e}");
                let _ = status_tx.send(ConnectionStatus::Error(e));
                schedule_reconnect(reconnect_tx, 5).await;
                return;
            }
        }
    };

    info!("connecting to worker: {ws_url}");

    // Connect WebSocket
    let ws_stream = match tokio_tungstenite::connect_async(&ws_url).await {
        Ok((stream, _)) => stream,
        Err(e) => {
            error!("websocket connect failed: {e}");
            let _ = status_tx.send(ConnectionStatus::Error(format!("WS connect failed: {e}")));
            schedule_reconnect(reconnect_tx, 5).await;
            return;
        }
    };

    let (mut ws_tx, mut ws_rx) = ws_stream.split();

    // Send auth message as a phone
    let auth_msg = json!({
        "type": "auth",
        "user_id": config.effective_token(),
        "role": "phone",
        "device_id": config.device_id,
        "last_ack": 0
    });

    if let Err(e) = ws_tx
        .send(Message::Text(auth_msg.to_string().into()))
        .await
    {
        error!("failed to send auth: {e}");
        let _ = status_tx.send(ConnectionStatus::Error(format!("Auth send failed: {e}")));
        schedule_reconnect(reconnect_tx, 5).await;
        return;
    }

    // Wait for auth_ok
    let auth_timeout = tokio::time::sleep(Duration::from_secs(10));
    tokio::pin!(auth_timeout);

    let auth_result = loop {
        tokio::select! {
            _ = &mut auth_timeout => {
                break Err("auth timeout".to_string());
            }
            msg = ws_rx.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(v) = serde_json::from_str::<Value>(&text.to_string()) {
                            match v.get("type").and_then(|t| t.as_str()) {
                                Some("auth_ok") => {
                                    let resume_from = v.get("resume_from").and_then(|r| r.as_i64()).unwrap_or(0);
                                    info!("authenticated, resume_from={resume_from}");
                                    break Ok(());
                                }
                                Some("auth_fail") => {
                                    let error = v.get("error").and_then(|e| e.as_str()).unwrap_or("unknown");
                                    break Err(format!("auth failed: {error}"));
                                }
                                _ => {}
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        break Err("connection closed during auth".to_string());
                    }
                    _ => {}
                }
            }
            _ = disconnect_rx.recv() => {
                break Err("disconnected by user".to_string());
            }
        }
    };

    if let Err(e) = auth_result {
        error!("auth failed: {e}");
        let _ = status_tx.send(ConnectionStatus::Error(e));
        schedule_reconnect(reconnect_tx, 10).await;
        return;
    }

    let _ = status_tx.send(ConnectionStatus::Connected);
    info!("connected and authenticated as phone");

    // Main message loop
    let mut heartbeat_interval = interval(Duration::from_secs(30));
    let mut last_pong = Instant::now();
    let config = Arc::new(config);

    loop {
        tokio::select! {
            msg = ws_rx.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        let text_str = text.to_string();
                        if let Ok(v) = serde_json::from_str::<Value>(&text_str) {
                            // Check for ping
                            if v.get("type").and_then(|t| t.as_str()) == Some("ping") {
                                let pong = json!({"type": "pong"});
                                if let Err(e) = ws_tx.send(Message::Text(pong.to_string().into())).await {
                                    warn!("failed to send pong: {e}");
                                    break;
                                }
                                last_pong = Instant::now();
                                continue;
                            }

                            // Check for command (has id + cmd fields)
                            if let (Some(id), Some(cmd)) = (
                                v.get("id").and_then(|i| i.as_i64()),
                                v.get("cmd").and_then(|c| c.as_str()).map(|s| s.to_string()),
                            ) {
                                info!("received command id={id} cmd={cmd}");

                                // First send ack
                                let ack = json!({"ack": id});
                                if let Err(e) = ws_tx.send(Message::Text(ack.to_string().into())).await {
                                    warn!("failed to send ack: {e}");
                                    break;
                                }

                                // Execute command in a blocking thread (some commands do I/O)
                                let params = v.get("params").cloned();
                                let config_clone = config.clone();
                                let response = tokio::task::spawn_blocking(move || {
                                    commands::execute_command(
                                        id,
                                        &cmd,
                                        params.as_ref(),
                                        &config_clone,
                                    )
                                })
                                .await
                                .unwrap_or_else(|e| {
                                    json!({
                                        "id": id,
                                        "status": "error",
                                        "error": format!("command panicked: {e}")
                                    })
                                });

                                let resp_str = response.to_string();
                                if let Err(e) = ws_tx.send(Message::Text(resp_str.into())).await {
                                    warn!("failed to send response: {e}");
                                    break;
                                }
                                info!("sent response for command id={id}");
                            }
                        }
                    }
                    Some(Ok(Message::Ping(data))) => {
                        let _ = ws_tx.send(Message::Pong(data)).await;
                        last_pong = Instant::now();
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        info!("websocket closed by server");
                        break;
                    }
                    Some(Err(e)) => {
                        warn!("websocket error: {e}");
                        break;
                    }
                    _ => {}
                }
            }
            _ = heartbeat_interval.tick() => {
                if last_pong.elapsed() > Duration::from_secs(90) {
                    warn!("no pong received in 90s, disconnecting");
                    break;
                }
            }
            _ = disconnect_rx.recv() => {
                info!("disconnect requested");
                let _ = ws_tx.close().await;
                return; // Don't reconnect
            }
        }
    }

    // Connection lost, schedule reconnect
    let _ = status_tx.send(ConnectionStatus::Reconnecting);
    schedule_reconnect(reconnect_tx, 3).await;
}

/// Run a connection directly to a specific worker URL (skipping discovery).
/// Used when SSE provides the worker URL directly.
async fn run_connection_to_worker(
    config: Config,
    ws_url: String,
    status_tx: watch::Sender<ConnectionStatus>,
    mut disconnect_rx: tokio::sync::broadcast::Receiver<()>,
    _reconnect_tx: mpsc::Sender<WsCommand>,
) {
    let _ = status_tx.send(ConnectionStatus::Connecting);

    let ws_url = to_ws_url(&ws_url);
    info!("connecting to worker (direct): {ws_url}");

    // Connect WebSocket
    let ws_stream = match tokio_tungstenite::connect_async(&ws_url).await {
        Ok((stream, _)) => stream,
        Err(e) => {
            error!("websocket connect failed: {e}");
            let _ = status_tx.send(ConnectionStatus::Error(format!("WS connect failed: {e}")));
            // Don't schedule reconnect here — SSE will send a new connect event
            return;
        }
    };

    let (mut ws_tx, mut ws_rx) = ws_stream.split();

    // Send auth message as a phone
    let auth_msg = json!({
        "type": "auth",
        "user_id": config.effective_token(),
        "role": "phone",
        "device_id": config.device_id,
        "last_ack": 0
    });

    if let Err(e) = ws_tx
        .send(Message::Text(auth_msg.to_string().into()))
        .await
    {
        error!("failed to send auth: {e}");
        let _ = status_tx.send(ConnectionStatus::Error(format!("Auth send failed: {e}")));
        return;
    }

    // Wait for auth_ok
    let auth_timeout = tokio::time::sleep(Duration::from_secs(10));
    tokio::pin!(auth_timeout);

    let auth_result = loop {
        tokio::select! {
            _ = &mut auth_timeout => {
                break Err("auth timeout".to_string());
            }
            msg = ws_rx.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(v) = serde_json::from_str::<Value>(&text.to_string()) {
                            match v.get("type").and_then(|t| t.as_str()) {
                                Some("auth_ok") => {
                                    let resume_from = v.get("resume_from").and_then(|r| r.as_i64()).unwrap_or(0);
                                    info!("authenticated, resume_from={resume_from}");
                                    break Ok(());
                                }
                                Some("auth_fail") => {
                                    let error = v.get("error").and_then(|e| e.as_str()).unwrap_or("unknown");
                                    break Err(format!("auth failed: {error}"));
                                }
                                _ => {}
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        break Err("connection closed during auth".to_string());
                    }
                    _ => {}
                }
            }
            _ = disconnect_rx.recv() => {
                break Err("disconnected by user".to_string());
            }
        }
    };

    if let Err(e) = auth_result {
        error!("auth failed: {e}");
        let _ = status_tx.send(ConnectionStatus::Error(e));
        return;
    }

    let _ = status_tx.send(ConnectionStatus::Connected);
    info!("connected and authenticated as phone (direct worker)");

    // Main message loop (same as run_connection)
    let mut heartbeat_interval = interval(Duration::from_secs(30));
    let mut last_pong = Instant::now();
    let config = Arc::new(config);

    loop {
        tokio::select! {
            msg = ws_rx.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        let text_str = text.to_string();
                        if let Ok(v) = serde_json::from_str::<Value>(&text_str) {
                            if v.get("type").and_then(|t| t.as_str()) == Some("ping") {
                                let pong = json!({"type": "pong"});
                                if let Err(e) = ws_tx.send(Message::Text(pong.to_string().into())).await {
                                    warn!("failed to send pong: {e}");
                                    break;
                                }
                                last_pong = Instant::now();
                                continue;
                            }

                            if let (Some(id), Some(cmd)) = (
                                v.get("id").and_then(|i| i.as_i64()),
                                v.get("cmd").and_then(|c| c.as_str()).map(|s| s.to_string()),
                            ) {
                                info!("received command id={id} cmd={cmd}");

                                let ack = json!({"ack": id});
                                if let Err(e) = ws_tx.send(Message::Text(ack.to_string().into())).await {
                                    warn!("failed to send ack: {e}");
                                    break;
                                }

                                let params = v.get("params").cloned();
                                let config_clone = config.clone();
                                let response = tokio::task::spawn_blocking(move || {
                                    commands::execute_command(id, &cmd, params.as_ref(), &config_clone)
                                })
                                .await
                                .unwrap_or_else(|e| {
                                    json!({
                                        "id": id,
                                        "status": "error",
                                        "error": format!("command panicked: {e}")
                                    })
                                });

                                let resp_str = response.to_string();
                                if let Err(e) = ws_tx.send(Message::Text(resp_str.into())).await {
                                    warn!("failed to send response: {e}");
                                    break;
                                }
                                info!("sent response for command id={id}");
                            }
                        }
                    }
                    Some(Ok(Message::Ping(data))) => {
                        let _ = ws_tx.send(Message::Pong(data)).await;
                        last_pong = Instant::now();
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        info!("websocket closed by server");
                        break;
                    }
                    Some(Err(e)) => {
                        warn!("websocket error: {e}");
                        break;
                    }
                    _ => {}
                }
            }
            _ = heartbeat_interval.tick() => {
                if last_pong.elapsed() > Duration::from_secs(90) {
                    warn!("no pong received in 90s, disconnecting");
                    break;
                }
            }
            _ = disconnect_rx.recv() => {
                info!("disconnect requested");
                let _ = ws_tx.close().await;
                return;
            }
        }
    }

    // Connection lost — don't auto-reconnect, SSE will trigger reconnection
    let _ = status_tx.send(ConnectionStatus::Reconnecting);
}

/// Listen for Server-Sent Events from the opensource API server.
/// When a "connect" event is received with a matching device_id, sends a
/// ConnectToWorker command to the WS manager.
async fn run_sse_listener(config: Config, cmd_tx: mpsc::Sender<WsCommand>) {
    let api_url = config.effective_api_url().trim_end_matches('/').to_string();
    let token = config.effective_token().to_string();
    let device_id = config.device_id.clone();

    let mut backoff_secs = 1u64;
    let max_backoff_secs = 60u64;

    loop {
        // Discover worker URL, then connect SSE there; fall back to api_url/api/events
        let sse_url = match discover_sse_url(&api_url, &token, &device_id).await {
            Some(url) => url,
            None => format!("{api_url}/api/events"),
        };
        info!("SSE: connecting to {sse_url}");

        let client = reqwest::Client::new();
        let result = client
            .get(&sse_url)
            .header("Authorization", format!("Bearer {token}"))
            .header("Accept", "text/event-stream")
            .send()
            .await;

        match result {
            Ok(resp) => {
                if !resp.status().is_success() {
                    let status = resp.status();
                    let body = resp.text().await.unwrap_or_default();
                    warn!("SSE: server returned {status}: {body}");
                    tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
                    backoff_secs = (backoff_secs * 2).min(max_backoff_secs);
                    continue;
                }

                // Reset backoff on successful connection
                backoff_secs = 1;
                info!("SSE: connected successfully");

                let mut stream = resp.bytes_stream();
                let mut buffer = String::new();

                while let Some(chunk_result) = stream.next().await {
                    match chunk_result {
                        Ok(chunk) => {
                            let text = String::from_utf8_lossy(&chunk);
                            buffer.push_str(&text);

                            // Process complete SSE messages (separated by double newline)
                            while let Some(pos) = buffer.find("\n\n") {
                                let message = buffer[..pos].to_string();
                                buffer = buffer[pos + 2..].to_string();

                                // Parse SSE message: look for "data:" lines
                                let mut data_str = String::new();
                                for line in message.lines() {
                                    if let Some(data) = line.strip_prefix("data:") {
                                        if !data_str.is_empty() {
                                            data_str.push('\n');
                                        }
                                        data_str.push_str(data.trim_start());
                                    }
                                }

                                if data_str.is_empty() {
                                    continue;
                                }

                                match serde_json::from_str::<Value>(&data_str) {
                                    Ok(event) => {
                                        let event_type = event.get("type").and_then(|t| t.as_str()).unwrap_or("");
                                        match event_type {
                                            "connect" => {
                                                let target = event.get("target_device_id")
                                                    .and_then(|v| v.as_str())
                                                    .unwrap_or("");
                                                let ws_url = event.get("wsUrl")
                                                    .and_then(|v| v.as_str())
                                                    .unwrap_or("");

                                                if target == device_id {
                                                    info!("SSE: connect event for our device, wsUrl={ws_url}");
                                                    if !ws_url.is_empty() {
                                                        let _ = cmd_tx.send(
                                                            WsCommand::ConnectToWorker(ws_url.to_string())
                                                        ).await;
                                                    }
                                                } else {
                                                    info!("SSE: connect event for different device ({target}), ignoring");
                                                }
                                            }
                                            other => {
                                                info!("SSE: received event type={other}");
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        warn!("SSE: failed to parse event JSON: {e}, data={data_str}");
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            warn!("SSE: stream error: {e}");
                            break;
                        }
                    }
                }

                info!("SSE: stream ended, reconnecting...");
            }
            Err(e) => {
                warn!("SSE: connection failed: {e}");
            }
        }

        // Backoff before reconnecting
        info!("SSE: reconnecting in {backoff_secs}s");
        tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
        backoff_secs = (backoff_secs * 2).min(max_backoff_secs);
    }
}

/// Call POST {api_url}/api/discover to get worker URL, convert to SSE endpoint.
/// Returns None if discover fails (caller should fall back to api_url/api/events).
async fn discover_sse_url(api_url: &str, token: &str, device_id: &str) -> Option<String> {
    let client = reqwest::Client::new();
    let body = serde_json::json!({ "device_id": device_id });
    let resp = client
        .post(format!("{api_url}/api/discover"))
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(body.to_string())
        .send()
        .await
        .ok()?;

    if !resp.status().is_success() {
        warn!("SSE: discover failed: HTTP {}", resp.status());
        return None;
    }

    let json: serde_json::Value = resp.json().await.ok()?;
    let ws_url = json.get("wsUrl")?.as_str()?;

    let http_url = ws_url
        .replace("wss://", "https://")
        .replace("ws://", "http://");
    let http_url = http_url.trim_end_matches('/');

    let sse_url = format!("{http_url}/events?device_id={device_id}");
    info!("SSE: discovered worker URL: {sse_url}");
    Some(sse_url)
}

async fn schedule_reconnect(tx: mpsc::Sender<WsCommand>, delay_secs: u64) {
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(delay_secs)).await;
        let _ = tx.send(WsCommand::Connect).await;
    });
}
