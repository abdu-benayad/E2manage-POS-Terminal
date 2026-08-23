//! The figures a refusal carries beside its code.
//!
//! The platform nests a third field inside the error envelope's `error` object — `{code, message,
//! details}` — and until this module existed the till deserialised the envelope successfully and
//! threw `details` away. That is this issue's own defect one layer further in: a decision arrives
//! carrying exactly what the till needs to act on it, and the transport drops the payload and
//! keeps the adjective.
//!
//! # Why these are eight types and not one bag of optional fields
//!
//! `details` is typed **per code** server-side (`api-error-catalog.ts` maps each code to its
//! payload interface, and `respondWithApiError` will not compile a mismatch). A
//! `serde_json::Value` here would rebuild on this side the shape being deleted from
//! `PinVerificationResult`: a struct whose fields are meaningful only if you already know which
//! outcome you are looking at. So each payload is its own type, and [`RefusalDetails`] is the
//! only thing a caller matches on.
//!
//! Two of them carry the same single field today and are still two types, for the reason the
//! server gives in `operator-pin.details.ts`: [`PinPolicyViolationDetails`] answers *what your
//! company requires of a new PIN*, [`PinRotationRequiredDetails`] answers *what your company now
//! requires of the PIN you just proved you know*. Different screens read them, they will acquire
//! different fields, and merging them to save six lines buys a type that means two things.
//!
//! # A payload that does not match is not a parse failure
//!
//! [`RefusalDetails::read`] is total. A `details` object on a code that carries none, a missing
//! one on a code that does, an unreadable shape — each is a contract breach worth a `warn!` and
//! **not** grounds to fail the whole envelope. The refusal itself still arrived, the status and
//! the code are still true, and refusing to read them because a figure beside them was wrong
//! would turn a cosmetic disagreement into an authentication outage.
//!
//! # `lockedUntil` is rendered, never stored
//!
//! See [`LockoutNotice`]. It is the one field here that will look like something it must not be.

use chrono::{DateTime, Utc};
use pos_models::{AttemptsRemaining, MaxAttempts, OperatorRole, ParseError, PinLength};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use thiserror::Error;
use tracing::warn;

use crate::failure::ServerErrorCode;

// ============================================================================
// The values the payloads are made of
// ============================================================================

/// When a standing lockout is due to lift, **for display only**.
///
/// # This is not an unlock timer, and the type is shaped to stop it becoming one
///
/// [`pos_models::LockState`] has no expiry field on purpose. PCI DSS v4.0 §8.3.4 permits a lockout
/// to end after thirty minutes *or* when the user's identity is confirmed, and the till takes the
/// second branch: the first is a timer read from the clock of whoever is holding the device, so
/// anyone who can set the date can end their own lockout. Identity confirmation cannot be wound
/// forward.
///
/// The server sends this instant so the till can say "locked until 14:32" to the person standing
/// at the drawer. That is the whole use. Deriving `PartialOrd` here would make `notice < Utc::now()`
/// compile, so it is deliberately not derived, and the only accessor says what it is for.
///
/// [`CredentialExpiry`](pos_models::CredentialExpiry) may hold a time because expiry fails
/// **closed** — a clock wound forward only locks a credential sooner. A lockout that expires fails
/// **open**.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LockoutNotice(DateTime<Utc>);

impl LockoutNotice {
    /// Records the instant the server reported.
    pub const fn new(instant: DateTime<Utc>) -> Self {
        Self(instant)
    }

    /// The instant, to render in the operator's own zone.
    ///
    /// Named for its one use. Comparing the result against the local clock and unlocking on it is
    /// the defect this whole type exists to keep visible.
    pub const fn instant_to_render(self) -> DateTime<Utc> {
        self.0
    }
}

/// An RBAC permission code, as the platform spells it — `POS_REFUND`, `POS_FLEET_DECOMMISSION`.
///
/// A newtype over an open set rather than an enum, on the same reasoning as
/// [`ServerErrorCode::Unrecognised`]: the platform's `PermissionCode` union is a back-office
/// catalogue that grows without the till, and a capability this till has not been taught is a
/// thing that legitimately arrives. It is still not a `String` — a capability and a message are
/// not interchangeable, and only the constructor decides what counts as one.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CapabilityCode(String);

