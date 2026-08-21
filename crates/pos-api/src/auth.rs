//! Authentication Module - Terminal login and session management
//!
//! Handles terminal registration, login, pairing, and heartbeat communication with the backend.

use super::client::ApiClient;
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use pos_models::OperatorId;

// ============================================================================
// REQUEST TYPES
// ============================================================================

/// Terminal registration request
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisterTerminalRequest {
    /// Unique hardware identifier (MAC address, CPU ID, etc.)
    pub hardware_id: String,
    /// Human-readable terminal name
    pub name: String,
    /// Business sector for the terminal
    pub sector: String,
    /// Optional branch identifier
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch_id: Option<String>,
}

/// Terminal login request
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginTerminalRequest {
    /// Terminal code (e.g., "TERM-001")
    pub terminal_code: String,
    /// Unique hardware identifier (must match registered terminal)
    pub hardware_id: String,
    /// Terminal secret (received during registration)
    pub secret: String,
}

/// Heartbeat request with terminal metrics
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HeartbeatRequest {
    /// Terminal uptime in seconds
    pub uptime_seconds: u64,
    /// CPU usage percentage (0-100)
    pub cpu_percent: f32,
    /// Memory usage in MB
    pub memory_mb: u64,
    /// Free disk space in MB
    pub disk_free_mb: u64,
    /// Number of pending offline transactions
    pub offline_txn_count: u32,
    /// Current application version
    pub app_version: String,
    /// Current shift ID (if active)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_shift_id: Option<String>,
    /// Current operator ID (if logged in)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_operator_id: Option<OperatorId>,
}

/// Operator PIN verification request
///
/// **Deliberately not `Debug`.** While `pin` is a `String`, a derived `Debug` puts a live PIN one
/// `tracing` call away from a rotated log file on the till's disk. Nothing formats this struct,
/// and `ApiClient::post` needs only `Serialize`, so the derive costs nothing to drop — and
/// `tests/guards.rs` fails the build if it comes back.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyPinRequest {
    /// Operator ID
    pub operator_id: OperatorId,
    /// PIN to verify
    ///
    /// Still a `String`. `pos_models::Pin` is the type that belongs here
    /// (`05-pin-and-pin-policy`), and once it lands the `Debug` derive is safe to restore, because
    /// `Pin`'s own `Debug` renders `Pin(****)`. Wiring it is `auth-outcome-and-offline-lockout`'s
    /// work: the PIN has to reach this struct from a policy-aware parse — which needs the
    /// terminal's configured PIN length — rather than as a bare string handed down from a caller.
    pub pin: String,
}

// ============================================================================
// RESPONSE TYPES
// ============================================================================

/// Terminal registration response
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RegisterTerminalResponse {
    /// Generated terminal ID
    pub terminal_id: String,
    /// Generated terminal code (e.g., "TERM-001")
    pub terminal_code: String,
    /// Secret for future logins
    pub secret: String,
    /// QR code for mobile pairing (optional)
    #[serde(default)]
    pub pairing_qr: Option<String>,
}

/// Terminal login response
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct LoginTerminalResponse {
    /// Session token for authenticated requests
    pub session_token: String,
    /// Terminal ID
    pub terminal_id: String,
    /// Terminal code
    pub terminal_code: String,
    /// Tenant ID
    pub tenant_id: String,
    /// Company ID
    pub company_id: String,
    /// Branch ID (if applicable)
    #[serde(default)]
    pub branch_id: Option<String>,
    /// Terminal configuration
    pub config: TerminalConfig,
    /// Available features for this terminal
    #[serde(default)]
    pub features: Vec<String>,
    /// Token expiration time
    #[serde(default)]
    pub expires_at: Option<String>,
}

/// Terminal configuration from server
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TerminalConfig {
    /// Locale (e.g., "ar", "en")
    #[serde(default)]
    pub locale: Option<String>,
    /// Currency code (e.g., "LYD", "USD")
    #[serde(default)]
    pub currency: Option<String>,
    /// Business sector (e.g., "RETAIL", "SUPERMARKET") - can be null
    #[serde(default)]
    pub business_sector: Option<String>,
    /// Tax configuration
    #[serde(default)]
    pub tax_config: Option<TaxConfig>,
    /// Receipt configuration
    #[serde(default)]
    pub receipt_config: Option<ReceiptConfig>,
    /// Enabled features
    #[serde(default)]
    pub features: Vec<String>,
}

