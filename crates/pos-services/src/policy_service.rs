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
//! // Check a policy. The match is exhaustive on purpose: the point of the last arm is that
//! // "the till could not decide" is an answer a caller must handle, not one it can fold into
//! // "allowed" — see [`PolicyResult`].
//! let result = service.check_boolean("ENCRYPTION_AT_REST").await;
//! match result {
//!     PolicyResult::Allow => println!("Allowed"),
//!     PolicyResult::Block(reason) => println!("Blocked: {}", reason),
//!     PolicyResult::Warn(reason) => println!("Warning: {}", reason),
//!     PolicyResult::Audit(reason) => println!("Allowed, audited: {}", reason),
//!     PolicyResult::NotEvaluable { code, reason } => {
//!         println!("No decision for {}: {}", code, reason.as_str())
//!     }
//! }
//! ```
//!
//! This example is fenced `rust,ignore` and therefore never compiles. It had drifted before this
//! module was touched — it was missing `Audit`, which had existed for as long as the enum had — so
//! it is the kind of claim that stays wrong indefinitely because nothing checks it.

use crate::policy_value::PolicyValue;
use pos_api::{ApiClient, EnforcementMode, GetResult, SecurityCategory, SecurityPoliciesResponse};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// Why the till was unable to decide.
///
/// Four of these are facts about the *platform's data or the till's knowledge of it*. The fifth,
/// [`NotEvaluableReason::CheckDoesNotFitTheDeclaredType`], is a fault in **this till** — and it is
/// separate precisely because the repairs differ: fix the platform's data, teach this till a new
/// policy type, or fix the caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotEvaluableReason {
    /// No refresh has ever succeeded, so the till does not know what its policies are.
    ///
    /// Distinct from *the platform sent no policies*, which is a real answer and permits.
    PoliciesNeverLoaded,
    /// The platform declared a type this till knows and sent a value that does not match it.
    ValueDoesNotMatchDeclaredType,
    /// The platform declared a policy type this till has never heard of.
    DeclaredTypeUnrecognised,
    /// The platform asked for an enforcement mode this till has never heard of.
    ///
    /// Load-bearing rather than pedantic: `EnforcementMode` carries `#[serde(other)]`, so **any**
    /// mode introduced in future deserialises to `Unknown` on every deployed till. Answering
    /// `Allow` there means the platform *tightening* a policy reads as no policy at all.
    EnforcementModeUnrecognised,
    /// The policy is readable and this check cannot interpret it — a boolean question asked of a
    /// range policy.
    ///
    /// The only variant that reports a defect in the till rather than in the platform's data.
    /// Nothing rejected this before: the caller chose the interpreter by choosing which `check_*`
    /// to call, so it was a well-typed call that silently misread a perfectly good value.
    CheckDoesNotFitTheDeclaredType,
}

impl NotEvaluableReason {
    /// A fixed description, suitable for a log line or a message to an operator.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::PoliciesNeverLoaded => "the till has never loaded its security policies",
            Self::ValueDoesNotMatchDeclaredType => {
                "the policy's value does not match the type the platform declared for it"
            }
            Self::DeclaredTypeUnrecognised => {
                "the platform declared a policy type this till does not recognise"
            }
            Self::EnforcementModeUnrecognised => {
                "the platform asked for an enforcement mode this till does not recognise"
            }
            Self::CheckDoesNotFitTheDeclaredType => {
                "this till asked a question the policy's declared type cannot answer"
            }
        }
    }
}

impl std::fmt::Display for NotEvaluableReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The answer to "may this action proceed under this policy?"
///
/// # Four verdicts and one non-verdict
///
/// `Allow`, `Block`, `Warn` and `Audit` are **decisions**. For as long as those were the only
/// answers, a check that *could not* decide still had to return one — and the only decision that
/// looks safe is `Allow`. The evidence that this was a defect rather than a simplification is that
/// the range check logged `warn!("Invalid range value for policy {}")` and then permitted: it had
/// correctly diagnosed the condition and had nowhere to put it.
///
/// [`PolicyResult::NotEvaluable`] is that missing place. It is **not** a fifth verdict and must
/// never be folded into one by a caller: it says the till has no basis for an answer, which is a
/// different thing from having decided to permit.
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
    /// The till could not evaluate the policy, and says so instead of guessing.
    NotEvaluable {
        /// The policy that could not be evaluated.
        code: String,
        /// What stopped the till deciding.
        reason: NotEvaluableReason,
    },
}

