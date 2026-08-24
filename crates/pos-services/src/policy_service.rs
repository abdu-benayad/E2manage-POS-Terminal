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

use crate::policy_value::PolicyValue;
use pos_api::{ApiClient, EnforcementMode, GetResult, SecurityCategory, SecurityPoliciesResponse};
use rust_decimal::prelude::ToPrimitive;
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

/// A policy as the till holds it, once its value has been read against its declared type.
///
/// # Why the cache does not hold the wire shape
///
/// [`pos_api::SecurityPolicy`] pairs an untyped `policy_value` with a `policy_type` that says how
/// to read it, and nothing read the declaration — so the interpreter was chosen by whichever
/// `check_*` the caller called. Reading happens **once**, here, at the moment the policy arrives.
/// After this point there is no untyped value left for a later reader to guess at.
#[derive(Debug, Clone)]
pub struct CachedPolicy {
    /// The policy's code, e.g. `MIN_PIN_LENGTH`.
    pub code: String,
    /// Which area of the system it governs.
    pub category: SecurityCategory,
    /// What the platform asks the till to do when the policy is not satisfied.
    pub enforcement_mode: EnforcementMode,
    /// The value, read against the type the platform declared for it.
    pub value: PolicyValue,
}

/// The policies the till holds, and whether it has ever held any.
///
/// # Why this is not a `HashMap`
///
/// It was one, constructed empty and only ever filled by a successful refresh — so *a till that
/// has never reached the platform* and *a till whose company configured no policies* were the same
/// value, and no code downstream could separate them. That matters more here than it looks:
/// every `check_*` answers `Allow` for a code it cannot find, and `src/platform.rs:88-92` swallows
/// a failed refresh and carries on. An offline-first till that boots without a network therefore
/// holds no policies and permits everything — its routine state, not an error path.
///
/// Naming the absence is the prerequisite for refusing it. Task 05 is where the checks act on it;
/// this type is what makes acting possible.
#[derive(Debug)]
enum HeldPolicies {
    /// No refresh has ever succeeded. The till does not know what its policies are.
    NeverLoaded,
    /// A refresh succeeded. An empty map here is a real answer — the company configured none.
    Loaded(HashMap<String, CachedPolicy>),
}

impl HeldPolicies {
    /// The policy with this code, if one is held.
    ///
    /// `None` from [`HeldPolicies::NeverLoaded`] and `None` from a loaded map without the code are
    /// deliberately the same answer *to this question* — "is there a policy called X" has one
    /// truthful answer in both cases. The distinction lives in [`HeldPolicies::is_loaded`], which
    /// is `loaded().is_some()`, which is the question the checks must ask first.
    fn get(&self, code: &str) -> Option<&CachedPolicy> {
        match self {
            Self::NeverLoaded => None,
            Self::Loaded(policies) => policies.get(code),
        }
    }

    /// The held policies, or `None` if none have ever been loaded.
    fn loaded(&self) -> Option<&HashMap<String, CachedPolicy>> {
        match self {
            Self::NeverLoaded => None,
            Self::Loaded(policies) => Some(policies),
        }
    }
}

/// What the till can say about its policies without handing them over.
///
/// Separate from [`HeldPolicies`] because the answer to *"do you know your policies?"* is a fact
/// about the cache, not the cache itself — it is `Copy`, comparable, and cannot be mistaken for
/// the policies. `Loaded { count: 0 }` and `NeverLoaded` are different values, which is the whole
/// point and what the predecessor `has_policies() -> bool` could not express.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyStanding {
    /// No refresh has ever succeeded.
    NeverLoaded,
    /// A refresh succeeded and the till holds this many policies, possibly none.
    Loaded {
        /// How many policies the platform sent.
        count: usize,
    },
}

/// Policy Service
///
/// Manages security policies fetched from the platform.
pub struct PolicyService {
    api: Arc<ApiClient>,
    /// Cached policies indexed by code
    policies: RwLock<HeldPolicies>,
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
            policies: RwLock::new(HeldPolicies::NeverLoaded),
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
        let mut held = HashMap::new();

        for policy in response.policies {
            // The one place the platform's declared type is consulted. A value that does not
            // match its declaration becomes `PolicyValue::Malformed` here rather than a
            // permissive default at the point of use, and it never fails the refresh — a till
            // that drops a whole response because one policy was unreadable is a till holding no
            // policies, which permits everything.
            let value = PolicyValue::from_declared(&policy.policy_type, &policy.policy_value);
            if !value.is_understood() {
                warn!(
                    "Policy {} declared {:?} and its value does not match; it will not be enforced",
                    policy.code, policy.policy_type
                );
            }
            debug!(
                "Caching policy: {} ({:?})",
                policy.code, policy.enforcement_mode
            );
            held.insert(
                policy.code.clone(),
                CachedPolicy {
                    code: policy.code,
                    category: policy.category,
                    enforcement_mode: policy.enforcement_mode,
                    value,
                },
            );
        }