/// Tax configuration
#[derive(Debug, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct TaxConfig {
    /// Default tax rate percentage
    #[serde(default)]
    pub default_rate: f64,
    /// Whether prices include tax
    #[serde(default)]
    pub tax_inclusive: bool,
    /// Tax registration number
    #[serde(default)]
    pub tax_number: Option<String>,
}

/// Receipt configuration
#[derive(Debug, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct ReceiptConfig {
    /// Header lines to print on receipt
    #[serde(default)]
    pub header_lines: Vec<String>,
    /// Footer lines to print on receipt
    #[serde(default)]
    pub footer_lines: Vec<String>,
    /// Whether to print QR code
    #[serde(default)]
    pub print_qr: bool,
    /// Logo URL for receipt
    #[serde(default)]
    pub logo_url: Option<String>,
}

/// Heartbeat response from server
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct HeartbeatResponse {
    /// Status ("ok", "needs_update", etc.)
    pub status: String,
    /// Commands to execute on terminal
    #[serde(default)]
    pub commands: Vec<TerminalCommand>,
    /// Next heartbeat interval in seconds (optional override)
    #[serde(default)]
    pub next_interval_seconds: Option<u64>,
}

/// Command to execute on terminal
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TerminalCommand {
    /// Command type
    pub command: String,
    /// Command parameters
    #[serde(default)]
    pub params: serde_json::Value,
}

/// PIN verification response
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct VerifyPinResponse {
    /// Whether PIN is valid
    pub valid: bool,
    /// Error message if invalid
    #[serde(default)]
    pub message: Option<String>,
    /// Remaining attempts before lockout
    #[serde(default)]
    pub remaining_attempts: Option<u32>,
}

// ============================================================================
// PAIRING REQUEST/RESPONSE TYPES
// ============================================================================

/// Request to get a pairing code for terminal registration
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestPairingRequest {
    /// Hardware ID of the terminal
    pub hardware_id: String,
    /// Device information
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_info: Option<DeviceInfo>,
}

/// Device information for pairing request
#[derive(Debug, Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo {
    /// Operating system name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub os_name: Option<String>,
    /// Operating system version
    #[serde(skip_serializing_if = "Option::is_none")]
    pub os_version: Option<String>,
    /// Application version
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_version: Option<String>,
    /// Screen resolution
    #[serde(skip_serializing_if = "Option::is_none")]
    pub screen_resolution: Option<String>,
}

/// Response from pairing code request
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RequestPairingResponse {
    /// The pairing code to display (e.g., "ABC-123-XYZ")
    pub pairing_code: String,
    /// When the pairing code expires
    pub expires_at: DateTime<Utc>,
    /// Hardware ID echoed back
    pub hardware_id: String,
}

/// Pairing status (for polling)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PairingStatus {
    /// Waiting for admin to complete pairing
    Pending,
    /// Pairing completed, terminal info available
    Completed,
    /// Pairing code expired
    Expired,
    /// Pairing was cancelled
    Cancelled,
}

impl<'de> Deserialize<'de> for PairingStatus {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.to_uppercase().as_str() {
            "PENDING" => Ok(PairingStatus::Pending),
            "COMPLETED" => Ok(PairingStatus::Completed),
            "EXPIRED" => Ok(PairingStatus::Expired),
            "CANCELLED" => Ok(PairingStatus::Cancelled),
            _ => Err(serde::de::Error::unknown_variant(
                &s,
                &["PENDING", "COMPLETED", "EXPIRED", "CANCELLED"],
            )),
        }
    }
}

/// Response from pairing status check
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PairingStatusResponse {
    /// Current status of the pairing request
    pub status: PairingStatus,
    /// The pairing code
    pub pairing_code: String,
    /// Expiration time
    pub expires_at: DateTime<Utc>,
    /// Terminal info (only present when status is COMPLETED)
    #[serde(default)]
    pub terminal: Option<PairedTerminalInfo>,
}

/// Terminal information after successful pairing
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PairedTerminalInfo {
    /// The assigned terminal ID
    pub terminal_id: String,
    /// The assigned terminal code (e.g., "TERM-001")
    pub terminal_code: String,
    /// Secret for authentication
    pub secret: String,
    /// Tenant ID
    pub tenant_id: String,
    /// Company name for display
    #[serde(default)]
    pub company_name: Option<String>,
}

