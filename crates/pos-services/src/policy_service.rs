//! Policy Service - Security policy caching and enforcement
//!
//! Provides functionality to:
//! - Fetch security policies from the platform
//! - Cache policies with ETag support
//! - Evaluate policies for specific operations
//! - Enforce policy rules based on enforcement mode
//!
//! ## Usage
//!
//! ```rust,ignore
//! use pos_services::{PolicyService, PolicyResult};
//! use pos_api::ApiClient;
//! use std::sync::Arc;
//!
//! let api = Arc::new(ApiClient::new("https://api.example.com"));
//! let service = PolicyService::new(api);
//!
//! // Refresh policies from server
//! service.refresh().await?;
//!
//! // Check a policy
//! let result = service.check_boolean("ENCRYPTION_AT_REST");
//! match result {
//!     PolicyResult::Allow => println!("Allowed"),
//!     PolicyResult::Block(reason) => println!("Blocked: {}", reason),
//!     PolicyResult::Warn(reason) => println!("Warning: {}", reason),
//! }
//! ```

use pos_api::{
    ApiClient, EnforcementMode, GetResult, SecurityCategory, SecurityPoliciesResponse,
    SecurityPolicy,
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// Result of evaluating a policy
#[derive(Debug, Clone, PartialEq)]
pub enum PolicyResult {
    /// Action is allowed
    Allow,
    /// Action is blocked (enforcement mode is BLOCK)
    Block(String),
    /// Action is allowed but a warning was issued (enforcement mode is WARN)
    Warn(String),
    /// Action is allowed but will be audited (enforcement mode is AUDIT)
    Audit(String),
}

impl PolicyResult {
    /// Returns true if the action is allowed (Allow, Warn, or Audit)
    pub fn is_allowed(&self) -> bool {
        !matches!(self, PolicyResult::Block(_))
    }

    /// Returns true if the action is blocked
    pub fn is_blocked(&self) -> bool {
        matches!(self, PolicyResult::Block(_))
    }

    /// Returns the message if the result is not Allow
    pub fn message(&self) -> Option<&str> {
        match self {
            PolicyResult::Allow => None,
            PolicyResult::Block(msg) | PolicyResult::Warn(msg) | PolicyResult::Audit(msg) => {
                Some(msg)
            }
        }
    }
}

/// Policy evaluation error
#[derive(Debug, Clone)]
pub enum PolicyError {
    /// Network error fetching policies
    NetworkError(String),
    /// Policy not found
    PolicyNotFound(String),
    /// Invalid policy value
    InvalidValue(String),
}

impl std::fmt::Display for PolicyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PolicyError::NetworkError(e) => write!(f, "Network error: {}", e),
            PolicyError::PolicyNotFound(code) => write!(f, "Policy not found: {}", code),
            PolicyError::InvalidValue(e) => write!(f, "Invalid policy value: {}", e),
        }
    }
}

impl std::error::Error for PolicyError {}

pub type PolicyServiceResult<T> = Result<T, PolicyError>;

/// Range policy value
#[derive(Debug, Clone)]
pub struct RangeValue {
    pub min: f64,
    pub max: f64,
}

/// Policy Service
///
/// Manages security policies fetched from the platform.
pub struct PolicyService {
    api: Arc<ApiClient>,
    /// Cached policies indexed by code
    policies: RwLock<HashMap<String, SecurityPolicy>>,
    /// Current version hash for caching
    version_hash: RwLock<Option<String>>,
    /// Last successful refresh timestamp
    last_refresh: RwLock<Option<std::time::Instant>>,
}

impl PolicyService {
    /// Creates a new policy service
    ///
    /// # Arguments
    ///
    /// * `api` - API client for server communication
    pub fn new(api: Arc<ApiClient>) -> Self {
        Self {
            api,
            policies: RwLock::new(HashMap::new()),
            version_hash: RwLock::new(None),
            last_refresh: RwLock::new(None),
        }
    }

    /// Refreshes policies from the server
    ///
    /// Uses ETag caching to avoid unnecessary data transfer.
    ///
    /// # Returns
    ///
    /// `true` if policies were updated, `false` if unchanged (304)
    pub async fn refresh(&self) -> PolicyServiceResult<bool> {
        let etag = self.version_hash.read().await.clone();
        let etag_ref = etag.as_deref();

        debug!("Refreshing security policies (etag: {:?})", etag_ref);

        let result = self
            .api
            .get_security_policies(etag_ref)
            .await
            .map_err(|e| PolicyError::NetworkError(e.to_string()))?;

        match result {
            GetResult::Data { data, etag: _ } => {
                self.update_policies(data).await;
                Ok(true)
            }
            GetResult::NotModified => {
                debug!("Policies not modified");
                Ok(false)
            }
        }
    }