impl CapabilityCode {
    /// Records a capability code, rejecting a blank one.
    ///
    /// A refusal that names no capability tells the till nothing it can put on a screen, and is
    /// not a shape the server can emit.
    pub fn new(code: String) -> Option<Self> {
        (!code.trim().is_empty()).then_some(Self(code))
    }

    /// The spelling the platform uses.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for CapabilityCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// The operator roles that hold a capability — **at least one, by construction**.
///
/// Mirrors the server's `readonly [POS_OperatorRole, ...POS_OperatorRole[]]`. The refusal this
/// travels on means "fetch someone who can", so a refusal asking for approval from nobody is not a
/// state: the empty case is a different code entirely
/// ([`ServerErrorCode::PosOperatorCapabilityDenied`]) carrying a different payload. Storing a
/// `Vec` here would make "supervisor approval required, from nobody" representable and leave every
/// reader to check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeldBy {
    lowest: OperatorRole,
    rest: Vec<OperatorRole>,
}

impl HeldBy {
    /// Builds the list, rejecting an empty one. Lowest role first, as the server orders it.
    pub fn new(roles: Vec<OperatorRole>) -> Option<Self> {
        let mut roles = roles.into_iter();
        let lowest = roles.next()?;
        Some(Self {
            lowest,
            rest: roles.collect(),
        })
    }

    /// The lowest role that holds the capability — the one to ask for.
    pub const fn lowest(&self) -> OperatorRole {
        self.lowest
    }

    /// Every role that holds it, lowest first.
    pub fn iter(&self) -> impl Iterator<Item = OperatorRole> + '_ {
        std::iter::once(self.lowest).chain(self.rest.iter().copied())
    }

    /// How many roles hold it. Never zero.
    pub fn len(&self) -> usize {
        1 + self.rest.len()
    }

    /// Always false. Present so clippy's `len_without_is_empty` reads the invariant rather than
    /// suggesting a method whose answer is a constant.
    pub const fn is_empty(&self) -> bool {
        false
    }
}

// ============================================================================
// The eight payloads
// ============================================================================

/// `POS_PIN_INVALID` — the PIN was wrong and the operator may still try again.
///
/// `attempts_remaining` is never zero, here or on the wire: the attempt that exhausts the budget
/// is a lockout and refuses with [`OperatorLockedDetails`] instead. See
/// [`RefusalDetails::PinBudgetExhausted`] for what happens when the server contradicts that.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PinInvalidDetails {
    /// Attempts left before the account locks.
    pub attempts_remaining: AttemptsRemaining,
}

/// `POS_OPERATOR_LOCKED` — locked out; no PIN was compared and no attempt spent.
///
/// Deliberately no failure count: the lockout is the fact, and how many attempts produced it is
/// not something a till acts on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OperatorLockedDetails {
    /// When the lock lifts, to show the operator. **Never an unlock condition** — see
    /// [`LockoutNotice`].
    pub locked_until: LockoutNotice,
}

/// `POS_PIN_POLICY_VIOLATION` — a PIN being **minted** breaks the tenant's length rule.
///
/// Raised where a credential is created — new operator, PIN reset, rotation — never where one is
/// presented: a PIN legally minted under a four-digit rule is not made wrong by the company later
/// requiring six.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PinPolicyViolationDetails {
    /// The length a new PIN must have.
    pub required_length: PinLength,
}

/// `POS_PIN_ROTATION_REQUIRED` — the PIN was **correct** and is the wrong length for the rule now
/// in force.
///
/// A separate type from [`PinPolicyViolationDetails`] even though the field matches today. See
/// this module's header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PinRotationRequiredDetails {
    /// The length the operator's PIN must now be rotated to.
    pub required_length: PinLength,
}

/// `POS_SUPERVISOR_APPROVAL_REQUIRED` — a higher operator role holds what this one was refused.
///
/// Actionable at the till: someone standing there can supply it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupervisorApprovalRequiredDetails {
    /// What was refused.
    pub capability: CapabilityCode,
    /// Who holds it, lowest first. Never empty — see [`HeldBy`].
    pub held_by: HeldBy,
}

/// `POS_OPERATOR_CAPABILITY_DENIED` — no operator role holds it, so escalating at the till cannot
/// help.
///
/// Carries no roles because there are none to name: the person who can do this is signed into the
/// admin UI, not standing at the drawer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperatorCapabilityDeniedDetails {
    /// What was refused.
    pub capability: CapabilityCode,
}

