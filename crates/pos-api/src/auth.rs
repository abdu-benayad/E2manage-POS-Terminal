//! Authentication Module - Terminal login and session management
//!
//! Handles terminal registration, login, pairing, and heartbeat communication with the backend.

use super::client::ApiClient;
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::client::Enveloped;
use crate::failure::ApiFailure;
use crate::session::OperatorSession;
use pos_models::{OperatorId, OperatorPermissions, OperatorRole};

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
    /// Terminal uptime in seconds.
    ///
    /// Named `uptime` on the wire, not `uptimeSeconds`: `TerminalMetricsDto.uptime`
    /// (`fleet.dto.ts:21`) is the platform's one **required** metric, and it is the only field
    /// here whose name disagreed. The rest — `cpuPercent`, `memoryMb`, `diskFreeMb`,
    /// `offlineTxnCount`, `appVersion` — were checked against `fleet.dto.ts:19-40` and agree.
    #[serde(rename = "uptime")]
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
    /// Current shift ID (if active).
    ///
    /// **The platform has no field for this**, nor for `current_operator_id`:
    /// `TerminalMetricsDto` (`fleet.dto.ts:19-43`) declares neither, and the handler reads only
    /// the fields it names. Both are sent and silently dropped. Kept rather than deleted because
    /// the gap may be on the platform's side — a till that knows which operator is on shift is
    /// worth a fleet view knowing it — and deleting would destroy the evidence for that. Recorded
    /// in `till/doc/till-consumer-surface-audit`; not this issue's to resolve.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_shift_id: Option<String>,
    /// Current operator ID (if logged in). See `current_shift_id` — also dropped by the platform.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_operator_id: Option<OperatorId>,
}

/// The heartbeat body as the platform reads it.
///
/// `fleet.controller.ts:197` reads `req.body.metrics`, so a flat body leaves `metrics` undefined
/// and the handler falls back to `{}`. There is no validator on the route and every field in
/// `terminal-heartbeat.handler.ts` is optional-with-guard, so nothing throws: the till got a
/// **200 recording zero telemetry**, indistinguishable from success in any manual check. This
/// wrapper exists so the nesting is stated once, in a type, rather than assembled at the call.
#[derive(Debug, Serialize)]
struct HeartbeatBody<'a> {
    metrics: &'a HeartbeatRequest,
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
    /// Still a `String`. `pos_models::Pin` is the type that belongs here, and once it lands the
    /// `Debug` derive is safe to restore, because `Pin`'s own `Debug` renders `Pin(****)`.
    ///
    /// The old note here said the wiring needed *a policy-aware parse — which needs the terminal's
    /// configured PIN length*. It no longer does: `Pin::parse` takes no policy, because a tenant's
    /// length rule governs minting and the till never mints a PIN. What remains is plumbing the
    /// parsed value down from PIN entry instead of a bare string, which is task 07's work.
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
    /// Whether the platform recorded the ping.
    pub acknowledged: bool,
    /// Server time, for clock comparison. ISO-8601; the platform sends a `Date`.
    pub server_time: String,
    /// Commands queued for this terminal.
    #[serde(default)]
    pub commands: Vec<TerminalCommand>,
    /// Cache-invalidation versions. Absent unless the platform has something newer.
    #[serde(default)]
    pub config_version: Option<String>,
    #[serde(default)]
    pub catalog_version: Option<String>,
    #[serde(default)]
    pub screen_version: Option<String>,
}

/// A refreshed terminal session.
///
/// Declared here rather than inside `refresh_token` so the pact can deserialise with **the till's
/// real type**. A contract test that restates the consumer's DTO records what its author believed,
/// not what the till does — the rule is in `crates/pos-contract/tests/contract.rs`'s module doc,
/// and this type was unreachable from there while it lived in a function body.
///
/// The platform also sends `expiresAt` (`terminal.controller.ts:251-258`). It is deliberately not
/// read: nothing in the till consumes it yet, and a field pinned but unused makes the contract
/// fail for a change that harms nobody.
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RefreshResponse {
    /// The new session token, which replaces the one presented.
    pub session_token: String,
}

