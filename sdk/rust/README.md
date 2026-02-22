# screenmcp — Rust SDK

Rust client library for ScreenMCP. Discover workers, connect via WebSocket, and control phones and desktops programmatically.

## Installation

```toml
[dependencies]
screenmcp = { path = "../screenmcp/sdk/rust" }
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
```

## Quick Start

```rust
use screenmcp::{ScreenMCPClient, ClientOptions};

#[tokio::main]
async fn main() -> screenmcp::Result<()> {
    let mut phone = ScreenMCPClient::new(ClientOptions {
        api_key: "pk_your_api_key_here".into(),
        api_url: None, // defaults to https://server10.doodkin.com
        device_id: None, // server picks first available
        command_timeout_ms: None, // defaults to 30000
        auto_reconnect: None, // defaults to true
    });

    phone.connect().await?;

    let screenshot = phone.screenshot().await?;
    println!("Got image: {} bytes", screenshot.image.len());

    phone.click(540, 1200).await?;
    phone.type_text("hello").await?;

    phone.disconnect().await?;
    Ok(())
}
```

## ClientOptions

| Field | Type | Default | Description |
|---|---|---|---|
| `api_key` | `String` | required | API key (`pk_` + 64 hex chars) |
| `api_url` | `Option<String>` | `https://server10.doodkin.com` | API server URL |
| `device_id` | `Option<String>` | `None` | Target device hex ID. Server picks first available if omitted |
| `command_timeout_ms` | `Option<u64>` | `30000` | Per-command timeout in milliseconds |
| `auto_reconnect` | `Option<bool>` | `true` | Auto-reconnect on connection drop |

## Phone Commands

### Screen

```rust
let result = phone.screenshot().await?;         // base64 WebP image
let tree = phone.ui_tree().await?;               // accessibility tree nodes
```

### Touch

```rust
phone.click(x, y).await?;                        // tap at coordinates
phone.long_click(x, y).await?;                   // long press (1000ms)
phone.drag(start_x, start_y, end_x, end_y).await?;
phone.scroll(ScrollDirection::Down, Some(500)).await?;
```

### Text

```rust
phone.type_text("hello world").await?;            // type into focused field
let result = phone.get_text().await?;             // read focused field text
phone.select_all().await?;
phone.copy().await?;
phone.paste(Some("clipboard text")).await?;
phone.get_clipboard().await?;
phone.set_clipboard("new text").await?;
```

### Navigation

```rust
phone.back().await?;
phone.home().await?;
phone.recents().await?;
```

### Camera

```rust
let cameras = phone.list_cameras().await?;        // discover camera IDs
let photo = phone.camera(Some("0")).await?;        // take photo, base64 WebP
```

### Keyboard (Desktop Only)

```rust
phone.hold_key("alt").await?;                     // hold a key
phone.press_key("tab").await?;                    // press + release
phone.release_key("alt").await?;                  // release held key
```

### Generic

```rust
use serde_json::json;
let resp = phone.send_command("custom_cmd", Some(json!({"foo": "bar"}))).await?;
```

## State

```rust
phone.connected().await       // true if WebSocket is authenticated
phone.phone_connected().await // true if target phone is online
phone.worker_url()            // current worker WebSocket URL
```

## Error Handling

All methods return `screenmcp::Result<T>`. The error type `ScreenMCPError` has these variants:

| Variant | When |
|---|---|
| `Auth(String)` | Worker rejected auth (bad API key, no access) |
| `Connection(String)` | WebSocket dropped or failed to connect |
| `Command(String)` | Phone returned an error for a command |
| `Timeout(String)` | Command didn't get a response within timeout |
| `NotConnected` | Tried to send a command before connecting |
| `Discovery { status, body }` | HTTP discovery call failed |
| `WebSocket(Error)` | Low-level WebSocket error |
| `Http(Error)` | HTTP client error |
| `Json(Error)` | JSON parse error |

## Connection Lifecycle

1. `connect()` calls `POST /api/discover` with Bearer token to get a worker URL
2. Opens WebSocket to the worker, sends auth message `{type: "auth", key, role: "controller", target_device_id, last_ack: 0}`
3. Waits for `{type: "auth_ok"}` from worker
4. Background tasks handle incoming messages and write outgoing messages
5. Commands are sent as `{cmd, params}`, worker assigns an ID via `{type: "cmd_accepted", id}`
6. Response arrives as `{id, status, result}` and resolves the pending oneshot channel
7. `disconnect()` closes the WebSocket and cancels pending commands

## Auto-Reconnect

When `auto_reconnect` is true and the connection drops, call `reconnect()` to attempt reconnection with exponential backoff (1s, 2s, 4s, 8s, 16s, 30s). Re-discovery happens on each attempt to handle worker failover.

## Architecture

```
ScreenMCPClient
  ├── HTTP client (reqwest) ──→ POST /api/discover ──→ wsUrl
  ├── WebSocket (tokio-tungstenite)
  │     ├── writer task: channel → WebSocket sink
  │     └── reader task: WebSocket stream → dispatch to pending commands
  └── pending commands: HashMap<i64, oneshot::Sender<CommandResponse>>
```

## Files

| File | Description |
|---|---|
| `src/lib.rs` | Public re-exports |
| `src/client.rs` | `ScreenMCPClient` — connection, commands, reconnect |
| `src/types.rs` | Options, result types, wire protocol message parsing |
| `src/error.rs` | `ScreenMCPError` enum and `Result` type alias |
