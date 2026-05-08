//! POS Updater - Auto-update and launcher for E2Manage POS Terminal
//!
//! This crate provides a robust update mechanism:
//! 1. Check for updates from backend
//! 2. Download update package (tar.gz)
//! 3. Verify checksum
//! 4. Extract and replace binary
//! 5. Launch main application
//! 6. Rollback on failure

pub mod config;
pub mod downloader;
pub mod extractor;
pub mod launcher;
pub mod rollback;
pub mod version;

pub use config::UpdaterConfig;
pub use downloader::Downloader;
pub use extractor::Extractor;
pub use launcher::Launcher;
pub use rollback::RollbackManager;
pub use version::{VersionChecker, VersionInfo};

use thiserror::Error;

#[derive(Error, Debug)]
pub enum UpdateError {
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Checksum mismatch: expected {expected}, got {actual}")]
    ChecksumMismatch { expected: String, actual: String },

    #[error("Version parse error: {0}")]
    VersionParse(String),

    #[error("Extract error: {0}")]
    Extract(String),

    #[error("Launch error: {0}")]
    Launch(String),

    #[error("Rollback error: {0}")]
    Rollback(String),

    #[error("Config error: {0}")]
    Config(String),

    #[error("No update available")]
    NoUpdate,
}

pub type Result<T> = std::result::Result<T, UpdateError>;

/// Main updater orchestrator
pub struct Updater {
    config: UpdaterConfig,
    version_checker: VersionChecker,
    downloader: Downloader,
    extractor: Extractor,
    rollback: RollbackManager,
    launcher: Launcher,
}

impl Updater {
    pub fn new(config: UpdaterConfig) -> Result<Self> {
        let version_checker = VersionChecker::new(&config.api_base_url);
        let downloader = Downloader::new();
        let extractor = Extractor::new();
        let rollback = RollbackManager::new(&config.install_dir, &config.backup_dir)?;
        let launcher = Launcher::new(&config.install_dir);

        Ok(Self {
            config,
            version_checker,
            downloader,
            extractor,
            rollback,
            launcher,
        })
    }

    /// Check for updates and apply if available
    pub async fn check_and_update(&self) -> Result<bool> {
        tracing::info!("Checking for updates...");

        // 1. Check version
        let current = self.config.current_version.clone();
        let version_info = self.version_checker.check(&current).await?;

        if !version_info.update_available {
            tracing::info!("No update available. Current: {}", current);
            return Ok(false);
        }

        tracing::info!(
            "Update available: {} -> {}",
            current,
            version_info.latest_version
        );

        // 2. Create backup before update
        self.rollback.create_backup()?;
        tracing::info!("Backup created");

        // 3. Download update
        let download_path = self
            .downloader
            .download(&version_info.download_url, &version_info.checksum)
            .await?;
        tracing::info!("Update downloaded to {:?}", download_path);

        // 4. Extract and install
        match self.extractor.extract(&download_path, &self.config.install_dir) {
            Ok(_) => {
                tracing::info!("Update extracted successfully");
                // Clean up old backup after successful update
                self.rollback.cleanup_old_backups(3)?;
                Ok(true)
            }
            Err(e) => {
                tracing::error!("Extract failed: {}. Rolling back...", e);
                self.rollback.restore_backup()?;
                Err(e)
            }
        }
    }

    /// Launch the main application
    pub fn launch(&self) -> Result<()> {
        self.launcher.launch()
    }

    /// Get current version
    pub fn current_version(&self) -> &str {
        &self.config.current_version
    }
}
