//! System Service - System-level operations
//!
//! Provides functionality to:
//! - Shutdown the machine
//! - Restart the machine
//! - Restart the application

use std::process::Command;
use tracing::{info, error};

/// System service for machine control
pub struct SystemService;

impl SystemService {
    /// Create a new system service
    pub fn new() -> Self {
        Self
    }

    /// Shutdown the machine
    #[cfg(unix)]
    pub fn shutdown_machine(&self) -> Result<(), String> {
        info!("Initiating system shutdown...");

        // Try systemctl first (systemd)
        let result = Command::new("systemctl")
            .args(["poweroff"])
            .status();

        match result {
            Ok(status) if status.success() => {
                info!("Shutdown command executed successfully");
                Ok(())
            }
            _ => {
                // Fallback to shutdown command
                let result = Command::new("sudo")
                    .args(["shutdown", "-h", "now"])
                    .status();

                match result {
                    Ok(status) if status.success() => {
                        info!("Shutdown command executed successfully");
                        Ok(())
                    }
                    Ok(status) => {
                        let msg = format!("Shutdown command failed with status: {}", status);
                        error!("{}", msg);
                        Err(msg)
                    }
                    Err(e) => {
                        let msg = format!("Failed to execute shutdown command: {}", e);
                        error!("{}", msg);
                        Err(msg)
                    }
                }
            }
        }
    }

    #[cfg(windows)]
    pub fn shutdown_machine(&self) -> Result<(), String> {
        info!("Initiating system shutdown...");

        let result = Command::new("shutdown")
            .args(["/s", "/t", "0"])
            .status();

        match result {
            Ok(status) if status.success() => {
                info!("Shutdown command executed successfully");
                Ok(())
            }
            Ok(status) => {
                let msg = format!("Shutdown command failed with status: {}", status);
                error!("{}", msg);
                Err(msg)
            }
            Err(e) => {
                let msg = format!("Failed to execute shutdown command: {}", e);
                error!("{}", msg);
                Err(msg)
            }
        }
    }

    /// Restart the machine
    #[cfg(unix)]
    pub fn restart_machine(&self) -> Result<(), String> {
        info!("Initiating system restart...");

        // Try systemctl first (systemd)
        let result = Command::new("systemctl")
            .args(["reboot"])
            .status();

        match result {
            Ok(status) if status.success() => {
                info!("Reboot command executed successfully");
                Ok(())
            }
            _ => {
                // Fallback to reboot command
                let result = Command::new("sudo")
                    .args(["reboot"])
                    .status();

                match result {
                    Ok(status) if status.success() => {
                        info!("Reboot command executed successfully");
                        Ok(())
                    }
                    Ok(status) => {
                        let msg = format!("Reboot command failed with status: {}", status);
                        error!("{}", msg);
                        Err(msg)
                    }
                    Err(e) => {
                        let msg = format!("Failed to execute reboot command: {}", e);
                        error!("{}", msg);
                        Err(msg)
                    }
                }
            }
        }
    }

    #[cfg(windows)]
    pub fn restart_machine(&self) -> Result<(), String> {
        info!("Initiating system restart...");

        let result = Command::new("shutdown")
            .args(["/r", "/t", "0"])
            .status();

        match result {
            Ok(status) if status.success() => {
                info!("Reboot command executed successfully");
                Ok(())
            }
            Ok(status) => {
                let msg = format!("Reboot command failed with status: {}", status);
                error!("{}", msg);
                Err(msg)
            }
            Err(e) => {
                let msg = format!("Failed to execute reboot command: {}", e);
                error!("{}", msg);
                Err(msg)
            }
        }
    }

    /// Restart the application
    /// Returns the path to the current executable for respawning
    pub fn get_executable_path(&self) -> Result<String, String> {
        std::env::current_exe()
            .map(|p| p.to_string_lossy().to_string())
            .map_err(|e| format!("Failed to get executable path: {}", e))
    }

    /// Spawn a new instance of the application and signal the current one to exit
    #[cfg(unix)]
    pub fn restart_app(&self) -> Result<(), String> {
        info!("Initiating application restart...");

        let exe_path = self.get_executable_path()?;

        // Spawn new process
        let result = Command::new(&exe_path)
            .spawn();

        match result {
            Ok(_child) => {
                info!("New application instance spawned, current will exit");
                Ok(())
            }
            Err(e) => {
                let msg = format!("Failed to restart application: {}", e);
                error!("{}", msg);
                Err(msg)
            }
        }
    }

    #[cfg(windows)]
    pub fn restart_app(&self) -> Result<(), String> {
        info!("Initiating application restart...");

        let exe_path = self.get_executable_path()?;

        let result = Command::new(&exe_path)
            .spawn();

        match result {
            Ok(_child) => {
                info!("New application instance spawned, current will exit");
                Ok(())
            }
            Err(e) => {
                let msg = format!("Failed to restart application: {}", e);
                error!("{}", msg);
                Err(msg)
            }
        }
    }
}

impl Default for SystemService {
    fn default() -> Self {
        Self::new()
    }
}
