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
    /// The policy is readable and the question asked of it cannot interpret it — a boolean
    /// question asked of a range policy, or a range accessor asked of a boolean one.
    ///
    /// The only variant that reports a defect in the till rather than in the platform's data.
    /// Nothing rejected this before: the caller chose the interpreter by choosing which method to
    /// call, so it was a well-typed call that silently misread a perfectly good value.
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
    ///
    /// The value-reading family says the same thing with
    /// [`PolicyReading::NotEvaluable`], over the same [`NotEvaluableReason`]. A reader who
    /// finds one of the two should find the other: they are one commitment applied to the
    /// two shapes a policy question comes in — *may I do this* and *what did the platform
    /// configure*.
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

/// What an accessor can say about a configured policy value.
///
/// The accessor-side twin of [`PolicyResult::NotEvaluable`]. That variant exists so a *check* with
/// no basis for a verdict says so instead of returning one. This type is the same commitment for
/// the family that reads *values* — which was left returning a bare `Option` when the checks were
/// repaired, and so still compressed four distinct states into one absence.
///
/// # Why three variants and not two
///
/// [`PolicyReading::NotConfigured`] means the till holds its policies and the platform configured
/// no such rule. That is **an answer**, and a caller may legitimately act on it.
/// [`PolicyReading::NotEvaluable`] means the till has no basis for one: it has never been online,
/// or the value does not match the type the platform declared for it.
///
/// Folding the two into a single absent case rebuilds the defect this type exists to remove, with
/// a better name on it. `None` meaning *the platform has decided* and `None` meaning *this till
/// does not know* is exactly the compression that let a till with no network answer permissively.
///
/// # A precedent, not an invention
///
/// The same shape already carries this distinction three times here: `PinVerification::Undetermined`
/// and `HardwareEnrolment::Undetermined` in `pos-models`, and [`crate::PlatformSync`]'s. Each names
/// *the till could not establish this* as a case a caller must handle, rather than an absence it
/// may default away.
///
/// # There is deliberately no `unwrap_or`
///
/// A fallback is a decision, and the point of this type is that the decision is made and visible at
/// the call site. [`PolicyReading::configured`] exists for a caller that has genuinely concluded
/// both non-answers mean the same thing to it — and that caller has to write it down.
#[derive(Debug, Clone, PartialEq)]
pub enum PolicyReading<T> {
    /// The platform configured this policy and the till read its value.
    Configured(T),
    /// The till holds its policies, and none of them has this code.
    NotConfigured,
    /// The till has no basis for an answer.
    NotEvaluable(NotEvaluableReason),
}

impl<T> PolicyReading<T> {
    /// The configured value, for a caller that has decided both non-answers mean the same to it.
    ///
    /// Named `configured` rather than `value` or `ok` because what it discards is *why* there is
    /// no value, and the name should make that discard legible where it happens. A caller reaching
    /// for this is asserting that *the platform configured no such rule* and *this till does not
    /// know* lead it to the same behaviour — sometimes true, and it must be said rather than
    /// assumed by the type.
    pub fn configured(self) -> Option<T> {
        match self {
            Self::Configured(value) => Some(value),
            Self::NotConfigured | Self::NotEvaluable(_) => None,
        }
    }

    /// Whether the till can answer at all — true for both [`PolicyReading::Configured`] and
    /// [`PolicyReading::NotConfigured`].
    ///
    /// **Not the complement of "has a value", and the gap is the point** — the same gap as the one
    /// between [`PolicyResult::is_allowed`] and [`PolicyResult::is_blocked`]. *The platform
    /// configured nothing* is knowledge; *this till has never been online* is not.
    pub fn is_known(&self) -> bool {
        matches!(self, Self::Configured(_) | Self::NotConfigured)
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
        PolicyResult::NotEvaluable {
            code: code.to_string(),
            reason: Self::unreadable_reason(code, value),
        }
    }

