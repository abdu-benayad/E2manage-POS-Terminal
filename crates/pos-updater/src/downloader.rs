//! Download module with progress tracking and checksum verification

use crate::{Result, UpdateError};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use tokio::io::AsyncWriteExt;

/// Download manager
pub struct Downloader {
    client: reqwest::Client,
    download_dir: PathBuf,
}

impl Downloader {
    pub fn new() -> Self {
        let download_dir = std::env::temp_dir().join("e2manage-pos-updates");
        std::fs::create_dir_all(&download_dir).ok();

        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(600)) // 10 min timeout for large files
                .build()
                .expect("Failed to create HTTP client"),
            download_dir,
        }
    }

    /// Download update package and verify checksum
    pub async fn download(&self, url: &str, expected_checksum: &str) -> Result<PathBuf> {
        tracing::info!("Downloading update from: {}", url);

        // Create temp file
        let filename = url.split('/').last().unwrap_or("update.tar.gz");
        let download_path = self.download_dir.join(filename);

        // Download with progress
        let response = self.client.get(url).send().await?;

        if !response.status().is_success() {
            return Err(UpdateError::Network(
                response.error_for_status().unwrap_err(),
            ));
        }

        let total_size = response.content_length().unwrap_or(0);
        tracing::info!("Download size: {} bytes", total_size);

        // Stream to file
        let mut file = tokio::fs::File::create(&download_path).await?;
        let mut downloaded: u64 = 0;
        let mut hasher = Sha256::new();

        let mut stream = response.bytes_stream();
        use futures_util::StreamExt;

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(UpdateError::Network)?;
            file.write_all(&chunk).await?;
            hasher.update(&chunk);

            downloaded += chunk.len() as u64;

            if total_size > 0 {
                let progress = (downloaded as f64 / total_size as f64) * 100.0;
                if downloaded % (1024 * 1024) < chunk.len() as u64 {
                    // Log every MB
                    tracing::info!("Download progress: {:.1}%", progress);
                }
            }
        }

        file.flush().await?;
        drop(file);

        // Verify checksum
        let actual_checksum = hex::encode(hasher.finalize());

        if !expected_checksum.is_empty() && actual_checksum != expected_checksum {
            // Clean up bad download
            tokio::fs::remove_file(&download_path).await.ok();

            return Err(UpdateError::ChecksumMismatch {
                expected: expected_checksum.to_string(),
                actual: actual_checksum,
            });
        }

        tracing::info!("Download complete. Checksum verified.");
        Ok(download_path)
    }

    /// Clean up old downloads
    pub fn cleanup(&self) -> Result<()> {
        if self.download_dir.exists() {
            std::fs::remove_dir_all(&self.download_dir)?;
            std::fs::create_dir_all(&self.download_dir)?;
        }
        Ok(())
    }
}

impl Default for Downloader {
    fn default() -> Self {
        Self::new()
    }
}