impl PolicyResult {
    /// Whether the action may proceed.
    ///
    /// # Why this is a positive match
    ///
    /// It was `!matches!(self, PolicyResult::Block(_))` — *anything that is not a block is
    /// allowed* — which would have made [`PolicyResult::NotEvaluable`] permit the moment it was
    /// added, silently, which is the exact failure this issue exists to remove. Written
    /// positively, a variant added later without thought is **refused** rather than permitted, and
    /// the compiler does not have to be relied on to notice.
    ///
    /// `Warn` and `Audit` remain allowed, deliberately: they are advisory modes, and the platform
    /// means them to proceed with a record.
    pub fn is_allowed(&self) -> bool {
        matches!(self, Self::Allow | Self::Warn(_) | Self::Audit(_))
    }

    /// Whether the platform refused the action outright.
    ///
    /// Not the complement of [`PolicyResult::is_allowed`], and the gap between them is the point:
    /// [`PolicyResult::NotEvaluable`] is neither allowed nor blocked. A caller that needs to act on
    /// one answer must handle three cases, not two.
    pub fn is_blocked(&self) -> bool {
        matches!(self, Self::Block(_))
    }

    /// What to tell someone, when there is anything to tell.
    pub fn message(&self) -> Option<&str> {
        match self {
            Self::Allow => None,
            Self::Block(msg) | Self::Warn(msg) | Self::Audit(msg) => Some(msg),
            Self::NotEvaluable { reason, .. } => Some(reason.as_str()),
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

/// What a check established before it looked at the policy's value.
///
/// Every check begins with the same three questions in the same order, and the order is the
/// fix: *has this till ever loaded its policies* comes **before** *is there one with this
/// code*, because the two used to produce the same answer and only one of them means the
/// action is permitted.
enum Lookup<'a> {
    /// The standing settles it; no value needs reading.
    Settled(PolicyResult),
    /// A policy is held for this code and is enforced. Read its value.
    Evaluate(&'a CachedPolicy),
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

    /// Resolves a code against the standing, before any value is interpreted.
    ///
    /// The three outcomes were one before this issue:
    ///
    /// - **never loaded** — the till does not know its policies, so it has no basis for an answer.
    ///   Previously indistinguishable from "no such policy", and therefore `Allow`. That is the
    ///   whole finding: `src/platform.rs:88-92` swallows a failed refresh and carries on, so an
    ///   offline-first till that boots without a network held no policies and permitted every
    ///   operation — its routine state, not an error path.
    /// - **loaded, no such code** — the platform told this till its policies and this is not among
    ///   them. Genuinely unconfigured, and `Allow` is correct.
    /// - **loaded, and this policy is `Disabled`** — the platform is explicitly not enforcing it.
    fn lookup<'a>(held: &'a HeldPolicies, code: &str) -> Lookup<'a> {
        let Some(policies) = held.loaded() else {
            warn!("cannot evaluate {code}: this till has never loaded its security policies");
            return Lookup::Settled(PolicyResult::NotEvaluable {
                code: code.to_string(),
                reason: NotEvaluableReason::PoliciesNeverLoaded,
            });
        };

        match policies.get(code) {
            None => {
                debug!("the platform configured no policy {code}");
                Lookup::Settled(PolicyResult::Allow)
            }
            Some(policy) if policy.enforcement_mode == EnforcementMode::Disabled => {
                Lookup::Settled(PolicyResult::Allow)
            }
            Some(policy) => Lookup::Evaluate(policy),
        }
    }

    /// The policy is held, and this check cannot get an answer out of it.
    ///
    /// Three different facts, kept apart because they call for different repairs: fix the
    /// platform's data, update this till, or fix this till's caller.
    fn cannot_read(code: &str, value: &PolicyValue) -> PolicyResult {
        let reason = match value {
            PolicyValue::Malformed { declared, .. } => {
                warn!("policy {code} declares {declared:?} and its value does not match");
                NotEvaluableReason::ValueDoesNotMatchDeclaredType
            }
            PolicyValue::UnknownType { .. } => {
                warn!("policy {code} declares a type this till does not recognise");
                NotEvaluableReason::DeclaredTypeUnrecognised
            }
            // The value is fine and the question is wrong: a boolean check against a range
            // policy. Nothing rejected this before — the caller chose the interpreter by
            // choosing which `check_*` to call, so it was a well-typed call that silently
            // misread the value.
            readable => {
                error!("policy {code} holds {readable:?}, which this check cannot interpret");
                NotEvaluableReason::CheckDoesNotFitTheDeclaredType
            }
        };

        PolicyResult::NotEvaluable {
            code: code.to_string(),
            reason,
        }
    }

    /// Evaluates a boolean policy.
    ///
    /// A value that is not a boolean no longer defaults to *required*. `as_bool().unwrap_or(true)`
    /// looked conservative and was not: it fed `apply_enforcement`, which permits under `Warn` and
    /// `Audit`, so an unreadable boolean refused an action only when that policy happened to be
    /// configured `Block`.
    pub async fn check_boolean(&self, code: &str) -> PolicyResult {
        let held = self.policies.read().await;
        let policy = match Self::lookup(&held, code) {
            Lookup::Settled(result) => return result,
            Lookup::Evaluate(policy) => policy,
        };

        match &policy.value {
            PolicyValue::Boolean(true) => self.apply_enforcement(
                &policy.enforcement_mode,
                &format!("Policy {} requires this feature to be enabled", code),
            ),
            PolicyValue::Boolean(false) => PolicyResult::Allow,
            other => Self::cannot_read(code, other),
        }
    }

    /// Checks a value against the range policy's bounds.
    ///
    /// Takes [`Decimal`]. The predecessor took `f64` because the bound was decoded with
    /// `as_f64()` at this point — money reached for as a float, against this codebase's first
    /// rule, because there was nowhere upstream to declare what the value meant.
    pub async fn check_range(&self, code: &str, value: Decimal) -> PolicyResult {
        let held = self.policies.read().await;
        let policy = match Self::lookup(&held, code) {
            Lookup::Settled(result) => return result,
            Lookup::Evaluate(policy) => policy,
        };

        let PolicyValue::Range(bounds) = &policy.value else {
            return Self::cannot_read(code, &policy.value);
        };

        if bounds.contains(value) {
            return PolicyResult::Allow;
        }

        let (limit, relation) = if value < bounds.min() {
            (bounds.min(), "below minimum")
        } else {
            (bounds.max(), "exceeds maximum")
        };
        self.apply_enforcement(
            &policy.enforcement_mode,
            &format!("Value {value} is {relation} {limit} for policy {code}"),
        )
    }

    /// Checks a value against the policy's allow-list.
    ///
    /// # The four roads to "allow everything", and why they are now four answers
    ///
    /// This was the worst of the checks and the clearest illustration of the issue. Every one of
    /// these produced `Allow`:
    ///
    /// | input | why it permitted |
    /// | --- | --- |
    /// | `{"allowed": []}` | a deliberate empty rule, read as *allow all* |
    /// | a value that is not an allow-list at all | `unwrap_or_default()` made it `vec![]` |
    /// | an allow-list of no readable elements | `filter_map` emptied it, then `vec![]` again |
    /// | no policy with this code | the missing-policy default |
    ///
    /// Three of the four are malformed, and the answer chosen for all four was the permissive one
    /// — a security control failing open on its own malformation. The middle two are now
    /// [`PolicyResult::NotEvaluable`], the last is `Allow` only once the till knows it has
    /// policies, and the empty rule is still a rule.
    ///
    /// **An empty allow-list still permits**, deliberately: `{"allowed": []}` is the platform
    /// saying so, and that is a decision it is entitled to make. What changed is that nothing else
    /// arrives here wearing it.
    pub async fn check_list(&self, code: &str, value: &str) -> PolicyResult {
        let held = self.policies.read().await;
        let policy = match Self::lookup(&held, code) {
            Lookup::Settled(result) => return result,
            Lookup::Evaluate(policy) => policy,
        };

        let PolicyValue::List(allowed) = &policy.value else {
            return Self::cannot_read(code, &policy.value);
        };

        if allowed.is_empty() || allowed.iter().any(|permitted| permitted == value) {
            return PolicyResult::Allow;
        }

        self.apply_enforcement(
            &policy.enforcement_mode,
            &format!("Value '{value}' is not in allowed list for policy {code}"),
        )
    }

    /// Checks a value against the single value the policy permits.
    ///
    /// `as_str().unwrap_or("")` had the same shape as the allow-list's empty case: an unreadable
    /// value became the empty string, and the empty string matched everything.
    pub async fn check_enum(&self, code: &str, value: &str) -> PolicyResult {
        let held = self.policies.read().await;
        let policy = match Self::lookup(&held, code) {
            Lookup::Settled(result) => return result,
            Lookup::Evaluate(policy) => policy,
        };

        let PolicyValue::Enum(expected) = &policy.value else {
            return Self::cannot_read(code, &policy.value);
        };

        if expected.is_empty() || expected == value {
            return PolicyResult::Allow;
        }

        self.apply_enforcement(
            &policy.enforcement_mode,
            &format!("Value '{value}' does not match expected '{expected}' for policy {code}"),
        )
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
        range.configured().to_u32()
    }

    /// Gets the session timeout in minutes
    pub async fn get_session_timeout_minutes(&self) -> Option<u32> {
        let policies = self.policies.read().await;
        let policy = policies.get("SESSION_TIMEOUT_MINUTES")?;
        let PolicyValue::Range(range) = &policy.value else {
            return None;
        };
        // Return min as default, or could return a default value from range
        range.configured().to_u32()
    }

    /// Gets the maximum offline transaction amount
    pub async fn get_offline_max_amount(&self) -> Option<f64> {
        let policies = self.policies.read().await;
        let policy = policies.get("OFFLINE_MAX_AMOUNT")?;
        let PolicyValue::Range(range) = &policy.value else {
            return None;
        };
        range.configured().to_f64()
    }

    /// Gets the heartbeat interval in seconds
    pub async fn get_heartbeat_interval_seconds(&self) -> Option<u32> {
        let policies = self.policies.read().await;
        let policy = policies.get("HEARTBEAT_INTERVAL_SECONDS")?;
        let PolicyValue::Range(range) = &policy.value else {
            return None;
        };
        range.configured().to_u32()
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
        range.configured().to_u32()
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

    /// A non-verdict is neither allowed nor blocked, and `is_allowed` is where that could rot.
    ///
    /// The predecessor was `!matches!(self, Block(_))`, so this variant would have been permitted
    /// from the moment it existed — silently, with no compiler error, which is the failure shape
    /// this whole issue is about. Written as a positive match, the next variant added without
    /// thought is refused instead.
    #[test]
    fn a_result_the_till_could_not_decide_is_not_allowed_and_not_blocked() {
        let undecided = PolicyResult::NotEvaluable {
            code: "MIN_PIN_LENGTH".to_string(),
            reason: NotEvaluableReason::PoliciesNeverLoaded,
        };

        assert!(!undecided.is_allowed(), "a non-verdict must not permit");
        assert!(!undecided.is_blocked(), "and it is not a refusal either");
        assert!(undecided.message().is_some(), "it has something to say");

        // The controls, in both directions. Without the first, an `is_allowed` broken open to
        // always-false would satisfy every assertion above and read as a pass. Without the
        // second, the same is true of `is_blocked`.
        assert!(PolicyResult::Allow.is_allowed());
        assert!(PolicyResult::Block("refused".to_string()).is_blocked());
    }

    /// `Warn` and `Audit` permit, and that is deliberate rather than an oversight of the same kind.
    ///
    /// Asserted so a later reader tightening `is_allowed` has to do it knowingly: these are
    /// advisory modes the platform means to proceed with a record, unlike the non-verdict above.
    #[test]
    fn advisory_verdicts_still_permit() {
        assert!(PolicyResult::Warn("noted".to_string()).is_allowed());
        assert!(PolicyResult::Audit("recorded".to_string()).is_allowed());
        assert!(!PolicyResult::Warn("noted".to_string()).is_blocked());
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

    /// Builds a response holding one policy, for the evaluation tests below.
    fn one_policy(
        code: &str,
        policy_type: PolicyType,
        value: serde_json::Value,
    ) -> SecurityPoliciesResponse {
        SecurityPoliciesResponse {
            version: "v1".to_string(),
            policies: vec![SecurityPolicy {
                code: code.to_string(),
                category: SecurityCategory::Authentication,
                policy_type,
                policy_value: value,
                enforcement_mode: EnforcementMode::Block,
            }],
        }
    }

    /// A till that has never loaded its policies cannot answer, and no longer pretends to.
    ///
    /// **This replaces `test_check_boolean_not_found`, which asserted `Allow` here.** That test
    /// was green for as long as the defect existed, because it pinned the defect as the contract:
    /// a fresh service has never refreshed, so every check permitted. Combined with
    /// `src/platform.rs:88-92` swallowing a failed refresh and carrying on, that is an
    /// offline-first till permitting every operation on any boot without a network — its routine
    /// state, not an error path.
    #[tokio::test]
    async fn a_till_that_never_loaded_its_policies_cannot_evaluate_one() {
        let service = create_test_service();

        assert_eq!(
            service.check_boolean("ENCRYPTION_AT_REST").await,
            PolicyResult::NotEvaluable {
                code: "ENCRYPTION_AT_REST".to_string(),
                reason: NotEvaluableReason::PoliciesNeverLoaded,
            }
        );
    }

    /// A till that *has* loaded them, and simply has no such policy, still permits.
    ///
    /// The control for the test above, and the reason the two had to become separable: without
    /// this, refusing everything would satisfy the never-loaded assertion and read as a pass. It
    /// is also the case that must not change — a platform that configured no such policy has said
    /// something, and the till is entitled to act on it.
    #[tokio::test]
    async fn a_loaded_till_permits_a_policy_the_platform_did_not_configure() {
        let service = create_test_service();
        service
            .update_policies(SecurityPoliciesResponse {
                version: "v1".to_string(),
                policies: vec![],
            })
            .await;

        assert_eq!(
            service.check_boolean("NON_EXISTENT_POLICY").await,
            PolicyResult::Allow
        );
    }

    /// A boolean question asked of a range policy is refused, not silently misread.
    ///
    /// This was a well-typed call nothing rejected: the caller chose the interpreter by choosing
    /// which `check_*` to call, and `as_bool()` on a range object returned `None`, which
    /// `unwrap_or(true)` turned into *required*.
    #[tokio::test]
    async fn a_check_that_does_not_fit_the_declared_type_is_refused() {
        let service = create_test_service();
        service
            .update_policies(one_policy(
                "MIN_PIN_LENGTH",
                PolicyType::Range,
                serde_json::json!({"min": 4, "max": 8, "default": 4}),
            ))
            .await;

        assert_eq!(
            service.check_boolean("MIN_PIN_LENGTH").await,
            PolicyResult::NotEvaluable {
                code: "MIN_PIN_LENGTH".to_string(),
                reason: NotEvaluableReason::CheckDoesNotFitTheDeclaredType,
            }
        );

        // The control: the same policy asked the question it can answer produces a real verdict.
        // Without it, a `check_*` broken open to refuse everything passes the assertion above.
        assert_eq!(
            service
                .check_range("MIN_PIN_LENGTH", Decimal::from(6))
                .await,
            PolicyResult::Allow
        );
    }

    /// A policy whose value contradicts its declared type is refused rather than permitted.
    #[tokio::test]
    async fn a_malformed_value_is_refused_rather_than_permitted() {
        let service = create_test_service();
        service
            .update_policies(one_policy(
                "ALLOWED_PAYMENT_METHODS",
                PolicyType::List,
                serde_json::json!("not-an-allow-list"),
            ))
            .await;

        assert_eq!(
            service.check_list("ALLOWED_PAYMENT_METHODS", "CASH").await,
            PolicyResult::NotEvaluable {
                code: "ALLOWED_PAYMENT_METHODS".to_string(),
                reason: NotEvaluableReason::ValueDoesNotMatchDeclaredType,
            }
        );
    }

    /// The four roads `check_list` had to "allow everything" now end in three different places.
    ///
    /// The whole issue in one test. Each row was `PolicyResult::Allow` before this task, and three
    /// of the four were malformed input reaching the permissive answer.
    #[tokio::test]
    async fn the_allow_list_roads_no_longer_meet() {
        let service = create_test_service();

        // 1. Never loaded — cannot answer.
        assert!(matches!(
            service.check_list("ALLOWED_PAYMENT_METHODS", "CASH").await,
            PolicyResult::NotEvaluable {
                reason: NotEvaluableReason::PoliciesNeverLoaded,
                ..
            }
        ));

        // 2. A deliberate empty rule — still permits, and that is the platform's decision to make.
        service
            .update_policies(one_policy(
                "ALLOWED_PAYMENT_METHODS",
                PolicyType::List,
                serde_json::json!({"allowed": []}),
            ))
            .await;
        assert_eq!(
            service.check_list("ALLOWED_PAYMENT_METHODS", "CASH").await,
            PolicyResult::Allow
        );

        // 3. An allow-list of no readable elements — was `vec![]`, which meant *allow all*.
        service
            .update_policies(one_policy(
                "ALLOWED_PAYMENT_METHODS",
                PolicyType::List,
                serde_json::json!({"allowed": [1, 2, 3]}),
            ))
            .await;
        assert!(matches!(
            service.check_list("ALLOWED_PAYMENT_METHODS", "CASH").await,
            PolicyResult::NotEvaluable {
                reason: NotEvaluableReason::ValueDoesNotMatchDeclaredType,
                ..
            }
        ));

        // 4. A real rule that does not admit the value — a genuine refusal, and the control that
        //    proves the checker still reaches a verdict rather than refusing everything.
        service
            .update_policies(one_policy(
                "ALLOWED_PAYMENT_METHODS",
                PolicyType::List,
                serde_json::json!({"allowed": ["CARD"]}),
            ))
            .await;
        assert!(service
            .check_list("ALLOWED_PAYMENT_METHODS", "CASH")
            .await
            .is_blocked());
        assert_eq!(
            service.check_list("ALLOWED_PAYMENT_METHODS", "CARD").await,
            PolicyResult::Allow
        );
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
