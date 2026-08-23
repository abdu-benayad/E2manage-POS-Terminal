//! Authentication Service - Terminal and operator authentication
//!
//! Handles terminal login, operator PIN verification, and session management.
//! Supports both online verification (via API) and offline verification (via local DB).

use anyhow::{anyhow, Result};
use bcrypt::{hash, verify, DEFAULT_COST};
use pos_api::{ApiClient, HeartbeatRequest, HeartbeatResponse, LoginTerminalResponse};
use pos_db::Database;
use pos_models::{OperatorId, OperatorRole, RecordedOperatorName};
use rusqlite::params;
use std::sync::Arc;
use tracing::{debug, error, info, warn};

/// Authentication service for terminal and operator management
pub struct AuthService {
    api: Arc<ApiClient>,
    db: Arc<Database>,
}

/// Represents the current terminal configuration
#[derive(Debug, Clone)]
pub struct TerminalSession {
    pub terminal_id: String,
    pub terminal_code: String,
    pub hardware_id: String,
    pub session_token: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub locale: String,
    pub currency: String,
    pub tax_rate: f64,
    pub tax_inclusive: bool,
    pub sector: String,
    pub features: Vec<String>,
}

/// Result of operator PIN verification
///
/// The `operator_role` is an `Option` because there is no role to report when the PIN was not
/// accepted. It previously held `String::new()` on every failure path — an empty string standing
/// in for "this was not a success", which is the confusion `pos_models::PinVerification` exists to
/// end. This struct is replaced wholesale by `auth-outcome-and-offline-lockout`; until then the
/// sentinel is at least spelled as the absence it is.
#[derive(Debug, Clone)]
pub struct PinVerificationResult {
    pub valid: bool,
    pub operator_id: OperatorId,
    pub operator_name: Option<RecordedOperatorName>,
    pub operator_role: Option<OperatorRole>,
    pub message: Option<String>,
}

impl AuthService {
    /// Creates a new authentication service
    ///
    /// # Arguments
    ///
    /// * `api` - API client for backend communication
    /// * `db` - Local database for offline support
    pub fn new(api: Arc<ApiClient>, db: Arc<Database>) -> Self {
        Self { api, db }
    }

    // ========================================================================
    // TERMINAL AUTHENTICATION
    // ========================================================================

    /// Logs in the terminal with the backend
    ///
    /// Authenticates using hardware ID and secret, stores the session locally.
    ///
    /// # Arguments
    ///
    /// * `hardware_id` - Terminal hardware identifier
    /// * `secret` - Terminal secret from registration
    ///
    /// # Returns
    ///
    /// Terminal session with configuration
    pub async fn login_terminal(
        &self,
        terminal_code: &str,
        hardware_id: &str,
        secret: &str,
    ) -> Result<TerminalSession> {
        info!(
            "Logging in terminal {} with hardware ID: {}",
            terminal_code, hardware_id
        );

        let response = self
            .api
            .login_terminal(terminal_code, hardware_id, secret)
            .await?;

        // Save terminal configuration to local database
        self.save_terminal_config(hardware_id, &response)?;

        info!(
            "Terminal logged in successfully: {} ({})",
            response.terminal_code, response.terminal_id
        );

        Ok(TerminalSession {
            terminal_id: response.terminal_id,
            terminal_code: response.terminal_code,
            hardware_id: hardware_id.to_string(),
            session_token: response.session_token,
            company_id: response.company_id,
            branch_id: response.branch_id,
            locale: response.config.locale.unwrap_or_else(|| "ar".to_string()),
            currency: response
                .config
                .currency
                .unwrap_or_else(|| "LYD".to_string()),
            tax_rate: response
                .config
                .tax_config
                .as_ref()
                .map(|t| t.default_rate)
                .unwrap_or(0.0),
            tax_inclusive: response
                .config
                .tax_config
                .as_ref()
                .map(|t| t.tax_inclusive)
                .unwrap_or(false),
            sector: response
                .config
                .business_sector
                .unwrap_or_else(|| "RETAIL".to_string()),
            features: response.features,
        })
    }