/// `POS_OFFLINE_REPORT_EXPIRED` — the credential the report is charged against stopped counting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OfflineReportExpiredDetails {
    /// When the credential lapsed.
    ///
    /// Unlike [`LockoutNotice`] this is safe to compare against a clock: a credential expiry fails
    /// **closed**, so a device whose clock runs fast only stops trusting its credential sooner.
    pub not_after: DateTime<Utc>,
}

/// `POS_OFFLINE_REPORT_OVER_BUDGET` — more failures claimed than the credential's own budget
/// allowed.
///
/// The figure is the budget **the credential carried**, not the tenant's current one: the till is
/// being told which rule it was handed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OfflineReportOverBudgetDetails {
    /// The attempt budget that credential was sealed with.
    pub max_failed_attempts: MaxAttempts,
}

// ============================================================================
// RefusalDetails
// ============================================================================

/// The typed payload a refusal carried, if it carried one.
///
/// Read with [`Self::read`], which is total: anything that does not match becomes `None` and a
/// logged contract breach, never a failure to read the refusal itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefusalDetails {
    /// See [`PinInvalidDetails`].
    PinInvalid(PinInvalidDetails),

    /// `POS_PIN_INVALID` arrived claiming **zero** attempts remain.
    ///
    /// The server contradicting its own partition: the attempt that empties the budget is supposed
    /// to answer `POS_OPERATOR_LOCKED`. Read as a lock, because that is what "wrong PIN and no
    /// attempts left" means — and read as a lock **with no instant**, because inventing a
    /// `lockedUntil` the server did not send would be a fabrication rendered to a cashier.
    ///
    /// It is not `None`: dropping it would leave the caller holding "wrong PIN, no figures", which
    /// is exactly the reading that spends an attempt against an operator who has none left.
    PinBudgetExhausted,

    /// See [`OperatorLockedDetails`].
    OperatorLocked(OperatorLockedDetails),
    /// See [`PinPolicyViolationDetails`].
    PinPolicyViolation(PinPolicyViolationDetails),
    /// See [`PinRotationRequiredDetails`].
    PinRotationRequired(PinRotationRequiredDetails),
    /// See [`SupervisorApprovalRequiredDetails`].
    SupervisorApprovalRequired(SupervisorApprovalRequiredDetails),
    /// See [`OperatorCapabilityDeniedDetails`].
    OperatorCapabilityDenied(OperatorCapabilityDeniedDetails),
    /// See [`OfflineReportExpiredDetails`].
    OfflineReportExpired(OfflineReportExpiredDetails),
    /// See [`OfflineReportOverBudgetDetails`].
    OfflineReportOverBudget(OfflineReportOverBudgetDetails),
}

impl RefusalDetails {
    /// Reads the `details` beside a code. Total: never fails, never panics.
    ///
    /// A breach is logged at `warn!` and answered with `None`, except the one case that has a
    /// safe reading — see [`Self::PinBudgetExhausted`]. `error!` is reserved for
    /// [`ApiFailure::Unreadable`](crate::ApiFailure::Unreadable), where the *refusal* could not be
    /// read; here the refusal is intact and a figure beside it is not.
    pub fn read(code: &ServerErrorCode, raw: Option<&serde_json::Value>) -> Option<Self> {
        match Self::parse(code, raw) {
            Ok(details) => details,
            Err(DetailsBreach::NoAttemptsLeft) => {
                warn!(
                    "contract breach: {code} claimed 0 attempts remaining, which its own partition \
                     spells `POS_OPERATOR_LOCKED`; reading it as a lockout"
                );
                Some(Self::PinBudgetExhausted)
            }
            Err(breach) => {
                warn!("contract breach in the `details` beside {code}: {breach}");
                None
            }
        }
    }

