use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio::time::{interval, timeout};
use tokio_tungstenite::tungstenite::Message;
use tracing::{error, info, warn};

use crate::{AuthBackend, IpTrackingBackend, StateBackend, UsageBackend, UsageCheck};
use crate::connections::Connections;
use crate::ip_whitelist::IpWhitelist;
use crate::protocol::{
    check_version_compatible, parse_controller_message, parse_phone_message,
    version_error, PhoneMessage, ServerMessage,
};

use std::collections::HashMap;

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);
const HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(60);
const AUTH_TIMEOUT: Duration = Duration::from_secs(10);

/// Parsed HTTP request fields.
struct HttpRequest {
    method: String,
    path: String,
    query: HashMap<String, String>,
    headers: HashMap<String, String>,
    body: String,
}

/// Parse a raw HTTP request into method, path, query params, headers, and body.
fn parse_http_request(raw: &str) -> HttpRequest {
    let mut method = String::new();
    let mut path = String::new();
    let mut query = HashMap::new();
    let mut headers = HashMap::new();

    // Split headers from body
    let (header_section, body_section) = if let Some(idx) = raw.find("\r\n\r\n") {
        (&raw[..idx], &raw[idx + 4..])
    } else if let Some(idx) = raw.find("\n\n") {
        (&raw[..idx], &raw[idx + 2..])
    } else {
        (raw, "")
    };
    let body = body_section.to_string();

    let mut lines = header_section.lines();

    // Request line: METHOD /path?query HTTP/1.1
    if let Some(request_line) = lines.next() {
        let parts: Vec<&str> = request_line.split_whitespace().collect();
        if parts.len() >= 2 {
            method = parts[0].to_string();
            let full_path = parts[1];
            if let Some(qmark) = full_path.find('?') {
                path = full_path[..qmark].to_string();
                let query_str = &full_path[qmark + 1..];
                for pair in query_str.split('&') {
                    if let Some(eq) = pair.find('=') {
                        query.insert(pair[..eq].to_string(), pair[eq + 1..].to_string());
                    }
                }
            } else {
                path = full_path.to_string();
            }
        }
    }

    // Headers
    for line in lines {
        if let Some(colon) = line.find(':') {
            let key = line[..colon].trim().to_lowercase();
            let value = line[colon + 1..].trim().to_string();
            headers.insert(key, value);
        }
    }

    HttpRequest { method, path, query, headers, body }
}

/// Extract the real client IP from X-Forwarded-For header, falling back to the peer address.
fn extract_client_ip(headers: &HashMap<String, String>, peer_addr: &SocketAddr) -> String {
    if let Some(xff) = headers.get("x-forwarded-for") {
        // X-Forwarded-For: client, proxy1, proxy2 — take the first (original client)
        if let Some(first) = xff.split(',').next() {
            let trimmed = first.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }
    }
    peer_addr.ip().to_string()
}