    /// Saves terminal configuration to local database
    fn save_terminal_config(
        &self,
        hardware_id: &str,
        response: &LoginTerminalResponse,
    ) -> Result<()> {
        let conn = self.db.connection();
        let conn = conn.lock();

        conn.execute(
            r#"
            INSERT OR REPLACE INTO terminal_config
            (id, terminal_id, terminal_code, hardware_id, session_token,
             company_id, branch_id, locale, currency,
             tax_rate, tax_inclusive, sector, updated_at)
            VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, datetime('now'))
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
                response
                    .config
                    .tax_config
                    .as_ref()
                    .map(|t| t.default_rate)
                    .unwrap_or(0.0),
                response
                    .config
                    .tax_config
                    .as_ref()
                    .map(|t| t.tax_inclusive as i32)
                    .unwrap_or(0),
                response.config.business_sector,
            ],
        )?;

        debug!("Terminal config saved to local database");
        Ok(())
    }

    /// Loads saved terminal session from local database
    ///
    /// Useful for restoring session after app restart
    pub fn load_saved_session(&self) -> Result<Option<TerminalSession>> {
        let conn = self.db.connection();
        let conn = conn.lock();

        let result = conn.query_row(
            r#"
            SELECT terminal_id, terminal_code, hardware_id, session_token,
                   company_id, branch_id, locale, currency,
                   tax_rate, tax_inclusive, sector
            FROM terminal_config
            WHERE id = 1
            "#,
            [],
            |row| {
                Ok(TerminalSession {
                    terminal_id: row.get(0)?,
                    terminal_code: row.get(1)?,
                    hardware_id: row.get(2)?,
                    session_token: row.get::<_, Option<String>>(3)?.unwrap_or_default(),
                    company_id: row.get::<_, Option<String>>(4)?.unwrap_or_default(),
                    branch_id: row.get(5)?,
                    locale: row
                        .get::<_, Option<String>>(6)?
                        .unwrap_or_else(|| "ar".to_string()),
                    currency: row
                        .get::<_, Option<String>>(7)?
                        .unwrap_or_else(|| "LYD".to_string()),
                    tax_rate: row.get::<_, Option<f64>>(8)?.unwrap_or(0.0),
                    tax_inclusive: row.get::<_, Option<i32>>(9)?.unwrap_or(0) != 0,
                    sector: row
                        .get::<_, Option<String>>(10)?
                        .unwrap_or_else(|| "RETAIL".to_string()),
                    features: vec![], // Features need to be synced
                })
            },
        );

        match result {
            Ok(session) => {
                if !session.session_token.is_empty() {
                    Ok(Some(session))
                } else {
                    Ok(None)
                }
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Sends a heartbeat to the backend
    pub async fn send_heartbeat(&self, metrics: &HeartbeatRequest) -> Result<HeartbeatResponse> {
        self.api.send_heartbeat(metrics).await
    }

    /// Checks if the backend is reachable and authenticated
    pub async fn is_online(&self) -> bool {
        self.api.is_online().await.is_online()
    }

    // ========================================================================
    // OPERATOR PIN VERIFICATION
    // ========================================================================

    /// Verifies an operator's PIN
    ///
    /// Tries online verification first, falls back to offline verification.
    ///
    /// # Arguments
    ///
    /// * `operator_id` - Operator ID to verify
    /// * `pin` - PIN entered by operator
    ///
    /// # Returns
    ///
    /// Verification result with operator info if valid
    pub async fn verify_pin(
        &self,
        operator_id: &OperatorId,
        pin: &str,
    ) -> Result<PinVerificationResult> {
        debug!("Verifying PIN for operator: {}", operator_id);

        // Try online verification first
        if self.api.is_online().await.is_online() {
            match self.verify_pin_online(operator_id, pin).await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    warn!("Online PIN verification failed, trying offline: {}", e);
                }
            }
        }

        // Fall back to offline verification
        self.verify_pin_offline(operator_id, pin)
    }

    /// Verifies PIN with the backend API
    async fn verify_pin_online(
        &self,
        operator_id: &OperatorId,
        pin: &str,
    ) -> Result<PinVerificationResult> {
        let response = self.api.verify_operator_pin(operator_id, pin).await?;

        if response.valid {
            // Get operator info from local database
            let operator = self.get_operator_info(operator_id)?;
            Ok(PinVerificationResult {
                valid: true,
                operator_id: operator_id.clone(),
                operator_name: Some(operator.0),
                operator_role: Some(operator.1),
                message: None,
            })
        } else {
            Ok(PinVerificationResult {
                valid: false,
                operator_id: operator_id.clone(),
                operator_name: None,
                operator_role: None,
                message: response.message,
            })
        }
    }

    /// Verifies PIN using local database (offline mode)
    pub fn verify_pin_offline(
        &self,
        operator_id: &OperatorId,
        pin: &str,
    ) -> Result<PinVerificationResult> {
        let conn = self.db.connection();
        let conn = conn.lock();

        let result: Result<(String, String, String), rusqlite::Error> = conn.query_row(
            "SELECT pin_hash, name, role FROM operators WHERE id = ?1 AND is_active = 1",
            [operator_id.as_str()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        );

        match result {
            Ok((pin_hash, name, role)) => {
                // A row whose role column holds something the server's enum does not admit is a
                // broken row, and refusing to authenticate against it is the fail-closed
                // direction. Parsing before the PIN check keeps that true for a wrong PIN too.
                let role: OperatorRole = role.parse()?;
                let name = RecordedOperatorName::new(name)?;
                let valid = verify(pin, &pin_hash).unwrap_or(false);
                Ok(PinVerificationResult {
                    valid,
                    operator_id: operator_id.clone(),
                    operator_name: valid.then_some(name),
                    operator_role: valid.then_some(role),
                    message: if valid {
                        None
                    } else {
                        Some("Invalid PIN".to_string())
                    },
                })
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                error!("Operator not found: {}", operator_id);
                Ok(PinVerificationResult {
                    valid: false,
                    operator_id: operator_id.clone(),
                    operator_name: None,
                    operator_role: None,
                    message: Some("Operator not found".to_string()),
                })
            }
            Err(e) => Err(e.into()),
        }
    }

    /// Verifies PIN synchronously (for offline-only verification)
    ///
    /// Use this when you know you're offline or want to avoid async
    pub fn verify_pin_sync(
        &self,
        operator_id: &OperatorId,
        pin: &str,
    ) -> Result<PinVerificationResult> {
        self.verify_pin_offline(operator_id, pin)
    }

    /// Gets operator info from local database
    fn get_operator_info(
        &self,
        operator_id: &OperatorId,
    ) -> Result<(RecordedOperatorName, OperatorRole)> {
        let conn = self.db.connection();
        let conn = conn.lock();

        let (name, role): (String, String) = conn
            .query_row(
                "SELECT name, role FROM operators WHERE id = ?1",
                [operator_id.as_str()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(|e| anyhow!("Operator not found: {}", e))?;

        Ok((RecordedOperatorName::new(name)?, role.parse()?))
    }

    // ========================================================================
    // PIN HASHING UTILITIES
    // ========================================================================

    /// Hashes a PIN for storage
    ///
    /// # Arguments
    ///
    /// * `pin` - Plain text PIN
    ///
    /// # Returns
    ///
    /// Bcrypt hash of the PIN
    pub fn hash_pin(pin: &str) -> Result<String> {
        hash(pin, DEFAULT_COST).map_err(|e| anyhow!("Failed to hash PIN: {}", e))
    }

    /// Verifies a PIN against a stored hash
    ///
    /// # Arguments
    ///
    /// * `pin` - Plain text PIN to verify
    /// * `hash` - Stored bcrypt hash
    ///
    /// # Returns
    ///
    /// `true` if PIN matches, `false` otherwise
    pub fn verify_pin_hash(pin: &str, hash: &str) -> bool {
        verify(pin, hash).unwrap_or(false)
    }

    // ========================================================================
    // SESSION MANAGEMENT
    // ========================================================================

    /// Clears the saved session
    pub fn clear_session(&self) -> Result<()> {
        let conn = self.db.connection();
        let conn = conn.lock();

        conn.execute(
            "UPDATE terminal_config SET session_token = NULL WHERE id = 1",
            [],
        )?;

        Ok(())
    }

    /// Updates the session token in local storage
    pub fn update_session_token(&self, token: &str) -> Result<()> {
        let conn = self.db.connection();
        let conn = conn.lock();

        conn.execute(
            "UPDATE terminal_config SET session_token = ?1, updated_at = datetime('now') WHERE id = 1",
            [token],
        )?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pos_db::init_memory_database;

    fn op_id(id: &str) -> OperatorId {
        OperatorId::new(id).expect("a fixture id is never blank")
    }

    fn create_test_service() -> AuthService {
        let db = init_memory_database().unwrap();
        let api = ApiClient::new("https://api.example.com");
        AuthService::new(Arc::new(api), Arc::new(db))
    }

    #[test]
    fn test_hash_pin() {
        let pin = "1234";
        let hashed = AuthService::hash_pin(pin).unwrap();

        // Hash should not equal plain PIN
        assert_ne!(hashed, pin);

        // Should be verifiable
        assert!(AuthService::verify_pin_hash(pin, &hashed));

        // Wrong PIN should not verify
        assert!(!AuthService::verify_pin_hash("5678", &hashed));
    }

    #[test]
    fn test_verify_pin_hash() {
        // Test with known bcrypt hash of "1234"
        let pin = "1234";
        let hash = AuthService::hash_pin(pin).unwrap();

        assert!(AuthService::verify_pin_hash("1234", &hash));
        assert!(!AuthService::verify_pin_hash("0000", &hash));
        assert!(!AuthService::verify_pin_hash("", &hash));
    }

    #[test]
    fn test_load_saved_session_empty() {
        let service = create_test_service();
        let session = service.load_saved_session().unwrap();
        assert!(session.is_none());
    }

    /// Every column of `terminal_config` carries a **distinct** value, and every field of the
    /// loaded `TerminalSession` is asserted.
    ///
    /// Both halves are load-bearing. `load_saved_session` reads its row by position, so a column
    /// added, removed or reordered in the SELECT list silently shifts every index after it —
    /// and because the columns either side are all TEXT, the swapped read compiles and satisfies
    /// rusqlite's type check. Asserting a subset leaves the unasserted positions free to swap;
    /// repeating a value across same-typed columns lets a swap between *those two* pass. So a
    /// later edit that tidies these fixtures into shared or omitted values disarms the test
    /// without failing it.
    ///
    /// The three columns with fallbacks — locale, currency, sector — are given values that
    /// differ from their defaults ("ar", "LYD", "RETAIL"), so a fallback firing over a stored
    /// value fails here too.
    #[test]
    fn test_load_saved_session_reads_every_column_into_its_own_field() {
        let service = create_test_service();

        {
            let conn = service.db.connection();
            let conn = conn.lock();
            conn.execute(
                r#"
                INSERT INTO terminal_config
                (id, terminal_id, terminal_code, hardware_id, session_token,
                 company_id, branch_id, locale, currency, tax_rate, tax_inclusive, sector)
                VALUES (1, 'terminal-id-1', 'terminal-code-1', 'hardware-id-1', 'session-token-1',
                        'company-id-1', 'branch-id-1', 'en', 'USD', 15.5, 1, 'PHARMACY')
                "#,
                [],
            )
            .unwrap();
        }

        let session = service.load_saved_session().unwrap().unwrap();

        assert_eq!(session.terminal_id, "terminal-id-1");
        assert_eq!(session.terminal_code, "terminal-code-1");
        assert_eq!(session.hardware_id, "hardware-id-1");
        assert_eq!(session.session_token, "session-token-1");
        assert_eq!(session.company_id, "company-id-1");
        assert_eq!(session.branch_id, Some("branch-id-1".to_string()));
        assert_eq!(session.locale, "en");
        assert_eq!(session.currency, "USD");
        assert_eq!(session.tax_rate, 15.5);
        assert!(session.tax_inclusive);
        assert_eq!(session.sector, "PHARMACY");
        assert!(
            session.features.is_empty(),
            "features are synced separately"
        );
    }

    /// The fallbacks are the other half of the mapping: a NULL in any nullable column must reach
    /// its own field's default, not another field's.
    #[test]
    fn test_load_saved_session_applies_each_fallback_to_its_own_field() {
        let service = create_test_service();

        {
            let conn = service.db.connection();
            let conn = conn.lock();
            conn.execute(
                r#"
                INSERT INTO terminal_config
                (id, terminal_id, terminal_code, hardware_id, session_token,
                 company_id, branch_id, locale, currency, tax_rate, tax_inclusive, sector)
                VALUES (1, 'terminal-id-1', 'terminal-code-1', 'hardware-id-1', 'session-token-1',
                        NULL, NULL, NULL, NULL, NULL, NULL, NULL)
                "#,
                [],
            )
            .unwrap();
        }

        let session = service.load_saved_session().unwrap().unwrap();

        assert_eq!(session.company_id, "");
        assert_eq!(session.branch_id, None);
        assert_eq!(session.locale, "ar");
        assert_eq!(session.currency, "LYD");
        assert_eq!(session.tax_rate, 0.0);
        assert!(!session.tax_inclusive);
        assert_eq!(session.sector, "RETAIL");
    }

    #[test]
    fn test_verify_pin_offline_operator_not_found() {
        let service = create_test_service();
        let result = service
            .verify_pin_offline(&op_id("nonexistent"), "1234")
            .unwrap();

        assert!(!result.valid);
        assert_eq!(result.message, Some("Operator not found".to_string()));
    }

    #[test]
    fn test_verify_pin_offline_with_operator() {
        let service = create_test_service();

        // Insert test operator
        {
            let conn = service.db.connection();
            let conn = conn.lock();
            let pin_hash = AuthService::hash_pin("1234").unwrap();
            conn.execute(
                r#"
                INSERT INTO operators (id, code, name, pin_hash, role, is_active)
                VALUES ('op1', 'C001', 'Ahmed', ?1, 'CASHIER', 1)
                "#,
                [&pin_hash],
            )
            .unwrap();
        }

        // Test correct PIN
        let result = service.verify_pin_offline(&op_id("op1"), "1234").unwrap();
        assert!(result.valid);
        assert_eq!(
            result.operator_name,
            Some(RecordedOperatorName::new("Ahmed").unwrap())
        );
        assert_eq!(result.operator_role, Some(OperatorRole::Cashier));

        // Test wrong PIN
        let result = service.verify_pin_offline(&op_id("op1"), "9999").unwrap();
        assert!(!result.valid);
        assert_eq!(result.message, Some("Invalid PIN".to_string()));
    }

    #[test]
    fn test_clear_session() {
        let service = create_test_service();

        // Insert a session
        {
            let conn = service.db.connection();
            let conn = conn.lock();
            conn.execute(
                r#"
                INSERT INTO terminal_config
                (id, terminal_id, terminal_code, hardware_id, session_token)
                VALUES (1, 'term1', 'TERM-001', 'HW123', 'token123')
                "#,
                [],
            )
            .unwrap();
        }

        // Clear session
        service.clear_session().unwrap();

        // Verify session is cleared
        let session = service.load_saved_session().unwrap();
        assert!(session.is_none());
    }

    #[test]
    fn test_update_session_token() {
        let service = create_test_service();

        // Insert initial config
        {
            let conn = service.db.connection();
            let conn = conn.lock();
            conn.execute(
                r#"
                INSERT INTO terminal_config
                (id, terminal_id, terminal_code, hardware_id, session_token)
                VALUES (1, 'term1', 'TERM-001', 'HW123', 'old-token')
                "#,
                [],
            )
            .unwrap();
        }

        // Update token
        service.update_session_token("new-token").unwrap();

        // Verify token updated
        let session = service.load_saved_session().unwrap().unwrap();
        assert_eq!(session.session_token, "new-token");
    }
}