    /// The pure half of [`Self::read`]. Exhaustive over every modelled code, with no catch-all
    /// arm: a code added to [`ServerErrorCode`] has to say here whether it carries a payload.
    fn parse(
        code: &ServerErrorCode,
        raw: Option<&serde_json::Value>,
    ) -> Result<Option<Self>, DetailsBreach> {
        use ServerErrorCode as C;

        match code {
            C::PosPinInvalid => {
                let wire: WirePinInvalid = payload(raw, "PinInvalidDetails")?;
                let attempts_remaining = AttemptsRemaining::new(wire.attempts_remaining)
                    .ok_or(DetailsBreach::NoAttemptsLeft)?;
                Ok(Some(Self::PinInvalid(PinInvalidDetails {
                    attempts_remaining,
                })))
            }
            C::PosOperatorLocked => {
                let wire: WireOperatorLocked = payload(raw, "OperatorLockedDetails")?;
                Ok(Some(Self::OperatorLocked(OperatorLockedDetails {
                    locked_until: LockoutNotice::new(wire.locked_until),
                })))
            }
            C::PosPinPolicyViolation => {
                let wire: WireRequiredLength = payload(raw, "PinPolicyViolationDetails")?;
                Ok(Some(Self::PinPolicyViolation(PinPolicyViolationDetails {
                    required_length: wire.pin_length()?,
                })))
            }
            C::PosPinRotationRequired => {
                let wire: WireRequiredLength = payload(raw, "PinRotationRequiredDetails")?;
                Ok(Some(Self::PinRotationRequired(PinRotationRequiredDetails {
                    required_length: wire.pin_length()?,
                })))
            }
            C::PosSupervisorApprovalRequired => {
                let wire: WireSupervisorApproval =
                    payload(raw, "SupervisorApprovalRequiredDetails")?;
                Ok(Some(Self::SupervisorApprovalRequired(
                    SupervisorApprovalRequiredDetails {
                        capability: capability(wire.capability)?,
                        held_by: held_by(wire.held_by)?,
                    },
                )))
            }
            C::PosOperatorCapabilityDenied => {
                let wire: WireCapability = payload(raw, "OperatorCapabilityDeniedDetails")?;
                Ok(Some(Self::OperatorCapabilityDenied(
                    OperatorCapabilityDeniedDetails {
                        capability: capability(wire.capability)?,
                    },
                )))
            }
            C::PosOfflineReportExpired => {
                let wire: WireOfflineReportExpired = payload(raw, "OfflineReportExpiredDetails")?;
                Ok(Some(Self::OfflineReportExpired(
                    OfflineReportExpiredDetails {
                        not_after: wire.not_after,
                    },
                )))
            }
            C::PosOfflineReportOverBudget => {
                let wire: WireOfflineReportOverBudget =
                    payload(raw, "OfflineReportOverBudgetDetails")?;
                let max_failed_attempts = MaxAttempts::new(wire.max_failed_attempts)
                    .map_err(|_| DetailsBreach::NoAttemptsPermitted)?;
                Ok(Some(Self::OfflineReportOverBudget(
                    OfflineReportOverBudgetDetails {
                        max_failed_attempts,
                    },
                )))
            }

            // A code this till has not been taught. Details beside it are unreadable by
            // definition and say nothing about whether the server is behaving: the unrecognised
            // *code* is already the signal, and a second warning about its payload would train
            // people to filter both.
            C::Unrecognised(_) => Ok(None),

            // Every code the catalogue gives no payload. Listed rather than matched with `_`, so
            // that a code gaining a payload server-side has to be moved out of this arm by hand.
            C::BadRequest
            | C::Unauthorized
            | C::Forbidden
            | C::NotFound
            | C::Conflict
            | C::ValidationError
            | C::InternalError
            | C::PosPinRequestInvalid
            | C::PosOperatorNotFound
            | C::PosOperatorInactive
            | C::PosPinUnchanged
            | C::PosOperatorSessionRequired
            | C::PosOperatorSessionInvalid
            | C::PosOperatorSessionExpired
            | C::PosOperatorSessionRevoked
            | C::PosTerminalTokenMissing
            | C::PosTerminalTokenInvalid
            | C::PosTerminalSessionExpired
            | C::PosTerminalSessionRevoked
            | C::PosTerminalNotActive
            | C::PosTerminalGone
            | C::PosTerminalNotProvisioned
            | C::PosTerminalAuthFailed
            | C::PosTerminalAuthRequired
            | C::PosCompanyInactive
            | C::PosTerminalActionNotAllowed
            | C::PosOfflineReportNoCredential
            | C::PosCommandTypeInvalid
            | C::PosCommandNotForTerminal
            // Fleet administration, and the till is not a fleet console. Both codes carry a
            // payload the catalogue types (`TerminalsNotFoundDetails`,
            // `CommandNotPendingDetails`) and neither reaches a till, so modelling them here
            // would be two shapes nothing reads. They are named so the next reader knows the
            // omission is a decision.
            | C::PosTerminalNotFound
            | C::PosCommandNotPending => match raw {
                None => Ok(None),
                Some(_) => Err(DetailsBreach::CarriesNone),
            },
        }
    }
}

