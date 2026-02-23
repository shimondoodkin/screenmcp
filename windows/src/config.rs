use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// API server URL for worker discovery (e.g. https://screenmcp.com)
    #[serde(default = "default_api_url")]
    pub api_url: String,

    /// Direct worker WebSocket URL (optional, bypasses discovery)
    #[serde(default)]
    pub worker_url: Option<String>,

    /// Auth token (Firebase ID token or API key starting with pk_)
    #[serde(default)]
    pub token: String,

    /// Auto-connect on startup
    #[serde(default = "default_true")]
    pub auto_connect: bool,

    /// Screenshot quality (1-100) for WebP encoding
    #[serde(default = "default_quality")]
    pub screenshot_quality: u8,

    /// Max screenshot width (resizes if larger)
    #[serde(default)]
    pub max_screenshot_width: Option<u32>,

    /// Max screenshot height (resizes if larger)
    #[serde(default)]
    pub max_screenshot_height: Option<u32>,

    /// Unique device ID (auto-generated cryptographically secure ID on first run)
    #[serde(default)]
    pub device_id: String,

    /// Enable open source server mode
    #[serde(default)]
    pub opensource_server_enabled: bool,

    /// User ID for open source server mode (used as Bearer token)
    #[serde(default)]
    pub opensource_user_id: String,

    /// API server URL for open source server mode
    #[serde(default)]
    pub opensource_api_url: String,
}

fn default_api_url() -> String {
    "https://screenmcp.com".to_string()
}

fn default_true() -> bool {
    true
}

fn default_quality() -> u8 {
    80
}

impl Default for Config {
    fn default() -> Self {
        Self {
            api_url: default_api_url(),
            worker_url: None,
            token: String::new(),
            auto_connect: true,
            screenshot_quality: default_quality(),
            max_screenshot_width: None,
            max_screenshot_height: None,
            device_id: String::new(),
            opensource_server_enabled: false,
            opensource_user_id: String::new(),
            opensource_api_url: String::new(),
        }
    }
}

impl Config {
    /// Get the config file path: ~/.screenmcp/config.toml
    pub fn config_path() -> PathBuf {
        let base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
        base.join("screenmcp").join("config.toml")
    }

    /// Load config from disk, or return default if not found.
    /// Auto-generates a cryptographically secure device_id on first load if missing, and saves it.
    pub fn load() -> Self {
        let path = Self::config_path();
        let mut config = match std::fs::read_to_string(&path) {
            Ok(contents) => match toml::from_str(&contents) {
                Ok(config) => config,
                Err(e) => {
                    tracing::warn!("failed to parse config at {}: {e}", path.display());
                    Self::default()
                }
            },
            Err(_) => Self::default(),
        };

        if config.device_id.is_empty() {
            let mut bytes = [0u8; 16];
            getrandom::getrandom(&mut bytes).expect("failed to generate random device_id");
            config.device_id = bytes.iter().map(|b| format!("{:02x}", b)).collect();
            tracing::info!("generated new device_id: {}", config.device_id);
            if let Err(e) = config.save() {
                tracing::warn!("failed to save config with new device_id: {e}");
            }
        }

        config
    }

    /// Save config to disk.
    pub fn save(&self) -> Result<(), String> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("failed to create config dir: {e}"))?;
        }
        let contents =
            toml::to_string_pretty(self).map_err(|e| format!("failed to serialize config: {e}"))?;
        std::fs::write(&path, contents)
            .map_err(|e| format!("failed to write config to {}: {e}", path.display()))?;
        tracing::info!("config saved to {}", path.display());
        Ok(())
    }

    /// Check if the config has enough info to connect.
    pub fn is_ready(&self) -> bool {
        if self.opensource_server_enabled {
            !self.opensource_user_id.is_empty() && !self.opensource_api_url.is_empty()
        } else {
            !self.token.is_empty()
        }
    }

    /// Get the effective auth token (opensource user_id or normal token).
    pub fn effective_token(&self) -> &str {
        if self.opensource_server_enabled {
            &self.opensource_user_id
        } else {
            &self.token
        }
    }

    /// Get the effective API URL (opensource or normal).
    pub fn effective_api_url(&self) -> &str {
        if self.opensource_server_enabled {
            &self.opensource_api_url
        } else {
            &self.api_url
        }
    }
}