// ============================================================================
// API CLIENT EXTENSION
// ============================================================================

impl ApiClient {
    /// Registers a new terminal with the backend
    ///
    /// # Arguments
    ///
    /// * `request` - Registration details
    ///
    /// # Returns
    ///
    /// Terminal credentials including ID and secret
    pub async fn register_terminal(
        &self,
        request: &RegisterTerminalRequest,
    ) -> Result<RegisterTerminalResponse> {
        self.post("/api/pos/terminals/register", request).await
    }

    /// Authenticates the terminal with the backend
    ///
    /// # Arguments
    ///
    /// * `hardware_id` - Terminal hardware ID
    /// * `secret` - Terminal secret from registration
    ///
    /// # Returns
    ///
    /// Login response with session token and configuration
    ///
    /// # Side Effects
    ///
    /// Sets the session token and terminal ID on the client
    pub async fn login_terminal(
        &self,
        terminal_code: &str,
        hardware_id: &str,
        secret: &str,
    ) -> Result<LoginTerminalResponse> {
        let request = LoginTerminalRequest {
            terminal_code: terminal_code.to_string(),
            hardware_id: hardware_id.to_string(),
            secret: secret.to_string(),
        };

        // Use post_envelope to unwrap { success, data } response
        let response: LoginTerminalResponse = self
            .post_envelope("/api/pos/terminals/login", &request)
            .await?;

        // Store session token and terminal ID
        self.set_token(response.session_token.clone()).await;
        self.set_terminal_id(response.terminal_id.clone()).await;

        Ok(response)
    }

    /// Sends a heartbeat to the backend
    ///
    /// # Arguments
    ///
    /// * `metrics` - Terminal metrics to report
    ///
    /// # Returns
    ///
    /// Heartbeat response with any commands to execute
    pub async fn send_heartbeat(&self, metrics: &HeartbeatRequest) -> Result<HeartbeatResponse> {
        let terminal_id = self
            .get_terminal_id()
            .await
            .ok_or_else(|| anyhow::anyhow!("Terminal ID not set"))?;

        let path = format!("/api/pos/fleet/{}/heartbeat", terminal_id);
        self.post(&path, metrics).await
    }

    /// Verifies an operator PIN with the backend (online verification)
    ///
    /// # Arguments
    ///
    /// * `operator_id` - Operator ID
    /// * `pin` - PIN to verify
    ///
    /// # Returns
    ///
    /// Verification result
    pub async fn verify_operator_pin(
        &self,
        operator_id: &OperatorId,
        pin: &str,
    ) -> Result<VerifyPinResponse> {
        let request = VerifyPinRequest {
            operator_id: operator_id.clone(),
            pin: pin.to_string(),
        };

        self.post("/api/pos/sync/operators/verify-pin", &request)
            .await
    }

    /// Refreshes the session token
    ///
    /// Should be called periodically to prevent token expiration
    pub async fn refresh_token(&self) -> Result<String> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct RefreshResponse {
            session_token: String,
        }