/// Command to execute on terminal
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TerminalCommand {
    /// Command ID, which an acknowledgement quotes back.
    pub id: String,
    /// What to do — `restart`, `sync`, `update`, `config_push`, `wipe`, `log_upload`
    /// (`fleet.dto.ts:79-86`).
    ///
    /// A `String` and not an enum **on purpose**. Nothing in the till branches on this yet, and a
    /// closed enum would make a command type the platform adds tomorrow fail the whole heartbeat
    /// rather than one command. When something does branch on it, it becomes a domain type with
    /// its own parse error, per the crate's `FromStr` convention — a value that is merely carried
    /// does not earn one first.
    #[serde(rename = "type")]
    pub command_type: String,
    /// Command payload.
    #[serde(default)]
    pub payload: serde_json::Value,
    /// When the platform queued it. ISO-8601.
    pub created_at: String,
}

/// What a **200** from `POST /api/pos/sync/operators/verify-pin` carries.
///
/// # There is no `valid` field, and its absence is the fix
///
/// This DTO required `valid: bool`, with no `#[serde(default)]`, unlike the two optional fields
/// that sat beside it. The server has never sent one, so **a correct PIN online produced a
/// deserialization failure** — which `AuthService::verify_pin` then absorbed as grounds to fall
/// back to offline verification. The platform states the rule at the response it writes:
///
/// > there is deliberately no `valid` field: a 200 IS the affirmative answer, and a client that
/// > needs a boolean to know it succeeded has not read the status. The till's DTO requires one;
/// > that is fixed by deleting the requirement.
///
/// A refusal is a non-2xx and arrives as [`ApiFailure::Refused`](crate::ApiFailure::Refused) with
/// a code and, since the `details` catalogue landed, the figures beside it. A server test asserts
/// this body has no `valid` key. Unknown fields are ignored by serde's default, so a platform that
/// starts over-supplying one does not break this till.
///
/// # `message` and `remaining_attempts` are gone too
///
/// Both were only ever populated on a refusal, which no longer arrives here. `remaining_attempts`
/// now travels as `details.attemptsRemaining` on the 401 — typed, and never zero. See
/// [`RefusalDetails`](crate::RefusalDetails).
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct VerifyPinResponse {
    /// The operator credential this verification minted.
    ///
    /// **Absent, not null**, when no terminal authenticated the request: the route falls back to
    /// `authMiddleware`, and rather than bind a credential to nothing the server mints none.
    ///
    /// A till always presents a terminal token, so a till always gets a session. `None` here is
    /// therefore a **till-side bug** — a request that went out without `X-Terminal-Token` — and
    /// not an ordinary branch to write a fallback for. Task 07 logs it loudly rather than
    /// degrading quietly.
    #[serde(default)]
    pub session: Option<OperatorSession>,
    /// The operator whose PIN was verified.
    pub operator_id: OperatorId,
    /// The HR employee behind the operator profile.
    pub employee_id: String,
    /// The employee's number, e.g. `EMP001`. Non-null on `Employee`.
    pub employee_number: String,
    /// Full name, English.
    ///
    /// Two fields rather than one `OperatorName`, as in [`OperatorDto`](crate::OperatorDto): a
    /// DTO is the shape of the wire, and the pair becomes one value at the boundary.
    pub name: String,
    /// Full name, Arabic. Null when either half is missing server-side.
    #[serde(default)]
    pub name_ar: Option<String>,
    /// The operator's POS role. No serde default — the column is non-null, so an absent or
    /// unrecognised role means the contract moved.
    pub role: OperatorRole,
    /// The operator's capabilities. `POS_OperatorProfile.permissions` is `Json?`, so absent is a
    /// real state; it is **not** defaulted to a permission set here, for the reason
    /// `tests/guards.rs::operator_permissions_has_exactly_one_definition_and_no_default` exists.
    #[serde(default)]
    pub permissions: Option<OperatorPermissions>,
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
    /// # This is dead code, and the raw read below is knowingly left drifted
    ///
    /// `POST /terminals/register` is guarded by `authMiddleware` + `POS_MANAGE`
    /// (`terminal.controller.ts:117`) — a **user JWT**, which the till never holds. It is the
    /// admin web UI's route; pairing is the till's only enrolment path (`doc/architecture`).
    ///
    /// The route does wrap its payload (`:317`), so this raw `post` cannot read it. That was
    /// left alone on purpose: repairing a method the till cannot call and does not use would
    /// make the drift invisible without making anything work. The verdict is recorded in
    /// `till/doc/till-consumer-surface-audit` instead, which is what an unreachable row is owed.
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

        // `Enveloped` unwraps `{ success, message, data }` (`terminal.controller.ts:213-224`).
        let response: LoginTerminalResponse = self
            .post::<_, Enveloped<_>>("/api/pos/terminals/login", &request)
            .await
            .map(Enveloped::into_inner)?;

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
        // No terminal id in the path, and none looked up here: the platform reads it from
        // `req.terminal.terminalUuid`, which `terminalAuthMiddleware` sets from the header
        // (`fleet.controller.ts:194-201`). The old URL carried the id as a path segment, matched
        // no route, and matched no CSRF exemption prefix either — so it was 404 in front of a 403.
        // Dropping the lookup deletes the "Terminal ID not set" failure with it: the header that
        // authenticates the request is the one that identifies it.
        let response: Enveloped<_> = self
            .post("/api/pos/fleet/heartbeat", &HeartbeatBody { metrics })
            .await?;
        Ok(response.into_inner())
    }

    /// Asks the platform whether this PIN is this operator's.
    ///
    /// # The two drifts this route carried, and why only one is left
    ///
    /// The **envelope** was repaired earlier: `sync.controller.ts:1168` wraps its payload, so this
    /// reads [`Enveloped`] rather than the raw `post` that was deserialising the envelope into the
    /// DTO. The **`valid: bool` requirement** is gone as of this issue — see
    /// [`VerifyPinResponse`], which is now the 200's real shape.
    ///
    /// The route is `/api/pos/sync/operators/verify-pin`, registered at `sync.controller.ts:207`.
    /// It is not `/api/pos/sync/verify-pin`; the shorter spelling is a 404. It is CSRF-non-exempt
    /// regardless.
    ///
    /// # It returns [`ApiFailure`], not `anyhow::Error`
    ///
    /// Every other method here widens the failure, which was fine while nothing branched on it.
    /// This one is the reason the enum exists: a wrong PIN, a standing lockout and a rotation
    /// requirement are three different 40x answers with three different things to say to the
    /// person at the till, and each carries typed figures in `details`. A caller that had to
    /// downcast to find that out would be one `?` away from treating all three as weather — which
    /// is exactly what `AuthService::verify_pin` does today, and what task 07 replaces.
    pub async fn verify_operator_pin(
        &self,
        operator_id: &OperatorId,
        pin: &str,
    ) -> std::result::Result<VerifyPinResponse, ApiFailure> {
        let request = VerifyPinRequest {
            operator_id: operator_id.clone(),
            pin: pin.to_string(),
        };

        let response: Enveloped<VerifyPinResponse> = self
            .post_or_failure("/api/pos/sync/operators/verify-pin", &request)
            .await?;
        Ok(response.into_inner())
    }

    /// Refreshes the session token
    ///
    /// Should be called periodically to prevent token expiration
    pub async fn refresh_token(&self) -> Result<String> {
        // `Enveloped`, not a raw read: this route wraps its payload
        // (`terminal.controller.ts:251-258`, `data:{sessionToken, expiresAt}`), so the raw `post`
        // this used to call was deserialising the envelope into `RefreshResponse` and finding no
        // `sessionToken` at the top level.
        let response: Enveloped<RefreshResponse> =
            self.post("/api/pos/terminals/refresh", &()).await?;
        let response = response.into_inner();
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

        // Deliberately a plain DTO and NOT `Enveloped`: this route answers `{success, message}`
        // with no `data` (`terminal.controller.ts:281-284`). Reading `success` at the top level is
        // correct here, and wrapping it would turn a working call into "no `data` payload". The
        // boundary is pinned by `a_no_data_route_is_read_with_a_plain_dto_not_with_enveloped` in
        // `client.rs`.
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

        self.post::<_, Enveloped<_>>("/api/pos/terminals/pairing/request", &request)
            .await
            .map(Enveloped::into_inner)
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
        let response: Enveloped<_> = self.get(&path).await?;
        Ok(response.into_inner())
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

        self.post::<_, Enveloped<_>>("/api/pos/terminals/pairing/recover", &request)
            .await
            .map(Enveloped::into_inner)
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
        assert!(json.contains("uptime"));
        assert!(!json.contains("uptimeSeconds"));
        assert!(json.contains("3600"));
        assert!(json.contains("currentShiftId"));
        assert!(!json.contains("currentOperatorId")); // None should be skipped
    }

    /// The assertion whose absence let a flat body ship.
    ///
    /// `fleet.controller.ts:197` reads `req.body.metrics`. A flat body leaves that undefined, the
    /// handler falls back to `{}`, and — no validator, every field optional-with-guard — the route
    /// answers **200 having recorded nothing**. That is invisible to a live smoke test against a
    /// real server, which is why it needs a test that looks at the shape rather than the outcome.
    ///
    /// Asserted positionally, never by `json.contains("uptime")`: the broken flat body contains
    /// that string too, so a containment check passes against exactly the defect being fixed.
    #[test]
    fn heartbeat_body_nests_the_metrics_where_the_platform_reads_them() {
        let metrics = HeartbeatRequest {
            uptime_seconds: 3600,
            cpu_percent: 12.5,
            memory_mb: 512,
            disk_free_mb: 20_480,
            offline_txn_count: 3,
            app_version: "1.2.3".to_string(),
            current_shift_id: None,
            current_operator_id: None,
        };

        let body = serde_json::to_value(HeartbeatBody { metrics: &metrics })
            .expect("the heartbeat body must serialize");

        assert_eq!(
            body["metrics"]["uptime"], 3600,
            "uptime must sit under `metrics`, and be spelled `uptime` — it is the platform's one \
             required metric (`fleet.dto.ts:21`)"
        );
        assert_eq!(body["metrics"]["appVersion"], "1.2.3");
        assert!(
            body["uptime"].is_null(),
            "nothing may sit at the top level: the platform reads `req.body.metrics` and ignores \
             the rest, so a flat body is a 200 that records nothing"
        );
    }

    /// This test used to assert `{status, commands:[{command, params}], nextIntervalSeconds}` and
    /// passed green against a shape **the platform has never sent**. It was a restatement of what
    /// its author believed, so it could only ever confirm that belief.
    ///
    /// The real payload is `TerminalHeartbeatResponse` (`fleet.dto.ts:198-211`), returned at
    /// `terminal-heartbeat.handler.ts:132-136`. `status` was required and absent, so the response
    /// was undeserialisable — the same failure mode as the `tenantId` break on `terminals/login`,
    /// on an endpoint whose 404 hid it.
    #[test]
    fn heartbeat_response_reads_what_the_platform_actually_sends() {
        let json = r#"{
            "acknowledged": true,
            "serverTime": "2026-08-23T10:00:00.000Z",
            "commands": [
                {"id": "cmd-1", "type": "sync", "payload": {}, "createdAt": "2026-08-23T09:59:00.000Z"},
                {"id": "cmd-2", "type": "restart", "payload": {"delay": 60}, "createdAt": "2026-08-23T09:59:30.000Z"}
            ],
            "catalogVersion": "v7"
        }"#;

        let response: HeartbeatResponse =
            serde_json::from_str(json).expect("the platform's real heartbeat payload must parse");

        assert!(response.acknowledged);
        assert_eq!(response.server_time, "2026-08-23T10:00:00.000Z");
        assert_eq!(response.commands.len(), 2);
        assert_eq!(response.commands[0].id, "cmd-1");
        assert_eq!(response.commands[0].command_type, "sync");
        assert_eq!(response.commands[1].payload["delay"], 60);
        assert_eq!(response.commands[1].created_at, "2026-08-23T09:59:30.000Z");
        assert_eq!(response.catalog_version.as_deref(), Some("v7"));
        assert_eq!(response.config_version, None);
    }

    /// The three version fields and `commands` are absent unless the platform has something to
    /// say, which is the common case — `terminal-heartbeat.handler.ts:132-136` returns only
    /// `acknowledged`, `serverTime` and `commands`. A required field here would break every
    /// quiet heartbeat.
    #[test]
    fn heartbeat_response_reads_a_quiet_acknowledgement() {
        let json =
            r#"{"acknowledged": true, "serverTime": "2026-08-23T10:00:00.000Z", "commands": []}"#;

        let response: HeartbeatResponse =
            serde_json::from_str(json).expect("a heartbeat with nothing to report must parse");

        assert!(response.acknowledged);
        assert!(response.commands.is_empty());
        assert_eq!(response.catalog_version, None);
        assert_eq!(response.screen_version, None);
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

    // ========================================================================
    // verify-pin: the 200's real shape
    // ========================================================================

    /// The body `sync.controller.ts` writes on a verified PIN, captured field for field.
    const VERIFIED: &str = r#"{
        "session": { "token": "op-sess-abc", "expiresAt": "2026-08-23T22:00:00.000Z" },
        "operatorId": "op-001",
        "employeeId": "emp-77",
        "employeeNumber": "EMP001",
        "name": "Sara Haddad",
        "nameAr": "سارة حداد",
        "role": "SUPERVISOR",
        "permissions": { "canVoid": true, "canDiscount": true, "maxDiscount": 20 }
    }"#;

    /// A 200 parses, and **every field is asserted with a distinct value**.
    ///
    /// Distinct on purpose: three of these are opaque identifier strings, and a DTO that wired
    /// `employeeId` to `operatorId` would read as correct against a fixture that reused one value.
    #[test]
    fn a_verified_pin_parses_with_its_operator_session() {
        let verified: VerifyPinResponse =
            serde_json::from_str(VERIFIED).expect("the response the controller writes");

        let session = verified
            .session
            .expect("a till presents a terminal token, so it gets one");
        assert_eq!(session.token().expose(), "op-sess-abc");
        assert_eq!(
            session.expires_at().to_rfc3339(),
            "2026-08-23T22:00:00+00:00"
        );
        assert_eq!(verified.operator_id.as_str(), "op-001");
        assert_eq!(verified.employee_id, "emp-77");
        assert_eq!(verified.employee_number, "EMP001");
        assert_eq!(verified.name, "Sara Haddad");
        assert_eq!(verified.name_ar.as_deref(), Some("سارة حداد"));
        assert_eq!(verified.role, OperatorRole::Supervisor);
        let permissions = verified.permissions.expect("the fixture grants some");
        assert!(permissions.allows(pos_models::Permission::VoidTransaction));
    }

    /// A server that starts sending `valid` again must not break the till.
    ///
    /// Serde ignores unknown fields by default, and that default is load-bearing here rather than
    /// incidental: the whole defect being fixed is a required field the server does not send, and
    /// swinging to `deny_unknown_fields` would be the same brittleness pointed the other way.
    #[test]
    fn a_body_that_still_carries_valid_parses_anyway() {
        let over_supplying = VERIFIED.replacen('{', r#"{ "valid": true,"#, 1);

        let verified: VerifyPinResponse =
            serde_json::from_str(&over_supplying).expect("an extra field is not a breach");

        assert_eq!(verified.operator_id.as_str(), "op-001");
    }

    /// `session` is **absent**, not null, when no terminal authenticated the request.
    ///
    /// A till always presents `X-Terminal-Token`, so reaching this state means the till sent a
    /// request without one. It parses — the response is well-formed — and the `None` is the
    /// signal.
    #[test]
    fn a_response_without_a_session_parses_and_says_so() {
        let no_session = r#"{
            "operatorId": "op-001",
            "employeeId": "emp-77",
            "employeeNumber": "EMP001",
            "name": "Sara Haddad",
            "nameAr": null,
            "role": "CASHIER",
            "permissions": null
        }"#;

        let verified: VerifyPinResponse =
            serde_json::from_str(no_session).expect("an absent session is a shape the route emits");

        assert!(verified.session.is_none());
        assert!(verified.name_ar.is_none());
        // Not defaulted to an empty permission set: an unreadable mapping and a legitimately
        // unprivileged operator are different facts. See `tests/guards.rs`.
        assert!(verified.permissions.is_none());
        assert_eq!(verified.role, OperatorRole::Cashier);
    }

    /// A blank session token is a contract breach at the boundary, not a credential to present.
    #[test]
    fn a_blank_session_token_does_not_become_a_session() {
        let blank = VERIFIED.replace(r#""token": "op-sess-abc""#, r#""token": "   ""#);

        let error = serde_json::from_str::<VerifyPinResponse>(&blank)
            .expect_err("a blank bearer token is unusable");

        assert!(
            error.to_string().contains("cannot be blank"),
            "the refusal must name what is wrong: {error}"
        );
    }

    /// A role the server's enum does not admit is a moved contract, not a cashier.
    #[test]
    fn an_unknown_role_is_refused_rather_than_defaulted() {
        let unknown = VERIFIED.replace(r#""role": "SUPERVISOR""#, r#""role": "AUDITOR""#);

        assert!(serde_json::from_str::<VerifyPinResponse>(&unknown).is_err());
    }
}