    /// Updates the cached policies
    async fn update_policies(&self, response: SecurityPoliciesResponse) {
        let mut policies = self.policies.write().await;
        policies.clear();

        for policy in response.policies {
            debug!(
                "Caching policy: {} ({:?})",
                policy.code, policy.enforcement_mode
            );
            policies.insert(policy.code.clone(), policy);
        }

        info!("Cached {} security policies", policies.len());
        drop(policies);

        *self.version_hash.write().await = Some(response.version);
        *self.last_refresh.write().await = Some(std::time::Instant::now());
    }

    /// Gets the current version hash
    pub async fn version_hash(&self) -> Option<String> {
        self.version_hash.read().await.clone()
    }

    /// Gets the number of cached policies
    pub async fn policy_count(&self) -> usize {
        self.policies.read().await.len()
    }

    /// Gets all policies in a category
    pub async fn get_policies_by_category(
        &self,
        category: SecurityCategory,
    ) -> Vec<SecurityPolicy> {
        self.policies
            .read()
            .await
            .values()
            .filter(|p| p.category == category)
            .cloned()
            .collect()
    }

    /// Gets a specific policy by code
    pub async fn get_policy(&self, code: &str) -> Option<SecurityPolicy> {
        self.policies.read().await.get(code).cloned()
    }

    /// Checks if policies have been loaded
    pub async fn has_policies(&self) -> bool {
        !self.policies.read().await.is_empty()
    }

    // =========================================================================
    // POLICY EVALUATION
    // =========================================================================

    /// Evaluates a boolean policy
    ///
    /// # Arguments
    ///
    /// * `code` - Policy code (e.g., "ENCRYPTION_AT_REST")
    ///
    /// # Returns
    ///
    /// `PolicyResult` based on policy value and enforcement mode
    pub async fn check_boolean(&self, code: &str) -> PolicyResult {
        let policies = self.policies.read().await;

        match policies.get(code) {
            None => {
                // Policy not found - allow by default
                debug!("Policy {} not found, allowing", code);
                PolicyResult::Allow
            }
            Some(policy) => {
                if policy.enforcement_mode == EnforcementMode::Disabled {
                    return PolicyResult::Allow;
                }

                let required = policy.policy_value.as_bool().unwrap_or(true);

                if required {
                    // Policy requires this feature to be enabled
                    self.apply_enforcement(
                        &policy.enforcement_mode,
                        &format!("Policy {} requires this feature to be enabled", code),
                    )
                } else {
                    PolicyResult::Allow
                }
            }
        }
    }

    /// Checks if a value is within the range policy
    ///
    /// # Arguments
    ///
    /// * `code` - Policy code (e.g., "MIN_PIN_LENGTH")
    /// * `value` - Value to check
    ///
    /// # Returns
    ///
    /// `PolicyResult` based on whether value is within range
    pub async fn check_range(&self, code: &str, value: f64) -> PolicyResult {
        let policies = self.policies.read().await;

        match policies.get(code) {
            None => PolicyResult::Allow,
            Some(policy) => {
                if policy.enforcement_mode == EnforcementMode::Disabled {
                    return PolicyResult::Allow;
                }

                let range = self.parse_range(&policy.policy_value);

                match range {
                    Some(r) => {
                        if value < r.min {
                            self.apply_enforcement(
                                &policy.enforcement_mode,
                                &format!(
                                    "Value {} is below minimum {} for policy {}",
                                    value, r.min, code
                                ),
                            )
                        } else if value > r.max {
                            self.apply_enforcement(
                                &policy.enforcement_mode,
                                &format!(
                                    "Value {} exceeds maximum {} for policy {}",
                                    value, r.max, code
                                ),
                            )
                        } else {
                            PolicyResult::Allow
                        }
                    }
                    None => {
                        warn!("Invalid range value for policy {}", code);
                        PolicyResult::Allow
                    }
                }
            }
        }
    }

