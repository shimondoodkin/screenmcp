mod connections;
mod db;
mod protocol;
mod state;
mod ws;

use std::env;
use std::net::SocketAddr;
use std::sync::Arc;

use tokio::net::TcpListener;
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

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

    let redis_url = env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".into());
    let database_url = env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://screenmcp:screenmcp@127.0.0.1:5432/screenmcp".into());

    let worker_id = env::var("WORKER_ID").unwrap_or_else(|_| uuid::Uuid::new_v4().to_string());

    // Registration config
    let register_with_server =
        env::var("REGISTER_WITH_SERVER").unwrap_or_default() == "true";
    let api_url = env::var("API_URL").unwrap_or_else(|_| "http://localhost:3000".into());
    let external_url = env::var("WORKER_EXTERNAL_URL")
        .unwrap_or_else(|_| format!("ws://localhost:{port}"));
    let region = env::var("WORKER_REGION").unwrap_or_else(|_| "local".into());

    // Connect to Redis
    let state = Arc::new(
        state::State::new(&redis_url, worker_id.clone())
            .expect("failed to create Redis client"),
    );

    // In-memory connection registry
    let connections = connections::Connections::new();

    // Connect to Postgres
    let _db = match db::Db::connect(&database_url).await {
        Ok(db) => {
            info!("postgres connected");
            Some(db)
        }
        Err(e) => {
            error!("failed to connect to postgres: {e}");
            None
        }
    };

    // Self-register with API server
    if register_with_server {
        info!(%api_url, %external_url, %region, "registering with server");
        match register_worker(&api_url, &worker_id, &external_url, &region).await {
            Ok(_) => info!("registered with server"),
            Err(e) => error!("failed to register with server: {e}"),
        }
    }

    // Start WebSocket server
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = TcpListener::bind(addr).await.expect("failed to bind");

    info!(%addr, %worker_id, %external_url, "worker listening");

    // Graceful shutdown handler
    let shutdown_api_url = api_url.clone();
    let shutdown_worker_id = worker_id.clone();
    let shutdown_register = register_with_server;
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        info!("shutting down...");
        if shutdown_register {
            match unregister_worker(&shutdown_api_url, &shutdown_worker_id).await {
                Ok(_) => info!("unregistered from server"),
                Err(e) => warn!("failed to unregister from server: {e}"),
            }
        }
        std::process::exit(0);
    });

    loop {
        match listener.accept().await {
            Ok((stream, peer_addr)) => {
                let state = Arc::clone(&state);
                let connections = Arc::clone(&connections);
                let api_url = api_url.clone();
                tokio::spawn(async move {
                    ws::handle_connection(stream, peer_addr, state, connections, api_url).await;
                });
            }
            Err(e) => {
                error!("accept error: {e}");
            }
        }
    }
}

async fn register_worker(
    api_url: &str,
    worker_id: &str,
    external_url: &str,
    region: &str,
) -> Result<(), reqwest::Error> {
    let client = reqwest::Client::new();
    client
        .post(format!("{api_url}/api/workers/register"))
        .json(&serde_json::json!({
            "workerId": worker_id,
            "domain": external_url,
            "region": region,
        }))
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}

async fn unregister_worker(api_url: &str, worker_id: &str) -> Result<(), reqwest::Error> {
    let client = reqwest::Client::new();
    client
        .post(format!("{api_url}/api/workers/unregister"))
        .json(&serde_json::json!({
            "workerId": worker_id,
        }))
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}
