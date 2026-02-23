mod auth;
mod commands;
mod config;
mod sse;
mod tray;
mod ws;

use config::Config;
use tokio::sync::{mpsc, watch};
use tracing::info;
use ws::{ConnectionStatus, WsCommand};

fn main() {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "screenmcp_linux=info".into()),
        )
        .init();

    info!("ScreenMCP Linux starting");

    // Load config
    let config = Config::load();
    info!("config loaded from {}", Config::config_path().display());

    if !config.is_ready() {
        info!("no token configured — saving default config and starting tray");
        if let Err(e) = config.save() {
            tracing::warn!("failed to save default config: {e}");
        }
        info!(
            "please edit {} and add your API token",
            Config::config_path().display()
        );
    }

    // Create channels
    let (ws_cmd_tx, ws_cmd_rx) = mpsc::channel::<WsCommand>(32);
    let (status_tx, status_rx) = watch::channel(ConnectionStatus::Disconnected);
    let (auth_event_tx, auth_event_rx) = mpsc::channel(4);
    let (port_tx, port_rx) = std::sync::mpsc::channel();

    // Start tokio runtime in background thread
    let config_clone = config.clone();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("failed to build tokio runtime");

        rt.block_on(async {
            // Start local HTTP server for auth callbacks
            let local_port = auth::start_local_server(auth_event_tx).await;
            let _ = port_tx.send(local_port);

            // If opensource server mode is enabled, start SSE listener
            if config_clone.opensource_server_enabled {
                info!("opensource server mode enabled, starting SSE listener");
                let (shutdown_tx, shutdown_rx) = tokio::sync::broadcast::channel::<()>(1);
                let sse_config = config_clone.clone();
                let sse_ws_tx = ws_cmd_tx.clone();
                tokio::spawn(async move {
                    sse::run_sse_listener(sse_config, sse_ws_tx, shutdown_rx).await;
                });
                // Note: shutdown_tx will be dropped when ws_manager exits
                let _ = shutdown_tx;
            }

            ws::run_ws_manager(ws_cmd_rx, status_tx, config_clone).await;
        });

        info!("ws manager thread exiting");
        std::process::exit(0);
    });

    // Wait for the local server port
    let local_port = port_rx.recv().expect("failed to get local server port");
    info!("local server on port {local_port}");

    // Run the tray on the main thread (required by most Linux desktop toolkits)
    tray::run_tray(ws_cmd_tx, status_rx, local_port, auth_event_rx);

    info!("ScreenMCP Linux shutting down");
}