    /// Checks if a value is in the allowed list
    ///
    /// # Arguments
    ///
    /// * `code` - Policy code (e.g., "ALLOWED_PAYMENT_METHODS")
    /// * `value` - Value to check
    ///
    /// # Returns
    ///
    /// `PolicyResult` based on whether value is in the list
    pub async fn check_list(&self, code: &str, value: &str) -> PolicyResult {
        let policies = self.policies.read().await;

        match policies.get(code) {
            None => PolicyResult::Allow,
            Some(policy) => {
                if policy.enforcement_mode == EnforcementMode::Disabled {
                    return PolicyResult::Allow;
                }

                let allowed_values = self.parse_list(&policy.policy_value);

                if allowed_values.is_empty() {
                    // Empty list means allow all
                    PolicyResult::Allow
                } else if allowed_values.contains(&value.to_string()) {
                    PolicyResult::Allow
                } else {
                    self.apply_enforcement(
                        &policy.enforcement_mode,
                        &format!(
                            "Value '{}' is not in allowed list for policy {}",
                            value, code
                        ),
                    )
                }
            }
        }
    }

    /// Checks if a value matches the enum policy
    ///
    /// # Arguments
    ///
    /// * `code` - Policy code
    /// * `value` - Value to check
    ///
    /// # Returns
    ///
    /// `PolicyResult` based on whether value matches
    pub async fn check_enum(&self, code: &str, value: &str) -> PolicyResult {
        let policies = self.policies.read().await;

        match policies.get(code) {
            None => PolicyResult::Allow,
            Some(policy) => {
                if policy.enforcement_mode == EnforcementMode::Disabled {
                    return PolicyResult::Allow;
                }

                let expected = policy.policy_value.as_str().unwrap_or("");

                if expected.is_empty() || value == expected {
                    PolicyResult::Allow
                } else {
                    self.apply_enforcement(
                        &policy.enforcement_mode,
                        &format!(
                            "Value '{}' does not match expected '{}' for policy {}",
                            value, expected, code
                        ),
                    )
                }
            }
        }
    }

    /// Parses a range policy value
    fn parse_range(&self, value: &serde_json::Value) -> Option<RangeValue> {
        let obj = value.as_object()?;
        let min = obj.get("min")?.as_f64()?;
        let max = obj.get("max")?.as_f64()?;
        Some(RangeValue { min, max })
    }

