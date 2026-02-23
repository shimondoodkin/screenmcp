use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

use crate::config::Config;

/// Events from the local server to the tray/main app.
#[derive(Debug)]
pub enum LocalServerEvent {
    /// API key + email received from browser Google sign-in.
    TokenReceived { token: String, email: String },
}

/// Start the local HTTP server for auth callback.
/// Returns the port the server is listening on.
pub async fn start_local_server(event_tx: mpsc::Sender<LocalServerEvent>) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("failed to bind local server");
    let port = listener.local_addr().unwrap().port();
    info!("local server listening on port {port}");

    tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let tx = event_tx.clone();
                    tokio::spawn(handle_connection(stream, tx));
                }
                Err(e) => {
                    warn!("local server accept error: {e}");
                }
            }
        }
    });

    port
}

/// Open browser to screenmcp.com for Google sign-in.
pub fn open_google_sign_in(port: u16, api_url: &str) {
    let url = format!("{api_url}/auth/desktop?port={port}");
    info!("opening browser for Google sign-in: {url}");
    if let Err(e) = open::that(&url) {
        error!("failed to open browser: {e}");
    }
}

async fn handle_connection(
    mut stream: tokio::net::TcpStream,
    event_tx: mpsc::Sender<LocalServerEvent>,
) {
    let mut buf = vec![0u8; 65536];
    let n = match stream.read(&mut buf).await {
        Ok(n) if n > 0 => n,
        _ => return,
    };

    let raw = String::from_utf8_lossy(&buf[..n]).to_string();

    // Parse request line
    let first_line = raw.lines().next().unwrap_or("");
    let mut parts = first_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let full_path = parts.next().unwrap_or("/");

    // Split path and query
    let (path, query) = match full_path.split_once('?') {
        Some((p, q)) => (p, q),
        None => (full_path, ""),
    };

    let response = match (method, path) {
        ("GET", "/callback") => handle_callback(query, &event_tx).await,
        _ => http_response(404, "text/plain", "Not Found"),
    };

    let _ = stream.write_all(response.as_bytes()).await;
}

/// Handle the auth callback from the browser.
async fn handle_callback(query: &str, event_tx: &mpsc::Sender<LocalServerEvent>) -> String {
    let token = parse_query_param(query, "token");
    let email = parse_query_param(query, "email").unwrap_or_default();

    match token {
        Some(token) if !token.is_empty() => {
            info!(
                "received auth token: {}..., email: {}",
                &token[..token.len().min(11)],
                if email.is_empty() { "(none)" } else { &email }
            );

            // Save to config and clear OSS settings (Google login takes precedence)
            let mut config = Config::load();
            config.token = token.clone();
            config.email = email.clone();
            config.opensource_server_enabled = false;
            config.opensource_user_id = String::new();
            config.opensource_api_url = String::new();
            if let Err(e) = config.save() {
                error!("failed to save config with token: {e}");
            }

            // Notify the tray
            let _ = event_tx
                .send(LocalServerEvent::TokenReceived { token, email })
                .await;

            http_response(
                200,
                "text/html",
                concat!(
                    "<!DOCTYPE html><html><body style='font-family:system-ui;text-align:center;padding:60px'>",
                    "<h2 style='color:#2e7d32'>&#10004; Signed in successfully!</h2>",
                    "<p style='color:#666'>You can close this tab and return to the ScreenMCP app.</p>",
                    "<a href='https://screenmcp.com' style='display:inline-block;margin-top:20px;padding:12px 32px;background:#1976d2;color:#fff;text-decoration:none;border-radius:6px;font-size:16px'>Go to ScreenMCP.com</a>",
                    "</body></html>"
                ),
            )
        }
        _ => http_response(
            400,
            "text/html",
            concat!(
                "<!DOCTYPE html><html><body style='font-family:system-ui;text-align:center;padding:60px'>",
                "<h2 style='color:#c62828'>Sign-in failed</h2>",
                "<p style='color:#666'>No token received. Please try again from the app.</p>",
                "</body></html>"
            ),
        ),
    }
}

fn parse_query_param(query: &str, name: &str) -> Option<String> {
    let prefix = format!("{name}=");
    query
        .split('&')
        .find_map(|param| param.strip_prefix(&prefix))
        .map(percent_decode)
}

fn percent_decode(s: &str) -> String {
    let mut result = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '%' {
            let hex: String = chars.by_ref().take(2).collect();
            if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                result.push(byte as char);
            }
        } else if c == '+' {
            result.push(' ');
        } else {
            result.push(c);
        }
    }
    result
}

fn http_response(status: u16, content_type: &str, body: &str) -> String {
    let status_text = match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        _ => "Error",
    };
    format!(
        "HTTP/1.1 {status} {status_text}\r\n\
         Content-Type: {content_type}\r\n\
         Content-Length: {}\r\n\
         Connection: close\r\n\
         \r\n\
         {body}",
        body.len()
    )
}
