use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::{broadcast, mpsc, RwLock};
use tokio::time::Instant;
use tracing::{info, warn};

/// In-memory connection registry for routing commands between controllers and phones.
pub struct Connections {
    /// Phone senders: uid → channel to send commands to the phone
    phones: RwLock<HashMap<String, mpsc::Sender<String>>>,
    /// Controller senders: uid → list of channels to send responses/events to controllers
    controllers: RwLock<HashMap<String, Vec<mpsc::Sender<String>>>>,
    /// Broadcast channel for command responses: (uid, cmd_id, response_json)
    response_tx: broadcast::Sender<(String, i64, String)>,
    /// Track last disconnect time per uid to enforce cooldown
    last_disconnect: RwLock<HashMap<String, Instant>>,
}

impl Connections {
    /// Minimum seconds between disconnect and next connect for same uid.
    const RECONNECT_COOLDOWN_SECS: u64 = 3;

    pub fn new() -> Arc<Self> {
        let (response_tx, _) = broadcast::channel(256);
        Arc::new(Self {
            phones: RwLock::new(HashMap::new()),
            controllers: RwLock::new(HashMap::new()),
            response_tx,
            last_disconnect: RwLock::new(HashMap::new()),
        })
    }

    /// Register a phone connection. Replaces any existing connection for this uid.
    /// The old connection's channel is dropped, causing it to disconnect gracefully.
    pub async fn register_phone(&self, uid: &str) -> mpsc::Receiver<String> {
        {
            let phones = self.phones.read().await;
            if let Some(existing) = phones.get(uid) {
                if !existing.is_closed() {
                    warn!(uid, "replacing existing phone connection");
                }
            }
        }

        let (tx, rx) = mpsc::channel(64);
        self.phones.write().await.insert(uid.to_string(), tx);
        info!(uid, "phone registered in connection registry");

        // Notify controllers that phone is connected
        self.notify_controllers(uid, "phone_status", true).await;

        rx
    }

    /// Unregister a phone connection.
    pub async fn unregister_phone(&self, uid: &str) {
        self.phones.write().await.remove(uid);
        self.last_disconnect.write().await.insert(uid.to_string(), Instant::now());
        info!(uid, "phone unregistered from connection registry");

        // Notify controllers that phone disconnected
        self.notify_controllers(uid, "phone_status", false).await;
    }

    /// Check if a phone is currently connected for this uid.
    pub async fn is_phone_connected(&self, uid: &str) -> bool {
        self.phones.read().await.contains_key(uid)
    }

    /// Register a controller connection. Returns an mpsc::Receiver for events/responses.
    pub async fn register_controller(&self, uid: &str) -> mpsc::Receiver<String> {
        let (tx, rx) = mpsc::channel(64);
        self.controllers
            .write()
            .await
            .entry(uid.to_string())
            .or_default()
            .push(tx);
        info!(uid, "controller registered in connection registry");
        rx
    }

    /// Unregister a specific controller (by matching the sender).
    pub async fn unregister_controller(&self, uid: &str, rx_ptr: usize) {
        let mut controllers = self.controllers.write().await;
        if let Some(senders) = controllers.get_mut(uid) {
            senders.retain(|tx| {
                // Use pointer identity to find the right sender
                let ptr = tx as *const _ as usize;
                ptr != rx_ptr
            });
            if senders.is_empty() {
                controllers.remove(uid);
            }
        }
        info!(uid, "controller unregistered from connection registry");
    }

    /// Send a command to the phone for this uid. Returns false if phone not connected.
    pub async fn send_to_phone(&self, uid: &str, message: &str) -> bool {
        let phones = self.phones.read().await;
        if let Some(tx) = phones.get(uid) {
            match tx.send(message.to_string()).await {
                Ok(_) => true,
                Err(_) => {
                    warn!(uid, "failed to send to phone — channel closed");
                    false
                }
            }
        } else {
            false
        }
    }

    /// Notify that a response was received from a phone (broadcast to waiting controllers).
    pub fn notify_response(&self, uid: &str, cmd_id: i64, response_json: &str) {
        let _ = self
            .response_tx
            .send((uid.to_string(), cmd_id, response_json.to_string()));
    }

    /// Subscribe to response notifications.
    pub fn subscribe_responses(&self) -> broadcast::Receiver<(String, i64, String)> {
        self.response_tx.subscribe()
    }

    /// Send a message to all controllers watching this uid.
    async fn notify_controllers(&self, uid: &str, event_type: &str, connected: bool) {
        let msg = serde_json::json!({
            "type": event_type,
            "connected": connected,
        })
        .to_string();

        let controllers = self.controllers.read().await;
        if let Some(senders) = controllers.get(uid) {
            for tx in senders {
                let _ = tx.send(msg.clone()).await;
            }
        }
    }
}