/// Handle a new connection: if it's a WebSocket upgrade, proceed; otherwise return HTTP 200.
pub async fn handle_connection(
    stream: TcpStream,
    addr: SocketAddr,
    state: Arc<dyn StateBackend>,
    connections: Arc<Connections>,
    auth: Arc<dyn AuthBackend>,
    usage: Arc<dyn UsageBackend>,
    ip_tracking: Arc<dyn IpTrackingBackend>,
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
    let peeked_headers = parse_http_request(&request_str);
    let is_websocket = request_str
        .lines()
        .any(|line| line.to_lowercase().starts_with("upgrade:") && line.to_lowercase().contains("websocket"));

    // Extract real client IP from X-Forwarded-For (set by load balancer)
    let client_ip = extract_client_ip(&peeked_headers.headers, &addr);

    if !is_websocket {
        // Plain HTTP request — parse and route
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let (mut reader, mut writer) = stream.into_split();

        // Read full request (peeked bytes + any remaining)
        let mut full_buf = vec![0u8; 65536];
        let n = reader.read(&mut full_buf).await.unwrap_or(0);
        let raw = String::from_utf8_lossy(&full_buf[..n]);
        let parsed = parse_http_request(&raw);

        match (parsed.method.as_str(), parsed.path.as_str()) {
            ("GET", "/events") => {
                handle_sse(writer, addr, &parsed, connections, auth).await;
            }
            ("POST", "/notify") => {
                handle_notify(writer, addr, &parsed.body, &parsed.headers, connections, auth).await;
            }
            ("OPTIONS", "/events") | ("OPTIONS", "/notify") => {
                let response = "HTTP/1.1 204 No Content\r\n\
                    Access-Control-Allow-Origin: *\r\n\
                    Access-Control-Allow-Methods: GET, POST, OPTIONS\r\n\
                    Access-Control-Allow-Headers: Authorization, Content-Type\r\n\
                    Content-Length: 0\r\n\
                    Connection: close\r\n\r\n";
                let _ = writer.write_all(response.as_bytes()).await;
                let _ = writer.shutdown().await;
            }
            _ => {
                let body = r#"{"status":"ok"}"#;
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = writer.write_all(response.as_bytes()).await;
                let _ = writer.shutdown().await;
            }
        }
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

    let auth_msg = match auth_result {
        Ok(Some(auth_msg)) => auth_msg,
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
    let token = match auth_msg.role.as_str() {
        "controller" => auth_msg.key.clone(),
        _ => auth_msg.user_id.clone(),
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

    // Verify token via auth backend
    let verify_result = match auth.verify_token(&token).await {
        Ok(r) => r,
        Err(e) => {
            warn!(%addr, "auth verification failed: {e}");
            let err = ServerMessage::auth_fail("invalid token");
            let json = serde_json::to_string(&err).unwrap();
            let _ = ws_tx.send(Message::Text(json.into())).await;
            let _ = ws_tx.close().await;
            return;
        }
    };
    let firebase_uid = verify_result.firebase_uid;
    let role = auth_msg.role.clone();

    // Use client-provided device_id for routing (required for phones, optional for controllers)
    let device_id = match auth_msg.device_id {
        Some(ref id) if !id.is_empty() => id.replace('-', ""),
        _ => firebase_uid.clone(), // fallback for backwards compat
    };

    // Verify device is allowed (file backend checks against config)
    if let Err(e) = auth.verify_device(&device_id).await {
        warn!(%addr, %device_id, "device verification failed: {e}");
        let err = ServerMessage::auth_fail("device not allowed");
        let json = serde_json::to_string(&err).unwrap();
        let _ = ws_tx.send(Message::Text(json.into())).await;
        let _ = ws_tx.close().await;
        return;
    }

    info!(%addr, %firebase_uid, %device_id, %role, %client_ip, "authenticated");

    // Check IP whitelist (uses whitelist from verify response — no extra API call)
    let whitelist = IpWhitelist::from_text(&verify_result.ip_whitelist);
    if let Err(e) = whitelist.check(&client_ip) {
        warn!(%addr, %firebase_uid, %client_ip, "IP blocked by whitelist: {e}");
        let err = ServerMessage::auth_fail("connection from this IP is not allowed");
        let json = serde_json::to_string(&err).unwrap();
        let _ = ws_tx.send(Message::Text(json.into())).await;
        let _ = ws_tx.close().await;
        return;
    }

    // Record connection IP (fire-and-forget in background)
    {
        let ip_tracking = Arc::clone(&ip_tracking);
        let firebase_uid_owned = firebase_uid.clone();
        let device_id_owned = device_id.clone();
        let role_owned = role.clone();
        let client_ip_owned = client_ip.clone();
        tokio::spawn(async move {
            ip_tracking.record_ip(&firebase_uid_owned, &device_id_owned, &client_ip_owned, &role_owned).await;
        });
    }

    // Check client version compatibility if version info was provided
    if let Some(ref version) = auth_msg.version {
        info!(%addr, %device_id, %version, "client version reported");
        if let Err(msg) = check_version_compatible(version) {
            warn!(%addr, %device_id, %version, "version incompatible: {msg}");
            let err = ServerMessage::version_error(version_error::OUTDATED_CLIENT, &msg);
            let json = serde_json::to_string(&err).unwrap();
            let _ = ws_tx.send(Message::Text(json.into())).await;
            let _ = ws_tx.close().await;
            return;
        }
        // Store version in connections registry
        connections.set_version(&device_id, version.clone()).await;
    }

    match role.as_str() {
        "controller" => {
            let target_device_id = auth_msg.target_device_id.map(|id| id.replace('-', "")).unwrap_or_else(|| device_id.clone());
            handle_controller_connection(ws_tx, ws_rx, addr, &device_id, &target_device_id, &token, &firebase_uid, state, connections, usage)
                .await;
        }
        _ => {
            // Default: phone
            handle_phone_connection(
                ws_tx,
                ws_rx,
                addr,
                &device_id,
                auth_msg.last_ack,
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
    state: Arc<dyn StateBackend>,
    connections: Arc<Connections>,
) {
    // Register in Connections (in-memory) — replaces any existing connection for this device_id
    let mut cmd_rx = connections.register_phone(device_id).await;

    // Register in state backend
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
                                // Store in state backend
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
    connections.remove_version(device_id).await;
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
    api_key: &str,
    firebase_uid: &str,
    state: Arc<dyn StateBackend>,
    connections: Arc<Connections>,
    usage: Arc<dyn UsageBackend>,
) {
    // Check if phone is connected
    let phone_connected = connections.is_phone_connected(target_device_id).await;

    // Check cross-version compatibility between controller and target phone
    if phone_connected {
        if let Some(phone_version) = connections.get_version(target_device_id).await {
            if let Err(msg) = check_version_compatible(&phone_version) {
                // The phone on the other end is outdated
                let controller_version = connections.get_version(device_id).await;
                let controller_ok = controller_version
                    .as_ref()
                    .map(|v| check_version_compatible(v).is_ok())
                    .unwrap_or(true);

                let (code, error_msg) = if !controller_ok {
                    (version_error::BOTH_OUTDATED, format!(
                        "Both your client and the target device ({}) are outdated. Please update both.",
                        phone_version
                    ))
                } else {
                    (version_error::OUTDATED_REMOTE, format!(
                        "The target device ({}) is outdated. {}",
                        phone_version, msg
                    ))
                };

                warn!(%addr, %device_id, %target_device_id, "cross-version incompatible: {error_msg}");
                let err = ServerMessage::version_error(code, &error_msg);
                let json = serde_json::to_string(&err).unwrap();
                let _ = ws_tx.send(Message::Text(json.into())).await;
                let _ = ws_tx.close().await;
                return;
            }
        }
    }

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
                                // Check usage limit before enqueuing
                                match usage.check_and_record(api_key, firebase_uid, &ctrl_cmd.cmd, Some(&target_device_id_owned)).await {
                                    UsageCheck::Allowed => {}
                                    UsageCheck::LimitReached { current, limit } => {
                                        let err = ServerMessage::error(format!(
                                            "Daily usage limit reached ({}/{}). Please upgrade your plan.",
                                            current, limit
                                        ));
                                        let json = serde_json::to_string(&err).unwrap();
                                        let _ = ws_tx.send(Message::Text(json.into())).await;
                                        continue;
                                    }
                                }

                                // Enqueue command in state backend
                                match state.enqueue_command(&target_device_id_owned, ctrl_cmd.cmd, ctrl_cmd.params).await {
                                    Ok(command) => {
                                        // Send command to phone via Connections
                                        let cmd_json = serde_json::to_string(&command).unwrap();
                                        let sent = connections.send_to_phone(&target_device_id_owned, &cmd_json).await;

                                        if !sent {
                                            // Phone not connected — command is queued
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

    // Flush any buffered usage data for this controller's API key
    usage.flush_key(api_key).await;

    // Cleanup — note: we can't easily get the pointer to unregister a specific controller,
    // but the channel will be dropped which effectively removes it
    let _ = ws_tx.close().await;
    info!(%addr, device_id, "controller disconnected");
}

/// Handle SSE connection: authenticate, register, stream events.
async fn handle_sse(
    mut writer: tokio::net::tcp::OwnedWriteHalf,
    addr: SocketAddr,
    req: &HttpRequest,
    connections: Arc<Connections>,
    auth: Arc<dyn AuthBackend>,
) {
    use tokio::io::AsyncWriteExt;

    let device_id = match req.query.get("device_id") {
        Some(id) if !id.is_empty() => id.replace('-', ""),
        _ => {
            let body = r#"{"error":"missing device_id query parameter"}"#;
            let resp = format!(
                "HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(), body
            );
            let _ = writer.write_all(resp.as_bytes()).await;
            let _ = writer.shutdown().await;
            return;
        }
    };

    // Auth: extract Bearer token
    let token = req.headers.get("authorization")
        .and_then(|v| v.strip_prefix("Bearer ").or_else(|| v.strip_prefix("bearer ")))
        .map(|s| s.to_string());
    let token = match token {
        Some(t) if !t.is_empty() => t,
        _ => {
            let body = r#"{"error":"missing or invalid Authorization header"}"#;
            let resp = format!(
                "HTTP/1.1 401 Unauthorized\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(), body
            );
            let _ = writer.write_all(resp.as_bytes()).await;
            let _ = writer.shutdown().await;
            return;
        }
    };

    if let Err(e) = auth.verify_token(&token).await.map(|_| ()) {
        warn!(%addr, %device_id, "SSE auth failed: {e}");
        let body = r#"{"error":"invalid token"}"#;
        let resp = format!(
            "HTTP/1.1 401 Unauthorized\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(), body
        );
        let _ = writer.write_all(resp.as_bytes()).await;
        let _ = writer.shutdown().await;
        return;
    }

    if let Err(e) = auth.verify_device(&device_id).await {
        warn!(%addr, %device_id, "SSE device verification failed: {e}");
        let body = r#"{"error":"device not allowed"}"#;
        let resp = format!(
            "HTTP/1.1 403 Forbidden\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(), body
        );
        let _ = writer.write_all(resp.as_bytes()).await;
        let _ = writer.shutdown().await;
        return;
    }

    info!(%addr, %device_id, "SSE client connected");

    // Write SSE response headers
    let headers = "HTTP/1.1 200 OK\r\n\
        Content-Type: text/event-stream\r\n\
        Cache-Control: no-cache\r\n\
        Connection: keep-alive\r\n\
        Access-Control-Allow-Origin: *\r\n\r\n";
    if writer.write_all(headers.as_bytes()).await.is_err() {
        return;
    }

    // Send initial connected event
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let connected_event = format!("data: {{\"type\":\"connected\",\"timestamp\":{}}}\n\n", ts);
    if writer.write_all(connected_event.as_bytes()).await.is_err() {
        return;
    }

    // Register SSE client
    let mut event_rx = connections.register_sse(&device_id).await;

    // Stream loop
    let mut heartbeat = interval(HEARTBEAT_INTERVAL);

    loop {
        tokio::select! {
            event = event_rx.recv() => {
                match event {
                    Some(data) => {
                        let line = format!("data: {}\n\n", data);
                        if writer.write_all(line.as_bytes()).await.is_err() {
                            break;
                        }
                    }
                    None => break, // channel closed (replaced by new SSE connection)
                }
            }
            _ = heartbeat.tick() => {
                if writer.write_all(b": heartbeat\n\n").await.is_err() {
                    break;
                }
            }
        }
    }

    // Cleanup
    connections.unregister_sse(&device_id).await;
    let _ = writer.shutdown().await;
    info!(%addr, %device_id, "SSE client disconnected");
}

/// Handle POST /notify: push an event to a specific device's SSE stream.
async fn handle_notify(
    mut writer: tokio::net::tcp::OwnedWriteHalf,
    addr: SocketAddr,
    body: &str,
    headers: &HashMap<String, String>,
    connections: Arc<Connections>,
    auth: Arc<dyn AuthBackend>,
) {
    use tokio::io::AsyncWriteExt;

    // Verify notify_secret if configured
    if let Some(expected) = auth.notify_secret() {
        let token = headers.get("authorization")
            .and_then(|v| v.strip_prefix("Bearer ").or_else(|| v.strip_prefix("bearer ")));
        match token {
            Some(t) if t == expected => {} // ok
            _ => {
                warn!(%addr, "POST /notify rejected: invalid or missing notify_secret");
                let resp_body = r#"{"error":"unauthorized"}"#;
                let resp = format!(
                    "HTTP/1.1 401 Unauthorized\r\nContent-Type: application/json\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n{}",
                    resp_body.len(), resp_body
                );
                let _ = writer.write_all(resp.as_bytes()).await;
                let _ = writer.shutdown().await;
                return;
            }
        }
    }

    let parsed: serde_json::Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(_) => {
            let resp_body = r#"{"error":"invalid JSON body"}"#;
            let resp = format!(
                "HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                resp_body.len(), resp_body
            );
            let _ = writer.write_all(resp.as_bytes()).await;
            let _ = writer.shutdown().await;
            return;
        }
    };

    let device_id = match parsed.get("device_id").and_then(|v| v.as_str()) {
        Some(id) if !id.is_empty() => id.replace('-', ""),
        _ => {
            let resp_body = r#"{"error":"missing device_id"}"#;
            let resp = format!(
                "HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                resp_body.len(), resp_body
            );
            let _ = writer.write_all(resp.as_bytes()).await;
            let _ = writer.shutdown().await;
            return;
        }
    };

    // Build event JSON — add timestamp if not present
    let event = if parsed.get("timestamp").is_none() {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let mut obj = parsed.clone();
        obj.as_object_mut().unwrap().insert("timestamp".to_string(), serde_json::json!(ts));
        obj.to_string()
    } else {
        parsed.to_string()
    };

    let sent = connections.send_sse_event(&device_id, &event).await;

    let (status, resp_body) = if sent {
        info!(%addr, %device_id, "SSE event delivered via /notify");
        ("200 OK", r#"{"ok":true}"#.to_string())
    } else {
        warn!(%addr, %device_id, "SSE event failed: device not connected");
        ("404 Not Found", r#"{"error":"device not connected"}"#.to_string())
    };

    let resp = format!(
        "HTTP/1.1 {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n{}",
        status, resp_body.len(), resp_body
    );
    let _ = writer.write_all(resp.as_bytes()).await;
    let _ = writer.shutdown().await;
}
