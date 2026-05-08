//! Extract and install update packages

use crate::{Result, UpdateError};
use flate2::read::GzDecoder;
use std::fs::File;
use std::path::Path;
use tar::Archive;

/// Extractor for update packages
pub struct Extractor;

impl Extractor {
    pub fn new() -> Self {
        Self
    }

    /// Extract tar.gz archive to installation directory
    pub fn extract(&self, archive_path: &Path, install_dir: &Path) -> Result<()> {
        tracing::info!(
            "Extracting {:?} to {:?}",
            archive_path,
            install_dir
        );

        // Ensure install directory exists
        std::fs::create_dir_all(install_dir)?;

        // Open the archive
        let file = File::open(archive_path)?;
        let decoder = GzDecoder::new(file);
        let mut archive = Archive::new(decoder);

        // Extract all files
        for entry in archive.entries()? {
            let mut entry = entry?;
            let path = entry.path()?;

            // Security: prevent path traversal
            if path.components().any(|c| c == std::path::Component::ParentDir) {
                return Err(UpdateError::Extract(
                    "Archive contains path traversal".to_string(),
                ));
            }

            let dest_path = install_dir.join(&path);

            // Create parent directories
            if let Some(parent) = dest_path.parent() {
                std::fs::create_dir_all(parent)?;
            }

            // Extract file
            if entry.header().entry_type().is_file() {
                tracing::debug!("Extracting: {:?}", dest_path);
                entry.unpack(&dest_path)?;

                // Make binary executable on Unix
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let file_name = path.file_name().unwrap_or_default();
                    if file_name == "e2manage-pos-terminal" || file_name == "pos-launcher" {
                        let mut perms = std::fs::metadata(&dest_path)?.permissions();
                        perms.set_mode(0o755);
                        std::fs::set_permissions(&dest_path, perms)?;
                    }
                }
            } else if entry.header().entry_type().is_dir() {
                std::fs::create_dir_all(&dest_path)?;
            }
        }

        tracing::info!("Extraction complete");
        Ok(())
    }

    /// Verify the extracted binary exists and is executable
    pub fn verify_installation(&self, install_dir: &Path, binary_name: &str) -> Result<()> {
        let binary_path = install_dir.join(binary_name);

        if !binary_path.exists() {
            return Err(UpdateError::Extract(format!(
                "Binary not found after extraction: {:?}",
                binary_path
            )));
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::metadata(&binary_path)?.permissions();
            if perms.mode() & 0o111 == 0 {
                return Err(UpdateError::Extract(
                    "Binary is not executable".to_string(),
                ));
            }
        }

        tracing::info!("Installation verified: {:?}", binary_path);
        Ok(())
    }
}

impl Default for Extractor {
    fn default() -> Self {
        Self::new()
    }
}
