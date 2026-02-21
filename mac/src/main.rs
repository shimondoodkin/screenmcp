mod commands;
mod config;
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
                .unwrap_or_else(|_| "screenmcp_mac=info".into()),
        )
        .init();

    info!("ScreenMCP Mac starting");

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
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("failed to build tokio runtime");

        rt.block_on(async {
            ws::run_ws_manager(ws_cmd_rx, status_tx, config_clone).await;
        });

        info!("ws manager thread exiting");
        // Force exit when WS manager shuts down (tray may have already exited)
        std::process::exit(0);
    });

    // Run the tray on the main thread (required by macOS for AppKit/menu bar)
    tray::run_tray(ws_cmd_tx, status_rx);

    info!("ScreenMCP Mac shutting down");
}
