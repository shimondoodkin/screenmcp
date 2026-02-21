use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

use crate::config::Config;
use crate::ws::WsCommand;

/// Run the SSE event listener for open source server mode.
/// Connects to `{api_url}/api/events` with Bearer token = user_id.
/// On receiving a "connect" event with matching device_id, sends ConnectToWorker command.
/// Reconnects on disconnect with exponential backoff.
pub async fn run_sse_listener(
    config: Config,
    ws_cmd_tx: mpsc::Sender<WsCommand>,
    mut shutdown_rx: tokio::sync::broadcast::Receiver<()>,
) {
    let api_url = config.opensource_api_url.trim_end_matches('/').to_string();
    let token = config.opensource_user_id.clone();
    let device_id = config.device_id.clone();

    info!("SSE listener starting");

    let mut backoff_secs: u64 = 1;
    let max_backoff: u64 = 60;

    loop {
        // Discover worker URL, then connect SSE there; fall back to api_url/api/events
        let sse_url = match discover_sse_url(&api_url, &token, &device_id).await {
            Some(url) => url,
            None => format!("{api_url}/api/events"),
        };
        info!("SSE connecting to {sse_url}");

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(0)) // No timeout for SSE stream
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

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
                    error!("SSE connection failed ({status}): {body}");
                } else {
                    info!("SSE connected");
                    backoff_secs = 1; // Reset backoff on successful connection

                    // Read the SSE stream using chunk()
                    let mut buffer = String::new();
                    let mut resp = resp;

                    loop {
                        tokio::select! {
                            chunk_result = resp.chunk() => {
                                match chunk_result {
                                    Ok(Some(bytes)) => {
                                        let text = String::from_utf8_lossy(&bytes);
                                        buffer.push_str(&text);

                                        // Process complete SSE events (separated by double newline)
                                        while let Some(pos) = buffer.find("\n\n") {
                                            let event_text = buffer[..pos].to_string();
                                            buffer = buffer[pos + 2..].to_string();

                                            if let Some(json_str) = parse_sse_data(&event_text) {
                                                handle_sse_event(
                                                    &json_str,
                                                    &device_id,
                                                    &ws_cmd_tx,
                                                ).await;
                                            }
                                        }
                                    }
                                    Ok(None) => {
                                        info!("SSE stream ended");
                                        break;
                                    }
                                    Err(e) => {
                                        warn!("SSE stream error: {e}");
                                        break;
                                    }
                                }
                            }
                            _ = shutdown_rx.recv() => {
                                info!("SSE listener shutting down");
                                return;
                            }
                        }
                    }
                }
            }
            Err(e) => {
                error!("SSE connection error: {e}");
            }
        }

        // Check for shutdown before reconnecting
        let delay = Duration::from_secs(backoff_secs);
        info!("SSE reconnecting in {backoff_secs}s");

        tokio::select! {
            _ = tokio::time::sleep(delay) => {}
            _ = shutdown_rx.recv() => {
                info!("SSE listener shutting down");
                return;
            }
        }

        backoff_secs = (backoff_secs * 2).min(max_backoff);
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
        warn!("SSE discover failed: HTTP {}", resp.status());
        return None;
    }

    let json: serde_json::Value = resp.json().await.ok()?;
    let ws_url = json.get("wsUrl")?.as_str()?;

    // Convert ws(s) URL to http(s)
    let http_url = ws_url
        .replace("wss://", "https://")
        .replace("ws://", "http://");
    let http_url = http_url.trim_end_matches('/');

    let sse_url = format!("{http_url}/events?device_id={device_id}");
    info!("SSE discovered worker URL: {sse_url}");
    Some(sse_url)
}

/// Parse the `data:` field from an SSE event block.
fn parse_sse_data(event_text: &str) -> Option<String> {
    let mut data_parts = Vec::new();
    for line in event_text.lines() {
        if let Some(data) = line.strip_prefix("data:") {
            data_parts.push(data.trim_start().to_string());
        }
    }
    if data_parts.is_empty() {
        return None;
    }
    Some(data_parts.join("\n"))
}

/// Handle a parsed SSE event JSON.
async fn handle_sse_event(
    json_str: &str,
    device_id: &str,
    ws_cmd_tx: &mpsc::Sender<WsCommand>,
) {
    let value: serde_json::Value = match serde_json::from_str(json_str) {
        Ok(v) => v,
        Err(e) => {
            warn!("SSE event parse error: {e}, data: {json_str}");
            return;
        }
    };

    let event_type = value.get("type").and_then(|t| t.as_str()).unwrap_or("");

    match event_type {
        "connect" => {
            let ws_url = value.get("wsUrl").and_then(|v| v.as_str()).unwrap_or("");
            let target_device_id = value
                .get("target_device_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            if target_device_id.is_empty() || ws_url.is_empty() {
                warn!("SSE connect event missing wsUrl or target_device_id");
                return;
            }

            if target_device_id != device_id {
                info!(
                    "SSE connect event for different device: {} (ours: {}), ignoring",
                    target_device_id, device_id
                );
                return;
            }

            info!("SSE connect event for our device, wsUrl={ws_url}");
            if let Err(e) = ws_cmd_tx
                .send(WsCommand::ConnectToWorker(ws_url.to_string()))
                .await
            {
                error!("failed to send ConnectToWorker command: {e}");
            }
        }
        other => {
            info!("SSE event type={other}, ignoring");
        }
    }
}
