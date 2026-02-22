use async_trait::async_trait;
use serde::Deserialize;
use tracing::{info, warn};

use crate::AuthBackend;

#[derive(Debug, Deserialize)]
pub struct FileConfig {
    pub user: UserConfig,
    pub auth: AuthConfig,
    #[serde(default)]
    pub devices: DevicesConfig,
}

#[derive(Debug, Deserialize)]
pub struct UserConfig {
    pub id: String,
}

#[derive(Debug, Deserialize)]
pub struct AuthConfig {
    pub api_keys: Vec<String>,
    #[serde(default)]
    pub notify_secret: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct DevicesConfig {
    #[serde(default)]
    pub allowed: Vec<String>,
}

pub struct FileAuth {
    config: FileConfig,
}

impl FileAuth {
    pub fn from_file(path: &str) -> Result<Self, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("failed to read config file {path}: {e}"))?;
        let config: FileConfig =
            toml::from_str(&content).map_err(|e| format!("failed to parse config: {e}"))?;

        if config.auth.api_keys.is_empty() {
            return Err("config must have at least one API key in [auth].api_keys".to_string());
        }

        info!(
            user_id = %config.user.id,
            api_keys = config.auth.api_keys.len(),
            allowed_devices = config.devices.allowed.len(),
            notify_secret = config.auth.notify_secret.is_some(),
            "loaded file auth config"
        );
        Ok(Self { config })
    }
}

#[async_trait]
impl AuthBackend for FileAuth {
    async fn verify_token(&self, token: &str) -> Result<String, String> {
        // Accept user_id as auth token (phones send this) or API keys (controllers)
        if token == self.config.user.id {
            return Ok(self.config.user.id.clone());
        }
        if self.config.auth.api_keys.iter().any(|k| k == token) {
            Ok(self.config.user.id.clone())
        } else {
            warn!("rejected token (not user_id or API key in config)");
            Err("invalid token — not found in worker.toml".to_string())
        }
    }

    fn notify_secret(&self) -> Option<&str> {
        self.config.auth.notify_secret.as_deref()
    }

    async fn verify_device(&self, device_id: &str) -> Result<(), String> {
        if self.config.devices.allowed.is_empty() {
            // Empty list = accept all devices
            return Ok(());
        }

        // Each entry is "device_id [optional description]" — match on first word
        let allowed = self.config.devices.allowed.iter().any(|entry| {
            let id = entry.split_whitespace().next().unwrap_or(entry);
            id == device_id
        });

        if allowed {
            Ok(())
        } else {
            warn!(
                device_id,
                "rejected device_id — add it to [devices].allowed in worker.toml to allow"
            );
            Err(format!(
                "device_id {device_id} not in allowed list — add it to [devices].allowed in worker.toml"
            ))
        }
    }
}
