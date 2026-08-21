//! Platform API Module - Platform-level terminal management
//!
//! Handles communication with the platform management APIs:
//! - Device registration to platform registry
//! - Periodic heartbeat reporting
//! - Security policy fetching
//! - Software update checking
//!
//! These APIs are separate from the tenant-level terminal APIs.

use super::client::ApiClient;
use anyhow::Result;
use serde::{Deserialize, Serialize};

// ============================================================================
// HEARTBEAT TYPES
// ============================================================================

/// Platform heartbeat request with device metrics
///
/// This is different from the tenant-level heartbeat - it reports
/// device health to the platform for monitoring.
#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PlatformHeartbeatRequest {
    /// Current heartbeat status
    pub status: HeartbeatStatus,
    /// Current application version
    pub current_version: String,
    /// CPU usage percentage (0.0-100.0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_usage: Option<f64>,
    /// Memory usage percentage (0.0-100.0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_usage: Option<f64>,
    /// Disk usage percentage (0.0-100.0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disk_usage: Option<f64>,
    /// Number of errors since last heartbeat
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_count: Option<u32>,
    /// Recent error messages
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errors: Option<Vec<String>>,
}

/// Heartbeat status indicating device health
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum HeartbeatStatus {
    /// Device is operating normally
    Healthy,
    /// Device has some issues but is operational
    Degraded,
    /// Device has errors
    Error,
    /// Device is going offline
    Offline,
}

impl Default for HeartbeatStatus {
    fn default() -> Self {
        Self::Healthy
    }
}

impl std::fmt::Display for HeartbeatStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HeartbeatStatus::Healthy => write!(f, "HEALTHY"),
            HeartbeatStatus::Degraded => write!(f, "DEGRADED"),
            HeartbeatStatus::Error => write!(f, "ERROR"),
            HeartbeatStatus::Offline => write!(f, "OFFLINE"),
        }
    }
}

/// Platform heartbeat response
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PlatformHeartbeatResponse {
    /// Whether heartbeat was acknowledged
    pub acknowledged: bool,
    /// Commands to execute on the terminal
    #[serde(default)]
    pub commands: Vec<PlatformCommand>,
}

/// Command from platform to execute on terminal
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PlatformCommand {
    /// Command type
    #[serde(rename = "type")]
    pub command_type: PlatformCommandType,
    /// Command parameters (depends on type)
    #[serde(default)]
    pub params: serde_json::Value,
}

/// Types of commands the platform can send
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PlatformCommandType {
    /// Update to a specific version
    Update,
    /// Restart the terminal
    Restart,
    /// Refresh security policies
    RefreshPolicies,
    /// Force logout all operators
    ForceLogout,
    /// Clear local cache
    ClearCache,
    /// Unknown command (for forward compatibility)
    #[serde(other)]
    Unknown,
}

// ============================================================================
// SECURITY POLICY TYPES
// ============================================================================

/// Security policies response from platform
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SecurityPoliciesResponse {
    /// List of enabled policies
    pub policies: Vec<SecurityPolicy>,
    /// Version hash for caching
    pub version: String,
}

/// Individual security policy
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SecurityPolicy {
    /// Unique policy code (e.g., "ENCRYPTION_AT_REST")
    pub code: String,
    /// Policy category
    pub category: SecurityCategory,
    /// Policy type (determines how to interpret value)
    pub policy_type: PolicyType,
    /// Policy value (interpretation depends on type)
    pub policy_value: serde_json::Value,
    /// Enforcement mode
    pub enforcement_mode: EnforcementMode,
}

/// Security policy categories
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SecurityCategory {
    Encryption,
    Authentication,
    Payment,
    Network,
    DataRetention,
    Compliance,
    #[serde(other)]
    Unknown,
}

/// Policy value types
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PolicyType {
    /// Simple on/off (value is boolean)
    Boolean,
    /// One of allowed values (value is string)
    Enum,
    /// Min/max numeric range (value is {min, max})
    Range,
    /// List of allowed values (value is array)
    List,
    /// Pattern match (value is regex string)
    Regex,
    #[serde(other)]
    Unknown,
}

/// How the policy is enforced
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EnforcementMode {
    /// Policy not enforced
    Disabled,
    /// Log warning but allow action
    Warn,
    /// Block non-compliant actions
    Block,
    /// Allow but create audit log
    Audit,
    #[serde(other)]
    Unknown,
}

// ============================================================================
// UPDATE CHECK TYPES
// ============================================================================

/// Update check response
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CheckUpdateResponse {
    /// Whether an update is available
    pub update_available: bool,
    /// New version (if available)
    #[serde(default)]
    pub version: Option<String>,
    /// Download URL (if available)
    #[serde(default)]
    pub download_url: Option<String>,
    /// SHA256 checksum of download (if available)
    #[serde(default)]
    pub checksum: Option<String>,
    /// Whether this update is mandatory
    #[serde(default)]
    pub is_mandatory: bool,
    /// Release notes (if available)
    #[serde(default)]
    pub release_notes: Option<String>,
}

