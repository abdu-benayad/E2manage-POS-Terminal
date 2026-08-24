//! Pairing Service - Terminal registration and pairing workflow
//!
//! Handles the terminal pairing flow:
//! 1. Check if terminal is registered
//! 2. Get hardware ID
//! 3. Request pairing code from server
//! 4. Poll for pairing completion
//! 5. Save registration credentials

use anyhow::Result;
use chrono::{DateTime, Utc};
use pos_api::{
    ApiClient, DeviceInfo, HardwareInfo, OsInfo, PairedTerminalInfo, PairingStatus,
    RegisterDeviceRequest,
};
use pos_models::HardwareEnrolment;
use pos_db::Database;
use rusqlite::params;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Terminal registration information stored locally
#[derive(Debug, Clone)]
pub struct TerminalRegistration {
    /// Hardware ID of this terminal
    pub hardware_id: String,
    /// Terminal ID assigned by server
    pub terminal_id: Option<String>,
    /// Terminal code (e.g., "TERM-001")
    pub terminal_code: Option<String>,
    /// Secret for authentication
    pub secret: Option<String>,
    /// Company name for display
    pub company_name: Option<String>,
    /// Whether the terminal is registered
    pub is_registered: bool,
    /// When the terminal was registered
    pub registered_at: Option<String>,
}

/// Current pairing state
#[derive(Debug, Clone)]
pub struct PairingState {
    /// The current pairing code
    pub pairing_code: String,
    /// When it expires
    pub expires_at: DateTime<Utc>,
    /// Current status
    pub status: PairingStatus,
    /// Hardware ID
    pub hardware_id: String,
    /// Whether the platform considers this hardware already enrolled.
    ///
    /// Read from the platform and never inferred here — see [`HardwareEnrolment`] for why the
    /// local store cannot answer it. `Undetermined` until the first status poll, because the
    /// pairing-request response carries no enrolment signal at all.
    pub enrolment: HardwareEnrolment,
}

/// Service for managing terminal pairing/registration
pub struct PairingService {
    api: Arc<ApiClient>,
    db: Arc<Database>,
}

impl PairingService {
    /// Creates a new pairing service
    pub fn new(api: Arc<ApiClient>, db: Arc<Database>) -> Self {
        Self { api, db }
    }

    // ========================================================================
    // REGISTRATION CHECK
    // ========================================================================

