use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio::time::{interval, timeout};
use tokio_tungstenite::tungstenite::Message;
use tracing::{error, info, warn};

use crate::connections::Connections;
use crate::protocol::{
    parse_controller_message, parse_phone_message, Command, PhoneMessage, ServerMessage,
};
use crate::state::State;

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);
const HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(60);
const AUTH_TIMEOUT: Duration = Duration::from_secs(10);

/// Verify a token against the API server, returning the firebase_uid for authorization.
async fn verify_token(token: &str, api_url: &str) -> Result<String, String> {
    let client = reqwest::Client::new();
    let res = client
        .post(format!("{api_url}/api/auth/verify"))
        .json(&serde_json::json!({ "token": token }))
        .send()
        .await
        .map_err(|e| format!("verify request failed: {e}"))?;

    if !res.status().is_success() {
        return Err(format!("verify returned {}", res.status()));
    }

    let body: serde_json::Value = res
        .json()
        .await
        .map_err(|e| format!("verify parse failed: {e}"))?;

    body.get("firebase_uid")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| "verify response missing firebase_uid".to_string())
}

/// Handle a new connection: if it's a WebSocket upgrade, proceed; otherwise return HTTP 200.
pub async fn handle_connection(
    stream: TcpStream,
    addr: SocketAddr,
    state: Arc<State>,
    connections: Arc<Connections>,
    api_url: String,
) {
    // Peek at the first bytes to check if this is a WebSocket upgrade
    let mut buf = [0u8; 2048];
    let n = match stream.peek(&mut buf).await {
        Ok(n) => n,
        Err(e) => {
            error!(%addr, "peek failed: {e}");
            return;
        }
    };

    let request_str = String::from_utf8_lossy(&buf[..n]);
    let is_websocket = request_str
        .lines()
        .any(|line| line.to_lowercase().starts_with("upgrade:") && line.to_lowercase().contains("websocket"));

    if !is_websocket {
        // Plain HTTP request — consume the request and respond with 200 OK
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let (mut reader, mut writer) = stream.into_split();
        let mut discard = vec![0u8; 4096];
        let _ = reader.read(&mut discard).await;
        let body = r#"{"status":"ok"}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        let _ = writer.write_all(response.as_bytes()).await;
        let _ = writer.shutdown().await;
        return;
    }

    let ws_stream = match tokio_tungstenite::accept_async(stream).await {
        Ok(ws) => ws,
        Err(e) => {
            error!(%addr, "websocket handshake failed: {e}");
            return;
        }
    };

    info!(%addr, "new connection");

    let (mut ws_tx, mut ws_rx) = ws_stream.split();

    // Wait for auth message with timeout
    let auth_result = timeout(AUTH_TIMEOUT, async {
        while let Some(msg) = ws_rx.next().await {
            match msg {
                Ok(Message::Text(text)) => match parse_phone_message(&text) {
                    Ok(PhoneMessage::Auth(auth)) if auth.msg_type == "auth" => {
                        return Some(auth);
                    }
                    _ => {
                        let err = ServerMessage::error("expected auth message");
                        let json = serde_json::to_string(&err).unwrap();
                        let _ = ws_tx.send(Message::Text(json.into())).await;
                    }
                },
                Ok(Message::Close(_)) => return None,
                Err(e) => {
                    warn!(%addr, "error during auth: {e}");
                    return None;
                }
                _ => {}
            }
        }
        None
    })
    .await;

    let auth = match auth_result {
        Ok(Some(auth)) => auth,
        _ => {
            warn!(%addr, "auth timeout or failed");
            let err = ServerMessage::auth_fail("auth timeout");
            let json = serde_json::to_string(&err).unwrap();
            let _ = ws_tx.send(Message::Text(json.into())).await;
            let _ = ws_tx.close().await;
            return;
        }
    };

    // Extract token from role-specific field: phones send user_id, controllers send key
    let token = match auth.role.as_str() {
        "controller" => auth.key.clone(),
        _ => auth.user_id.clone(),
    };
    let token = match token {
        Some(t) if !t.is_empty() => t,
        _ => {
            warn!(%addr, "missing auth credential");
            let err = ServerMessage::auth_fail("missing auth credential");
            let json = serde_json::to_string(&err).unwrap();
            let _ = ws_tx.send(Message::Text(json.into())).await;
            let _ = ws_tx.close().await;
            return;
        }
    };

    // Verify token via API server (validates auth, returns firebase_uid)
    let firebase_uid = match verify_token(&token, &api_url).await {
        Ok(uid) => uid,
        Err(e) => {
            warn!(%addr, "auth verification failed: {e}");
            let err = ServerMessage::auth_fail("invalid token");
            let json = serde_json::to_string(&err).unwrap();
            let _ = ws_tx.send(Message::Text(json.into())).await;
            let _ = ws_tx.close().await;
            return;
        }
    };
    let role = auth.role.clone();

    // Use client-provided device_id for routing (required for phones, optional for controllers)
    let device_id = match auth.device_id {
        Some(ref id) if !id.is_empty() => id.clone(),
        _ => firebase_uid.clone(), // fallback for backwards compat
    };

    info!(%addr, %firebase_uid, %device_id, %role, "authenticated");

    match role.as_str() {
        "controller" => {
            let target_device_id = auth.target_device_id.unwrap_or_else(|| device_id.clone());
            handle_controller_connection(ws_tx, ws_rx, addr, &device_id, &target_device_id, state, connections)
                .await;
        }
        _ => {
            // Default: phone
            handle_phone_connection(
                ws_tx,
                ws_rx,
                addr,
                &device_id,
                auth.last_ack,
                state,
                connections,
            )
            .await;
        }
    }
}