    /// Why a held policy yielded no answer — the discrimination itself, with no verdict on it.
    ///
    /// Shared by the check family (through [`PolicyService::cannot_read`]) and by the accessors
    /// (through [`PolicyService::configured_range`]), which need the same three facts and return
    /// different types. What is shared is deliberately the **reason**, not a `PolicyResult`: that
    /// type carries four verdicts an accessor can never produce, and pushing a value question
    /// through it would need either a catch-all that is only currently unreachable or an
    /// `unreachable!()`, which is a panic path this codebase does not accept.
    fn unreadable_reason(code: &str, value: &PolicyValue) -> NotEvaluableReason {
        match value {
            PolicyValue::Malformed { declared, .. } => {
                warn!("policy {code} declares {declared:?} and its value does not match");
                NotEvaluableReason::ValueDoesNotMatchDeclaredType
            }
            PolicyValue::UnknownType { .. } => {
                warn!("policy {code} declares a type this till does not recognise");
                NotEvaluableReason::DeclaredTypeUnrecognised
            }
            // The value is fine and the question is wrong: a boolean check against a range
            // policy, or a range accessor against a boolean one. Nothing rejected this before —
            // the caller chose the interpreter by choosing which method to call, so it was a
            // well-typed call that silently misread the value.
            readable => {
                error!("policy {code} holds {readable:?}, which this question cannot interpret");
                NotEvaluableReason::CheckDoesNotFitTheDeclaredType
            }
        }
    }

    /// Resolves a code to the numeric setting the platform configured for it.
    ///
    /// The accessor-side counterpart of [`PolicyService::lookup`], and deliberately **not** built
    /// on it. `lookup` answers *may this proceed*; this answers *what did the platform configure*,
    /// and the two differ on more than their return type:
    ///
    /// - `lookup` folds *loaded, no such code* into `Allow`, because an unconfigured policy
    ///   correctly permits. Here it is [`PolicyReading::NotConfigured`], which is an answer in its
    ///   own right and not the same as the till not knowing.
    /// - **This does not look at `enforcement_mode`, and that is a decision.** `lookup` treats
    ///   `Disabled` as `Allow`, because a policy the platform is not enforcing cannot refuse
    ///   anything. A disabled policy still *has* a configured value, and an accessor is being
    ///   asked what that value is, not whether to enforce it. A caller that needs to know whether
    ///   the policy is live reads [`CachedPolicy::enforcement_mode`] through
    ///   [`PolicyService::get_policy`].
    ///
    /// The first `loaded()` is the whole repair: [`HeldPolicies::get`] returns `None` for
    /// *never loaded* and for *loaded, absent* alike — deliberately, because that is one answer to
    /// *is there a policy called X* — so an accessor that starts from `get` cannot recover the
    /// distinction afterwards. Asking the standing first is what makes the two separable.
    fn configured_range(held: &HeldPolicies, code: &str) -> PolicyReading<Decimal> {
        let Some(policies) = held.loaded() else {
            debug!("cannot read {code}: this till has never loaded its security policies");
            return PolicyReading::NotEvaluable(NotEvaluableReason::PoliciesNeverLoaded);
        };

        let Some(policy) = policies.get(code) else {
            debug!("the platform configured no policy {code}");
            return PolicyReading::NotConfigured;
        };

        match &policy.value {
            PolicyValue::Range(range) => PolicyReading::Configured(range.configured()),
            other => PolicyReading::NotEvaluable(Self::unreadable_reason(code, other)),
        }
    }

