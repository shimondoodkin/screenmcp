use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Instant;

use async_trait::async_trait;
use tokio::sync::RwLock;
use tracing::{info, warn};

use crate::protocol::Command;
use super::{BackendError, StateBackend};

/// Per-device state held in memory.
struct DeviceState {
    pending: Vec<Command>,
    last_ack: i64,
    cmd_counter: AtomicI64,
}

impl DeviceState {
    fn new() -> Self {
        Self {
            pending: Vec::new(),
            last_ack: 0,
            cmd_counter: AtomicI64::new(0),
        }
    }
}

struct StoredResponse {
    json: String,
    created: Instant,
}

/// In-memory state backend — no Redis required.
pub struct FileState {
    devices: RwLock<HashMap<String, DeviceState>>,
    responses: RwLock<HashMap<String, StoredResponse>>,
}

impl FileState {
    pub fn new() -> Self {
        let state = Self {
            devices: RwLock::new(HashMap::new()),
            responses: RwLock::new(HashMap::new()),
        };
        state.start_cleanup_task();
        state
    }

    fn start_cleanup_task(&self) {
        // We can't move &self into the spawned task, so we'll handle cleanup
        // in store_response instead (lazy cleanup). For a long-running worker,
        // this is sufficient since responses expire after 5 minutes.
    }
}

#[async_trait]
impl StateBackend for FileState {
    async fn register_connection(&self, device_id: &str) -> Result<(), BackendError> {
        let mut devices = self.devices.write().await;
        devices.entry(device_id.to_string()).or_insert_with(DeviceState::new);
        info!(device_id, "registered connection (in-memory)");
        Ok(())
    }

    async fn unregister_connection(&self, device_id: &str) -> Result<(), BackendError> {
        // Keep device state for reconnection (don't remove)
        info!(device_id, "unregistered connection (in-memory)");
        Ok(())
    }

    async fn get_last_ack(&self, device_id: &str) -> Result<i64, BackendError> {
        let devices = self.devices.read().await;
        Ok(devices.get(device_id).map(|d| d.last_ack).unwrap_or(0))
    }

    async fn process_ack(&self, device_id: &str, ack_id: i64) -> Result<(), BackendError> {
        let mut devices = self.devices.write().await;
        if let Some(device) = devices.get_mut(device_id) {
            device.last_ack = ack_id;
            device.pending.retain(|c| c.id > ack_id);
        }
        info!(device_id, ack_id, "processed ack (in-memory)");
        Ok(())
    }

    async fn get_pending_commands(
        &self,
        device_id: &str,
        since_ack: i64,
    ) -> Result<Vec<Command>, BackendError> {
        let devices = self.devices.read().await;
        let commands = match devices.get(device_id) {
            Some(device) => device
                .pending
                .iter()
                .filter(|c| c.id > since_ack)
                .cloned()
                .collect(),
            None => vec![],
        };
        info!(device_id, since_ack, count = commands.len(), "pending commands to replay (in-memory)");
        Ok(commands)
    }

    async fn enqueue_command(
        &self,
        device_id: &str,
        cmd: String,
        params: Option<serde_json::Value>,
    ) -> Result<Command, BackendError> {
        let mut devices = self.devices.write().await;
        let device = devices
            .entry(device_id.to_string())
            .or_insert_with(DeviceState::new);

        let id = device.cmd_counter.fetch_add(1, Ordering::SeqCst) + 1;
        let command = Command { id, cmd, params };

        if device.pending.len() >= 50 {
            warn!(device_id, "max pending commands reached (50)");
        }

        device.pending.push(command.clone());
        info!(device_id, id, "enqueued command (in-memory)");
        Ok(command)
    }

    async fn store_response(
        &self,
        device_id: &str,
        cmd_id: i64,
        response_json: &str,
    ) -> Result<(), BackendError> {
        let mut responses = self.responses.write().await;

        // Lazy cleanup: remove expired responses (older than 5 minutes)
        let now = Instant::now();
        responses.retain(|_, v| now.duration_since(v.created).as_secs() < 300);

        let key = format!("{device_id}:{cmd_id}");
        responses.insert(
            key,
            StoredResponse {
                json: response_json.to_string(),
                created: now,
            },
        );
        Ok(())
    }
}
