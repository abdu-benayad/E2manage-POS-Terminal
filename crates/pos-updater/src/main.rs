//! POS Launcher - Entry point for the auto-updater
//!
//! This is the main binary that should be run instead of the POS terminal directly.
//! It handles:
//! 1. Checking for updates on startup
//! 2. Downloading and installing updates
//! 3. Launching the main application
//! 4. Rolling back on failure

use pos_updater::{Updater, UpdaterConfig, Result, UpdateError};
use std::env;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("pos_updater=info".parse().unwrap()),
        )
        .init();

    tracing::info!("===========================================");
    tracing::info!("  E2Manage POS Launcher v{}", env!("CARGO_PKG_VERSION"));
    tracing::info!("===========================================");

    // Parse command line arguments
    let args: Vec<String> = env::args().collect();
    let skip_update = args.contains(&"--no-update".to_string());
    let force_update = args.contains(&"--force-update".to_string());
    let check_only = args.contains(&"--check-only".to_string());

    // Load or create configuration
    let mut config = UpdaterConfig::load();

    // Override install dir if running from a different location
    if let Ok(exe_path) = env::current_exe() {
        if let Some(parent) = exe_path.parent() {
            config.install_dir = parent.to_path_buf();
            config.backup_dir = parent.join("backups");
        }
    }

    // Allow API URL override via environment
    if let Ok(api_url) = env::var("E2M_API_URL") {
        config.api_base_url = api_url;
    }

    tracing::info!("Install directory: {:?}", config.install_dir);
    tracing::info!("API URL: {}", config.api_base_url);
    tracing::info!("Current version: {}", config.current_version);

    // Create updater
    let updater = Updater::new(config.clone())?;

    // Check for updates unless skipped
    if !skip_update {
        match updater.check_and_update().await {
            Ok(true) => {
                tracing::info!("Update installed successfully!");
                // Update config with new version
                // (would need to read from the installed binary)
            }
            Ok(false) => {
                tracing::info!("No update available.");
            }
            Err(UpdateError::NoUpdate) => {
                tracing::info!("Already up to date.");
            }
            Err(e) => {
                tracing::warn!("Update check failed: {}. Continuing with current version.", e);
                // Don't fail - continue with existing version
            }
        }
    } else {
        tracing::info!("Update check skipped (--no-update)");
    }

    // If check-only mode, exit now
    if check_only {
        tracing::info!("Check-only mode. Exiting.");
        return Ok(());
    }

    // Launch the main application
    tracing::info!("Launching POS Terminal...");

    match updater.launch() {
        Ok(_) => {
            tracing::info!("Application launched successfully. Launcher exiting.");
            Ok(())
        }
        Err(e) => {
            tracing::error!("Failed to launch application: {}", e);
            Err(e)
        }
    }
}