    /// The same reading, narrowed to the whole-number quantity four of these policies are.
    ///
    /// A minute count, a second count, a day count and a PIN length are whole numbers; the
    /// platform sends them inside the same `Range`, in [`Decimal`]. The narrowing can fail — a
    /// negative bound, or one past `u32::MAX` — and that failure is why this family exists:
    /// `range.min as u32` saturated silently, so a bound the till could not make sense of became
    /// a minimum PIN length of **zero**.
    ///
    /// A setting that will not narrow is reported as
    /// [`NotEvaluableReason::ValueDoesNotMatchDeclaredType`]: the platform declared a whole-number
    /// quantity and sent something that is not one.
    fn configured_whole_number(held: &HeldPolicies, code: &str) -> PolicyReading<u32> {
        match Self::configured_range(held, code) {
            PolicyReading::Configured(setting) => match setting.to_u32() {
                Some(whole) => PolicyReading::Configured(whole),
                None => {
                    warn!("policy {code} is set to {setting}, which is not a whole count");
                    PolicyReading::NotEvaluable(NotEvaluableReason::ValueDoesNotMatchDeclaredType)
                }
            },
            // Restated rather than mapped, so a variant added to `PolicyReading` later is a
            // compile error here instead of being folded into whichever arm a catch-all chose.
            PolicyReading::NotConfigured => PolicyReading::NotConfigured,
            PolicyReading::NotEvaluable(reason) => PolicyReading::NotEvaluable(reason),
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
                code,
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
            code,
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
            code,
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
            code,
            &policy.enforcement_mode,
            &format!("Value '{value}' does not match expected '{expected}' for policy {code}"),
        )
    }