// ============================================================================
// Reading the wire
// ============================================================================

/// Why a `details` payload could not be read as the type its code names.
#[derive(Debug, Error)]
enum DetailsBreach {
    #[error("the payload did not match `{expected}`: {source}")]
    Shape {
        expected: &'static str,
        #[source]
        source: serde_json::Error,
    },
    #[error("this code carries a `{expected}` payload and none arrived")]
    Omitted { expected: &'static str },
    #[error("this code carries no payload, and one arrived")]
    CarriesNone,
    #[error("`attemptsRemaining` was 0, which the server's own partition spells as a lockout")]
    NoAttemptsLeft,
    #[error("`maxFailedAttempts` was 0 — a budget no operator could satisfy")]
    NoAttemptsPermitted,
    #[error("`requiredLength` was {digits}, which is not a PIN length this till accepts")]
    UnsupportedPinLength { digits: u8 },
    #[error("`heldBy` was empty — a refusal asking for approval from nobody")]
    NobodyHoldsIt,
    /// Carries `pos_models`' own parse failure rather than restating it: that error already
    /// names the rejected spelling and the roles the server's enum admits, and a second copy of
    /// that sentence here is a second thing to keep in step.
    #[error("`heldBy` names a role this till does not know: {0}")]
    UnknownRole(#[source] ParseError),
    #[error("`capability` was blank")]
    BlankCapability,
}

/// Deserialises the payload a code promises, naming the type it failed to be.
fn payload<W: DeserializeOwned>(
    raw: Option<&serde_json::Value>,
    expected: &'static str,
) -> Result<W, DetailsBreach> {
    let raw = raw.ok_or(DetailsBreach::Omitted { expected })?;
    // Cloning rather than borrowing: `serde_json::from_value` consumes, and a details payload is
    // a handful of scalars. Reading it in place would need a lifetime on every wire struct to
    // save nothing measurable.
    serde_json::from_value(raw.clone()).map_err(|source| DetailsBreach::Shape { expected, source })
}

fn capability(raw: String) -> Result<CapabilityCode, DetailsBreach> {
    CapabilityCode::new(raw).ok_or(DetailsBreach::BlankCapability)
}

fn held_by(raw: Vec<String>) -> Result<HeldBy, DetailsBreach> {
    let roles = raw
        .into_iter()
        .map(|spelling| {
            spelling
                .parse::<OperatorRole>()
                .map_err(DetailsBreach::UnknownRole)
        })
        .collect::<Result<Vec<_>, _>>()?;
    HeldBy::new(roles).ok_or(DetailsBreach::NobodyHoldsIt)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WirePinInvalid {
    attempts_remaining: u8,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireOperatorLocked {
    locked_until: DateTime<Utc>,
}

/// The shape both PIN-length payloads share on the wire. One wire struct, two domain types: the
/// distinction the server draws is between the *refusals*, not between two JSON objects.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireRequiredLength {
    required_length: u8,
}

impl WireRequiredLength {
    fn pin_length(&self) -> Result<PinLength, DetailsBreach> {
        PinLength::try_from(self.required_length).map_err(|_| DetailsBreach::UnsupportedPinLength {
            digits: self.required_length,
        })
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireSupervisorApproval {
    capability: String,
    held_by: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireCapability {
    capability: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireOfflineReportExpired {
    not_after: DateTime<Utc>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireOfflineReportOverBudget {
    max_failed_attempts: u8,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The `error` object as `respondWithApiError` writes it, read the way production reads it.
    ///
    /// Going through [`crate::ApiErrorDetail`]'s own `Deserialize` rather than calling
    /// [`RefusalDetails::read`] directly: the hand-written impl is the part that could silently
    /// stop wiring `details` to its code, and a test that skips it would keep passing if it did.
    fn detail(code: &str, details: serde_json::Value) -> crate::ApiErrorDetail {
        serde_json::from_value(json!({
            "code": code,
            "message": "refused",
            "details": details,
        }))
        .expect("the error object always parses; a bad `details` is not a parse failure")
    }

    fn read(code: &str, details: serde_json::Value) -> Option<RefusalDetails> {
        detail(code, details).details
    }

    #[test]
    fn every_payload_the_catalogue_types_for_a_till_is_read() {
        assert_eq!(
            read("POS_PIN_INVALID", json!({ "attemptsRemaining": 2 })),
            Some(RefusalDetails::PinInvalid(PinInvalidDetails {
                attempts_remaining: AttemptsRemaining::new(2).expect("2 is not 0"),
            }))
        );

        let locked = read(
            "POS_OPERATOR_LOCKED",
            json!({ "lockedUntil": "2026-08-23T14:32:00.000Z" }),
        );
        let Some(RefusalDetails::OperatorLocked(locked)) = locked else {
            panic!("a locked refusal must carry its notice, got {locked:?}");
        };
        assert_eq!(
            locked.locked_until.instant_to_render().to_rfc3339(),
            "2026-08-23T14:32:00+00:00"
        );

        assert_eq!(
            read("POS_PIN_ROTATION_REQUIRED", json!({ "requiredLength": 6 })),
            Some(RefusalDetails::PinRotationRequired(
                PinRotationRequiredDetails {
                    required_length: PinLength::Six,
                }
            ))
        );
        assert_eq!(
            read("POS_PIN_POLICY_VIOLATION", json!({ "requiredLength": 4 })),
            Some(RefusalDetails::PinPolicyViolation(
                PinPolicyViolationDetails {
                    required_length: PinLength::Four,
                }
            ))
        );

        let approval = read(
            "POS_SUPERVISOR_APPROVAL_REQUIRED",
            json!({ "capability": "POS_REFUND", "heldBy": ["SUPERVISOR", "MANAGER"] }),
        );
        let Some(RefusalDetails::SupervisorApprovalRequired(approval)) = approval else {
            panic!("an escalatable refusal must name who can approve, got {approval:?}");
        };
        assert_eq!(approval.capability.as_str(), "POS_REFUND");
        assert_eq!(approval.held_by.lowest(), OperatorRole::Supervisor);
        assert_eq!(
            approval.held_by.iter().collect::<Vec<_>>(),
            vec![OperatorRole::Supervisor, OperatorRole::Manager]
        );

        assert_eq!(
            read(
                "POS_OPERATOR_CAPABILITY_DENIED",
                json!({ "capability": "POS_FLEET_DECOMMISSION" })
            ),
            Some(RefusalDetails::OperatorCapabilityDenied(
                OperatorCapabilityDeniedDetails {
                    capability: CapabilityCode::new("POS_FLEET_DECOMMISSION".to_string())
                        .expect("not blank"),
                }
            ))
        );

        let expired = read(
            "POS_OFFLINE_REPORT_EXPIRED",
            json!({ "notAfter": "2026-08-20T00:00:00.000Z" }),
        );
        let Some(RefusalDetails::OfflineReportExpired(expired)) = expired else {
            panic!("an expired credential must name when, got {expired:?}");
        };
        assert_eq!(expired.not_after.to_rfc3339(), "2026-08-20T00:00:00+00:00");

        assert_eq!(
            read(
                "POS_OFFLINE_REPORT_OVER_BUDGET",
                json!({ "maxFailedAttempts": 5 })
            ),
            Some(RefusalDetails::OfflineReportOverBudget(
                OfflineReportOverBudgetDetails {
                    max_failed_attempts: MaxAttempts::new(5).expect("5 is not 0"),
                }
            ))
        );
    }

    /// A code that carries no payload reads cleanly with `details` absent.
    #[test]
    fn a_code_that_carries_nothing_reads_with_no_details() {
        let detail: crate::ApiErrorDetail =
            serde_json::from_value(json!({ "code": "POS_OPERATOR_NOT_FOUND", "message": "no" }))
                .expect("the error object");

        assert_eq!(detail.code, ServerErrorCode::PosOperatorNotFound);
        assert!(detail.details.is_none());
    }

    /// Zero remaining attempts is the server contradicting its own partition. It reads as a lock,
    /// **not** as an `AttemptsRemaining(0)` that cannot exist and not as "wrong PIN, no figures".
    ///
    /// Dropping it to `None` is the reading that spends an attempt against an operator who has
    /// none left — this issue's defect, one layer in.
    #[test]
    fn zero_attempts_remaining_is_read_as_a_lock_and_not_as_a_counter() {
        assert_eq!(
            read("POS_PIN_INVALID", json!({ "attemptsRemaining": 0 })),
            Some(RefusalDetails::PinBudgetExhausted)
        );
    }

    /// "Supervisor approval required, from nobody" is unconstructible, so it cannot be read either.
    #[test]
    fn an_empty_held_by_is_refused() {
        assert!(HeldBy::new(Vec::new()).is_none());
        assert_eq!(
            read(
                "POS_SUPERVISOR_APPROVAL_REQUIRED",
                json!({ "capability": "POS_REFUND", "heldBy": [] })
            ),
            None
        );
    }

    /// Every way a payload can disagree with its code answers `None` — and the refusal survives.
    ///
    /// That second half is the point: `ApiErrorDetail` still parses, so the till acts on the code
    /// it was given. A figure it could not read must not cost it the decision that figure was
    /// attached to.
    #[test]
    fn a_payload_that_disagrees_with_its_code_costs_the_figures_and_not_the_refusal() {
        let disagreements = [
            // A shape that is not the payload the code names.
            (
                "POS_PIN_INVALID",
                json!({ "lockedUntil": "2026-08-23T14:32:00.000Z" }),
            ),
            // A length outside the set the platform itself accepts.
            ("POS_PIN_ROTATION_REQUIRED", json!({ "requiredLength": 7 })),
            // A budget nobody can satisfy.
            (
                "POS_OFFLINE_REPORT_OVER_BUDGET",
                json!({ "maxFailedAttempts": 0 }),
            ),
            // A role this till does not know.
            (
                "POS_SUPERVISOR_APPROVAL_REQUIRED",
                json!({ "capability": "POS_REFUND", "heldBy": ["AUDITOR"] }),
            ),
            // A capability that names nothing.
            (
                "POS_OPERATOR_CAPABILITY_DENIED",
                json!({ "capability": "   " }),
            ),
            // A payload on a code the catalogue gives none.
            ("POS_OPERATOR_NOT_FOUND", json!({ "attemptsRemaining": 3 })),
            // An unreadable instant.
            (
                "POS_OPERATOR_LOCKED",
                json!({ "lockedUntil": "half past two" }),
            ),
        ];

        for (code, details) in disagreements {
            let detail = detail(code, details.clone());
            assert!(
                detail.details.is_none(),
                "{code} with {details} must not produce figures"
            );
            assert_eq!(
                detail.code,
                ServerErrorCode::from(code.to_string()),
                "the refusal itself must survive an unreadable payload"
            );
        }
    }

    /// A code carrying a payload that did not arrive is a breach, and still not a parse failure.
    #[test]
    fn an_omitted_payload_leaves_the_refusal_intact() {
        let detail: crate::ApiErrorDetail =
            serde_json::from_value(json!({ "code": "POS_PIN_INVALID", "message": "wrong" }))
                .expect("the error object");

        assert_eq!(detail.code, ServerErrorCode::PosPinInvalid);
        assert!(detail.details.is_none());
    }

    /// A code this till has not been taught carries figures it cannot type, and that is not the
    /// server misbehaving.
    #[test]
    fn an_unrecognised_code_keeps_its_refusal_and_drops_its_figures() {
        let detail = detail("POS_SHIPPED_ON_TUESDAY", json!({ "anything": 1 }));

        assert!(!detail.code.is_recognised());
        assert!(detail.details.is_none());
    }

    /// The two PIN-length refusals stay two types even though the wire shape is one.
    ///
    /// Asserted rather than trusted to review: the temptation to merge them is exactly six lines
    /// of saving, and the server's own note says why not.
    #[test]
    fn the_two_pin_length_refusals_do_not_collapse_into_one() {
        let rotation = read("POS_PIN_ROTATION_REQUIRED", json!({ "requiredLength": 6 }));
        let policy = read("POS_PIN_POLICY_VIOLATION", json!({ "requiredLength": 6 }));

        assert_ne!(rotation, policy);
    }

    /// `lockedUntil` is a thing to draw, and this is the accessor that says so.
    ///
    /// The type has no ordering, so the comparison that would turn it into an unlock timer does
    /// not compile. `tests/guards.rs::a_lockout_notice_is_never_stored` holds the other half —
    /// that nothing writes it to the store.
    #[test]
    fn a_lockout_notice_is_only_ever_rendered() {
        let notice = LockoutNotice::new(
            "2026-08-23T14:32:00Z"
                .parse::<DateTime<Utc>>()
                .expect("an RFC 3339 instant"),
        );

        assert_eq!(
            notice.instant_to_render().to_rfc3339(),
            "2026-08-23T14:32:00+00:00"
        );
    }
}