    /// Checks if the terminal is registered
    pub fn is_registered(&self) -> Result<bool> {
        let conn = self.db.connection();
        let conn = conn.lock();

        let is_registered: i32 = conn
            .query_row(
                "SELECT is_registered FROM terminal_registration WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        Ok(is_registered == 1)
    }

    /// Gets the current terminal registration info
    pub fn get_registration(&self) -> Result<Option<TerminalRegistration>> {
        let conn = self.db.connection();
        let conn = conn.lock();

        let result = conn.query_row(
            r#"
            SELECT hardware_id, terminal_id, terminal_code, secret,
                   company_name, is_registered, registered_at
            FROM terminal_registration
            WHERE id = 1
            "#,
            [],
            |row| {
                Ok(TerminalRegistration {
                    hardware_id: row.get(0)?,
                    terminal_id: row.get(1)?,
                    terminal_code: row.get(2)?,
                    secret: row.get(3)?,
                    company_name: row.get(4)?,
                    is_registered: row.get::<_, i32>(5)? == 1,
                    registered_at: row.get(6)?,
                })
            },
        );

        match result {
            Ok(reg) if reg.is_registered => Ok(Some(reg)),
            Ok(_) => Ok(None),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Gets the hardware ID for this terminal
    pub fn get_hardware_id(&self) -> Result<String> {
        // Try to get from database first
        let conn = self.db.connection();
        let conn = conn.lock();

        let stored_id: String = conn
            .query_row(
                "SELECT hardware_id FROM terminal_registration WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .unwrap_or_default();

        if !stored_id.is_empty() {
            return Ok(stored_id);
        }

        // Generate a new hardware ID
        let hardware_id = generate_hardware_id();

        // Store it
        drop(conn);
        self.save_hardware_id(&hardware_id)?;

        Ok(hardware_id)
    }

    /// Saves the hardware ID
    fn save_hardware_id(&self, hardware_id: &str) -> Result<()> {
        let conn = self.db.connection();
        let conn = conn.lock();

        conn.execute(
            "INSERT OR REPLACE INTO terminal_registration (id, hardware_id, is_registered) VALUES (1, ?1, 0)",
            [hardware_id],
        )?;

        Ok(())
    }

    // ========================================================================
    // PAIRING WORKFLOW
    // ========================================================================

    /// Requests a new pairing code from the server.
    ///
    /// **Hardware the platform already knows is not a special case here.** It answers 200 with the
    /// same `{pairingCode, expiresAt, hardwareId}` body a first enrolment gets, flags the request
    /// as a re-pair on its own side, and an administrator authorises it. The till displays the code
    /// and polls, exactly as it does for a first enrolment.
    ///
    /// The till used to detect that case by substring-matching a prose error message and then ask
    /// the platform to hand its own secret back. Both halves are gone. Self-service recovery cannot
    /// be made safe, only removed: **a till that has lost its secret is indistinguishable from an
    /// attacker claiming that hardware id** — whatever the device could prove is the thing it lost.
    pub async fn request_pairing_code(&self) -> Result<PairingState> {
        let hardware_id = self.get_hardware_id()?;

        let device_info = DeviceInfo {
            os_name: Some(std::env::consts::OS.to_string()),
            os_version: Some(std::env::consts::ARCH.to_string()),
            app_version: Some(env!("CARGO_PKG_VERSION").to_string()),
            screen_resolution: None,
        };

        info!("Requesting pairing code for hardware ID: {}", hardware_id);

        let resp = self
            .api
            .request_pairing(&hardware_id, Some(device_info))
            .await?;

        info!("Received pairing code: {}", resp.pairing_code);

        Ok(PairingState {
            pairing_code: resp.pairing_code,
            expires_at: resp.expires_at,
            status: PairingStatus::Pending,
            hardware_id,
            // A constant, and deliberately so. This route answers 200 with the same body for a
            // first enrolment and a re-pair — there is no field to read here. The answer arrives
            // on the first `check_pairing_status`. Do NOT fill this in from the local store: a
            // stored secret proves this device was enrolled here once, not that a *working*
            // terminal would be replaced, and it answers wrong in both directions.
            enrolment: HardwareEnrolment::Undetermined,
        })
    }

    /// Checks the status of a pairing request
    ///
    /// Returns the current status and terminal info if completed.
    pub async fn check_pairing_status(&self, pairing_code: &str) -> Result<PairingState> {
        debug!("Checking pairing status for code: {}", pairing_code);

        let response = self.api.check_pairing_status(pairing_code).await?;

        // Said once, where it first becomes known, and only when there is something to say.
        // `Undetermined` is logged by nothing: a line reporting that we do not know, on every
        // poll, is what trains a reader to skip the line that matters. Neither the pairing code
        // nor the hardware id appears beside it — the code retrieves a credential and the id was
        // the lookup key for one, which is why the platform removed both from its own logs.
        match response.enrolment {
            HardwareEnrolment::AlreadyEnrolled => info!(
                "This hardware is already enrolled: approving this code re-enrols the terminal \
                 and archives the one currently in service"
            ),
            HardwareEnrolment::NotEnrolled | HardwareEnrolment::Undetermined => {}
        }

        // If completed, save the registration and set token on API client
        if response.status == PairingStatus::Completed {
            if let Some(ref terminal) = response.terminal {
                info!(
                    "Pairing completed! Terminal ID: {}, Code: {}",
                    terminal.terminal_id, terminal.terminal_code
                );
                self.save_registration(terminal)?;

                // Login with the terminal credentials to get a session token
                if !terminal.secret.is_empty() {
                    let hardware_id = self.get_hardware_id()?;
                    info!("Logging in terminal to get session token");
                    match self
                        .api
                        .login_terminal(&terminal.terminal_code, &hardware_id, &terminal.secret)
                        .await
                    {
                        Ok(login_response) => {
                            info!("Terminal logged in successfully, session token set");
                            // login_terminal already calls set_token internally
                            // Also save the full terminal config to DB for persistence
                            if let Err(e) = self.save_terminal_config(&hardware_id, &login_response)
                            {
                                warn!("Failed to save terminal config to DB: {}", e);
                            }
                        }
                        Err(e) => {
                            warn!("Failed to login terminal after pairing: {}", e);
                        }
                    }
                } else {
                    warn!("Pairing completed but no secret received");
                }
            }
        }

        let hardware_id = self.get_hardware_id()?;

        Ok(PairingState {
            pairing_code: response.pairing_code,
            expires_at: response.expires_at,
            status: response.status,
            hardware_id,
            enrolment: response.enrolment,
        })
    }

    /// Saves successful registration to local database
    fn save_registration(&self, terminal: &PairedTerminalInfo) -> Result<()> {
        let conn = self.db.connection();
        let conn = conn.lock();

        conn.execute(
            r#"
            UPDATE terminal_registration
            SET terminal_id = ?1,
                terminal_code = ?2,
                secret = ?3,
                company_name = ?4,
                is_registered = 1,
                registered_at = datetime('now')
            WHERE id = 1
            "#,
            params![
                terminal.terminal_id,
                terminal.terminal_code,
                terminal.secret,
                terminal.company_name,
            ],
        )?;

        info!("Terminal registration saved successfully");
        Ok(())
    }

    /// Saves the full terminal config from login response for persistence across restarts
    fn save_terminal_config(
        &self,
        hardware_id: &str,
        response: &pos_api::LoginTerminalResponse,
    ) -> Result<()> {
        let conn = self.db.connection();
        let conn = conn.lock();

        conn.execute(
            r#"
            INSERT OR REPLACE INTO terminal_config
            (id, terminal_id, terminal_code, hardware_id, session_token,
             company_id, branch_id, locale, currency, sector, updated_at)
            VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, datetime('now'))
            "#,
            params![
                response.terminal_id,
                response.terminal_code,
                hardware_id,
                response.session_token,
                response.company_id,
                response.branch_id,
                response.config.locale,
                response.config.currency,
                response.config.business_sector,
            ],
        )?;

        info!("Terminal config saved to database");
        Ok(())
    }

    /// Gets the stored credentials for login
    pub fn get_credentials(&self) -> Result<Option<(String, String)>> {
        let conn = self.db.connection();
        let conn = conn.lock();

        let result = conn.query_row(
            r#"
            SELECT hardware_id, secret
            FROM terminal_registration
            WHERE id = 1 AND is_registered = 1 AND secret IS NOT NULL
            "#,
            [],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        );

        match result {
            Ok((hardware_id, secret)) => Ok(Some((hardware_id, secret))),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Clears the registration and all tenant-specific data.
    ///
    /// Wipes products, categories, operators, sync state, offline transactions,
    /// and all other tenant-scoped data so that re-pairing to a different
    /// company starts with a clean slate.
    pub fn clear_registration(&self) -> Result<()> {
        // Clear all tenant-specific cached data first
        self.db.clear_tenant_data()?;

        let conn = self.db.connection();
        let conn = conn.lock();

        conn.execute(
            r#"
            UPDATE terminal_registration
            SET terminal_id = NULL,
                terminal_code = NULL,
                secret = NULL,
                is_registered = 0,
                registered_at = NULL
            WHERE id = 1
            "#,
            [],
        )?;

        warn!("Terminal registration and all tenant data cleared");
        Ok(())
    }

    /// Stops presenting this till's credentials — the terminal's and the operator's both.
    ///
    /// Call it after [`Self::clear_registration`] so the next pairing request goes out
    /// presenting nothing. Both, not just the terminal's: an operator token held without a
    /// terminal token is a credential that can no longer be used and can still do harm, because
    /// it is the field the platform attributes a write to. [`ApiClient::clear_credentials`]
    /// enforces that at the field; this wrapper must not describe less than it does.
    pub async fn clear_api_credentials(&self) {
        self.api.clear_credentials().await;
    }

    // ========================================================================
    // SETTINGS MANAGEMENT
    // ========================================================================

    /// Default server URL
    pub const DEFAULT_SERVER_URL: &'static str = "https://jooher.app";

    /// Gets a setting value from the settings table
    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let conn = self.db.connection();
        let conn = conn.lock();

        let result = conn.query_row("SELECT value FROM settings WHERE key = ?1", [key], |row| {
            row.get::<_, String>(0)
        });

        match result {
            Ok(value) => Ok(Some(value)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Sets a setting value in the settings table
    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        let conn = self.db.connection();
        let conn = conn.lock();

        conn.execute(
            r#"
            INSERT INTO settings (key, value, updated_at)
            VALUES (?1, ?2, datetime('now'))
            ON CONFLICT (key) DO UPDATE SET value = ?2, updated_at = datetime('now')
            "#,
            [key, value],
        )?;

        debug!("Setting '{}' updated to '{}'", key, value);
        Ok(())
    }

    /// Gets the configured server URL, or the default if not set
    pub fn get_server_url(&self) -> Result<String> {
        match self.get_setting("server_url")? {
            Some(url) if !url.is_empty() => {
                info!("Using configured server URL: {}", url);
                Ok(url)
            }
            _ => {
                info!("Using default server URL: {}", Self::DEFAULT_SERVER_URL);
                Ok(Self::DEFAULT_SERVER_URL.to_string())
            }
        }
    }

    /// Sets the server URL
    pub fn set_server_url(&self, url: &str) -> Result<()> {
        // Normalize the URL (remove trailing slash)
        let normalized = url.trim_end_matches('/');
        self.set_setting("server_url", normalized)?;
        info!("Server URL updated to: {}", normalized);
        Ok(())
    }

    // ========================================================================
    // PLATFORM REGISTRATION
    // ========================================================================

    /// Registers the device with the platform registry
    ///
    /// This is called after successful pairing to register the device
    /// for platform-level monitoring and management.
    ///
    /// # Arguments
    ///
    /// * `device_name` - Friendly name for the device
    ///
    /// # Returns
    ///
    /// The license key assigned to the device
    pub async fn register_with_platform(&self, device_name: &str) -> Result<String> {
        let hardware_id = self.get_hardware_id()?;

        info!(
            "Registering device with platform: hardware_id={}, name={}",
            hardware_id, device_name
        );

        let request = RegisterDeviceRequest {
            device_id: hardware_id.clone(),
            device_fingerprint: self.generate_device_fingerprint()?,
            device_name: device_name.to_string(),
            os_info: OsInfo {
                name: std::env::consts::OS.to_string(),
                version: std::env::consts::ARCH.to_string(),
            },
            hardware_info: self.collect_hardware_info(),
        };

        let response = self.api.register_device_platform(&request).await?;

        info!(
            "Device registered with platform: license_key={}, status={:?}",
            response.license_key, response.license_status
        );

        // Save the license key locally
        self.save_platform_license(&response.license_key)?;

        Ok(response.license_key)
    }

    /// Generates a device fingerprint based on hardware characteristics
    fn generate_device_fingerprint(&self) -> Result<String> {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();

        // Hash hostname
        if let Ok(hostname) = hostname::get() {
            hostname.hash(&mut hasher);
        }

        // Hash OS info
        std::env::consts::OS.hash(&mut hasher);
        std::env::consts::ARCH.hash(&mut hasher);

        // Hash machine ID if available (Linux)
        #[cfg(target_os = "linux")]
        {
            if let Ok(machine_id) = std::fs::read_to_string("/etc/machine-id") {
                machine_id.trim().hash(&mut hasher);
            }
        }

        let hash = hasher.finish();
        Ok(format!("FP-{:016X}", hash))
    }

    /// Collects hardware information for registration
    fn collect_hardware_info(&self) -> HardwareInfo {
        let cpu = self.get_cpu_info();
        let memory = self.get_total_memory_mb();

        HardwareInfo { cpu, memory }
    }

    /// Gets CPU information
    fn get_cpu_info(&self) -> String {
        #[cfg(target_os = "linux")]
        {
            if let Ok(content) = std::fs::read_to_string("/proc/cpuinfo") {
                for line in content.lines() {
                    if line.starts_with("model name") {
                        if let Some(value) = line.split(':').nth(1) {
                            return value.trim().to_string();
                        }
                    }
                }
            }
        }
        std::env::consts::ARCH.to_string()
    }

    /// Gets total memory in MB
    fn get_total_memory_mb(&self) -> u64 {
        #[cfg(target_os = "linux")]
        {
            if let Ok(content) = std::fs::read_to_string("/proc/meminfo") {
                for line in content.lines() {
                    if line.starts_with("MemTotal:") {
                        let kb: u64 = line
                            .split_whitespace()
                            .nth(1)
                            .and_then(|s| s.parse().ok())
                            .unwrap_or(0);
                        return kb / 1024; // Convert KB to MB
                    }
                }
            }
        }
        0
    }

    /// Saves the platform license key locally
    fn save_platform_license(&self, license_key: &str) -> Result<()> {
        let conn = self.db.connection();
        let conn = conn.lock();

        conn.execute(
            r#"
            UPDATE terminal_registration
            SET license_key = ?1
            WHERE id = 1
            "#,
            [license_key],
        )?;

        info!("Platform license key saved");
        Ok(())
    }

    /// Gets the stored platform license key
    pub fn get_platform_license(&self) -> Result<Option<String>> {
        let conn = self.db.connection();
        let conn = conn.lock();

        let result = conn.query_row(
            "SELECT license_key FROM terminal_registration WHERE id = 1",
            [],
            |row| row.get::<_, Option<String>>(0),
        );

        match result {
            Ok(license_key) => Ok(license_key),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Validates the platform license with the server
    pub async fn validate_platform_license(&self) -> Result<bool> {
        let hardware_id = self.get_hardware_id()?;
        let license_key = self.get_platform_license()?;

        match license_key {
            Some(key) => {
                let valid = self.api.validate_license(&hardware_id, &key).await?;
                Ok(valid)
            }
            None => {
                warn!("No platform license key stored");
                Ok(false)
            }
        }
    }
}

/// Generates a unique hardware ID for this terminal
fn generate_hardware_id() -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();

    // Hash various system properties
    if let Ok(hostname) = hostname::get() {
        hostname.hash(&mut hasher);
    }

    // Add some randomness for uniqueness
    let random: u64 = rand::random();
    random.hash(&mut hasher);

    // Add timestamp
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
        .hash(&mut hasher);

    let hash = hasher.finish();
    format!("POS-{:016X}", hash)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pos_db::init_memory_database;

    fn create_test_service() -> PairingService {
        let db = Arc::new(init_memory_database().unwrap());
        let api = Arc::new(ApiClient::new("https://api.example.com"));
        PairingService::new(api, db)
    }

    #[test]
    fn test_not_registered_initially() {
        let service = create_test_service();
        assert!(!service.is_registered().unwrap());
    }

    /// Every column of `terminal_registration` carries a **distinct** value, and every field of
    /// the loaded `TerminalRegistration` is asserted.
    ///
    /// `get_registration` reads its row by position, so a column added, removed or reordered in
    /// the SELECT list shifts every index after it. Five of the seven columns are TEXT, so a swap
    /// among them compiles and passes rusqlite's type check — it just attributes one terminal's
    /// details to another. Asserting a subset leaves the unasserted positions free to swap, and
    /// repeating a value across same-typed columns lets a swap between *those two* pass: a later
    /// edit that tidies these fixtures into shared or omitted values disarms the test without
    /// failing it.
    #[test]
    fn test_get_registration_reads_every_column_into_its_own_field() {
        let service = create_test_service();

        {
            let conn = service.db.connection();
            let conn = conn.lock();
            conn.execute(
                r#"
                UPDATE terminal_registration
                SET hardware_id = 'hardware-id-1',
                    terminal_id = 'terminal-id-1',
                    terminal_code = 'terminal-code-1',
                    secret = 'secret-1',
                    company_name = 'company-name-1',
                    registered_at = '2026-08-23T12:00:00Z',
                    is_registered = 1
                WHERE id = 1
                "#,
                [],
            )
            .unwrap();
        }

        let registration = service.get_registration().unwrap().unwrap();

        assert_eq!(registration.hardware_id, "hardware-id-1");
        assert_eq!(registration.terminal_id, Some("terminal-id-1".to_string()));
        assert_eq!(
            registration.terminal_code,
            Some("terminal-code-1".to_string())
        );
        assert_eq!(registration.secret, Some("secret-1".to_string()));
        assert_eq!(
            registration.company_name,
            Some("company-name-1".to_string())
        );
        assert!(registration.is_registered);
        assert_eq!(
            registration.registered_at,
            Some("2026-08-23T12:00:00Z".to_string())
        );
    }

    /// A row that exists but is not registered is not a registration. The credentials are present
    /// on that row during pairing, so returning it would hand callers a terminal identity the
    /// server has not confirmed.
    #[test]
    fn test_get_registration_is_none_while_unregistered() {
        let service = create_test_service();

        {
            let conn = service.db.connection();
            let conn = conn.lock();
            conn.execute(
                r#"
                UPDATE terminal_registration
                SET hardware_id = 'hardware-id-1',
                    terminal_id = 'terminal-id-1',
                    terminal_code = 'terminal-code-1',
                    secret = 'secret-1',
                    is_registered = 0
                WHERE id = 1
                "#,
                [],
            )
            .unwrap();
        }

        assert!(service.get_registration().unwrap().is_none());
    }

    #[test]
    fn test_get_hardware_id() {
        let service = create_test_service();
        let id1 = service.get_hardware_id().unwrap();
        let id2 = service.get_hardware_id().unwrap();

        // Same ID returned each time
        assert_eq!(id1, id2);
        assert!(id1.starts_with("POS-"));
    }

    #[test]
    fn test_save_registration() {
        let service = create_test_service();

        // Set hardware ID first
        service.save_hardware_id("TEST-HW-123").unwrap();

        // Save registration
        let terminal = PairedTerminalInfo {
            terminal_id: "TERM-001".to_string(),
            terminal_code: "TERM-001".to_string(),
            secret: "secret123".to_string(),
            company_name: None,
        };

        service.save_registration(&terminal).unwrap();

        // Verify registered
        assert!(service.is_registered().unwrap());

        // Verify credentials
        let creds = service.get_credentials().unwrap();
        assert!(creds.is_some());
        let (hw_id, secret) = creds.unwrap();
        assert_eq!(hw_id, "TEST-HW-123");
        assert_eq!(secret, "secret123");
    }

    #[test]
    fn test_clear_registration() {
        let service = create_test_service();

        // Register first
        service.save_hardware_id("TEST-HW-123").unwrap();
        let terminal = PairedTerminalInfo {
            terminal_id: "TERM-001".to_string(),
            terminal_code: "TERM-001".to_string(),
            secret: "secret123".to_string(),
            company_name: None,
        };
        service.save_registration(&terminal).unwrap();
        assert!(service.is_registered().unwrap());

        // Clear registration
        service.clear_registration().unwrap();
        assert!(!service.is_registered().unwrap());
        assert!(service.get_credentials().unwrap().is_none());
    }

    #[test]
    fn test_generate_hardware_id() {
        let id1 = generate_hardware_id();
        let id2 = generate_hardware_id();

        // IDs should be different (randomness)
        assert_ne!(id1, id2);

        // IDs should start with POS-
        assert!(id1.starts_with("POS-"));
        assert!(id2.starts_with("POS-"));
    }
}