        let response: RefreshResponse = self.post("/api/pos/terminals/refresh", &()).await?;
        self.set_token(response.session_token.clone()).await;
        Ok(response.session_token)
    }

    /// Logs out the terminal
    ///
    /// Invalidates the session token on the server and clears local token
    pub async fn logout_terminal(&self) -> Result<()> {
        #[derive(Deserialize)]
        struct LogoutResponse {
            #[allow(dead_code)]
            success: bool,
        }

        let _: LogoutResponse = self.post("/api/pos/terminals/logout", &()).await?;
        self.clear_token().await;
        Ok(())
    }

    // ========================================================================
    // PAIRING API METHODS
    // ========================================================================

    /// Requests a pairing code for terminal registration
    ///
    /// This is the first step in the pairing workflow. The terminal displays
    /// the returned pairing code, and an admin enters it in the web UI.
    ///
    /// # Arguments
    ///
    /// * `hardware_id` - Unique hardware identifier for this terminal
    /// * `device_info` - Optional device information
    ///
    /// # Returns
    ///
    /// Pairing code and expiration time
    pub async fn request_pairing(
        &self,
        hardware_id: &str,
        device_info: Option<DeviceInfo>,
    ) -> Result<RequestPairingResponse> {
        let request = RequestPairingRequest {
            hardware_id: hardware_id.to_string(),
            device_info,
        };

        self.post_envelope("/api/pos/terminals/pairing/request", &request)
            .await
    }

    /// Checks the status of a pairing request
    ///
    /// Called periodically by the terminal to see if the admin has completed
    /// the pairing. When status is COMPLETED, the terminal info is returned.
    ///
    /// # Arguments
    ///
    /// * `pairing_code` - The pairing code to check
    ///
    /// # Returns
    ///
    /// Current status and terminal info if completed
    pub async fn check_pairing_status(&self, pairing_code: &str) -> Result<PairingStatusResponse> {
        let path = format!("/api/pos/terminals/pairing/status/{}", pairing_code);
        self.get_envelope(&path).await
    }

    /// Recovers terminal registration when local data was lost
    ///
    /// Called when the terminal receives a 409 (already registered) error.
    /// The backend returns the terminal credentials if they are still available.
    ///
    /// # Arguments
    ///
    /// * `hardware_id` - The hardware ID of the terminal
    ///
    /// # Returns
    ///
    /// Terminal credentials for re-registration
    pub async fn recover_registration(&self, hardware_id: &str) -> Result<PairedTerminalInfo> {
        let request = RequestPairingRequest {
            hardware_id: hardware_id.to_string(),
            device_info: None,
        };

        self.post_envelope("/api/pos/terminals/pairing/recover", &request)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_login_request_serialization() {
        let request = LoginTerminalRequest {
            terminal_code: "TERM-001".to_string(),
            hardware_id: "ABC123".to_string(),
            secret: "secret-key".to_string(),
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("hardwareId"));
        assert!(json.contains("ABC123"));
    }

    #[test]
    fn test_login_response_deserialization() {
        let json = r#"{
            "sessionToken": "tok_123",
            "terminalId": "term_456",
            "terminalCode": "TERM-001",
            "tenantId": "tenant_789",
            "companyId": "comp_012",
            "config": {
                "locale": "ar",
                "currency": "LYD",
                "businessSector": "RETAIL",
                "features": ["RETURNS", "DISCOUNTS"]
            },
            "features": ["RETURNS", "DISCOUNTS"]
        }"#;

        let response: LoginTerminalResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.session_token, "tok_123");
        assert_eq!(response.terminal_id, "term_456");
        assert_eq!(response.config.locale, Some("ar".to_string()));
        assert_eq!(response.config.currency, Some("LYD".to_string()));
        assert_eq!(response.features.len(), 2);
    }

    #[test]
    fn test_heartbeat_request_serialization() {
        let request = HeartbeatRequest {
            uptime_seconds: 3600,
            cpu_percent: 25.5,
            memory_mb: 512,
            disk_free_mb: 1024,
            offline_txn_count: 3,
            app_version: "0.1.0".to_string(),
            current_shift_id: Some("shift_123".to_string()),
            current_operator_id: None,
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("uptimeSeconds"));
        assert!(json.contains("3600"));
        assert!(json.contains("currentShiftId"));
        assert!(!json.contains("currentOperatorId")); // None should be skipped
    }

    #[test]
    fn test_heartbeat_response_deserialization() {
        let json = r#"{
            "status": "ok",
            "commands": [
                {"command": "SYNC_CATALOG", "params": {}},
                {"command": "RESTART", "params": {"delay": 60}}
            ],
            "nextIntervalSeconds": 120
        }"#;

        let response: HeartbeatResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.status, "ok");
        assert_eq!(response.commands.len(), 2);
        assert_eq!(response.commands[0].command, "SYNC_CATALOG");
        assert_eq!(response.next_interval_seconds, Some(120));
    }

    #[test]
    fn test_terminal_config_with_defaults() {
        let json = r#"{
            "locale": "en",
            "currency": "USD",
            "businessSector": "SUPERMARKET"
        }"#;

        let config: TerminalConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.locale, Some("en".to_string()));
        assert!(config.tax_config.is_none());
        assert!(config.receipt_config.is_none());
        assert!(config.features.is_empty());
    }

    #[test]
    fn test_register_request_with_optional_branch() {
        let request = RegisterTerminalRequest {
            hardware_id: "HW123".to_string(),
            name: "Checkout 1".to_string(),
            sector: "RETAIL".to_string(),
            branch_id: None,
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(!json.contains("branchId")); // None should be skipped
    }
}
