use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// API server URL for worker discovery (e.g. https://server10.doodkin.com)
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
}

fn default_api_url() -> String {
    "https://server10.doodkin.com".to_string()
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
        }
    }
}

impl Config {
    /// Get the config file path.
    /// On macOS: ~/Library/Application Support/phonemcp/config.toml
    /// Falls back to dirs::config_dir() which returns the correct path per platform.
    pub fn config_path() -> PathBuf {
        // On macOS, dirs::config_dir() returns ~/Library/Application Support/
        // which is the standard macOS location for application configuration.
        let base = dirs::config_dir().unwrap_or_else(|| {
            // Fallback: use home directory
            dirs::home_dir()
                .map(|h| h.join("Library").join("Application Support"))
                .unwrap_or_else(|| PathBuf::from("."))
        });
        base.join("phonemcp").join("config.toml")
    }

    /// Load config from disk, or return default if not found.
    pub fn load() -> Self {
        let path = Self::config_path();
        match std::fs::read_to_string(&path) {
            Ok(contents) => match toml::from_str(&contents) {
                Ok(config) => config,
                Err(e) => {
                    tracing::warn!("failed to parse config at {}: {e}", path.display());
                    Self::default()
                }
            },
            Err(_) => Self::default(),
        }
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
        !self.token.is_empty()
    }
}