        info!("Cached {} security policies", held.len());
        *self.policies.write().await = HeldPolicies::Loaded(held);

        *self.version_hash.write().await = Some(response.version);
        *self.last_refresh.write().await = Some(std::time::Instant::now());
    }

    /// Gets the current version hash
    pub async fn version_hash(&self) -> Option<String> {
        self.version_hash.read().await.clone()
    }

    /// Gets the number of cached policies
    pub async fn policy_count(&self) -> usize {
        self.policies.read().await.loaded().map_or(0, HashMap::len)
    }

    /// What the till knows about its policies.
    ///
    /// Replaces `has_policies() -> bool`, which answered `false` for both *never refreshed* and
    /// *refreshed, and the company configured none* — a boolean standing in for a question with
    /// three answers.
    pub async fn standing(&self) -> PolicyStanding {
        match self.policies.read().await.loaded() {
            None => PolicyStanding::NeverLoaded,
            Some(policies) => PolicyStanding::Loaded {
                count: policies.len(),
            },
        }
    }

    /// Gets all policies in a category
    pub async fn get_policies_by_category(&self, category: SecurityCategory) -> Vec<CachedPolicy> {
        self.policies
            .read()
            .await
            .loaded()
            .map(|policies| {
                policies
                    .values()
                    .filter(|p| p.category == category)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Gets a specific policy by code
    pub async fn get_policy(&self, code: &str) -> Option<CachedPolicy> {
        self.policies.read().await.get(code).cloned()
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

                // Behaviour preserved: a value that is not a bool defaults to "required",
                // exactly as `as_bool().unwrap_or(true)` did. Task 05 replaces this with an
                // answer that says the policy could not be evaluated.
                let required = match &policy.value {
                    PolicyValue::Boolean(required) => *required,
                    _ => true,
                };

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

                let range = match &policy.value {
                    PolicyValue::Range(bounds) => Some(bounds.clone()),
                    _ => None,
                };

                match range {
                    Some(r) => {
                        if value < r.min().to_f64().unwrap_or(f64::MIN) {
                            self.apply_enforcement(
                                &policy.enforcement_mode,
                                &format!(
                                    "Value {} is below minimum {} for policy {}",
                                    value,
                                    r.min(),
                                    code
                                ),
                            )
                        } else if value > r.max().to_f64().unwrap_or(f64::MAX) {
                            self.apply_enforcement(
                                &policy.enforcement_mode,
                                &format!(
                                    "Value {} exceeds maximum {} for policy {}",
                                    value,
                                    r.max(),
                                    code
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

                // Behaviour preserved: anything that is not a readable list yields an empty
                // one, which the branch below reads as "allow all". Task 05 is where those stop
                // being the same value.
                let allowed_values = match &policy.value {
                    PolicyValue::List(values) => values.clone(),
                    _ => Vec::new(),
                };

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

                let expected = match &policy.value {
                    PolicyValue::Enum(expected) => expected.as_str(),
                    _ => "",
                };

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
        let PolicyValue::Range(range) = &policy.value else {
            return None;
        };
        range.min().to_u32()
    }

    /// Gets the session timeout in minutes
    pub async fn get_session_timeout_minutes(&self) -> Option<u32> {
        let policies = self.policies.read().await;
        let policy = policies.get("SESSION_TIMEOUT_MINUTES")?;
        let PolicyValue::Range(range) = &policy.value else {
            return None;
        };
        // Return min as default, or could return a default value from range
        range.min().to_u32()
    }

    /// Gets the maximum offline transaction amount
    pub async fn get_offline_max_amount(&self) -> Option<f64> {
        let policies = self.policies.read().await;
        let policy = policies.get("OFFLINE_MAX_AMOUNT")?;
        let PolicyValue::Range(range) = &policy.value else {
            return None;
        };
        range.max().to_f64()
    }

    /// Gets the heartbeat interval in seconds
    pub async fn get_heartbeat_interval_seconds(&self) -> Option<u32> {
        let policies = self.policies.read().await;
        let policy = policies.get("HEARTBEAT_INTERVAL_SECONDS")?;
        let PolicyValue::Range(range) = &policy.value else {
            return None;
        };
        range.min().to_u32()
    }

    /// Checks if PCI compliance mode is enabled
    pub async fn is_pci_compliance_enabled(&self) -> bool {
        let policies = self.policies.read().await;
        policies
            .get("PCI_COMPLIANCE_MODE")
            .map(|p| matches!(&p.value, PolicyValue::Boolean(true)))
            .unwrap_or(false)
    }

    /// Gets allowed payment methods
    pub async fn get_allowed_payment_methods(&self) -> Vec<String> {
        let policies = self.policies.read().await;
        policies
            .get("ALLOWED_PAYMENT_METHODS")
            .map(|p| match &p.value {
                PolicyValue::List(values) => values.clone(),
                _ => Vec::new(),
            })
            .unwrap_or_default()
    }

    /// Gets the receipt retention days
    pub async fn get_receipt_retention_days(&self) -> Option<u32> {
        let policies = self.policies.read().await;
        let policy = policies.get("RECEIPT_RETENTION_DAYS")?;
        let PolicyValue::Range(range) = &policy.value else {
            return None;
        };
        range.min().to_u32()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pos_api::{ApiClient, PolicyType, SecurityPolicy};

    fn create_test_service() -> PolicyService {
        let api = Arc::new(ApiClient::new("https://api.example.com"));
        PolicyService::new(api)
    }

    #[tokio::test]
    async fn a_fresh_service_has_never_loaded_its_policies() {
        let service = create_test_service();

        assert_eq!(service.standing().await, PolicyStanding::NeverLoaded);
        assert_eq!(service.policy_count().await, 0);
    }

    /// A platform that sends no policies is a different fact from a platform never reached.
    ///
    /// The predecessor `has_policies()` was `!is_empty()`, so both were `false` and nothing
    /// downstream could separate them. This is the distinction task 05 acts on: a till that has
    /// never loaded must not evaluate a policy check, while a till told there are no policies
    /// legitimately has none to apply.
    #[tokio::test]
    async fn a_refresh_that_returns_no_policies_is_not_the_same_as_never_refreshing() {
        let service = create_test_service();

        service
            .update_policies(SecurityPoliciesResponse {
                version: "v1".to_string(),
                policies: vec![],
            })
            .await;

        assert_eq!(
            service.standing().await,
            PolicyStanding::Loaded { count: 0 }
        );
        assert_eq!(service.policy_count().await, 0);

        // The control, and the reason it is asserted rather than left implied: both of the
        // assertions above hold for `NeverLoaded` too if `count` is ignored, so without this the
        // test passes on the very confusion it exists to rule out.
        assert_ne!(
            service.standing().await,
            PolicyStanding::NeverLoaded,
            "an empty load must not be indistinguishable from never having loaded"
        );
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

    /// The cache holds values already read against their declaration.
    ///
    /// Replaces four tests that exercised `parse_range`/`parse_list` directly; those parsers are
    /// gone and `policy_value`'s own suite covers their behaviour more precisely.
    ///
    /// **One of them is worth not restoring.** `test_parse_list_empty` asserted that
    /// `"not a list"` produced an empty `Vec` — which `check_list` then read as *allow all*. The
    /// suite pinned the compression as correct behaviour, so the defect was green for as long as
    /// it existed. `policy_value::tests::a_deliberate_empty_list_is_not_the_same_value_as_an_unreadable_one`
    /// asserts the opposite.
    #[tokio::test]
    async fn a_refresh_caches_values_read_against_their_declared_type() {
        let service = create_test_service();

        service
            .update_policies(SecurityPoliciesResponse {
                version: "v1".to_string(),
                policies: vec![
                    SecurityPolicy {
                        code: "MIN_PIN_LENGTH".to_string(),
                        category: SecurityCategory::Authentication,
                        policy_type: PolicyType::Range,
                        policy_value: serde_json::json!({"min": 4, "max": 8}),
                        enforcement_mode: EnforcementMode::Block,
                    },
                    SecurityPolicy {
                        code: "BROKEN".to_string(),
                        category: SecurityCategory::Authentication,
                        policy_type: PolicyType::Range,
                        policy_value: serde_json::json!("not a range"),
                        enforcement_mode: EnforcementMode::Block,
                    },
                ],
            })
            .await;

        let good = service.get_policy("MIN_PIN_LENGTH").await.expect("cached");
        assert!(matches!(good.value, PolicyValue::Range(_)));

        // The unreadable policy is cached as unreadable, not dropped and not fatal. A refresh
        // that discarded the whole response over one bad policy would leave the till holding
        // none — and a till with no policies permits everything.
        let bad = service
            .get_policy("BROKEN")
            .await
            .expect("cached, not dropped");
        assert!(matches!(bad.value, PolicyValue::Malformed { .. }));
        assert_eq!(service.policy_count().await, 2);
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
