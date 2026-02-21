#[cfg(feature = "api")]
pub mod api_auth;
#[cfg(feature = "api")]
pub mod api_state;
#[cfg(not(feature = "api"))]
pub mod file_auth;
#[cfg(not(feature = "api"))]
pub mod file_state;

use async_trait::async_trait;

use crate::protocol::Command;

pub type BackendError = Box<dyn std::error::Error + Send + Sync>;

#[async_trait]
pub trait AuthBackend: Send + Sync + 'static {
    /// Verify a token/API key, returning a user ID on success.
    async fn verify_token(&self, token: &str) -> Result<String, String>;

    /// Check if a device_id is allowed to connect.
    /// API backend always returns Ok. File backend checks against allowed list.
    async fn verify_device(&self, device_id: &str) -> Result<(), String> {
        let _ = device_id;
        Ok(())
    }

    /// Called on worker startup (e.g. register with API server).
    async fn on_startup(&self) -> Result<(), String> {
        Ok(())
    }

    /// Called on worker shutdown (e.g. unregister from API server).
    async fn on_shutdown(&self) -> Result<(), String> {
        Ok(())
    }
}

#[async_trait]
pub trait StateBackend: Send + Sync + 'static {
    async fn register_connection(&self, device_id: &str) -> Result<(), BackendError>;
    async fn unregister_connection(&self, device_id: &str) -> Result<(), BackendError>;
    async fn get_last_ack(&self, device_id: &str) -> Result<i64, BackendError>;
    async fn process_ack(&self, device_id: &str, ack_id: i64) -> Result<(), BackendError>;
    async fn get_pending_commands(
        &self,
        device_id: &str,
        since_ack: i64,
    ) -> Result<Vec<Command>, BackendError>;
    async fn enqueue_command(
        &self,
        device_id: &str,
        cmd: String,
        params: Option<serde_json::Value>,
    ) -> Result<Command, BackendError>;
    async fn store_response(
        &self,
        device_id: &str,
        cmd_id: i64,
        response_json: &str,
    ) -> Result<(), BackendError>;
}
