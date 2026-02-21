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

    // Create channels for communication between tray and WS manager
    let (ws_cmd_tx, ws_cmd_rx) = mpsc::channel::<WsCommand>(32);
    let (status_tx, status_rx) = watch::channel(ConnectionStatus::Disconnected);

    // Start the tokio runtime for the WS manager in a background thread
    let config_clone = config.clone();
    let ws_cmd_tx_for_sse = ws_cmd_tx.clone();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("failed to build tokio runtime");

        rt.block_on(async {
            // If opensource server mode is enabled, start SSE listener alongside WS manager
            let (sse_shutdown_tx, sse_shutdown_rx) = tokio::sync::broadcast::channel::<()>(1);

            if config_clone.opensource_server_enabled && config_clone.is_ready() {
                let sse_config = config_clone.clone();
                let sse_tx = ws_cmd_tx_for_sse.clone();
                tokio::spawn(async move {
                    sse::run_sse_listener(sse_config, sse_tx, sse_shutdown_rx).await;
                });
                info!("SSE listener started for open source server mode");
            }

            ws::run_ws_manager(ws_cmd_rx, status_tx, config_clone).await;

            // Shut down SSE listener when WS manager exits
            let _ = sse_shutdown_tx.send(());
        });

        info!("ws manager thread exiting");
        // Force exit when WS manager shuts down (tray may have already exited)
        std::process::exit(0);
    });

    // Run the tray on the main thread (required by most Linux desktop toolkits)
    tray::run_tray(ws_cmd_tx, status_rx);

    info!("ScreenMCP Linux shutting down");
}