/// Report version request (after successful update)
#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ReportVersionRequest {
    /// New version that was installed
    pub version: String,
}

// ============================================================================
// DEVICE REGISTRATION TYPES
// ============================================================================

/// Device registration request for platform registry
#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RegisterDeviceRequest {
    /// Unique device identifier (hardware ID)
    pub device_id: String,
    /// Device hardware fingerprint
    pub device_fingerprint: String,
    /// Human-readable device name
    pub device_name: String,
    /// Operating system information
    pub os_info: OsInfo,
    /// Hardware information
    pub hardware_info: HardwareInfo,
}

/// OS information for registration
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct OsInfo {
    /// OS name (e.g., "Linux", "Windows")
    pub name: String,
    /// OS version
    pub version: String,
}

/// Hardware information for registration
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct HardwareInfo {
    /// CPU description
    pub cpu: String,
    /// Memory in MB
    pub memory: u64,
}

/// Device registration response
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RegisterDeviceResponse {
    /// Generated license key
    pub license_key: String,
    /// License status
    pub license_status: LicenseStatus,
    /// Expiration date (if applicable)
    #[serde(default)]
    pub expires_at: Option<String>,
}

/// Device license status
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LicenseStatus {
    /// Awaiting activation
    Pending,
    /// Licensed and operational
    Active,
    /// Temporarily suspended
    Suspended,
    /// Permanently revoked
    Revoked,
    /// License expired
    Expired,
    #[serde(other)]
    Unknown,
}

// ============================================================================
// API CLIENT EXTENSION
// ============================================================================

impl ApiClient {
    /// Sends a platform heartbeat
    ///
    /// Reports device health metrics to the platform for monitoring.
    /// Should be called periodically (e.g., every 60 seconds).
    ///
    /// # Arguments
    ///
    /// * `request` - Heartbeat metrics
    ///
    /// # Returns
    ///
    /// Heartbeat response with optional commands to execute
    pub async fn platform_heartbeat(
        &self,
        request: &PlatformHeartbeatRequest,
    ) -> Result<PlatformHeartbeatResponse> {
        self.post_envelope("/api/pos/platform/heartbeat", request)
            .await
    }

    /// Gets security policies from the platform
    ///
    /// Fetches the current security policies. Supports ETag caching.
    ///
    /// # Arguments
    ///
    /// * `etag` - Optional ETag from previous request for caching
    ///
    /// # Returns
    ///
    /// Security policies with version hash, or NotModified if unchanged
    pub async fn get_security_policies(
        &self,
        etag: Option<&str>,
    ) -> Result<super::GetResult<SecurityPoliciesResponse>> {
        self.get_with_etag_envelope("/api/pos/platform/security-policies", etag)
            .await
    }

    /// Checks for available updates
    ///
    /// # Arguments
    ///
    /// * `current_version` - Current application version
    /// * `release_channel` - Release channel (e.g., "stable", "beta")
    ///
    /// # Returns
    ///
    /// Update information if available
    pub async fn platform_check_update(
        &self,
        current_version: &str,
        release_channel: &str,
    ) -> Result<CheckUpdateResponse> {
        let path = format!(
            "/api/pos/platform/check-update?currentVersion={}&releaseChannel={}",
            current_version, release_channel
        );
        self.get_envelope(&path).await
    }

    /// Reports successful version update
    ///
    /// Called after a successful update to inform the platform
    /// of the new running version.
    ///
    /// # Arguments
    ///
    /// * `version` - The new version that was installed
    pub async fn report_version(&self, version: &str) -> Result<()> {
        let request = ReportVersionRequest {
            version: version.to_string(),
        };

        // Response is just {success: true, message: "..."}
        #[derive(Deserialize)]
        struct Response {
            #[allow(dead_code)]
            success: bool,
        }

        let _: Response = self
            .post_envelope("/api/pos/platform/report-version", &request)
            .await?;
        Ok(())
    }

    /// Registers device with platform registry
    ///
    /// This is called during the initial pairing flow to register
    /// the device in the platform-wide registry.
    ///
    /// Note: This endpoint requires user authentication (not terminal auth).
    ///
    /// # Arguments
    ///
    /// * `request` - Device registration details
    ///
    /// # Returns
    ///
    /// License key and status
    pub async fn register_device_platform(
        &self,
        request: &RegisterDeviceRequest,
    ) -> Result<RegisterDeviceResponse> {
        self.post_envelope("/api/pos/platform/register", request)
            .await
    }

    /// Validates a device license
    ///
    /// Checks if a device's license is still valid.
    ///
    /// # Arguments
    ///
    /// * `device_id` - Device hardware ID
    /// * `license_key` - License key to validate
    ///
    /// # Returns
    ///
    /// Whether the license is valid
    pub async fn validate_license(&self, device_id: &str, license_key: &str) -> Result<bool> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Request {
            device_id: String,
            license_key: String,
        }

