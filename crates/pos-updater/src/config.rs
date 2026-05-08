//! Configuration for the updater

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Updater configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdaterConfig {
    /// Base URL for the update API
    pub api_base_url: String,

    /// Current installed version
    pub current_version: String,

    /// Installation directory (where the main binary lives)
    pub install_dir: PathBuf,

    /// Backup directory for rollback support
    pub backup_dir: PathBuf,

    /// Path to the main application binary
    pub app_binary: String,

    /// Whether to auto-update on startup
    pub auto_update: bool,

    /// Check interval in seconds (0 = only on startup)
    pub check_interval: u64,
}

impl Default for UpdaterConfig {
    fn default() -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let install_dir = home.join(".e2manage-pos");
        let backup_dir = install_dir.join("backups");

        Self {
            api_base_url: "http://localhost:3000".to_string(),
            current_version: env!("CARGO_PKG_VERSION").to_string(),
            install_dir,
            backup_dir,
            app_binary: "e2manage-pos-terminal".to_string(),
            auto_update: true,
            check_interval: 0, // Only check on startup
        }
    }
}

impl UpdaterConfig {
    /// Load config from file or use defaults
    pub fn load() -> Self {
        let config_path = Self::config_path();

        if config_path.exists() {
            match std::fs::read_to_string(&config_path) {
                Ok(content) => {
                    match serde_json::from_str(&content) {
                        Ok(config) => return config,
                        Err(e) => {
                            tracing::warn!("Failed to parse config: {}. Using defaults.", e);
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to read config: {}. Using defaults.", e);
                }
            }
        }

        Self::default()
    }

    /// Save config to file
    pub fn save(&self) -> std::io::Result<()> {
        let config_path = Self::config_path();

        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let content = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

        std::fs::write(config_path, content)
    }

    /// Get the config file path
    fn config_path() -> PathBuf {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        home.join(".e2manage-pos").join("updater.json")
    }

    /// Update the current version after successful update
    pub fn set_version(&mut self, version: &str) {
        self.current_version = version.to_string();
    }
}