    /// Parses a list policy value
    fn parse_list(&self, value: &serde_json::Value) -> Vec<String> {
        value
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Applies the enforcement mode to create the appropriate result
    fn apply_enforcement(&self, mode: &EnforcementMode, message: &str) -> PolicyResult {
        match mode {
            EnforcementMode::Disabled => PolicyResult::Allow,
            EnforcementMode::Warn => {
                warn!("Policy warning: {}", message);
                PolicyResult::Warn(message.to_string())
            }
            EnforcementMode::Block => {
                error!("Policy violation: {}", message);
                PolicyResult::Block(message.to_string())
            }
            EnforcementMode::Audit => {
                info!("Policy audit: {}", message);
                PolicyResult::Audit(message.to_string())
            }
            EnforcementMode::Unknown => PolicyResult::Allow,
        }
    }

    // =========================================================================
    // CONVENIENCE METHODS FOR COMMON POLICIES
    // =========================================================================

    /// Gets the minimum PIN length policy
    pub async fn get_min_pin_length(&self) -> Option<u32> {
        let policies = self.policies.read().await;
        let policy = policies.get("MIN_PIN_LENGTH")?;
        let range = self.parse_range(&policy.policy_value)?;
        Some(range.min as u32)
    }

    /// Gets the session timeout in minutes
    pub async fn get_session_timeout_minutes(&self) -> Option<u32> {
        let policies = self.policies.read().await;
        let policy = policies.get("SESSION_TIMEOUT_MINUTES")?;
        let range = self.parse_range(&policy.policy_value)?;
        // Return min as default, or could return a default value from range
        Some(range.min as u32)
    }

    /// Gets the maximum offline transaction amount
    pub async fn get_offline_max_amount(&self) -> Option<f64> {
        let policies = self.policies.read().await;
        let policy = policies.get("OFFLINE_MAX_AMOUNT")?;
        let range = self.parse_range(&policy.policy_value)?;
        Some(range.max)
    }

    /// Gets the heartbeat interval in seconds
    pub async fn get_heartbeat_interval_seconds(&self) -> Option<u32> {
        let policies = self.policies.read().await;
        let policy = policies.get("HEARTBEAT_INTERVAL_SECONDS")?;
        let range = self.parse_range(&policy.policy_value)?;
        Some(range.min as u32)
    }

    /// Checks if PCI compliance mode is enabled
    pub async fn is_pci_compliance_enabled(&self) -> bool {
        let policies = self.policies.read().await;
        policies
            .get("PCI_COMPLIANCE_MODE")
            .and_then(|p| p.policy_value.as_bool())
            .unwrap_or(false)
    }

    /// Gets allowed payment methods
    pub async fn get_allowed_payment_methods(&self) -> Vec<String> {
        let policies = self.policies.read().await;
        policies
            .get("ALLOWED_PAYMENT_METHODS")
            .map(|p| self.parse_list(&p.policy_value))
            .unwrap_or_default()
    }

    /// Gets the receipt retention days
    pub async fn get_receipt_retention_days(&self) -> Option<u32> {
        let policies = self.policies.read().await;
        let policy = policies.get("RECEIPT_RETENTION_DAYS")?;
        let range = self.parse_range(&policy.policy_value)?;
        Some(range.min as u32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pos_api::ApiClient;

    fn create_test_service() -> PolicyService {
        let api = Arc::new(ApiClient::new("https://api.example.com"));
        PolicyService::new(api)
    }

    #[tokio::test]
    async fn test_new_service() {
        let service = create_test_service();
        assert!(!service.has_policies().await);
        assert_eq!(service.policy_count().await, 0);
    }

    #[tokio::test]
    async fn test_check_boolean_not_found() {
        let service = create_test_service();
        let result = service.check_boolean("NON_EXISTENT_POLICY").await;
        assert_eq!(result, PolicyResult::Allow);
    }

    #[test]
    fn test_policy_result_is_allowed() {
        assert!(PolicyResult::Allow.is_allowed());
        assert!(PolicyResult::Warn("test".to_string()).is_allowed());
        assert!(PolicyResult::Audit("test".to_string()).is_allowed());
        assert!(!PolicyResult::Block("test".to_string()).is_allowed());
    }

    #[test]
    fn test_policy_result_is_blocked() {
        assert!(!PolicyResult::Allow.is_blocked());
        assert!(!PolicyResult::Warn("test".to_string()).is_blocked());
        assert!(PolicyResult::Block("test".to_string()).is_blocked());
    }

    #[test]
    fn test_policy_result_message() {
        assert!(PolicyResult::Allow.message().is_none());
        assert_eq!(
            PolicyResult::Block("blocked".to_string()).message(),
            Some("blocked")
        );
        assert_eq!(
            PolicyResult::Warn("warning".to_string()).message(),
            Some("warning")
        );
    }

    #[test]
    fn test_parse_range() {
        let service = create_test_service();
        let value = serde_json::json!({"min": 4, "max": 8});
        let range = service.parse_range(&value).unwrap();
        assert_eq!(range.min, 4.0);
        assert_eq!(range.max, 8.0);
    }

    #[test]
    fn test_parse_range_invalid() {
        let service = create_test_service();
        let value = serde_json::json!("not a range");
        assert!(service.parse_range(&value).is_none());
    }

    #[test]
    fn test_parse_list() {
        let service = create_test_service();
        let value = serde_json::json!(["CASH", "CARD", "QR"]);
        let list = service.parse_list(&value);
        assert_eq!(list, vec!["CASH", "CARD", "QR"]);
    }

    #[test]
    fn test_parse_list_empty() {
        let service = create_test_service();
        let value = serde_json::json!("not a list");
        let list = service.parse_list(&value);
        assert!(list.is_empty());
    }

    #[test]
    fn test_apply_enforcement_disabled() {
        let service = create_test_service();
        let result = service.apply_enforcement(&EnforcementMode::Disabled, "test");
        assert_eq!(result, PolicyResult::Allow);
    }

    #[test]
    fn test_apply_enforcement_warn() {
        let service = create_test_service();
        let result = service.apply_enforcement(&EnforcementMode::Warn, "test message");
        assert_eq!(result, PolicyResult::Warn("test message".to_string()));
    }

    #[test]
    fn test_apply_enforcement_block() {
        let service = create_test_service();
        let result = service.apply_enforcement(&EnforcementMode::Block, "test message");
        assert_eq!(result, PolicyResult::Block("test message".to_string()));
    }

    #[test]
    fn test_apply_enforcement_audit() {
        let service = create_test_service();
        let result = service.apply_enforcement(&EnforcementMode::Audit, "test message");
        assert_eq!(result, PolicyResult::Audit("test message".to_string()));
    }
}