        #[derive(Deserialize)]
        struct Response {
            valid: bool,
        }

        let request = Request {
            device_id: device_id.to_string(),
            license_key: license_key.to_string(),
        };

        let response: Response = self
            .post_envelope("/api/pos/platform/validate-license", &request)
            .await?;
        Ok(response.valid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_heartbeat_status_serialization() {
        assert_eq!(
            serde_json::to_string(&HeartbeatStatus::Healthy).unwrap(),
            "\"HEALTHY\""
        );
        assert_eq!(
            serde_json::to_string(&HeartbeatStatus::Degraded).unwrap(),
            "\"DEGRADED\""
        );
    }

    #[test]
    fn test_heartbeat_request_serialization() {
        let request = PlatformHeartbeatRequest {
            status: HeartbeatStatus::Healthy,
            current_version: "1.0.0".to_string(),
            cpu_usage: Some(25.5),
            memory_usage: Some(50.0),
            disk_usage: None,
            error_count: None,
            errors: None,
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"status\":\"HEALTHY\""));
        assert!(json.contains("\"currentVersion\":\"1.0.0\""));
        assert!(json.contains("\"cpuUsage\":25.5"));
        assert!(!json.contains("diskUsage")); // None should be skipped
    }

    #[test]
    fn test_heartbeat_response_deserialization() {
        let json = r#"{
            "acknowledged": true,
            "commands": [
                {"type": "REFRESH_POLICIES", "params": {}},
                {"type": "UPDATE", "params": {"version": "1.1.0"}}
            ]
        }"#;

        let response: PlatformHeartbeatResponse = serde_json::from_str(json).unwrap();
        assert!(response.acknowledged);
        assert_eq!(response.commands.len(), 2);
        assert_eq!(
            response.commands[0].command_type,
            PlatformCommandType::RefreshPolicies
        );
        assert_eq!(
            response.commands[1].command_type,
            PlatformCommandType::Update
        );
    }

    #[test]
    fn test_security_policy_deserialization() {
        let json = r#"{
            "code": "MIN_PIN_LENGTH",
            "category": "AUTHENTICATION",
            "policyType": "RANGE",
            "policyValue": {"min": 4, "max": 8},
            "enforcementMode": "BLOCK"
        }"#;

        let policy: SecurityPolicy = serde_json::from_str(json).unwrap();
        assert_eq!(policy.code, "MIN_PIN_LENGTH");
        assert_eq!(policy.category, SecurityCategory::Authentication);
        assert_eq!(policy.policy_type, PolicyType::Range);
        assert_eq!(policy.enforcement_mode, EnforcementMode::Block);
    }

    #[test]
    fn test_security_policies_response_deserialization() {
        let json = r#"{
            "policies": [
                {
                    "code": "ENCRYPTION_AT_REST",
                    "category": "ENCRYPTION",
                    "policyType": "BOOLEAN",
                    "policyValue": true,
                    "enforcementMode": "BLOCK"
                }
            ],
            "version": "abc123"
        }"#;

        let response: SecurityPoliciesResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.policies.len(), 1);
        assert_eq!(response.version, "abc123");
    }

    #[test]
    fn test_check_update_response_deserialization() {
        let json = r#"{
            "updateAvailable": true,
            "version": "1.1.0",
            "downloadUrl": "https://example.com/update.tar.gz",
            "checksum": "abc123",
            "isMandatory": false,
            "releaseNotes": "Bug fixes"
        }"#;

        let response: CheckUpdateResponse = serde_json::from_str(json).unwrap();
        assert!(response.update_available);
        assert_eq!(response.version, Some("1.1.0".to_string()));
        assert!(!response.is_mandatory);
    }

    #[test]
    fn test_check_update_response_no_update() {
        let json = r#"{
            "updateAvailable": false
        }"#;

        let response: CheckUpdateResponse = serde_json::from_str(json).unwrap();
        assert!(!response.update_available);
        assert!(response.version.is_none());
        assert!(response.download_url.is_none());
    }

    #[test]
    fn test_license_status_deserialization() {
        assert_eq!(
            serde_json::from_str::<LicenseStatus>("\"ACTIVE\"").unwrap(),
            LicenseStatus::Active
        );
        assert_eq!(
            serde_json::from_str::<LicenseStatus>("\"SUSPENDED\"").unwrap(),
            LicenseStatus::Suspended
        );
    }

    #[test]
    fn test_unknown_command_type() {
        let json = r#"{"type": "FUTURE_COMMAND", "params": {}}"#;
        let command: PlatformCommand = serde_json::from_str(json).unwrap();
        assert_eq!(command.command_type, PlatformCommandType::Unknown);
    }
}