/// Handle a phone connection: register, replay commands, relay responses.
async fn handle_phone_connection(
    mut ws_tx: futures_util::stream::SplitSink<
        tokio_tungstenite::WebSocketStream<TcpStream>,
        Message,
    >,
    mut ws_rx: futures_util::stream::SplitStream<tokio_tungstenite::WebSocketStream<TcpStream>>,
    addr: SocketAddr,
    device_id: &str,
    last_ack: i64,
    state: Arc<State>,
    connections: Arc<Connections>,
) {
    // Register in Connections (in-memory) — replaces any existing connection for this device_id
    let mut cmd_rx = connections.register_phone(device_id).await;

    // Register in Redis
    if let Err(e) = state.register_connection(device_id).await {
        error!(device_id, "failed to register connection: {e}");
    }

    // Send auth_ok
    let server_ack = state.get_last_ack(device_id).await.unwrap_or(0);
    let resume_from = last_ack.max(server_ack);
    let auth_ok = ServerMessage::auth_ok(resume_from);
    let json = serde_json::to_string(&auth_ok).unwrap();
    if ws_tx.send(Message::Text(json.into())).await.is_err() {
        connections.unregister_phone(device_id).await;
        return;
    }

    // Replay pending commands
    match state.get_pending_commands(device_id, last_ack).await {
        Ok(commands) => {
            for cmd in commands {
                let json = serde_json::to_string(&cmd).unwrap();
                if ws_tx.send(Message::Text(json.into())).await.is_err() {
                    error!(device_id, "failed to replay command");
                    connections.unregister_phone(device_id).await;
                    return;
                }
            }
        }
        Err(e) => {
            warn!(device_id, "failed to get pending commands: {e}");
        }
    }

    // Main connection loop
    let mut heartbeat = interval(HEARTBEAT_INTERVAL);
    let mut last_pong = tokio::time::Instant::now();

    loop {
        tokio::select! {
            // Incoming message from phone
            msg = ws_rx.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        match parse_phone_message(&text) {
                            Ok(PhoneMessage::Ack(ack)) => {
                                if let Err(e) = state.process_ack(device_id, ack.ack).await {
                                    warn!(device_id, "failed to process ack: {e}");
                                }
                            }
                            Ok(PhoneMessage::Response(resp)) => {
                                info!(device_id, id = resp.id, status = %resp.status, "command response");
                                let resp_json = serde_json::to_string(&resp).unwrap_or_default();
                                // Store in Redis
                                if let Err(e) = state.store_response(device_id, resp.id, &resp_json).await {
                                    warn!(device_id, "failed to store response: {e}");
                                }
                                // Notify controllers via broadcast
                                connections.notify_response(device_id, resp.id, &resp_json);
                                // Also process as an ack
                                if let Err(e) = state.process_ack(device_id, resp.id).await {
                                    warn!(device_id, "failed to process response ack: {e}");
                                }
                            }
                            Ok(PhoneMessage::Pong(_)) => {
                                last_pong = tokio::time::Instant::now();
                            }
                            Ok(PhoneMessage::Auth(_)) => {
                                warn!(device_id, "unexpected auth message after authentication");
                            }
                            Err(e) => {
                                warn!(device_id, "failed to parse message: {e}");
                            }
                        }
                    }
                    Some(Ok(Message::Pong(_))) => {
                        last_pong = tokio::time::Instant::now();
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        info!(device_id, "phone connection closed");
                        break;
                    }
                    Some(Err(e)) => {
                        warn!(device_id, "websocket error: {e}");
                        break;
                    }
                    _ => {}
                }
            }

            // External command to send to phone (from controllers via Connections)
            Some(cmd_json) = cmd_rx.recv() => {
                if ws_tx.send(Message::Text(cmd_json.into())).await.is_err() {
                    error!(device_id, "failed to send command to phone");
                    break;
                }
            }

            // Heartbeat tick
            _ = heartbeat.tick() => {
                if last_pong.elapsed() > HEARTBEAT_TIMEOUT {
                    warn!(device_id, "heartbeat timeout, disconnecting phone");
                    break;
                }
                let ping = ServerMessage::ping();
                let json = serde_json::to_string(&ping).unwrap();
                if ws_tx.send(Message::Text(json.into())).await.is_err() {
                    break;
                }
            }
        }
    }

    // Cleanup
    connections.unregister_phone(device_id).await;
    if let Err(e) = state.unregister_connection(device_id).await {
        error!(device_id, "failed to unregister: {e}");
    }
    let _ = ws_tx.close().await;
    info!(%addr, device_id, "phone disconnected");
}

