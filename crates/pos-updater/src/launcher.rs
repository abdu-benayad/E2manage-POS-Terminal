//! Application launcher

use crate::{Result, UpdateError};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Application launcher
pub struct Launcher {
    install_dir: PathBuf,
    app_binary: String,
}

impl Launcher {
    pub fn new(install_dir: &Path) -> Self {
        Self {
            install_dir: install_dir.to_path_buf(),
            app_binary: "e2manage-pos-terminal".to_string(),
        }
    }

    pub fn with_binary(mut self, binary_name: &str) -> Self {
        self.app_binary = binary_name.to_string();
        self
    }

    /// Launch the main application
    pub fn launch(&self) -> Result<()> {
        let binary_path = self.install_dir.join(&self.app_binary);

        if !binary_path.exists() {
            return Err(UpdateError::Launch(format!(
                "Application binary not found: {:?}",
                binary_path
            )));
        }

        tracing::info!("Launching application: {:?}", binary_path);

        // Spawn the application as a detached process
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;

            let mut cmd = Command::new(&binary_path);
            cmd.current_dir(&self.install_dir);

            // Set environment variables
            cmd.env("E2M_LAUNCHED_BY_UPDATER", "1");

            // Create a new session (detach from launcher)
            unsafe {
                cmd.pre_exec(|| {
                    libc::setsid();
                    Ok(())
                });
            }

            cmd.spawn()
                .map_err(|e| UpdateError::Launch(e.to_string()))?;
        }

        #[cfg(windows)]
        {
            Command::new(&binary_path)
                .current_dir(&self.install_dir)
                .env("E2M_LAUNCHED_BY_UPDATER", "1")
                .spawn()
                .map_err(|e| UpdateError::Launch(e.to_string()))?;
        }

        Ok(())
    }

    /// Launch and wait for the application to exit
    pub fn launch_and_wait(&self) -> Result<i32> {
        let binary_path = self.install_dir.join(&self.app_binary);

        if !binary_path.exists() {
            return Err(UpdateError::Launch(format!(
                "Application binary not found: {:?}",
                binary_path
            )));
        }

        tracing::info!("Launching application (waiting): {:?}", binary_path);

        let status = Command::new(&binary_path)
            .current_dir(&self.install_dir)
            .env("E2M_LAUNCHED_BY_UPDATER", "1")
            .status()
            .map_err(|e| UpdateError::Launch(e.to_string()))?;

        Ok(status.code().unwrap_or(-1))
    }

    /// Check if the application is currently running
    #[cfg(unix)]
    pub fn is_running(&self) -> bool {
        use std::process::Command;

        let output = Command::new("pgrep")
            .arg("-f")
            .arg(&self.app_binary)
            .output();

        match output {
            Ok(out) => !out.stdout.is_empty(),
            Err(_) => false,
        }
    }

    #[cfg(windows)]
    pub fn is_running(&self) -> bool {
        // Windows implementation would use tasklist
        false
    }

    /// Kill any running instances of the application
    #[cfg(unix)]
    pub fn kill_running(&self) -> Result<()> {
        use std::process::Command;

        let _ = Command::new("pkill")
            .arg("-f")
            .arg(&self.app_binary)
            .status();

        // Wait a bit for process to terminate
        std::thread::sleep(std::time::Duration::from_millis(500));

        Ok(())
    }

    #[cfg(windows)]
    pub fn kill_running(&self) -> Result<()> {
        // Windows implementation would use taskkill
        Ok(())
    }
}
