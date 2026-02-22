use std::env;
use std::net::SocketAddr;
use std::sync::Arc;

use tokio::net::TcpListener;
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

use screenmcp_worker::file_auth::FileAuth;
use screenmcp_worker::file_state::FileState;
use screenmcp_worker::{AuthBackend, StateBackend};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let port: u16 = env::var("PORT")
        .unwrap_or_else(|_| "8080".into())
        .parse()
        .expect("PORT must be a number");

    let worker_id = env::var("WORKER_ID").unwrap_or_else(|_| uuid::Uuid::new_v4().to_string());

    let config_path = env::var("SCREENMCP_CONFIG").unwrap_or_else(|_| {
        let home = env::var("HOME").unwrap_or_else(|_| ".".into());
        format!("{home}/.screenmcp/worker.toml")
    });

    info!(%config_path, "using file backend");

    let file_auth = FileAuth::from_file(&config_path)
        .unwrap_or_else(|e| panic!("failed to load config: {e}"));
    let file_state = FileState::new();

    let auth: Arc<dyn AuthBackend> = Arc::new(file_auth);
    let state: Arc<dyn StateBackend> = Arc::new(file_state);

    // In-memory connection registry
    let connections = screenmcp_worker::connections::Connections::new();

    // Lifecycle: startup
    match auth.on_startup().await {
        Ok(_) => {}
        Err(e) => error!("startup hook failed: {e}"),
    }

    // Start WebSocket server
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = TcpListener::bind(addr).await.expect("failed to bind");

    info!(%addr, %worker_id, backend = "file", "worker listening");

    // Graceful shutdown handler
    let shutdown_auth = Arc::clone(&auth);
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        info!("shutting down...");
        match shutdown_auth.on_shutdown().await {
            Ok(_) => {}
            Err(e) => warn!("shutdown hook failed: {e}"),
        }
        std::process::exit(0);
    });

    loop {
        match listener.accept().await {
            Ok((stream, peer_addr)) => {
                let state = Arc::clone(&state);
                let connections = Arc::clone(&connections);
                let auth = Arc::clone(&auth);
                tokio::spawn(async move {
                    screenmcp_worker::ws::handle_connection(stream, peer_addr, state, connections, auth).await;
                });
            }
            Err(e) => {
                error!("accept error: {e}");
            }
        }
    }
}