/// Handle a controller connection: auth with phone status, relay commands, stream responses.
async fn handle_controller_connection(
    mut ws_tx: futures_util::stream::SplitSink<
        tokio_tungstenite::WebSocketStream<TcpStream>,
        Message,
    >,
    mut ws_rx: futures_util::stream::SplitStream<tokio_tungstenite::WebSocketStream<TcpStream>>,
    addr: SocketAddr,
    device_id: &str,
    target_device_id: &str,
    state: Arc<State>,
    connections: Arc<Connections>,
) {
    // Check if phone is connected
    let phone_connected = connections.is_phone_connected(target_device_id).await;

    // Register controller
    let mut event_rx = connections.register_controller(target_device_id).await;

    // Subscribe to response broadcast
    let mut response_rx = connections.subscribe_responses();

    // Send auth_ok with phone status
    let auth_ok = ServerMessage::auth_ok_controller(phone_connected);
    let json = serde_json::to_string(&auth_ok).unwrap();
    if ws_tx.send(Message::Text(json.into())).await.is_err() {
        return;
    }

    info!(%addr, device_id, target_device_id, phone_connected, "controller connected");

    // Main connection loop
    let mut heartbeat = interval(HEARTBEAT_INTERVAL);
    let mut last_pong = tokio::time::Instant::now();
    let target_device_id_owned = target_device_id.to_string();

    loop {
        tokio::select! {
            // Incoming message from controller
            msg = ws_rx.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        // Try to parse as controller command
                        match parse_controller_message(&text) {
                            Ok(ctrl_cmd) => {
                                // Enqueue command in Redis
                                match state.enqueue_command(&target_device_id_owned, ctrl_cmd.cmd, ctrl_cmd.params).await {
                                    Ok(command) => {
                                        // Send command to phone via Connections
                                        let cmd_json = serde_json::to_string(&command).unwrap();
                                        let sent = connections.send_to_phone(&target_device_id_owned, &cmd_json).await;

                                        if !sent {
                                            // Phone not connected — command is queued in Redis
                                            warn!(device_id, target_device_id = %target_device_id_owned, "phone not connected, command queued");
                                        }

                                        // Send cmd_accepted to controller
                                        let accepted = ServerMessage::cmd_accepted(command.id);
                                        let json = serde_json::to_string(&accepted).unwrap();
                                        if ws_tx.send(Message::Text(json.into())).await.is_err() {
                                            break;
                                        }
                                    }
                                    Err(e) => {
                                        error!(device_id, "failed to enqueue command: {e}");
                                        let err = ServerMessage::error(format!("failed to enqueue: {e}"));
                                        let json = serde_json::to_string(&err).unwrap();
                                        let _ = ws_tx.send(Message::Text(json.into())).await;
                                    }
                                }
                            }
                            Err(_) => {
                                // Check for pong
                                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
                                    if v.get("type").and_then(|t| t.as_str()) == Some("pong") {
                                        last_pong = tokio::time::Instant::now();
                                        continue;
                                    }
                                }
                                warn!(device_id, "failed to parse controller message");
                            }
                        }
                    }
                    Some(Ok(Message::Pong(_))) => {
                        last_pong = tokio::time::Instant::now();
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        info!(device_id, "controller connection closed");
                        break;
                    }
                    Some(Err(e)) => {
                        warn!(device_id, "controller websocket error: {e}");
                        break;
                    }
                    _ => {}
                }
            }

            // Events from Connections (phone_status changes)
            Some(event_json) = event_rx.recv() => {
                if ws_tx.send(Message::Text(event_json.into())).await.is_err() {
                    break;
                }
            }

            // Response broadcast — relay matching responses to this controller
            Ok((resp_device_id, _cmd_id, resp_json)) = response_rx.recv() => {
                if resp_device_id == target_device_id_owned {
                    if ws_tx.send(Message::Text(resp_json.into())).await.is_err() {
                        break;
                    }
                }
            }

            // Heartbeat tick
            _ = heartbeat.tick() => {
                if last_pong.elapsed() > HEARTBEAT_TIMEOUT {
                    warn!(device_id, "controller heartbeat timeout");
                    break;
                }
                let ping = ServerMessage::ping();
                let json = serde_json::to_string(&ping).unwrap();
                if ws_tx.send(Message::Text(json.into())).await.is_err() {
                    break;
                }
            }
        }
    }

    // Cleanup — note: we can't easily get the pointer to unregister a specific controller,
    // but the channel will be dropped which effectively removes it
    let _ = ws_tx.close().await;
    info!(%addr, device_id, "controller disconnected");
}
