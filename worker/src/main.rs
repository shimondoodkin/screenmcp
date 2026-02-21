mod backend;
mod connections;
mod protocol;
mod ws;

use std::env;
use std::net::SocketAddr;
use std::sync::Arc;

use tokio::net::TcpListener;
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

use backend::{AuthBackend, StateBackend};

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

    let (auth, state, backend_name): (Arc<dyn AuthBackend>, Arc<dyn StateBackend>, &str) =
        init_backend(port, &worker_id);

    // In-memory connection registry
    let connections = connections::Connections::new();

    // Lifecycle: startup
    match auth.on_startup().await {
        Ok(_) => {}
        Err(e) => error!("startup hook failed: {e}"),
    }

    // Start WebSocket server
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = TcpListener::bind(addr).await.expect("failed to bind");

    info!(%addr, %worker_id, backend = %backend_name, "worker listening");

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
                    ws::handle_connection(stream, peer_addr, state, connections, auth).await;
                });
            }
            Err(e) => {
                error!("accept error: {e}");
            }
        }
    }
}

/// Initialize backends based on compile-time features.
///
/// With `--features api`: uses API auth (web server) + Redis state.
/// Without (default): uses file auth (TOML config) + in-memory state.
fn init_backend(
    _port: u16,
    _worker_id: &str,
) -> (Arc<dyn AuthBackend>, Arc<dyn StateBackend>, &'static str) {
    #[cfg(feature = "api")]
    {
        use backend::api_auth::ApiAuth;
        use backend::api_state::ApiState;

        let redis_url =
            env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".into());
        let register_with_server =
            env::var("REGISTER_WITH_SERVER").unwrap_or_default() == "true";
        let api_url = env::var("API_URL").unwrap_or_else(|_| "http://localhost:3000".into());
        let external_url = env::var("WORKER_EXTERNAL_URL")
            .unwrap_or_else(|_| format!("ws://localhost:{_port}"));
        let region = env::var("WORKER_REGION").unwrap_or_else(|_| "local".into());

        let notify_secret = env::var("NOTIFY_SECRET").ok();
        let api_auth = ApiAuth::new(
            api_url,
            _worker_id.to_string(),
            external_url,
            region,
            register_with_server,
            notify_secret,
        );

        let api_state = ApiState::new(&redis_url, _worker_id.to_string())
            .expect("failed to create Redis client");

        (Arc::new(api_auth), Arc::new(api_state), "api")
    }

    #[cfg(not(feature = "api"))]
    {
        use backend::file_auth::FileAuth;
        use backend::file_state::FileState;

        let config_path = env::var("SCREENMCP_CONFIG").unwrap_or_else(|_| {
            let home = env::var("HOME").unwrap_or_else(|_| ".".into());
            format!("{home}/.screenmcp/worker.toml")
        });

        info!(%config_path, "using file backend");

        let file_auth = FileAuth::from_file(&config_path)
            .unwrap_or_else(|e| panic!("failed to load config: {e}"));
        let file_state = FileState::new();

        (Arc::new(file_auth), Arc::new(file_state), "file")
    }
}
