//! Version checking module

use crate::{Result, UpdateError};
use serde::Deserialize;

/// Version information from the update server
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionInfo {
    pub current_version: String,
    pub latest_version: String,
    pub update_available: bool,
    pub force_update: bool,
    pub download_url: String,
    pub checksum: String,
    pub release_date: String,
    pub release_notes: ReleaseNotes,
    pub file_size: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReleaseNotes {
    pub en: String,
    pub ar: String,
}

#[derive(Debug, Deserialize)]
struct ApiResponse<T> {
    success: bool,
    data: T,
    #[serde(default)]
    message: Option<String>,
}

/// Version checker client
pub struct VersionChecker {
    base_url: String,
    client: reqwest::Client,
}

impl VersionChecker {
    pub fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.to_string(),
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("Failed to create HTTP client"),
        }
    }

    /// Check for updates
    pub async fn check(&self, current_version: &str) -> Result<VersionInfo> {
        let url = format!(
            "{}/api/pos/version/check?current={}&platform={}",
            self.base_url,
            current_version,
            std::env::consts::OS
        );

        tracing::debug!("Checking version at: {}", url);

        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            return Err(UpdateError::Network(reqwest::Error::from(
                response.error_for_status().unwrap_err(),
            )));
        }

        let api_response: ApiResponse<VersionInfo> = response.json().await?;

        if !api_response.success {
            return Err(UpdateError::VersionParse(
                api_response.message.unwrap_or_else(|| "Unknown error".to_string()),
            ));
        }

        Ok(api_response.data)
    }

    /// Compare two semantic versions
    /// Returns: -1 if a < b, 0 if a == b, 1 if a > b
    pub fn compare_versions(a: &str, b: &str) -> i32 {
        let parse = |v: &str| -> Vec<u32> {
            v.trim_start_matches('v')
                .split('.')
                .filter_map(|s| s.parse().ok())
                .collect()
        };

        let va = parse(a);
        let vb = parse(b);

        for i in 0..va.len().max(vb.len()) {
            let na = va.get(i).copied().unwrap_or(0);
            let nb = vb.get(i).copied().unwrap_or(0);

            if na < nb {
                return -1;
            }
            if na > nb {
                return 1;
            }
        }

        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_compare() {
        assert_eq!(VersionChecker::compare_versions("1.0.0", "1.0.0"), 0);
        assert_eq!(VersionChecker::compare_versions("1.0.0", "1.0.1"), -1);
        assert_eq!(VersionChecker::compare_versions("1.0.1", "1.0.0"), 1);
        assert_eq!(VersionChecker::compare_versions("2.0.0", "1.9.9"), 1);
        assert_eq!(VersionChecker::compare_versions("v1.0.0", "1.0.0"), 0);
    }
}
