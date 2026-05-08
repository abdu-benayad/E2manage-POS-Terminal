//! Update Service - Check for application updates
//!
//! Provides functionality to:
//! - Check for available updates from the platform
//! - Compare version numbers
//! - Handle force update scenarios
//! - Report version after successful update
//!
//! ## Usage
//!
//! ```rust,ignore
//! use pos_services::UpdateService;
//! use pos_api::ApiClient;
//! use std::sync::Arc;
//!
//! let api = Arc::new(ApiClient::new("https://api.example.com"));
//! let service = UpdateService::new(api);
//!
//! // Check for updates
//! let info = service.check_for_updates("stable").await?;
//! if info.update_available {
//!     println!("Update available: {}", info.version.unwrap());
//! }
//!
//! // Report version after update
//! service.report_version("1.1.0").await?;
//! ```

use pos_api::{ApiClient, CheckUpdateResponse};
use serde::Deserialize;
use std::process::Command;
use std::sync::Arc;
use tracing::{debug, error, info};

/// Version information from update check
#[derive(Debug, Clone)]
pub struct VersionInfo {
    /// Current running version
    pub current_version: String,
    /// Latest available version
    pub latest_version: String,
    /// Whether an update is available
    pub update_available: bool,
    /// Whether this update is mandatory
    pub force_update: bool,
    /// Release notes for the update
    pub release_notes: ReleaseNotes,
    /// Download URL for the update
    pub download_url: String,
    /// Release date
    pub release_date: String,
}

/// Release notes in multiple languages
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ReleaseNotes {
    /// English release notes
    pub en: String,
    /// Arabic release notes
    pub ar: String,
}

impl ReleaseNotes {
    /// Creates release notes from a single string (used when only one language available)
    pub fn from_string(notes: &str) -> Self {
        Self {
            en: notes.to_string(),
            ar: notes.to_string(),
        }
    }

    /// Gets the release notes for a locale
    pub fn get(&self, locale: &str) -> &str {
        if locale.starts_with("ar") {
            &self.ar
        } else {
            &self.en
        }
    }
}

/// Update service errors
#[derive(Debug, Clone)]
pub enum UpdateError {
    /// Network communication error
    NetworkError(String),
    /// Failed to parse response
    ParseError(String),
    /// Device is offline
    Offline,
    /// Download failed
    DownloadError(String),
    /// Checksum verification failed
    ChecksumError(String),
    /// Installation failed
    InstallError(String),
}

impl std::fmt::Display for UpdateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UpdateError::NetworkError(e) => write!(f, "Network error: {}", e),
            UpdateError::ParseError(e) => write!(f, "Parse error: {}", e),
            UpdateError::Offline => write!(f, "Device is offline"),
            UpdateError::DownloadError(e) => write!(f, "Download error: {}", e),
            UpdateError::ChecksumError(e) => write!(f, "Checksum verification failed: {}", e),
            UpdateError::InstallError(e) => write!(f, "Installation failed: {}", e),
        }
    }
}

impl std::error::Error for UpdateError {}

pub type UpdateResult<T> = Result<T, UpdateError>;

/// Default release channel
pub const DEFAULT_RELEASE_CHANNEL: &str = "STABLE";

/// Update Service
///
/// Manages application updates by checking the platform for new versions.
pub struct UpdateService {
    api: Arc<ApiClient>,
    current_version: String,
}

