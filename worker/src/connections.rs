use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::{broadcast, mpsc, RwLock};
use tokio::time::Instant;
use tracing::{info, warn};

use crate::protocol::ClientVersion;

/// In-memory connection registry for routing commands between controllers and phones.
pub struct Connections {
    /// Phone senders: device_id -> channel to send commands to the phone
    phones: RwLock<HashMap<String, mpsc::Sender<String>>>,
    /// Controller senders: device_id -> list of channels to send responses/events to controllers
    controllers: RwLock<HashMap<String, Vec<mpsc::Sender<String>>>>,
    /// Broadcast channel for command responses: (device_id, cmd_id, response_json)
    response_tx: broadcast::Sender<(String, i64, String)>,
    /// Track last disconnect time per device_id to enforce cooldown
    last_disconnect: RwLock<HashMap<String, Instant>>,
    /// SSE client senders: device_id -> channel to push SSE events
    sse_clients: RwLock<HashMap<String, mpsc::Sender<String>>>,
    /// Version info per connection: device_id -> ClientVersion
    versions: RwLock<HashMap<String, ClientVersion>>,
}

impl Connections {
    /// Minimum seconds between disconnect and next connect for same device_id.
    const RECONNECT_COOLDOWN_SECS: u64 = 3;

    pub fn new() -> Arc<Self> {
        let (response_tx, _) = broadcast::channel(256);
        Arc::new(Self {
            phones: RwLock::new(HashMap::new()),
            controllers: RwLock::new(HashMap::new()),
            response_tx,
            last_disconnect: RwLock::new(HashMap::new()),
            sse_clients: RwLock::new(HashMap::new()),
            versions: RwLock::new(HashMap::new()),
        })
    }

    /// Register a phone connection. Replaces any existing connection for this device_id.
    /// The old connection's channel is dropped, causing it to disconnect gracefully.
    pub async fn register_phone(&self, device_id: &str) -> mpsc::Receiver<String> {
        {
            let phones = self.phones.read().await;
            if let Some(existing) = phones.get(device_id) {
                if !existing.is_closed() {
                    warn!(device_id, "replacing existing phone connection");
                }
            }
        }

        let (tx, rx) = mpsc::channel(64);
        self.phones.write().await.insert(device_id.to_string(), tx);
        info!(device_id, "phone registered in connection registry");

        // Notify controllers that phone is connected
        self.notify_controllers(device_id, "phone_status", true).await;

        rx
    }

    /// Unregister a phone connection.
    pub async fn unregister_phone(&self, device_id: &str) {
        self.phones.write().await.remove(device_id);
        self.last_disconnect.write().await.insert(device_id.to_string(), Instant::now());
        info!(device_id, "phone unregistered from connection registry");

        // Notify controllers that phone disconnected
        self.notify_controllers(device_id, "phone_status", false).await;
    }

    /// Check if a phone is currently connected for this device_id.
    pub async fn is_phone_connected(&self, device_id: &str) -> bool {
        self.phones.read().await.contains_key(device_id)
    }

    /// Register a controller connection. Returns an mpsc::Receiver for events/responses.
    pub async fn register_controller(&self, device_id: &str) -> mpsc::Receiver<String> {
        let (tx, rx) = mpsc::channel(64);
        self.controllers
            .write()
            .await
            .entry(device_id.to_string())
            .or_default()
            .push(tx);
        info!(device_id, "controller registered in connection registry");
        rx
    }

    /// Unregister a specific controller (by matching the sender).
    pub async fn unregister_controller(&self, device_id: &str, rx_ptr: usize) {
        let mut controllers = self.controllers.write().await;
        if let Some(senders) = controllers.get_mut(device_id) {
            senders.retain(|tx| {
                // Use pointer identity to find the right sender
                let ptr = tx as *const _ as usize;
                ptr != rx_ptr
            });
            if senders.is_empty() {
                controllers.remove(device_id);
            }
        }
        info!(device_id, "controller unregistered from connection registry");
    }

    /// Send a command to the phone for this device_id. Returns false if phone not connected.
    pub async fn send_to_phone(&self, device_id: &str, message: &str) -> bool {
        let phones = self.phones.read().await;
        if let Some(tx) = phones.get(device_id) {
            match tx.send(message.to_string()).await {
                Ok(_) => true,
                Err(_) => {
                    warn!(device_id, "failed to send to phone — channel closed");
                    false
                }
            }
        } else {
            false
        }
    }

    /// Notify that a response was received from a phone (broadcast to waiting controllers).
    pub fn notify_response(&self, device_id: &str, cmd_id: i64, response_json: &str) {
        let _ = self
            .response_tx
            .send((device_id.to_string(), cmd_id, response_json.to_string()));
    }

    /// Subscribe to response notifications.
    pub fn subscribe_responses(&self) -> broadcast::Receiver<(String, i64, String)> {
        self.response_tx.subscribe()
    }

    /// Register an SSE client for a device_id. Returns a Receiver for events.
    /// Replaces any existing SSE connection for the same device_id.
    pub async fn register_sse(&self, device_id: &str) -> mpsc::Receiver<String> {
        let (tx, rx) = mpsc::channel(64);
        let mut clients = self.sse_clients.write().await;
        if clients.contains_key(device_id) {
            warn!(device_id, "replacing existing SSE connection");
        }
        clients.insert(device_id.to_string(), tx);
        info!(device_id, "SSE client registered");
        rx
    }

    /// Unregister an SSE client for a device_id.
    pub async fn unregister_sse(&self, device_id: &str) {
        self.sse_clients.write().await.remove(device_id);
        info!(device_id, "SSE client unregistered");
    }

    /// Send an SSE event to a specific device. Returns true if sent.
    pub async fn send_sse_event(&self, device_id: &str, event_json: &str) -> bool {
        let clients = self.sse_clients.read().await;
        if let Some(tx) = clients.get(device_id) {
            tx.try_send(event_json.to_string()).is_ok()
        } else {
            false
        }
    }

    /// Store the version info for a connected device.
    pub async fn set_version(&self, device_id: &str, version: ClientVersion) {
        info!(device_id, %version, "storing client version");
        self.versions.write().await.insert(device_id.to_string(), version);
    }

    /// Get the stored version for a device, if any.
    pub async fn get_version(&self, device_id: &str) -> Option<ClientVersion> {
        self.versions.read().await.get(device_id).cloned()
    }

    /// Remove version info when a device disconnects.
    pub async fn remove_version(&self, device_id: &str) {
        self.versions.write().await.remove(device_id);
    }

    /// Send a message to all controllers watching this device_id.
    async fn notify_controllers(&self, device_id: &str, event_type: &str, connected: bool) {
        let msg = serde_json::json!({
            "type": event_type,
            "connected": connected,
        })
        .to_string();

        let controllers = self.controllers.read().await;
        if let Some(senders) = controllers.get(device_id) {
            for tx in senders {
                let _ = tx.send(msg.clone()).await;
            }
        }
    }
}
