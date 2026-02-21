use crate::config::Config;
use crate::ws::WsCommand;
use reqwest::Client;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{info, warn};

/// Run the SSE listener for open source server mode.
/// Connects to `{api_url}/api/events` with Bearer token = user_id.
/// On receiving a "connect" event with matching device_id, sends a ConnectToWorker command.
/// Reconnects with exponential backoff on disconnect.
pub async fn run_sse_listener(
    config: Config,
    ws_cmd_tx: mpsc::Sender<WsCommand>,
    mut shutdown_rx: tokio::sync::broadcast::Receiver<()>,
) {
    let api_url = config.opensource_api_url.trim_end_matches('/').to_string();
    let token = config.opensource_user_id.clone();
    let device_id = config.device_id.clone();

    let mut backoff_secs: u64 = 1;
    let max_backoff: u64 = 60;

    loop {
        let sse_url = format!("{api_url}/api/events");
        info!("SSE: connecting to {sse_url}");

        let result = run_sse_connection(
            &sse_url,
            &token,
            &device_id,
            &ws_cmd_tx,
            &mut shutdown_rx,
        )
        .await;

        match result {
            SseResult::Shutdown => {
                info!("SSE: shutdown requested, stopping listener");
                return;
            }
            SseResult::Connected => {
                // Was connected and then lost connection; reset backoff
                backoff_secs = 1;
                warn!("SSE: connection lost, reconnecting in {backoff_secs}s");
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(backoff_secs)) => {}
                    _ = shutdown_rx.recv() => {
                        info!("SSE: shutdown during backoff");
                        return;
                    }
                }
            }
            SseResult::Disconnected(reason) => {
                warn!("SSE: disconnected: {reason}, reconnecting in {backoff_secs}s");
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(backoff_secs)) => {}
                    _ = shutdown_rx.recv() => {
                        info!("SSE: shutdown during backoff");
                        return;
                    }
                }
                backoff_secs = (backoff_secs * 2).min(max_backoff);
            }
        }
    }
}

enum SseResult {
    Shutdown,
    /// Successfully connected then lost connection (reset backoff).
    Connected,
    /// Failed to connect (increase backoff).
    Disconnected(String),
}

async fn run_sse_connection(
    url: &str,
    token: &str,
    device_id: &str,
    ws_cmd_tx: &mpsc::Sender<WsCommand>,
    shutdown_rx: &mut tokio::sync::broadcast::Receiver<()>,
) -> SseResult {
    let client = Client::new();
    let response = match client
        .get(url)
        .header("Authorization", format!("Bearer {token}"))
        .header("Accept", "text/event-stream")
        .header("Cache-Control", "no-cache")
        .send()
        .await
    {
        Ok(resp) => resp,
        Err(e) => {
            return SseResult::Disconnected(format!("request failed: {e}"));
        }
    };

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return SseResult::Disconnected(format!("HTTP {status}: {body}"));
    }

    info!("SSE: connected, reading event stream");

    // Read the response body as a byte stream and parse SSE events
    let mut stream = response.bytes_stream();
    use futures_util::StreamExt;
    let mut buffer = String::new();

    loop {
        tokio::select! {
            chunk = stream.next() => {
                match chunk {
                    Some(Ok(bytes)) => {
                        let text = String::from_utf8_lossy(&bytes);
                        buffer.push_str(&text);

                        // Process complete SSE events from the buffer.
                        // SSE events are delimited by double newlines (\n\n or \r\n\r\n).
                        loop {
                            // Find the earliest double-newline separator
                            let sep_pos = buffer.find("\n\n")
                                .map(|p| (p, 2))
                                .or_else(|| buffer.find("\r\n\r\n").map(|p| (p, 4)));

                            if let Some((pos, sep_len)) = sep_pos {
                                let event_block = buffer[..pos].to_string();
                                buffer = buffer[pos + sep_len..].to_string();

                                if let Some(action) = parse_sse_event(&event_block, device_id) {
                                    match action {
                                        SseAction::ConnectToWorker(ws_url) => {
                                            info!("SSE: received connect event, wsUrl={ws_url}");
                                            let _ = ws_cmd_tx.send(WsCommand::ConnectToWorker(ws_url)).await;
                                        }
                                    }
                                }
                            } else {
                                break;
                            }
                        }
                    }
                    Some(Err(_)) => {
                        return SseResult::Connected;
                    }
                    None => {
                        return SseResult::Connected;
                    }
                }
            }
            _ = shutdown_rx.recv() => {
                return SseResult::Shutdown;
            }
        }
    }
}

enum SseAction {
    ConnectToWorker(String),
}

/// Parse an SSE event block. SSE format:
/// ```
/// event: <type>
/// data: <json>
/// ```
/// or just:
/// ```
/// data: <json>
/// ```
fn parse_sse_event(block: &str, my_device_id: &str) -> Option<SseAction> {
    let mut data_lines: Vec<&str> = Vec::new();

    for line in block.lines() {
        if let Some(rest) = line.strip_prefix("data:") {
            data_lines.push(rest.trim_start());
        }
        // We ignore "event:", "id:", "retry:" fields and just use the data payload.
    }

    if data_lines.is_empty() {
        return None;
    }

    let data_str = data_lines.join("\n");
    let json: serde_json::Value = match serde_json::from_str(&data_str) {
        Ok(v) => v,
        Err(e) => {
            warn!("SSE: failed to parse event data: {e}, raw: {data_str}");
            return None;
        }
    };

    let event_type = json.get("type").and_then(|v| v.as_str()).unwrap_or("");

    match event_type {
        "connect" => {
            let ws_url = json.get("wsUrl").and_then(|v| v.as_str())?;
            let target_device_id = json.get("target_device_id").and_then(|v| v.as_str()).unwrap_or("");

            if target_device_id == my_device_id {
                info!("SSE: connect event matches our device_id");
                Some(SseAction::ConnectToWorker(ws_url.to_string()))
            } else {
                info!("SSE: connect event for device {target_device_id}, ignoring (we are {my_device_id})");
                None
            }
        }
        _ => {
            info!("SSE: ignoring event type: {event_type}");
            None
        }
    }
}
