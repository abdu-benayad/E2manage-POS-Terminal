//! Rollback management for safe updates

use crate::{Result, UpdateError};
use chrono::Utc;
use std::path::{Path, PathBuf};

/// Rollback manager for backup and restore
pub struct RollbackManager {
    install_dir: PathBuf,
    backup_dir: PathBuf,
}

impl RollbackManager {
    pub fn new(install_dir: &Path, backup_dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(backup_dir)?;

        Ok(Self {
            install_dir: install_dir.to_path_buf(),
            backup_dir: backup_dir.to_path_buf(),
        })
    }

    /// Create a backup of the current installation
    pub fn create_backup(&self) -> Result<PathBuf> {
        let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
        let backup_name = format!("backup_{}", timestamp);
        let backup_path = self.backup_dir.join(&backup_name);

        tracing::info!("Creating backup at: {:?}", backup_path);

        // Copy current installation to backup
        self.copy_dir_recursive(&self.install_dir, &backup_path)?;

        // Save metadata
        let metadata_path = backup_path.join(".backup_metadata");
        let metadata = BackupMetadata {
            created_at: Utc::now().to_rfc3339(),
            source_dir: self.install_dir.to_string_lossy().to_string(),
        };
        std::fs::write(
            metadata_path,
            serde_json::to_string_pretty(&metadata).unwrap_or_default(),
        )?;

        tracing::info!("Backup created successfully");
        Ok(backup_path)
    }

    /// Restore from the latest backup
    pub fn restore_backup(&self) -> Result<()> {
        let latest = self.get_latest_backup()?;

        tracing::warn!("Restoring from backup: {:?}", latest);

        // Remove current installation
        if self.install_dir.exists() {
            std::fs::remove_dir_all(&self.install_dir)?;
        }

        // Copy backup to installation directory
        self.copy_dir_recursive(&latest, &self.install_dir)?;

        // Remove metadata file from restored installation
        let metadata_path = self.install_dir.join(".backup_metadata");
        if metadata_path.exists() {
            std::fs::remove_file(metadata_path).ok();
        }

        tracing::info!("Rollback complete");
        Ok(())
    }

    /// Get the most recent backup
    fn get_latest_backup(&self) -> Result<PathBuf> {
        let mut backups: Vec<_> = std::fs::read_dir(&self.backup_dir)?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("backup_")
            })
            .collect();

        backups.sort_by(|a, b| b.file_name().cmp(&a.file_name()));

        backups
            .first()
            .map(|e| e.path())
            .ok_or_else(|| UpdateError::Rollback("No backup found".to_string()))
    }

    /// Clean up old backups, keeping only the most recent n
    pub fn cleanup_old_backups(&self, keep: usize) -> Result<()> {
        let mut backups: Vec<_> = std::fs::read_dir(&self.backup_dir)?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("backup_")
            })
            .collect();

        backups.sort_by(|a, b| b.file_name().cmp(&a.file_name()));

        // Remove old backups beyond the keep limit
        for backup in backups.into_iter().skip(keep) {
            tracing::info!("Removing old backup: {:?}", backup.path());
            std::fs::remove_dir_all(backup.path()).ok();
        }

        Ok(())
    }

    /// List all available backups
    pub fn list_backups(&self) -> Result<Vec<BackupInfo>> {
        let mut backups = Vec::new();

        for entry in std::fs::read_dir(&self.backup_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() && entry.file_name().to_string_lossy().starts_with("backup_") {
                let metadata_path = path.join(".backup_metadata");
                let metadata: Option<BackupMetadata> = std::fs::read_to_string(&metadata_path)
                    .ok()
                    .and_then(|s| serde_json::from_str(&s).ok());

                backups.push(BackupInfo {
                    path: path.clone(),
                    name: entry.file_name().to_string_lossy().to_string(),
                    created_at: metadata.map(|m| m.created_at),
                });
            }
        }

        backups.sort_by(|a, b| b.name.cmp(&a.name));
        Ok(backups)
    }

    /// Recursive directory copy
    fn copy_dir_recursive(&self, src: &Path, dst: &Path) -> Result<()> {
        if !src.exists() {
            return Ok(()); // Nothing to copy
        }

        std::fs::create_dir_all(dst)?;

        for entry in std::fs::read_dir(src)? {
            let entry = entry?;
            let src_path = entry.path();
            let dst_path = dst.join(entry.file_name());

            if src_path.is_dir() {
                self.copy_dir_recursive(&src_path, &dst_path)?;
            } else {
                std::fs::copy(&src_path, &dst_path)?;
            }
        }

        Ok(())
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct BackupMetadata {
    created_at: String,
    source_dir: String,
}

#[derive(Debug)]
pub struct BackupInfo {
    pub path: PathBuf,
    pub name: String,
    pub created_at: Option<String>,
}