impl UpdateService {
    /// Creates a new update service with API client
    ///
    /// # Arguments
    ///
    /// * `api` - API client for server communication
    pub fn new(api: Arc<ApiClient>) -> Self {
        Self {
            api,
            current_version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    /// Creates update service with a base URL (legacy constructor)
    ///
    /// # Arguments
    ///
    /// * `base_url` - Base URL of the API server
    #[deprecated(since = "0.2.0", note = "Use new() with ApiClient instead")]
    pub fn with_base_url(base_url: String) -> Self {
        let api = Arc::new(ApiClient::new(&base_url));
        Self::new(api)
    }

    /// Gets the current application version
    pub fn current_version(&self) -> &str {
        &self.current_version
    }

    /// Checks for updates from the platform (async)
    ///
    /// # Arguments
    ///
    /// * `release_channel` - Release channel (e.g., "stable", "beta", "alpha")
    ///
    /// # Returns
    ///
    /// Version information including whether an update is available
    pub async fn check_for_updates(&self, release_channel: &str) -> UpdateResult<VersionInfo> {
        debug!(
            "Checking for updates (current: {}, channel: {})",
            self.current_version, release_channel
        );

        let response = self
            .api
            .platform_check_update(&self.current_version, release_channel)
            .await
            .map_err(|e| UpdateError::NetworkError(e.to_string()))?;

        self.convert_response(response)
    }

    /// Checks for updates synchronously (blocking)
    ///
    /// Uses the async API internally. Prefer `check_for_updates` in async contexts.
    ///
    /// # Arguments
    ///
    /// * `release_channel` - Release channel
    pub fn check_for_updates_sync(&self, release_channel: &str) -> UpdateResult<VersionInfo> {
        // Use tokio runtime if available, otherwise create one
        match tokio::runtime::Handle::try_current() {
            Ok(handle) => {
                // We're inside a tokio runtime, use block_in_place
                tokio::task::block_in_place(|| {
                    handle.block_on(self.check_for_updates(release_channel))
                })
            }
            Err(_) => {
                // No runtime, create a new one
                let rt = tokio::runtime::Runtime::new()
                    .map_err(|e| UpdateError::NetworkError(e.to_string()))?;
                rt.block_on(self.check_for_updates(release_channel))
            }
        }
    }

    /// Reports the current version to the platform after a successful update
    ///
    /// # Arguments
    ///
    /// * `version` - The new version that was installed
    pub async fn report_version(&self, version: &str) -> UpdateResult<()> {
        info!("Reporting version {} to platform", version);

        self.api
            .report_version(version)
            .await
            .map_err(|e| UpdateError::NetworkError(e.to_string()))?;

        info!("Version {} reported successfully", version);
        Ok(())
    }

    /// Converts API response to VersionInfo
    fn convert_response(&self, response: CheckUpdateResponse) -> UpdateResult<VersionInfo> {
        let latest_version = response.version.clone().unwrap_or_else(|| self.current_version.clone());
        let download_url = response.download_url.clone().unwrap_or_default();
        let release_notes = response
            .release_notes
            .as_ref()
            .map(|notes| ReleaseNotes::from_string(notes))
            .unwrap_or_default();

        Ok(VersionInfo {
            current_version: self.current_version.clone(),
            latest_version,
            update_available: response.update_available,
            force_update: response.is_mandatory,
            release_notes,
            download_url,
            release_date: String::new(), // Not provided by platform API
        })
    }

    /// Opens the download page in the system browser
    pub fn open_download_page(&self) {
        let url = "https://github.com/e2manage/pos-terminal/releases";

        #[cfg(target_os = "linux")]
        {
            let _ = Command::new("xdg-open").arg(url).spawn();
        }

        #[cfg(target_os = "windows")]
        {
            let _ = Command::new("cmd").args(["/C", "start", url]).spawn();
        }

        #[cfg(target_os = "macos")]
        {
            let _ = Command::new("open").arg(url).spawn();
        }
    }

    /// Opens a specific URL in the system browser
    pub fn open_url(url: &str) {
        #[cfg(target_os = "linux")]
        {
            let _ = Command::new("xdg-open").arg(url).spawn();
        }

        #[cfg(target_os = "windows")]
        {
            let _ = Command::new("cmd").args(["/C", "start", url]).spawn();
        }

        #[cfg(target_os = "macos")]
        {
            let _ = Command::new("open").arg(url).spawn();
        }
    }

    /// Compares two semantic version strings
    ///
    /// # Arguments
    ///
    /// * `a` - First version string (e.g., "1.2.3")
    /// * `b` - Second version string
    ///
    /// # Returns
    ///
    /// * `-1` if a < b
    /// * `0` if a == b
    /// * `1` if a > b
    pub fn compare_versions(a: &str, b: &str) -> i32 {
        let parse_version = |v: &str| -> Vec<u32> {
            v.split('.')
                .filter_map(|s| s.parse::<u32>().ok())
                .collect()
        };

        let parts_a = parse_version(a);
        let parts_b = parse_version(b);

        for i in 0..std::cmp::max(parts_a.len(), parts_b.len()) {
            let num_a = parts_a.get(i).copied().unwrap_or(0);
            let num_b = parts_b.get(i).copied().unwrap_or(0);

            if num_a < num_b {
                return -1;
            }
            if num_a > num_b {
                return 1;
            }
        }

        0
    }

    /// Checks if a version is newer than the current version
    pub fn is_newer(&self, version: &str) -> bool {
        Self::compare_versions(version, &self.current_version) > 0
    }

    /// Downloads an update file
    ///
    /// # Arguments
    ///
    /// * `url` - URL to download from
    /// * `checksum` - Expected SHA256 checksum
    /// * `dest_path` - Destination file path
    ///
    /// # Returns
    ///
    /// Path to the downloaded file
    pub async fn download_update(
        &self,
        url: &str,
        checksum: Option<&str>,
        dest_path: &std::path::Path,
    ) -> UpdateResult<()> {
        info!("Downloading update from {}", url);

        // Use curl for downloading (could use reqwest for more control)
        let status = Command::new("curl")
            .args(["-L", "-o"])
            .arg(dest_path)
            .arg(url)
            .status()
            .map_err(|e| UpdateError::DownloadError(e.to_string()))?;

        if !status.success() {
            return Err(UpdateError::DownloadError("curl failed".to_string()));
        }

        // Verify checksum if provided
        if let Some(expected) = checksum {
            let actual = self.calculate_sha256(dest_path)?;
            if actual.to_lowercase() != expected.to_lowercase() {
                error!(
                    "Checksum mismatch: expected {}, got {}",
                    expected, actual
                );
                return Err(UpdateError::ChecksumError(format!(
                    "Expected {}, got {}",
                    expected, actual
                )));
            }
            info!("Checksum verified");
        }

        Ok(())
    }

    /// Calculates SHA256 checksum of a file
    fn calculate_sha256(&self, path: &std::path::Path) -> UpdateResult<String> {
        let output = Command::new("sha256sum")
            .arg(path)
            .output()
            .map_err(|e| UpdateError::ChecksumError(e.to_string()))?;

        if !output.status.success() {
            return Err(UpdateError::ChecksumError("sha256sum failed".to_string()));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let checksum = stdout
            .split_whitespace()
            .next()
            .ok_or_else(|| UpdateError::ChecksumError("Invalid sha256sum output".to_string()))?;

        Ok(checksum.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_service() -> UpdateService {
        let api = Arc::new(ApiClient::new("https://api.example.com"));
        UpdateService::new(api)
    }

    #[test]
    fn test_compare_versions_equal() {
        assert_eq!(UpdateService::compare_versions("1.0.0", "1.0.0"), 0);
        assert_eq!(UpdateService::compare_versions("2.3.4", "2.3.4"), 0);
    }

    #[test]
    fn test_compare_versions_less_than() {
        assert_eq!(UpdateService::compare_versions("1.0.0", "1.0.1"), -1);
        assert_eq!(UpdateService::compare_versions("1.0.0", "1.1.0"), -1);
        assert_eq!(UpdateService::compare_versions("1.0.0", "2.0.0"), -1);
    }

    #[test]
    fn test_compare_versions_greater_than() {
        assert_eq!(UpdateService::compare_versions("1.0.1", "1.0.0"), 1);
        assert_eq!(UpdateService::compare_versions("1.1.0", "1.0.0"), 1);
        assert_eq!(UpdateService::compare_versions("2.0.0", "1.9.9"), 1);
    }

    #[test]
    fn test_compare_versions_different_lengths() {
        assert_eq!(UpdateService::compare_versions("1.0", "1.0.0"), 0);
        assert_eq!(UpdateService::compare_versions("1", "1.0.0"), 0);
        assert_eq!(UpdateService::compare_versions("1.0.0", "1.0"), 0);
    }

    #[test]
    fn test_is_newer() {
        let service = create_test_service();
        let current = service.current_version();

        // These depend on the actual cargo version, so we test with known values
        assert!(UpdateService::compare_versions("99.0.0", current) > 0);
        assert!(UpdateService::compare_versions("0.0.1", current) <= 0);
    }

    #[test]
    fn test_current_version() {
        let service = create_test_service();
        assert!(!service.current_version().is_empty());
    }

    #[test]
    fn test_release_notes_from_string() {
        let notes = ReleaseNotes::from_string("Bug fixes and improvements");
        assert_eq!(notes.en, "Bug fixes and improvements");
        assert_eq!(notes.ar, "Bug fixes and improvements");
    }

    #[test]
    fn test_release_notes_get_locale() {
        let notes = ReleaseNotes {
            en: "English notes".to_string(),
            ar: "ملاحظات عربية".to_string(),
        };

        assert_eq!(notes.get("en"), "English notes");
        assert_eq!(notes.get("en-US"), "English notes");
        assert_eq!(notes.get("ar"), "ملاحظات عربية");
        assert_eq!(notes.get("ar-SA"), "ملاحظات عربية");
    }

    #[test]
    fn test_convert_response_no_update() {
        let service = create_test_service();
        let response = CheckUpdateResponse {
            update_available: false,
            version: None,
            download_url: None,
            checksum: None,
            is_mandatory: false,
            release_notes: None,
        };

        let info = service.convert_response(response).unwrap();
        assert!(!info.update_available);
        assert!(!info.force_update);
    }

    #[test]
    fn test_convert_response_with_update() {
        let service = create_test_service();
        let response = CheckUpdateResponse {
            update_available: true,
            version: Some("2.0.0".to_string()),
            download_url: Some("https://example.com/update.tar.gz".to_string()),
            checksum: Some("abc123".to_string()),
            is_mandatory: true,
            release_notes: Some("New features!".to_string()),
        };

        let info = service.convert_response(response).unwrap();
        assert!(info.update_available);
        assert!(info.force_update);
        assert_eq!(info.latest_version, "2.0.0");
        assert_eq!(info.download_url, "https://example.com/update.tar.gz");
        assert_eq!(info.release_notes.en, "New features!");
    }
}
