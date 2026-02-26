pub mod connections;
pub mod protocol;
pub mod ws;
pub mod file_auth;
pub mod file_state;
pub mod ip_whitelist;

use async_trait::async_trait;

use crate::protocol::Command;

pub type BackendError = Box<dyn std::error::Error + Send + Sync>;

// ---------------------------------------------------------------------------
// IP tracking backend — extensible trait for recording connection IPs
// ---------------------------------------------------------------------------

/// Pluggable IP tracking. The open-source worker ships with `NoopIpTracking`;
/// the cloud worker overrides with real IP recording.
#[async_trait]
pub trait IpTrackingBackend: Send + Sync + 'static {
    /// Record a connection IP after successful auth.
    async fn record_ip(
        &self,
        _firebase_uid: &str,
        _device_id: &str,
        _ip_address: &str,
        _role: &str,
    ) {}
}

/// No-op IP tracking backend — never records.
pub struct NoopIpTracking;

#[async_trait]
impl IpTrackingBackend for NoopIpTracking {}

// ---------------------------------------------------------------------------
// Usage backend — extensible trait for command-level usage tracking
// ---------------------------------------------------------------------------

/// Result of a usage check.
#[derive(Debug, Clone)]
pub enum UsageCheck {
    Allowed,
    LimitReached { current: i64, limit: i64 },
}

/// Pluggable usage tracking.  The open-source worker ships with `NoopUsage`;
/// the cloud worker overrides with real limit checking + logging.
#[async_trait]
pub trait UsageBackend: Send + Sync + 'static {
    /// Check whether the command is allowed and, if so, record it.
    async fn check_and_record(
        &self,
        api_key: &str,
        firebase_uid: &str,
        command: &str,
        device_id: Option<&str>,
    ) -> UsageCheck;

    /// Flush any buffered usage data for the given API key (e.g. on disconnect).
    async fn flush_key(&self, _api_key: &str) {}
}

/// No-op usage backend — always allows, never records.
pub struct NoopUsage;

#[async_trait]
impl UsageBackend for NoopUsage {
    async fn check_and_record(
        &self,
        _api_key: &str,
        _firebase_uid: &str,
        _command: &str,
        _device_id: Option<&str>,
    ) -> UsageCheck {
        UsageCheck::Allowed
    }
}

/// Result from token verification, includes user ID and optional IP whitelist.
#[derive(Debug, Clone)]
pub struct VerifyResult {
    pub firebase_uid: String,
    /// IP whitelist text (one IP/CIDR per line). Empty = whitelist disabled.
    pub ip_whitelist: String,
}

#[async_trait]
pub trait AuthBackend: Send + Sync + 'static {
    /// Verify a token/API key, returning user info on success.
    async fn verify_token(&self, token: &str) -> Result<VerifyResult, String>;

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

    /// Shared secret for POST /notify authentication.
    /// If None, /notify is unauthenticated (backwards compat / dev).
    fn notify_secret(&self) -> Option<&str> {
        None
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
