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

/// Verify a token against the API server, returning the resolved UID.
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

    body.get("uid")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| "verify response missing uid".to_string())
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

    // Verify token via API server
    let uid = match verify_token(&auth.token, &api_url).await {
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

    info!(%addr, %uid, %role, "authenticated");

    match role.as_str() {
        "controller" => {
            let target_uid = auth.target_uid.unwrap_or_else(|| uid.clone());
            handle_controller_connection(ws_tx, ws_rx, addr, &uid, &target_uid, state, connections)
                .await;
        }
        _ => {
            // Default: phone
            handle_phone_connection(
                ws_tx,
                ws_rx,
                addr,
                &uid,
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
    uid: &str,
    last_ack: i64,
    state: Arc<State>,
    connections: Arc<Connections>,
) {
    // Register in Connections (in-memory) — replaces any existing connection for this uid
    let mut cmd_rx = connections.register_phone(uid).await;

    // Register in Redis
    if let Err(e) = state.register_connection(uid).await {
        error!(uid, "failed to register connection: {e}");
    }

    // Send auth_ok
    let server_ack = state.get_last_ack(uid).await.unwrap_or(0);
    let resume_from = last_ack.max(server_ack);
    let auth_ok = ServerMessage::auth_ok(resume_from);
    let json = serde_json::to_string(&auth_ok).unwrap();
    if ws_tx.send(Message::Text(json.into())).await.is_err() {
        connections.unregister_phone(uid).await;
        return;
    }

    // Replay pending commands
    match state.get_pending_commands(uid, last_ack).await {
        Ok(commands) => {
            for cmd in commands {
                let json = serde_json::to_string(&cmd).unwrap();
                if ws_tx.send(Message::Text(json.into())).await.is_err() {
                    error!(uid, "failed to replay command");
                    connections.unregister_phone(uid).await;
                    return;
                }
            }
        }
        Err(e) => {
            warn!(uid, "failed to get pending commands: {e}");
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
                                if let Err(e) = state.process_ack(uid, ack.ack).await {
                                    warn!(uid, "failed to process ack: {e}");
                                }
                            }
                            Ok(PhoneMessage::Response(resp)) => {
                                info!(uid, id = resp.id, status = %resp.status, "command response");
                                let resp_json = serde_json::to_string(&resp).unwrap_or_default();
                                // Store in Redis
                                if let Err(e) = state.store_response(uid, resp.id, &resp_json).await {
                                    warn!(uid, "failed to store response: {e}");
                                }
                                // Notify controllers via broadcast
                                connections.notify_response(uid, resp.id, &resp_json);
                                // Also process as an ack
                                if let Err(e) = state.process_ack(uid, resp.id).await {
                                    warn!(uid, "failed to process response ack: {e}");
                                }
                            }
                            Ok(PhoneMessage::Pong(_)) => {
                                last_pong = tokio::time::Instant::now();
                            }
                            Ok(PhoneMessage::Auth(_)) => {
                                warn!(uid, "unexpected auth message after authentication");
                            }
                            Err(e) => {
                                warn!(uid, "failed to parse message: {e}");
                            }
                        }
                    }
                    Some(Ok(Message::Pong(_))) => {
                        last_pong = tokio::time::Instant::now();
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        info!(uid, "phone connection closed");
                        break;
                    }
                    Some(Err(e)) => {
                        warn!(uid, "websocket error: {e}");
                        break;
                    }
                    _ => {}
                }
            }

            // External command to send to phone (from controllers via Connections)
            Some(cmd_json) = cmd_rx.recv() => {
                if ws_tx.send(Message::Text(cmd_json.into())).await.is_err() {
                    error!(uid, "failed to send command to phone");
                    break;
                }
            }

            // Heartbeat tick
            _ = heartbeat.tick() => {
                if last_pong.elapsed() > HEARTBEAT_TIMEOUT {
                    warn!(uid, "heartbeat timeout, disconnecting phone");
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
    connections.unregister_phone(uid).await;
    if let Err(e) = state.unregister_connection(uid).await {
        error!(uid, "failed to unregister: {e}");
    }
    let _ = ws_tx.close().await;
    info!(%addr, uid, "phone disconnected");
}

/// Handle a controller connection: auth with phone status, relay commands, stream responses.
async fn handle_controller_connection(
    mut ws_tx: futures_util::stream::SplitSink<
        tokio_tungstenite::WebSocketStream<TcpStream>,
        Message,
    >,
    mut ws_rx: futures_util::stream::SplitStream<tokio_tungstenite::WebSocketStream<TcpStream>>,
    addr: SocketAddr,
    uid: &str,
    target_uid: &str,
    state: Arc<State>,
    connections: Arc<Connections>,
) {
    // Check if phone is connected
    let phone_connected = connections.is_phone_connected(target_uid).await;

    // Register controller
    let mut event_rx = connections.register_controller(target_uid).await;

    // Subscribe to response broadcast
    let mut response_rx = connections.subscribe_responses();

    // Send auth_ok with phone status
    let auth_ok = ServerMessage::auth_ok_controller(phone_connected);
    let json = serde_json::to_string(&auth_ok).unwrap();
    if ws_tx.send(Message::Text(json.into())).await.is_err() {
        return;
    }

    info!(%addr, uid, target_uid, phone_connected, "controller connected");

    // Main connection loop
    let mut heartbeat = interval(HEARTBEAT_INTERVAL);
    let mut last_pong = tokio::time::Instant::now();
    let target_uid_owned = target_uid.to_string();

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
                                match state.enqueue_command(&target_uid_owned, ctrl_cmd.cmd, ctrl_cmd.params).await {
                                    Ok(command) => {
                                        // Send command to phone via Connections
                                        let cmd_json = serde_json::to_string(&command).unwrap();
                                        let sent = connections.send_to_phone(&target_uid_owned, &cmd_json).await;

                                        if !sent {
                                            // Phone not connected — command is queued in Redis
                                            warn!(uid, target_uid = %target_uid_owned, "phone not connected, command queued");
                                        }

                                        // Send cmd_accepted to controller
                                        let accepted = ServerMessage::cmd_accepted(command.id);
                                        let json = serde_json::to_string(&accepted).unwrap();
                                        if ws_tx.send(Message::Text(json.into())).await.is_err() {
                                            break;
                                        }
                                    }
                                    Err(e) => {
                                        error!(uid, "failed to enqueue command: {e}");
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
                                warn!(uid, "failed to parse controller message");
                            }
                        }
                    }
                    Some(Ok(Message::Pong(_))) => {
                        last_pong = tokio::time::Instant::now();
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        info!(uid, "controller connection closed");
                        break;
                    }
                    Some(Err(e)) => {
                        warn!(uid, "controller websocket error: {e}");
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
            Ok((resp_uid, _cmd_id, resp_json)) = response_rx.recv() => {
                if resp_uid == target_uid_owned {
                    if ws_tx.send(Message::Text(resp_json.into())).await.is_err() {
                        break;
                    }
                }
            }

            // Heartbeat tick
            _ = heartbeat.tick() => {
                if last_pong.elapsed() > HEARTBEAT_TIMEOUT {
                    warn!(uid, "controller heartbeat timeout");
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
    info!(%addr, uid, "controller disconnected");
}