    /// Applies the enforcement mode to create the appropriate result
    /// Turns "this policy is not satisfied" into what the platform asked the till to do about it.
    ///
    /// # An enforcement mode this till does not recognise must not permit
    ///
    /// `EnforcementMode` carries `#[serde(other)]`, so **any** mode the platform introduces in
    /// future deserialises to `Unknown` on every till already in the field. This arm used to
    /// return `Allow`, and composing the two gives a forward-compatibility failure in the one
    /// direction a security control must never fail in: **the platform tightening a policy would
    /// read at the till as no policy at all** — silently, with no error and no log line.
    ///
    /// It cannot be `Block` either. The till does not know what the mode means, and refusing an
    /// action on a rule it cannot read is a different wrong answer, not a safer one. It is
    /// [`PolicyResult::NotEvaluable`]: the till has no basis for a verdict and says so.
    ///
    /// # Why the sibling catch-alls are left alone
    ///
    /// Four other enums in `pos-api/src/platform.rs` carry the same attribute and only this one is
    /// changed:
    ///
    /// - `SecurityCategory` — a label. Nothing gates on it; its only reader has no callers.
    /// - `PolicyType` — already handled, and earlier: an unrecognised declared type becomes
    ///   [`PolicyValue::UnknownType`] at the boundary, so it never reaches a verdict.
    /// - `LicenseStatus` — not read at all. Received, `{:?}`-formatted into a log line, discarded.
    ///   Filed separately; it is an uncalled leaf, not something this issue can fix by typing.
    /// - `PlatformCommandType` — **deliberately unchanged.** Ignoring a command this till does not
    ///   recognise is correct for every command defined today, all five of which are optional.
    ///
    /// That last one is a property of the **current vocabulary**, not of the type. A future
    /// *lock-this-terminal-now* would be a constraint wearing an instruction's clothes, and
    /// ignoring it would be exactly as dangerous as permitting an unrecognised enforcement mode.
    /// The rule that separates them is about what a variant *does*, not what its enum is called:
    ///
    /// > An unrecognised **instruction to act** should be ignored. An unrecognised **constraint on
    /// > acting** must not be.
    fn apply_enforcement(&self, code: &str, mode: &EnforcementMode, message: &str) -> PolicyResult {
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
            EnforcementMode::Unknown => {
                error!(
                    "policy {code} asks for an enforcement mode this till does not recognise; it \
                     cannot be applied and will not be treated as absent"
                );
                PolicyResult::NotEvaluable {
                    code: code.to_string(),
                    reason: NotEvaluableReason::EnforcementModeUnrecognised,
                }
            }
        }
    }

    // =========================================================================
    // CONVENIENCE METHODS FOR COMMON POLICIES
    // =========================================================================

    /// The shortest PIN this till should accept.
    ///
    /// # Why a reading and not `u32`, and why not `Option<u32>` either
    ///
    /// This read `range.min as u32` — a saturating float-to-integer cast with no error path, so a
    /// bound the till could not make sense of became **a minimum PIN length of zero**. That is the
    /// one member of this family that reaches a person: a till accepting an empty PIN.
    ///
    /// Narrowing through `to_u32()` closed the invented number. It left the *absence* wrong:
    /// `None` meant the platform set no minimum, and it meant this till has never been online, and
    /// a caller choosing a fallback could not tell which it had. [`PolicyReading`] separates them,
    /// and offers no `unwrap_or`, so the caller's fallback is written where the caller can see it.
    pub async fn get_min_pin_length(&self) -> PolicyReading<u32> {
        let held = self.policies.read().await;
        Self::configured_whole_number(&held, "MIN_PIN_LENGTH")
    }

    /// How long a session may idle before this till locks it.
    pub async fn get_session_timeout_minutes(&self) -> PolicyReading<u32> {
        let held = self.policies.read().await;
        Self::configured_whole_number(&held, "SESSION_TIMEOUT_MINUTES")
    }

    /// The largest sale this till may complete while offline.
    ///
    /// # `Decimal`, and why that is not a formality here
    ///
    /// This is money — the value of a sale — and this codebase's first rule is that money is never
    /// a float. It returned `Option<f64>` because the bound was decoded with `serde_json`'s
    /// `as_f64()` at the point of use, back when the policy value arrived untyped and there was
    /// nowhere to declare what it meant.
    ///
    /// Anything other than [`PolicyReading::Configured`] means **the till has no ceiling to apply**
    /// — and it does not mean zero. A caller must not treat it as one: an offline ceiling of zero
    /// refuses every sale. Which of the two non-answers it is matters here more than anywhere else
    /// in this family, because they call for opposite handling. `NotConfigured` is the platform
    /// declining to cap offline sales; `NotEvaluable` is a till that cannot yet say whether it is
    /// capped, which is a decision for whoever owns the offline path rather than a licence.
    pub async fn get_offline_max_amount(&self) -> PolicyReading<Decimal> {
        let held = self.policies.read().await;
        Self::configured_range(&held, "OFFLINE_MAX_AMOUNT")
    }

    /// How often this till should report itself to the platform.
    pub async fn get_heartbeat_interval_seconds(&self) -> PolicyReading<u32> {
        let held = self.policies.read().await;
        Self::configured_whole_number(&held, "HEARTBEAT_INTERVAL_SECONDS")
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

    /// How long this till should keep receipts before discarding them.
    pub async fn get_receipt_retention_days(&self) -> PolicyReading<u32> {
        let held = self.policies.read().await;
        Self::configured_whole_number(&held, "RECEIPT_RETENTION_DAYS")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pos_api::{ApiClient, PolicyType, SecurityPolicy};
    use serde_json::json;

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

    /// The three cases a value reading has, and the two questions worth asking of one.
    ///
    /// The assertion that matters is the last pair: `NotConfigured` is **known** and
    /// `NotEvaluable` is not, while both answer `None` to `configured()`. A `PolicyReading`
    /// collapsed back to two variants would still satisfy every `configured()` assertion here
    /// and fail these, which is the only reason they are separate lines.
    #[test]
    fn a_reading_separates_what_the_platform_decided_from_what_the_till_does_not_know() {
        let configured: PolicyReading<u32> = PolicyReading::Configured(6);
        let absent: PolicyReading<u32> = PolicyReading::NotConfigured;
        let unknown: PolicyReading<u32> =
            PolicyReading::NotEvaluable(NotEvaluableReason::PoliciesNeverLoaded);

        assert_eq!(configured.clone().configured(), Some(6));
        assert_eq!(absent.clone().configured(), None);
        assert_eq!(unknown.clone().configured(), None);

        assert!(configured.is_known(), "a value the platform set is known");
        assert!(
            absent.is_known(),
            "and so is the platform having set nothing — that is an answer"
        );
        assert!(
            !unknown.is_known(),
            "a till that has never been online knows neither"
        );
    }

    /// The reason survives the reading, so a caller can tell *why* it cannot answer.
    ///
    /// Without this, `NotEvaluable` would be as uninformative as the `None` it replaces — the
    /// caller would know the till cannot answer and have nothing to log or show.
    #[test]
    fn a_reading_that_cannot_be_evaluated_carries_why() {
        let unknown: PolicyReading<Decimal> =
            PolicyReading::NotEvaluable(NotEvaluableReason::ValueDoesNotMatchDeclaredType);

        let PolicyReading::NotEvaluable(reason) = unknown else {
            panic!("constructed as NotEvaluable");
        };
        assert_eq!(reason, NotEvaluableReason::ValueDoesNotMatchDeclaredType);
        assert_ne!(
            reason,
            NotEvaluableReason::PoliciesNeverLoaded,
            "the reasons are distinguishable, so the assertion above is not vacuous"
        );
    }

    #[tokio::test]
    async fn a_fresh_service_has_never_loaded_its_policies() {
        let service = create_test_service();

        assert_eq!(service.standing().await, PolicyStanding::NeverLoaded);

        // The control. `NeverLoaded` is the *only* other variant, so asserting it alone cannot
        // fail if `standing()` were broken to a constant — the sibling test below pins the other
        // one against the same service type, and these two together are what make either
        // assertion mean anything.
        assert_ne!(
            service.standing().await,
            PolicyStanding::Loaded { count: 0 },
            "never loaded is not the same fact as loaded and empty"
        );
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

    /// `check_enum` refuses what it cannot evaluate, like its three siblings.
    ///
    /// # Why this is a separate test and not a line in one of the others
    ///
    /// It is the fourth enforcement primitive and it had **zero references in the tree** — no
    /// caller, and, until this, no test. The other three were each asserted while this issue was
    /// being built, so "every `check_*` fails open" was reported closed on evidence covering three
    /// of four. That is the same shape as `get_min_pin_length` being correct by coincidence in task
    /// 09: a family checked through the members that happened to be reachable.
    ///
    /// Four assertions, because refusing everything and permitting everything each satisfy half of
    /// them: the two not-evaluable reasons, a match that permits, and a mismatch that blocks.
    #[tokio::test]
    async fn the_fourth_enforcement_primitive_refuses_what_it_cannot_evaluate_too() {
        let never_loaded = create_test_service();
        assert_eq!(
            never_loaded.check_enum("PIN_COMPLEXITY", "NUMERIC").await,
            PolicyResult::NotEvaluable {
                code: "PIN_COMPLEXITY".to_string(),
                reason: NotEvaluableReason::PoliciesNeverLoaded,
            },
            "a till with no policies has no basis for an enum verdict either"
        );

        let mistyped = create_test_service();
        mistyped
            .update_policies(one_policy(
                "MIN_PIN_LENGTH",
                PolicyType::Range,
                serde_json::json!({"min": 4, "max": 8, "default": 4}),
            ))
            .await;
        assert_eq!(
            mistyped.check_enum("MIN_PIN_LENGTH", "NUMERIC").await,
            PolicyResult::NotEvaluable {
                code: "MIN_PIN_LENGTH".to_string(),
                reason: NotEvaluableReason::CheckDoesNotFitTheDeclaredType,
            },
            "an enum question asked of a range policy is the caller's fault, not the platform's"
        );

        let configured = create_test_service();
        configured
            .update_policies(one_policy(
                "PIN_COMPLEXITY",
                PolicyType::Enum,
                serde_json::json!("NUMERIC"),
            ))
            .await;

        // The positive control: a service that refused everything would satisfy both assertions
        // above and read as a pass.
        assert_eq!(
            configured.check_enum("PIN_COMPLEXITY", "NUMERIC").await,
            PolicyResult::Allow
        );
        // And the negative: one that permitted everything would satisfy the positive.
        assert!(
            configured
                .check_enum("PIN_COMPLEXITY", "ALPHANUMERIC")
                .await
                .is_blocked(),
            "a value the policy does not name must be refused under Block enforcement"
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
        assert_eq!(
            service.standing().await,
            PolicyStanding::Loaded { count: 2 }
        );
    }

    fn range_policy(code: &str, value: serde_json::Value) -> SecurityPolicy {
        SecurityPolicy {
            code: code.to_string(),
            category: SecurityCategory::Authentication,
            policy_type: PolicyType::Range,
            policy_value: value,
            enforcement_mode: EnforcementMode::Block,
        }
    }

    async fn service_holding(policies: Vec<SecurityPolicy>) -> PolicyService {
        let service = create_test_service();
        service
            .update_policies(SecurityPoliciesResponse {
                version: "v1".to_string(),
                policies,
            })
            .await;
        service
    }

    /// Every state `get_min_pin_length` can be in, asserted as a distinct variant.
    ///
    /// Written as one test on purpose. Split across four, each would assert a single value and
    /// none would show that the four are *different* — which is the entire claim. The predecessor
    /// returned `Option<u32>` and answered `None` to four of these five questions, so a caller
    /// picking a fallback could not tell a platform that set no minimum from a till that has
    /// never been online. This is the till accepting an empty PIN, one type away.
    #[tokio::test]
    async fn a_pin_length_reading_says_which_of_the_five_things_it_is() {
        let never_loaded = create_test_service();
        assert_eq!(
            never_loaded.get_min_pin_length().await,
            PolicyReading::NotEvaluable(NotEvaluableReason::PoliciesNeverLoaded),
            "a till that has never reached the platform knows no minimum"
        );

        let loaded_without_it = service_holding(vec![]).await;
        assert_eq!(
            loaded_without_it.get_min_pin_length().await,
            PolicyReading::NotConfigured,
            "and a platform that configured no minimum is a different fact"
        );

        let configured = service_holding(vec![range_policy(
            "MIN_PIN_LENGTH",
            json!({"min": 4, "max": 8}),
        )])
        .await;
        assert_eq!(
            configured.get_min_pin_length().await,
            PolicyReading::Configured(4)
        );

        let unreadable =
            service_holding(vec![range_policy("MIN_PIN_LENGTH", json!("not a range"))]).await;
        assert_eq!(
            unreadable.get_min_pin_length().await,
            PolicyReading::NotEvaluable(NotEvaluableReason::ValueDoesNotMatchDeclaredType)
        );

        // The narrowing failure, which is the original defect stated as a test: `range.min as u32`
        // turned this into a minimum PIN length of **zero**, and a saturating cast has no error
        // path to notice it in.
        let will_not_narrow = service_holding(vec![range_policy(
            "MIN_PIN_LENGTH",
            json!({"min": -1, "max": 8}),
        )])
        .await;
        assert_eq!(
            will_not_narrow.get_min_pin_length().await,
            PolicyReading::NotEvaluable(NotEvaluableReason::ValueDoesNotMatchDeclaredType)
        );
        assert_ne!(
            will_not_narrow.get_min_pin_length().await,
            PolicyReading::Configured(0),
            "the cast that invented this zero is what the whole family exists to close"
        );
        // …and this is that case rather than the unreadable one above it. Both report the same
        // reason, so without pinning the cached value the two are indistinguishable here and the
        // narrowing path could stop being exercised without any assertion noticing.
        assert!(
            matches!(
                will_not_narrow
                    .get_policy("MIN_PIN_LENGTH")
                    .await
                    .expect("cached")
                    .value,
                PolicyValue::Range(_)
            ),
            "the range parsed; it is the narrowing to u32 that refuses"
        );
    }

    /// A question the policy's declared type cannot answer is reported as that, not as absence.
    ///
    /// The accessor reaches the same discrimination the `check_*` family does, through the shared
    /// `unreadable_reason`. Without this the boolean case would fall into whichever reason the
    /// range path happened to use, and the log would blame the platform's data for a defect in
    /// the till's call.
    #[tokio::test]
    async fn a_range_accessor_asked_of_a_boolean_policy_says_so() {
        let service = service_holding(vec![SecurityPolicy {
            code: "MIN_PIN_LENGTH".to_string(),
            category: SecurityCategory::Authentication,
            policy_type: PolicyType::Boolean,
            policy_value: json!(true),
            enforcement_mode: EnforcementMode::Block,
        }])
        .await;

        assert_eq!(
            service.get_min_pin_length().await,
            PolicyReading::NotEvaluable(NotEvaluableReason::CheckDoesNotFitTheDeclaredType)
        );
    }

    /// A disabled policy still has a configured value, and an accessor reports it.
    ///
    /// Deliberate, and the one place the accessors part company with `lookup`, which folds
    /// `Disabled` into `Allow` because a policy the platform is not enforcing cannot refuse
    /// anything. An accessor is asked *what did the platform configure*, not *may this proceed*.
    /// Asserted so that a later reader "fixing" the resolver to consult `enforcement_mode` has to
    /// delete a test that says why it does not.
    #[tokio::test]
    async fn an_unenforced_policy_still_has_a_value_to_read() {
        let service = service_holding(vec![SecurityPolicy {
            code: "SESSION_TIMEOUT_MINUTES".to_string(),
            category: SecurityCategory::Authentication,
            policy_type: PolicyType::Range,
            policy_value: json!({"min": 5, "max": 60, "default": 15}),
            enforcement_mode: EnforcementMode::Disabled,
        }])
        .await;

        assert_eq!(
            service.get_session_timeout_minutes().await,
            PolicyReading::Configured(15)
        );
    }

    /// The money accessor keeps its value in [`Decimal`] and its non-answers apart.
    ///
    /// `NotConfigured` and `NotEvaluable` are asserted as distinct here rather than through
    /// `is_none()`, because the caller that owns the offline path has to treat them differently:
    /// one is the platform declining to cap offline sales, the other is a till that cannot say.
    #[tokio::test]
    async fn an_offline_ceiling_that_is_absent_is_not_a_ceiling_that_is_unknown() {
        let loaded_without_it = service_holding(vec![]).await;
        assert_eq!(
            loaded_without_it.get_offline_max_amount().await,
            PolicyReading::NotConfigured
        );

        let never_loaded = create_test_service();
        assert_eq!(
            never_loaded.get_offline_max_amount().await,
            PolicyReading::NotEvaluable(NotEvaluableReason::PoliciesNeverLoaded)
        );

        // Money keeps its exact decimal — the predecessor returned `Option<f64>`, and this
        // codebase's first rule is that a sale value is never a float.
        let configured = service_holding(vec![range_policy(
            "OFFLINE_MAX_AMOUNT",
            json!({"min": 0, "max": 10000, "default": 1500.25}),
        )])
        .await;
        assert_eq!(
            configured.get_offline_max_amount().await,
            PolicyReading::Configured(Decimal::new(150025, 2))
        );
    }

    /// The remaining three whole-number accessors read their own codes and not each other's.
    ///
    /// The five share one resolver, so a transposed code string would be invisible in a test that
    /// only ever configures one policy at a time: each accessor would still find *a* range and
    /// return *a* number. Holding all three at once with distinct values is what makes the
    /// mapping assertable.
    #[tokio::test]
    async fn each_accessor_reads_its_own_policy_code() {
        let service = service_holding(vec![
            range_policy(
                "SESSION_TIMEOUT_MINUTES",
                json!({"min": 5, "max": 60, "default": 15}),
            ),
            range_policy(
                "HEARTBEAT_INTERVAL_SECONDS",
                json!({"min": 30, "max": 300, "default": 60}),
            ),
            range_policy(
                "RECEIPT_RETENTION_DAYS",
                json!({"min": 1, "max": 365, "default": 90}),
            ),
        ])
        .await;

        assert_eq!(
            service.get_session_timeout_minutes().await,
            PolicyReading::Configured(15)
        );
        assert_eq!(
            service.get_heartbeat_interval_seconds().await,
            PolicyReading::Configured(60)
        );
        assert_eq!(
            service.get_receipt_retention_days().await,
            PolicyReading::Configured(90)
        );

        // The control for the three above: an accessor whose code is absent from a service that
        // holds three other ranges must still say `NotConfigured`, not pick up a neighbour's.
        assert_eq!(
            service.get_min_pin_length().await,
            PolicyReading::NotConfigured
        );
    }

    /// An enforcement mode this till has never heard of does not permit.
    ///
    /// **The payload is deserialised rather than constructed**, because the defect is the
    /// composition of two things and building `EnforcementMode::Unknown` by hand would exercise
    /// only one of them. `#[serde(other)]` is what turns an unfamiliar string into `Unknown`
    /// instead of a parse error, and that is the half that makes this reach every till already in
    /// the field the moment the platform adds a mode.
    ///
    /// The mechanism has its own passing control in the producing crate —
    /// `pos-api/src/platform.rs:597` proves `#[serde(other)]` swallows an unrecognised string —
    /// so this test is about the consequence, not the mechanism.
    #[tokio::test]
    async fn an_enforcement_mode_this_till_does_not_know_is_not_permission() {
        let response: SecurityPoliciesResponse = serde_json::from_str(
            r#"{
                "version": "v1",
                "policies": [{
                    "code": "ENCRYPTION_AT_REST",
                    "category": "ENCRYPTION",
                    "policyType": "BOOLEAN",
                    "policyValue": true,
                    "enforcementMode": "LOCKOUT"
                }]
            }"#,
        )
        .expect("an unfamiliar enforcement mode must not fail the whole refresh");

        let service = create_test_service();
        service.update_policies(response).await;

        let result = service.check_boolean("ENCRYPTION_AT_REST").await;

        assert_eq!(
            result,
            PolicyResult::NotEvaluable {
                code: "ENCRYPTION_AT_REST".to_string(),
                reason: NotEvaluableReason::EnforcementModeUnrecognised,
            },
            "a mode the till cannot apply must not read as no policy at all"
        );
        assert!(!result.is_allowed(), "and it must not permit");

        // Two controls. The first: the same policy under a mode the till *does* know still
        // reaches a real verdict, so this is not a checker that refuses everything. The second:
        // `Unknown` is genuinely what deserialisation produced — if `"LOCKOUT"` had failed to
        // parse, the `expect` above would have fired and this test would be about nothing.
        let known: SecurityPoliciesResponse = serde_json::from_str(
            r#"{"version":"v2","policies":[{"code":"ENCRYPTION_AT_REST","category":"ENCRYPTION",
                 "policyType":"BOOLEAN","policyValue":true,"enforcementMode":"BLOCK"}]}"#,
        )
        .expect("a known mode parses");
        service.update_policies(known).await;
        assert!(service
            .check_boolean("ENCRYPTION_AT_REST")
            .await
            .is_blocked());
    }

    #[test]
    fn test_apply_enforcement_disabled() {
        let service = create_test_service();
        let result = service.apply_enforcement("A_POLICY", &EnforcementMode::Disabled, "test");
        assert_eq!(result, PolicyResult::Allow);
    }

    #[test]
    fn test_apply_enforcement_warn() {
        let service = create_test_service();
        let result = service.apply_enforcement("A_POLICY", &EnforcementMode::Warn, "test message");
        assert_eq!(result, PolicyResult::Warn("test message".to_string()));
    }

    #[test]
    fn test_apply_enforcement_block() {
        let service = create_test_service();
        let result = service.apply_enforcement("A_POLICY", &EnforcementMode::Block, "test message");
        assert_eq!(result, PolicyResult::Block("test message".to_string()));
    }

    #[test]
    fn test_apply_enforcement_audit() {
        let service = create_test_service();
        let result = service.apply_enforcement("A_POLICY", &EnforcementMode::Audit, "test message");
        assert_eq!(result, PolicyResult::Audit("test message".to_string()));
    }
}
